//! The Microsoft identity platform device-code flow, in the shape
//! `lib/ado_cli/auth.ex` implements it: request a device code for the ARM resource,
//! poll the token endpoint at the interval the server names, then exchange the ARM
//! refresh token for a DevOps access token.
//!
//! The tenant, resource and endpoint constants are the Elixir's, verbatim. The
//! identity origin is a parameter rather than a constant, so a test can point the
//! flow at a local fake server — the frozen CLI hardcodes the endpoint, and this
//! module deliberately has no environment override of its own.

use std::time::Duration;

use serde_json::Value;

use crate::error::{AdoError, ErrorCode};

/// The Microsoft identity origin the flow talks to.
pub const IDENTITY_BASE: &str = "https://login.microsoftonline.com";

/// `@tenant` — the multi-tenant issuer that accepts work/school and, through the
/// ARM exchange below, personal accounts.
pub const TENANT: &str = "organizations";

/// The MSA fallback issuer the ARM→DevOps exchange tries second (`"consumers"`).
pub const FALLBACK_TENANT: &str = "consumers";

/// `@ado_resource` — the Azure DevOps resource the exchange asks for.
pub const DEVOPS_RESOURCE: &str = "499b84ac-1321-427f-aa17-267ca6975798";

/// `@arm_resource` — ARM accepts MSAs at the sign-in page, where the DevOps
/// resource does not, so the device code is requested for it first.
pub const ARM_RESOURCE: &str = "https://management.core.windows.net";

/// `@ado_client_id` — the Azure CLI public client, which accepts work/school and
/// personal Microsoft accounts. The Elixir also lets `ADO_OAUTH_CLIENT_ID` override
/// it; this build has no surface for that.
pub const CLIENT_ID: &str = "04b07795-8ddb-461a-bbee-02f9e1bf7b46";

/// The device-code grant type the poll posts.
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// The oracle's `slow_down` penalty: `interval + 5` seconds, which also becomes the
/// interval every later poll waits.
pub const SLOW_DOWN_INCREMENT: Duration = Duration::from_secs(5);

/// The oracle's `attempts > 120` guard, i.e. at most 121 polls before the flow
/// gives up — the run can never loop forever on a server that never grants.
pub const POLL_ATTEMPT_LIMIT: u32 = 120;

/// The identity requests' timeouts, shaped like the ADO client's.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The code the user enters on any device, with the interval the server asks us to
/// poll at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: Duration,
}

/// Requests a device code for the ARM resource
/// (`request_device_code/1`).
pub fn request(identity_base: &str) -> Result<DeviceCode, AdoError> {
    let agent = agent();
    let (status, body) = post_form(
        &agent,
        &endpoint(identity_base, &format!("{TENANT}/oauth2/devicecode")),
        &[("client_id", CLIENT_ID), ("resource", ARM_RESOURCE)],
    )?;

    if status != 200 {
        return Err(auth_failed(format!(
            "Device code request failed (HTTP {status}): {}",
            reason(&body)
        )));
    }

    let value = json_or_null(&body);
    let device = string_field(&value, "device_code");
    let user_code = string_field(&value, "user_code");
    // Microsoft answers with both spellings; the RFC 8628 one is the fallback.
    let verification_uri = string_field(&value, "verification_url")
        .or_else(|| string_field(&value, "verification_uri"));
    let interval = value.get("interval").and_then(Value::as_u64);

    match (device, user_code, verification_uri, interval) {
        (Some(device_code), Some(user_code), Some(verification_uri), Some(interval)) => {
            Ok(DeviceCode {
                device_code,
                user_code,
                verification_uri,
                interval: Duration::from_secs(interval),
            })
        }
        // The oracle's `with` clause would raise on this response; a clear error is
        // the same outcome without the crash.
        _ => Err(auth_failed(
            "Device code response is missing a required field.",
        )),
    }
}

/// Polls the token endpoint until the server grants a token, and answers the
/// **refresh** token: that is what the DevOps exchange needs, while the ARM access
/// token that arrives with it is the oracle's discarded `_arm_token`.
///
/// `authorization_pending` waits the server's interval, `slow_down` waits
/// `interval + SLOW_DOWN_INCREMENT` and keeps that as the new interval, and every
/// other error is terminal. A transport failure is a `network_error` (§6.2) rather
/// than the oracle's flat "Token polling failed".
pub fn poll(identity_base: &str, device: &DeviceCode) -> Result<Option<String>, AdoError> {
    let agent = agent();
    let url = endpoint(identity_base, &format!("{TENANT}/oauth2/token"));
    let mut interval = device.interval;
    let mut attempts = 0;

    loop {
        if attempts > POLL_ATTEMPT_LIMIT {
            return Err(auth_failed("Authentication timed out. Please try again."));
        }
        attempts += 1;

        let (status, body) = post_form(
            &agent,
            &url,
            &[
                ("grant_type", DEVICE_CODE_GRANT),
                ("client_id", CLIENT_ID),
                ("device_code", &device.device_code),
            ],
        )?;

        match outcome(status, &body) {
            Outcome::Granted(refresh) => return Ok(refresh),
            Outcome::Pending => std::thread::sleep(interval),
            Outcome::SlowDown => {
                interval += SLOW_DOWN_INCREMENT;
                std::thread::sleep(interval);
            }
            Outcome::Failed(error) => return Err(error),
        }
    }
}

