//! The stable error model: the nine codes of spec §6.2, their HTTP and transport
//! classification, and the exit code every one of them maps to.

use serde_json::{Value, json};
use thiserror::Error;

/// The nine stable `error.code` values. Agents match on these strings, so the
/// spellings are contract, not free to change (spec §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    AuthRequired,
    NotFound,
    ValidationError,
    ApiError,
    NetworkError,
    Forbidden,
    Conflict,
    Cancelled,
    Unknown,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 9] = [
        ErrorCode::AuthRequired,
        ErrorCode::NotFound,
        ErrorCode::ValidationError,
        ErrorCode::ApiError,
        ErrorCode::NetworkError,
        ErrorCode::Forbidden,
        ErrorCode::Conflict,
        ErrorCode::Cancelled,
        ErrorCode::Unknown,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::AuthRequired => "auth_required",
            ErrorCode::NotFound => "not_found",
            ErrorCode::ValidationError => "validation_error",
            ErrorCode::ApiError => "api_error",
            ErrorCode::NetworkError => "network_error",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::Conflict => "conflict",
            ErrorCode::Cancelled => "cancelled",
            ErrorCode::Unknown => "unknown",
        }
    }

    /// The human label the plain (non-JSON) error line leads with.
    pub fn label(&self) -> &'static str {
        match self {
            ErrorCode::AuthRequired => "Auth required",
            ErrorCode::NotFound => "Not found",
            ErrorCode::ValidationError => "Validation error",
            ErrorCode::ApiError => "API error",
            ErrorCode::NetworkError => "Network error",
            ErrorCode::Forbidden => "Forbidden",
            ErrorCode::Conflict => "Conflict",
            ErrorCode::Cancelled => "Cancelled",
            ErrorCode::Unknown => "Error",
        }
    }
}

/// One failure, in the shape both renderers and every command share. The single
/// error style in `ado-core` is `thiserror` (spec D11), so `Display` is the
/// message every renderer prints and the type is usable with `?`.
#[derive(Debug, Clone, PartialEq, Error)]
#[error("{message}")]
pub struct AdoError {
    pub code: ErrorCode,
    pub status: Option<u16>,
    pub message: String,
    pub details: Option<Value>,
}

impl AdoError {
    pub fn auth_required() -> AdoError {
        AdoError {
            code: ErrorCode::AuthRequired,
            status: None,
            message: "Not authenticated. Run 'ado login --method pat --org ORG --pat TOKEN' or set ADO_ORG+ADO_PAT.".to_owned(),
            details: Some(json!({
                "hint": "Set ADO_ORG and ADO_PAT env vars, or run `ado login --method pat --org ORG --pat TOKEN`",
                "scopes": "PAT must have: vso.work, vso.code, vso.project, vso.build, vso.release",
            })),
        }
    }

    pub fn validation(message: impl Into<String>) -> AdoError {
        AdoError {
            code: ErrorCode::ValidationError,
            status: None,
            message: message.into(),
            details: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> AdoError {
        AdoError {
            code: ErrorCode::NotFound,
            status: None,
            message: message.into(),
            details: None,
        }
    }

    /// The refusal at an interactive confirmation prompt (D32): nothing was done,
    /// so the run exits 1 like every other error, but a refusal is not an API
    /// failure — the renderer writes its message to stderr in both modes and
    /// emits no envelope, leaving `--json` stdout without a document that would
    /// claim the work happened.
    pub fn cancelled(message: impl Into<String>) -> AdoError {
        AdoError {
            code: ErrorCode::Cancelled,
            status: None,
            message: message.into(),
            details: None,
        }
    }

    /// The error for an HTTP response the API rejected, classified per spec §6.2.
    pub fn from_status(status: u16, body: impl Into<String>) -> AdoError {
        let body = body.into();
        let message = status_message(status)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("API error {status}: {body}"));

        AdoError {
            code: classify_status(status),
            status: Some(status),
            message,
            details: Some(json!({ "status": status, "body": body })),
        }
    }

    /// The error for a request that produced no usable response — a transport
    /// failure, or a status error that leaked past the client's own status
    /// handling. `details` carries the reason, mirroring the Elixir CLI's
    /// `%{"reason" => inspect(reason)}` (spec §6.2).
    pub fn from_transport(error: &ureq::Error) -> AdoError {
        let (code, message) = classify_transport(error);
        let status = match error {
            ureq::Error::StatusCode(status) => Some(*status),
            _ => None,
        };

        AdoError {
            code,
            status,
            message,
            details: Some(json!({ "reason": format!("{error:?}") })),
        }
    }
}

pub fn classify_status(status: u16) -> ErrorCode {
    match status {
        302 | 401 => ErrorCode::AuthRequired,
        403 | 429 => ErrorCode::Forbidden,
        404 => ErrorCode::NotFound,
        409 => ErrorCode::Conflict,
        422 => ErrorCode::ValidationError,
        _ => ErrorCode::ApiError,
    }
}

pub fn classify_transport(error: &ureq::Error) -> (ErrorCode, String) {
    match error {
        ureq::Error::Timeout(_) => (
            ErrorCode::NetworkError,
            "Request timed out. Check your network connection.".to_owned(),
        ),
        ureq::Error::HostNotFound => (
            ErrorCode::NetworkError,
            "DNS lookup failed. Check the server URL.".to_owned(),
        ),
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::ConnectionRefused => (
            ErrorCode::NetworkError,
            "Connection refused. Is the server reachable?".to_owned(),
        ),
        // Defense in depth: a status that leaked through as an error is still a
        // status, and it must classify by the §6.2 table, never as a network error.
        ureq::Error::StatusCode(status) => (
            classify_status(*status),
            status_message(*status)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("API error {status}")),
        ),
        other => (ErrorCode::NetworkError, format!("Request failed: {other}")),
    }
}

