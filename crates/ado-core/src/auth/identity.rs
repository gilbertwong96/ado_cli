//! The plumbing the two interactive OAuth flows share: the identity requests'
//! timeouts, their form encoding, and the JSON helpers `AdoCli.Auth` applies to
//! both the device-code and the browser path (`safe_decode/1`, `extract_token/1`,
//! `list_accounts/1`).

use std::time::Duration;

use serde_json::Value;

use crate::error::{AdoError, ErrorCode};

/// The identity requests' timeouts, shaped like the ADO client's.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub fn endpoint(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path)
}

/// The same shape the ADO client uses: statuses are the caller's to read, a
/// redirect is not followed, and a request cannot hang forever.
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .build()
        .new_agent()
}

/// One form-encoded `POST`, answered with its status and body. `send_form` encodes
/// `URI.encode_query/1` style — a space as `+`, everything outside the unreserved
/// set percent-encoded — so the wire body is the oracle's, pair for pair.
pub fn post_form(
    agent: &ureq::Agent,
    url: &str,
    form: &[(&str, &str)],
) -> Result<(u16, String), AdoError> {
    let mut response = agent
        .post(url)
        .send_form(form.iter().copied())
        .map_err(|error| AdoError::from_transport(&error))?;

    read_response(&mut response)
}

/// One `GET` with a bearer token, answered with its status and body — the
/// accounts endpoint's shape (`list_accounts/1`'s `Finch.build(:get, url, headers)`).
pub fn get_bearer(agent: &ureq::Agent, url: &str, token: &str) -> Result<(u16, String), AdoError> {
    let mut response = agent
        .get(url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|error| AdoError::from_transport(&error))?;

    read_response(&mut response)
}

fn read_response(
    response: &mut ureq::http::Response<ureq::Body>,
) -> Result<(u16, String), AdoError> {
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| AdoError::from_transport(&error))?;

    Ok((status, body))
}

pub fn json_or_null(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or(Value::Null)
}

pub fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// `safe_decode/1`: the server's `error_description`, else its `message`, else
/// "Unknown error" for a JSON body, else the raw body.
pub fn reason(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return body.to_owned();
    };

    if !value.is_object() {
        return "Unknown error".to_owned();
    }

    string_field(&value, "error_description")
        .or_else(|| string_field(&value, "message"))
        .unwrap_or_else(|| "Unknown error".to_owned())
}

/// The flows' own failures: this CLI's `auth_required`, which the commands then
/// prefix with "Login failed: " the way the oracle's `Output.error` does. The one
/// error that keeps its own code is a transport failure, which
/// [`AdoError::from_transport`] classifies per §6.2.
pub fn auth_failed(message: impl Into<String>) -> AdoError {
    AdoError {
        code: ErrorCode::AuthRequired,
        status: None,
        message: message.into(),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `safe_decode/1` exactly: the description, else the message, else
    /// "Unknown error" for JSON, else the raw body.
    #[test]
    fn reason_mirrors_safe_decode() {
        assert_eq!(
            reason(&json!({"error_description": "why"}).to_string()),
            "why"
        );
        assert_eq!(reason(&json!({"message": "sorry"}).to_string()), "sorry");
        assert_eq!(
            reason(&json!({"error": "nope"}).to_string()),
            "Unknown error"
        );
        assert_eq!(reason(&json!([1, 2]).to_string()), "Unknown error");
        assert_eq!(reason("<html>502</html>"), "<html>502</html>");
    }

    #[test]
    fn endpoint_joins_a_trailing_slash_without_doubling_it() {
        assert_eq!(
            endpoint("http://127.0.0.1:9999/", "organizations/oauth2/token"),
            "http://127.0.0.1:9999/organizations/oauth2/token"
        );
    }
}