/// Exchanges the ARM refresh token for a DevOps access token, trying the primary
/// issuer then the MSA fallback (`exchange_refresh_for_devops/2` → `try_tenants/2`).
/// Both failing is the oracle's one message for that case.
pub fn exchange(identity_base: &str, refresh_token: Option<&str>) -> Result<String, AdoError> {
    let Some(refresh_token) = refresh_token else {
        return Err(auth_failed(
            "No refresh token available for DevOps exchange",
        ));
    };

    let agent = agent();

    for tenant in [TENANT, FALLBACK_TENANT] {
        if let Ok(token) = exchange_at(&agent, identity_base, tenant, refresh_token) {
            return Ok(token);
        }
    }

    Err(auth_failed("DevOps token exchange failed with all tenants"))
}

/// One tenant's exchange: the v1.0 endpoint with the DevOps `resource`, which is
/// what the Azure CLI uses internally (`do_exchange_refresh_token/2`).
fn exchange_at(
    agent: &ureq::Agent,
    identity_base: &str,
    tenant: &str,
    refresh_token: &str,
) -> Result<String, AdoError> {
    let (status, body) = post_form(
        agent,
        &endpoint(identity_base, &format!("{tenant}/oauth2/token")),
        &[
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("resource", DEVOPS_RESOURCE),
        ],
    )?;

    if status != 200 {
        return Err(auth_failed(format!(
            "DevOps token exchange failed (HTTP {status}): {}",
            reason(&body)
        )));
    }

    match string_field(&json_or_null(&body), "access_token") {
        Some(token) => Ok(token),
        None => Err(auth_failed("Invalid DevOps token response")),
    }
}

/// What one poll response asks the flow to do (`extract_token/1` and
/// `handle_token_error/4`).
#[derive(Debug, PartialEq)]
enum Outcome {
    Granted(Option<String>),
    Pending,
    SlowDown,
    Failed(AdoError),
}

fn outcome(status: u16, body: &str) -> Outcome {
    let value = json_or_null(body);

    match status {
        200 => match string_field(&value, "access_token") {
            Some(_) => Outcome::Granted(string_field(&value, "refresh_token")),
            None => Outcome::Failed(auth_failed("Invalid token response")),
        },
        400 => match string_field(&value, "error").as_deref() {
            Some("authorization_pending") => Outcome::Pending,
            Some("slow_down") => Outcome::SlowDown,
            // The oracle handles `authorization_declined`; `access_denied` is RFC
            // 8628's spelling of the same terminal outcome.
            Some("authorization_declined" | "access_denied") => {
                Outcome::Failed(auth_failed("Authorization declined by user."))
            }
            Some("expired_token") => {
                Outcome::Failed(auth_failed("Device code expired. Please try again."))
            }
            error => Outcome::Failed(unknown_token_error(error, &value)),
        },
        // The oracle's fallback for a status it does not handle; ours names it, and
        // no transport failure reaches here (it classifies as a network error).
        _ => Outcome::Failed(auth_failed(format!("Token polling failed (HTTP {status})"))),
    }
}

/// The oracle's fallback is the flat "Unknown token error"; keeping the server's own
/// spelling makes a new terminal error diagnosable.
fn unknown_token_error(error: Option<&str>, value: &Value) -> AdoError {
    let error = error.unwrap_or("unknown");

    match string_field(value, "error_description") {
        Some(description) => auth_failed(format!("Token error '{error}': {description}")),
        None => auth_failed(format!("Token error '{error}'.")),
    }
}

/// The flow's own failures: this CLI's `auth_required`, which the command then
/// prefixes with "Login failed: " the way the oracle's `Output.error` does. The one
/// error that keeps its own code is a transport failure, which
/// [`AdoError::from_transport`] classifies per §6.2.
fn auth_failed(message: impl Into<String>) -> AdoError {
    AdoError {
        code: ErrorCode::AuthRequired,
        status: None,
        message: message.into(),
        details: None,
    }
}

fn endpoint(identity_base: &str, path: &str) -> String {
    format!("{}/{}", identity_base.trim_end_matches('/'), path)
}

/// The same shape the ADO client uses: statuses are the caller's to read, a
/// redirect is not followed, and a request cannot hang forever.
fn agent() -> ureq::Agent {
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
fn post_form(
    agent: &ureq::Agent,
    url: &str,
    form: &[(&str, &str)],
) -> Result<(u16, String), AdoError> {
    let mut response = agent
        .post(url)
        .send_form(form.iter().copied())
        .map_err(|error| AdoError::from_transport(&error))?;

    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| AdoError::from_transport(&error))?;

    Ok((status, body))
}