/// Every error path exits 1 (spec §6.3). `lib/ado_cli/cli/output.ex`'s richer table
/// has no callers, so it is deliberately not ported.
pub fn exit_code_for(_code: &ErrorCode) -> i32 {
    1
}

fn status_message(status: u16) -> Option<&'static str> {
    match status {
        302 => Some("API redirected to sign-in page. Run 'ado login' to authenticate."),
        401 => Some("Authentication failed. PAT is invalid or expired."),
        403 => Some(
            "Forbidden — your PAT lacks the required scope, or you don't have access to this resource.",
        ),
        404 => Some("Resource not found. Check the project/repo/build ID and your permissions."),
        409 => Some("Conflict — the resource already exists or is in an invalid state."),
        422 => Some("Azure DevOps rejected the request as invalid."),
        429 => Some("Rate limited by Azure DevOps. Slow down and retry."),
        500..=599 => Some("Azure DevOps server error. Retry later."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use serde_json::json;

    use super::*;

    #[test]
    fn classify_status_302_and_401_are_auth_required() {
        assert_eq!(classify_status(302), ErrorCode::AuthRequired);
        assert_eq!(classify_status(401), ErrorCode::AuthRequired);
    }

    #[test]
    fn classify_status_403_and_429_are_forbidden() {
        assert_eq!(classify_status(403), ErrorCode::Forbidden);
        assert_eq!(classify_status(429), ErrorCode::Forbidden);
    }

    #[test]
    fn classify_status_404_is_not_found() {
        assert_eq!(classify_status(404), ErrorCode::NotFound);
    }

    #[test]
    fn classify_status_409_is_conflict() {
        assert_eq!(classify_status(409), ErrorCode::Conflict);
    }

    #[test]
    fn classify_status_422_is_validation_error() {
        assert_eq!(classify_status(422), ErrorCode::ValidationError);
    }

    #[test]
    fn classify_status_500_and_503_are_api_error() {
        assert_eq!(classify_status(500), ErrorCode::ApiError);
        assert_eq!(classify_status(503), ErrorCode::ApiError);
    }

    #[test]
    fn classify_status_418_is_api_error() {
        assert_eq!(classify_status(418), ErrorCode::ApiError);
    }

    #[test]
    fn classify_transport_timeout_is_network_error() {
        let (code, message) = classify_transport(&ureq::Error::Timeout(ureq::Timeout::Global));

        assert_eq!(code, ErrorCode::NetworkError);
        assert_eq!(message, "Request timed out. Check your network connection.");
    }

    #[test]
    fn classify_transport_dns_failure_is_network_error() {
        let (code, message) = classify_transport(&ureq::Error::HostNotFound);

        assert_eq!(code, ErrorCode::NetworkError);
        assert_eq!(message, "DNS lookup failed. Check the server URL.");
    }

    #[test]
    fn classify_transport_connection_refused_is_network_error() {
        let refused = ureq::Error::Io(io::Error::new(io::ErrorKind::ConnectionRefused, "refused"));
        let (code, message) = classify_transport(&refused);

        assert_eq!(code, ErrorCode::NetworkError);
        assert_eq!(message, "Connection refused. Is the server reachable?");
    }

    #[test]
    fn classify_transport_other_is_network_error() {
        let (code, message) = classify_transport(&ureq::Error::TooManyRedirects);

        assert_eq!(code, ErrorCode::NetworkError);
        assert_eq!(message, "Request failed: too many redirects");
    }

    #[test]
    fn classify_transport_a_leaked_status_is_never_a_network_error() {
        let (code, message) = classify_transport(&ureq::Error::StatusCode(404));
        assert_eq!(code, ErrorCode::NotFound);
        assert_eq!(
            message,
            "Resource not found. Check the project/repo/build ID and your permissions."
        );

        let (code, message) = classify_transport(&ureq::Error::StatusCode(503));
        assert_eq!(code, ErrorCode::ApiError);
        assert_eq!(message, "Azure DevOps server error. Retry later.");

        let (code, message) = classify_transport(&ureq::Error::StatusCode(418));
        assert_eq!(code, ErrorCode::ApiError);
        assert_eq!(message, "API error 418");
    }

    #[test]
    fn from_transport_carries_the_reason() {
        let refused = ureq::Error::Io(io::Error::new(io::ErrorKind::ConnectionRefused, "refused"));
        let error = AdoError::from_transport(&refused);

        assert_eq!(error.code, ErrorCode::NetworkError);
        assert_eq!(error.status, None);
        assert_eq!(
            error.message,
            "Connection refused. Is the server reachable?"
        );
        assert!(
            error.details.expect("the reason")["reason"]
                .as_str()
                .expect("a string reason")
                .contains("refused")
        );

        let leaked = AdoError::from_transport(&ureq::Error::StatusCode(404));
        assert_eq!(leaked.code, ErrorCode::NotFound);
        assert_eq!(leaked.status, Some(404));
    }

    #[test]
    fn exit_code_for_is_always_one() {
        for code in ErrorCode::ALL {
            assert_eq!(exit_code_for(&code), 1, "code {code:?}");
        }
    }

    #[test]
    fn codes_match_the_contract() {
        let codes = ErrorCode::ALL.map(|code| code.as_str());

        assert_eq!(
            codes,
            [
                "auth_required",
                "not_found",
                "validation_error",
                "api_error",
                "network_error",
                "forbidden",
                "conflict",
                "cancelled",
                "unknown",
            ]
        );
    }

    #[test]
    fn labels_match_the_human_contract() {
        let labels = ErrorCode::ALL.map(|code| code.label());

        assert_eq!(
            labels,
            [
                "Auth required",
                "Not found",
                "Validation error",
                "API error",
                "Network error",
                "Forbidden",
                "Conflict",
                "Cancelled",
                "Error",
            ]
        );
    }

    #[test]
    fn auth_required_carries_the_not_authenticated_contract() {
        let error = AdoError::auth_required();

        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert_eq!(error.status, None);
        assert_eq!(
            error.message,
            "Not authenticated. Run 'ado login --method pat --org ORG --pat TOKEN' or set ADO_ORG+ADO_PAT."
        );
        assert!(
            error
                .details
                .as_ref()
                .expect("the hint")
                .to_string()
                .contains("ado login"),
            "message and hint disagree on the login command: {error:?}"
        );
        assert_eq!(
            error.details,
            Some(json!({
                "hint": "Set ADO_ORG and ADO_PAT env vars, or run `ado login --method pat --org ORG --pat TOKEN`",
                "scopes": "PAT must have: vso.work, vso.code, vso.project, vso.build, vso.release",
            }))
        );
    }

    #[test]
    fn the_error_type_displays_its_message() {
        let error = AdoError::validation("Unknown shell 'x'.");

        let as_std: &dyn std::error::Error = &error;
        assert_eq!(as_std.to_string(), "Unknown shell 'x'.");
    }

    #[test]
    fn validation_carries_the_message() {
        let error = AdoError::validation("Unknown shell 'x'.");

        assert_eq!(error.code, ErrorCode::ValidationError);
        assert_eq!(error.status, None);
        assert_eq!(error.message, "Unknown shell 'x'.");
        assert_eq!(error.details, None);
    }

    #[test]
    fn not_found_carries_the_message() {
        let error = AdoError::not_found("no command named \"x\".");

        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(error.status, None);
        assert_eq!(error.message, "no command named \"x\".");
        assert_eq!(error.details, None);
    }

    #[test]
    fn cancelled_carries_the_refusal() {
        let error = AdoError::cancelled("Aborted.");

        assert_eq!(error.code, ErrorCode::Cancelled);
        assert_eq!(error.status, None);
        assert_eq!(error.message, "Aborted.");
        assert_eq!(error.details, None);
    }

    #[test]
    fn from_status_uses_the_listed_message() {
        let error = AdoError::from_status(404, "{\"message\":\"gone\"}");

        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(error.status, Some(404));
        assert_eq!(
            error.message,
            "Resource not found. Check the project/repo/build ID and your permissions."
        );
        assert_eq!(
            error.details,
            Some(json!({ "status": 404, "body": "{\"message\":\"gone\"}" }))
        );
    }

    #[test]
    fn from_status_uses_the_server_error_message() {
        let error = AdoError::from_status(503, "unavailable");

        assert_eq!(error.code, ErrorCode::ApiError);
        assert_eq!(error.status, Some(503));
        assert_eq!(error.message, "Azure DevOps server error. Retry later.");
    }

    #[test]
    fn from_status_falls_back_to_the_status_and_body() {
        let error = AdoError::from_status(418, "teapot");

        assert_eq!(error.code, ErrorCode::ApiError);
        assert_eq!(error.status, Some(418));
        assert_eq!(error.message, "API error 418: teapot");
    }
}
