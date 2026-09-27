//! The HTTP client for the Azure DevOps REST API: URL building, one pooled
//! `ureq::Agent`, and the spec §6.2 classification of every response.

use std::time::Duration;

use serde_json::{Value, json};
use ureq::http;

use crate::auth::auth_header;
use crate::credentials::Credentials;
use crate::env::{ENV_SERVER, EnvSource, ProcessEnv};
use crate::error::{AdoError, ErrorCode};

/// The REST API version merged into every URL; a caller's own `api-version` wins.
pub const API_VERSION: &str = "7.1";

const REDIRECT_STATUSES: [u16; 4] = [301, 302, 307, 308];
const REDIRECT_TO_SIGN_IN: &str =
    "API redirected to sign-in page. Run 'ado login' to authenticate.";
const REDIRECT_WITHOUT_LOCATION: &str =
    "API redirected without a Location header. Run 'ado login' to authenticate.";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// An authenticated client for one organization.
pub struct Client {
    agent: ureq::Agent,
    base: Base,
    org: String,
    auth: (String, String),
}

/// Where requests go.
enum Base {
    /// No `ADO_SERVER`: `https://{org}.visualstudio.com`, the host today's CLI
    /// rewrites `https://dev.azure.com` to.
    Cloud,
    /// A self-hosted server, with its trailing slashes trimmed.
    Server(String),
}

impl Client {
    /// The production constructor: `ADO_SERVER` comes from the process environment.
    /// The `Result` is the constructor convention `ado-core` shares; building the
    /// agent cannot fail.
    pub fn new(credentials: &Credentials) -> Result<Client, AdoError> {
        Client::from_env(credentials, &ProcessEnv)
    }

    /// The same client with the environment supplied, so tests drive `ADO_SERVER`
    /// through `MapEnv` instead of mutating the process environment.
    pub fn from_env(credentials: &Credentials, env: &dyn EnvSource) -> Result<Client, AdoError> {
        let base = match env
            .get(ENV_SERVER)
            .map(|server| server.trim_end_matches('/').to_owned())
            .filter(|server| !server.is_empty())
        {
            Some(server) => Base::Server(server),
            None => Base::Cloud,
        };

        // Statuses are the API's to classify (§6.2), not the transport's: a 4xx/5xx
        // response must reach `from_status`, and a 3xx must reach the redirect
        // messages, so neither is turned into an error and redirects are not followed.
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(REQUEST_TIMEOUT))
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .build()
            .new_agent();

        Ok(Client {
            agent,
            base,
            org: credentials.org.clone(),
            auth: auth_header(credentials),
        })
    }

    /// The absolute URL for `path` and `params`, with `api-version={API_VERSION}`
    /// merged first so a caller's own `api-version` wins.
    pub fn url_for(&self, path: &str, params: &[(String, String)]) -> String {
        let path = path.trim_start_matches('/');
        let query = encode_query(&with_api_version(params));

        match &self.base {
            Base::Cloud => format!("https://{}.visualstudio.com/{path}?{query}", self.org),
            Base::Server(server) => with_org(server, &self.org, path, &query),
        }
    }

    /// `GET` with the JSON body decoded.
    pub fn get(&self, path: &str, params: &[(String, String)]) -> Result<Value, AdoError> {
        self.send("GET", path, params, None)?.json()
    }

    /// `GET` for the list endpoints: `{"value": [...]}` unwraps to the array, and a
    /// bare array passes through untouched.
    pub fn list(&self, path: &str, params: &[(String, String)]) -> Result<Value, AdoError> {
        Ok(match self.get(path, params)? {
            Value::Object(mut object) => match object.remove("value") {
                Some(value) => value,
                None => Value::Object(object),
            },
            other => other,
        })
    }

    /// `POST` with a JSON body, returning the decoded response body.
    pub fn post(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.send("POST", path, params, Some(body))?.json()
    }

    /// `PATCH` with a JSON body, returning the decoded response body.
    pub fn patch(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.send("PATCH", path, params, Some(body))?.json()
    }

    /// `PUT` with a JSON body, returning the decoded response body.
    pub fn put(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.send("PUT", path, params, Some(body))?.json()
    }

    /// `DELETE`: Azure answers 204 with no body, so a 2xx is the whole result.
    pub fn delete(&self, path: &str, params: &[(String, String)]) -> Result<(), AdoError> {
        self.send("DELETE", path, params, None)?.accepted()
    }

    fn send(
        &self,
        method: &str,
        path: &str,
        params: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<Reply, AdoError> {
        let url = self.url_for(path, params);
        let builder = http::Request::builder()
            .method(method)
            .uri(&url)
            .header(self.auth.0.as_str(), self.auth.1.as_str());

        let mut response = match body {
            Some(value) => {
                let payload = serde_json::to_vec(value).map_err(|error| encode_failed(&error))?;
                let request = builder
                    .header("content-type", "application/json")
                    .body(payload)
                    .map_err(|error| build_failed(&error))?;

                self.agent.run(request)
            }
            None => {
                let request = builder.body(()).map_err(|error| build_failed(&error))?;

                self.agent.run(request)
            }
        }
        .map_err(|error| AdoError::from_transport(&error))?;

        let status = response.status().as_u16();
        let has_location = response.headers().contains_key("location");
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| AdoError::from_transport(&error))?;

        if REDIRECT_STATUSES.contains(&status) {
            return Err(redirect_error(status, has_location));
        }

        Ok(Reply { status, body })
    }
}