fn json_or_null(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or(Value::Null)
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// `safe_decode/1`: the server's `error_description`, else its `message`, else
/// "Unknown error" for a JSON body, else the raw body.
fn reason(body: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The flow's constants are the frozen module's, verbatim
    /// (`lib/ado_cli/auth.ex`): tenant, fallback tenant, resources, client id and
    /// the two endpoint paths.
    #[test]
    fn the_constants_are_the_elixir_modules() {
        assert_eq!(IDENTITY_BASE, "https://login.microsoftonline.com");
        assert_eq!(TENANT, "organizations");
        assert_eq!(FALLBACK_TENANT, "consumers");
        assert_eq!(DEVOPS_RESOURCE, "499b84ac-1321-427f-aa17-267ca6975798");
        assert_eq!(ARM_RESOURCE, "https://management.core.windows.net");
        assert_eq!(CLIENT_ID, "04b07795-8ddb-461a-bbee-02f9e1bf7b46");
        assert_eq!(SLOW_DOWN_INCREMENT, Duration::from_secs(5));
        assert_eq!(POLL_ATTEMPT_LIMIT, 120);

        let base = IDENTITY_BASE;
        assert_eq!(
            endpoint(base, &format!("{TENANT}/oauth2/devicecode")),
            "https://login.microsoftonline.com/organizations/oauth2/devicecode"
        );
        assert_eq!(
            endpoint(base, &format!("{TENANT}/oauth2/token")),
            "https://login.microsoftonline.com/organizations/oauth2/token"
        );
        assert_eq!(
            endpoint(
                base,
                &format!("{tenant}/oauth2/token", tenant = FALLBACK_TENANT)
            ),
            "https://login.microsoftonline.com/consumers/oauth2/token"
        );
        assert_eq!(
            endpoint("http://127.0.0.1:9999/", "organizations/oauth2/token"),
            "http://127.0.0.1:9999/organizations/oauth2/token",
            "a base with a trailing slash is still one path segment"
        );
    }

    #[test]
    fn a_granted_poll_carries_the_refresh_token() {
        assert_eq!(
            outcome(
                200,
                &json!({"access_token": "arm", "refresh_token": "refresh"}).to_string()
            ),
            Outcome::Granted(Some("refresh".to_owned())),
            "the refresh token is what the DevOps exchange uses"
        );
        assert_eq!(
            outcome(200, &json!({"access_token": "arm"}).to_string()),
            Outcome::Granted(None),
            "an access token without a refresh token is still a grant (the oracle's `{{:ok, access, nil}}`)"
        );
        assert_eq!(
            outcome(200, &json!({"token_type": "Bearer"}).to_string()),
            Outcome::Failed(auth_failed("Invalid token response"))
        );
        assert_eq!(
            outcome(200, "not json"),
            Outcome::Failed(auth_failed("Invalid token response"))
        );
    }

    #[test]
    fn pending_and_slow_down_are_not_errors() {
        assert_eq!(
            outcome(400, &json!({"error": "authorization_pending"}).to_string()),
            Outcome::Pending
        );
        assert_eq!(
            outcome(400, &json!({"error": "slow_down"}).to_string()),
            Outcome::SlowDown
        );
    }

    /// The terminal errors carry the oracle's messages, and a stable code: the
    /// command turns every one of them into `auth_required`.
    #[test]
    fn the_terminal_errors_are_the_oracles_messages() {
        let expired = outcome(400, &json!({"error": "expired_token"}).to_string());
        assert_eq!(
            expired,
            Outcome::Failed(auth_failed("Device code expired. Please try again."))
        );

        for declined in ["authorization_declined", "access_denied"] {
            let outcome = outcome(400, &json!({"error": declined}).to_string());
            let Outcome::Failed(error) = outcome else {
                panic!("{declined} is terminal");
            };
            assert_eq!(error.code, ErrorCode::AuthRequired);
            assert_eq!(error.message, "Authorization declined by user.");
        }
    }

    #[test]
    fn an_unknown_token_error_keeps_the_servers_spelling() {
        let described = outcome(
            400,
            &json!({"error": "invalid_grant", "error_description": "AADSTS70000: nope"})
                .to_string(),
        );
        let Outcome::Failed(error) = described else {
            panic!("invalid_grant is terminal");
        };
        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert_eq!(
            error.message,
            "Token error 'invalid_grant': AADSTS70000: nope"
        );

        let bare = outcome(400, &json!({"error": "invalid_grant"}).to_string());
        assert_eq!(
            bare,
            Outcome::Failed(auth_failed("Token error 'invalid_grant'."))
        );
    }

    #[test]
    fn any_other_status_is_terminal_and_names_it() {
        let Outcome::Failed(server) = outcome(503, "unavailable") else {
            panic!("503 is terminal");
        };
        assert_eq!(server.code, ErrorCode::AuthRequired);
        assert_eq!(server.message, "Token polling failed (HTTP 503)");
    }

    #[test]
    fn a_grant_without_a_refresh_token_is_a_stable_error() {
        let error = exchange(IDENTITY_BASE, None).expect_err("no refresh token");

        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert_eq!(
            error.message,
            "No refresh token available for DevOps exchange"
        );
    }

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
}
