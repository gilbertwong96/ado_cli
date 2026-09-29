//! The HTTP client for the Azure DevOps REST API: URL building, one pooled
//! `ureq::Agent`, and the spec §6.2 classification of every response.

use std::io::{self, Read};
use std::time::Duration;

use serde_json::{Value, json};
use ureq::http;

use crate::auth::auth_header;
use crate::credentials::Credentials;
use crate::env::{ENV_SERVER, EnvSource, ProcessEnv, non_empty};
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
            .filter(|server| non_empty(server))
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

    /// `GET` a download URL for its raw body. `url` is used **verbatim**: an
    /// absolute `resource.downloadUrl` carries its own query, so no `api-version`
    /// is merged in (D25). A relative path is resolved with [`Client::url_for`]
    /// first, which does merge the version.
    ///
    /// The status is classified before any body byte is handed over: a 2xx answers
    /// an open [`RawBody`], which streams without a size cap, and anything else
    /// classifies per spec §6.2 from a bounded read of the error body, decoded
    /// lossily so a non-text body still classifies by its status.
    pub fn get_raw(&self, url: &str) -> Result<RawBody, AdoError> {
        let request = http::Request::builder()
            .method("GET")
            .uri(url)
            .header(self.auth.0.as_str(), self.auth.1.as_str())
            .body(())
            .map_err(|error| build_failed(&error))?;

        let mut response = self
            .agent
            .run(request)
            .map_err(|error| AdoError::from_transport(&error))?;

        let status = response.status().as_u16();
        let has_location = response.headers().contains_key("location");

        if REDIRECT_STATUSES.contains(&status) {
            return Err(redirect_error(status, has_location));
        }

        if !is_success(status) {
            let body = response
                .body_mut()
                .read_to_vec()
                .map_err(|error| AdoError::from_transport(&error))?;

            return Err(AdoError::from_status(
                status,
                String::from_utf8_lossy(&body).into_owned(),
            ));
        }

        Ok(RawBody {
            reader: response.into_body().into_reader(),
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

    /// `POST` with a raw binary body and `application/octet-stream`, the frozen
    /// `Client.post_binary/3` the secure-file upload uses: the payload is the
    /// file's bytes, not a JSON encoding, and the response is decoded as JSON
    /// like every other write path.
    pub fn post_binary(
        &self,
        path: &str,
        body: &[u8],
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.dispatch(
            "POST",
            path,
            params,
            Some(("application/octet-stream", body.to_vec())),
        )?
        .json()
    }

    fn send(
        &self,
        method: &str,
        path: &str,
        params: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<Reply, AdoError> {
        match body {
            Some(value) => {
                let payload = serde_json::to_vec(value).map_err(|error| encode_failed(&error))?;

                self.dispatch(method, path, params, Some(("application/json", payload)))
            }
            None => self.dispatch(method, path, params, None),
        }
    }

    /// One request and its response, before the status is classified.
    fn dispatch(
        &self,
        method: &str,
        path: &str,
        params: &[(String, String)],
        body: Option<(&str, Vec<u8>)>,
    ) -> Result<Reply, AdoError> {
        let url = self.url_for(path, params);
        let builder = http::Request::builder()
            .method(method)
            .uri(&url)
            .header(self.auth.0.as_str(), self.auth.1.as_str());

        let mut response = match body {
            Some((content_type, payload)) => {
                let request = builder
                    .header("content-type", content_type)
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

/// An open 2xx response body for a download. [`RawBody::read_chunk`] streams it
/// without a size cap; the JSON paths keep their bounded `read_to_string`.
pub struct RawBody {
    reader: ureq::BodyReader<'static>,
}

impl RawBody {
    /// The next chunk of the body, `Ok(0)` at the end. A connection dropped after
    /// the headers is classified here — the §6.2 transport error with the reader's
    /// io error as the opaque reason — so a short body can never read as a clean
    /// end.
    pub fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize, AdoError> {
        self.reader
            .read(buffer)
            .map_err(|error| read_failed(&error))
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

/// A failure while a response body was being read — a connection dropped
/// mid-stream — is a transport failure, so it takes the §6.2 classification and
/// the same opaque `details.reason` shape as a request failure.
fn read_failed(error: &io::Error) -> AdoError {
    AdoError::from_transport(&ureq::Error::Io(io::Error::new(
        error.kind(),
        error.to_string(),
    )))
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

/// One path segment: every byte outside RFC 3986's unreserved set is
/// percent-encoded with uppercase hex, and a space is `%20` — not the query
/// encoder's `+`. Deliberately stricter than Elixir's `URI.encode/1`, which
/// leaves the reserved set (`/`, `?`, `+`, `'`, …) alone and so lets a name
/// change the URL's structure; a name that reaches this function cannot (D22).
pub fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
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
    fn a_blank_server_is_read_as_unset() {
        for blank in ["", "  ", "///"] {
            assert_eq!(
                client(Some(blank)).url_for("_apis/projects", &[]),
                "https://myorg.visualstudio.com/_apis/projects?api-version=7.1",
                "{blank:?} is not a server"
            );
        }
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

    /// The one path-segment encoder the area modules share: the unreserved set
    /// survives, a space is `%20` (not the query encoder's `+`), and the reserved
    /// set Elixir's `URI.encode/1` leaves alone is escaped so a name cannot change
    /// the URL's structure (D22).
    #[test]
    fn encode_path_segment_escapes_more_than_uri_encode() {
        assert_eq!(encode_path_segment("Alpha"), "Alpha");
        assert_eq!(encode_path_segment("My Project"), "My%20Project");
        assert_eq!(encode_path_segment("a/b+c"), "a%2Fb%2Bc");
        assert_eq!(
            encode_path_segment("6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c"),
            "6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c"
        );
        assert_eq!(
            encode_path_segment("a/b?c"),
            "a%2Fb%3Fc",
            "Elixir's URI.encode/1 keeps / and ? — the deliberate tightening (D22)"
        );
        assert_eq!(encode_path_segment("a'b"), "a%27b");
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