/// One response whose status the client has not classified yet.
struct Reply {
    status: u16,
    body: String,
}

impl Reply {
    /// A 2xx decodes the JSON body; anything else is classified per spec §6.2.
    fn json(self) -> Result<Value, AdoError> {
        if is_success(self.status) {
            serde_json::from_str(&self.body).map_err(|error| decode_failed(&error))
        } else {
            Err(AdoError::from_status(self.status, self.body))
        }
    }

    /// A 2xx is the whole result; anything else is classified per spec §6.2.
    fn accepted(self) -> Result<(), AdoError> {
        if is_success(self.status) {
            Ok(())
        } else {
            Err(AdoError::from_status(self.status, self.body))
        }
    }
}

fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

/// The `api-version` default followed by the caller's params, where a caller's own
/// `api-version` replaces the default in place.
fn with_api_version(params: &[(String, String)]) -> Vec<(String, String)> {
    let mut query = vec![("api-version".to_owned(), API_VERSION.to_owned())];

    for (key, value) in params {
        match query.iter_mut().find(|(existing, _)| existing == key) {
            Some(slot) => slot.1.clone_from(value),
            None => query.push((key.clone(), value.clone())),
        }
    }

    query
}

fn encode_query(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// `URI.encode_query/1`-compatible: unreserved characters survive, a space becomes
/// `+`, and everything else is percent-encoded with uppercase hex.
fn encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }

    encoded
}

/// A self-hosted base URL with `/{org}` inserted after the host, so a collection
/// URL such as `https://server.test/tfs` keeps its path.
fn with_org(server: &str, org: &str, path: &str, query: &str) -> String {
    match server.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('/') {
            Some((host, tail)) => format!("{scheme}://{host}/{org}/{tail}/{path}?{query}"),
            None => format!("{scheme}://{rest}/{org}/{path}?{query}"),
        },
        None => format!("{server}/{org}/{path}?{query}"),
    }
}

/// Today's CLI turns a 3xx into `%{status: 302, body: <message>}`, and the message
/// depends on whether the redirect carried a `Location` header.
fn redirect_error(status: u16, has_location: bool) -> AdoError {
    let message = if has_location {
        REDIRECT_TO_SIGN_IN
    } else {
        REDIRECT_WITHOUT_LOCATION
    };

    AdoError {
        code: ErrorCode::AuthRequired,
        status: Some(status),
        message: message.to_owned(),
        details: Some(json!({ "status": status, "body": message })),
    }
}

/// The current classification calls an undecodable 2xx body a network error; the
/// client keeps that, and carries the decode reason the way the CLI does.
fn decode_failed(error: &serde_json::Error) -> AdoError {
    AdoError {
        code: ErrorCode::NetworkError,
        status: None,
        message: format!("Request failed: {error}"),
        details: Some(json!({ "reason": format!("{error:?}") })),
    }
}

fn encode_failed(error: &serde_json::Error) -> AdoError {
    AdoError::validation(format!("Cannot encode the request body: {error}"))
}

fn build_failed(error: &http::Error) -> AdoError {
    AdoError::validation(format!("Cannot build the request: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthMethod;

    fn client(server: Option<&str>) -> Client {
        let credentials = Credentials {
            org: "myorg".to_owned(),
            method: AuthMethod::Pat,
            token: "pat".to_owned(),
        };
        let mut env = crate::env::MapEnv::new();
        if let Some(server) = server {
            env = env.set(ENV_SERVER, server);
        }

        Client::from_env(&credentials, &env).expect("a client")
    }

    #[test]
    fn self_hosted_collection_paths_keep_their_tail() {
        assert_eq!(
            client(Some("https://server.test/tfs/")).url_for("_apis/projects", &[]),
            "https://server.test/myorg/tfs/_apis/projects?api-version=7.1"
        );
    }

    #[test]
    fn query_encoding_matches_the_elixir_cli() {
        assert_eq!(
            encode_query(&[("path".to_owned(), "/a b/c*d~e".to_owned())]),
            "path=%2Fa+b%2Fc%2Ad~e"
        );
        assert_eq!(
            encode_query(&[("$top".to_owned(), "10".to_owned())]),
            "%24top=10"
        );
    }

    #[test]
    fn redirect_messages_are_the_contract_strings() {
        let with_location = redirect_error(307, true);
        assert_eq!(with_location.code, ErrorCode::AuthRequired);
        assert_eq!(with_location.status, Some(307));
        assert_eq!(with_location.message, REDIRECT_TO_SIGN_IN);

        let without_location = redirect_error(302, false);
        assert_eq!(without_location.code, ErrorCode::AuthRequired);
        assert_eq!(without_location.message, REDIRECT_WITHOUT_LOCATION);
    }
}
