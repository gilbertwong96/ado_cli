//! The HTTP client for the Azure DevOps REST API: URL building, one pooled
//! `ureq::Agent`, and the spec §6.2 classification of every response. The raw
//! download path attaches the credential only to the client's own origin
//! (`same_origin`, Ruling A3).

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

/// The content type the work-item write API requires (captured): a JSON-patch
/// body sent as `application/json` is rejected by Azure DevOps.
const JSON_PATCH_CONTENT_TYPE: &str = "application/json-patch+json";

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
    origins: Vec<Origin>,
}

/// An origin the client's credential belongs to (Ruling A3): scheme, host
/// (lowercased) and the explicit port. A URL outside every origin is fetched
/// without the `Authorization` header.
struct Origin {
    scheme: String,
    host: String,
    port: Option<u16>,
}

/// Where requests go.
enum Base {
    /// No `ADO_SERVER`: `https://{org}.visualstudio.com`, the host today's CLI
    /// rewrites `https://dev.azure.com` to.
    Cloud,
    /// A self-hosted server, with its trailing slashes trimmed.
    Server(String),
}

/// An Azure DevOps hub: a REST surface that lives on its own host rather than on
/// the organization's.
///
/// Azure serves the classic Release API from `vsrm`, entitlements from `vsaex`
/// and extension management from `extmgmt`, at `{org}.vsrm.visualstudio.com`,
/// `{org}.vsaex.visualstudio.com` and `{org}.extmgmt.visualstudio.com`. The
/// frozen CLI addressed all three on the organization's own host and every one
/// of them answers 404 against a live organization (`w4-live-org-findings.md`
/// F2-F4), so a cloud request for one of those surfaces goes to the hub's host
/// instead — the deliberate divergence D57. Azure DevOps Server has no hub
/// hosts: a self-hosted base keeps every surface on its own server.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hub {
    /// Classic releases.
    Releases,
    /// User entitlements.
    Entitlements,
    /// Extension management.
    Extensions,
}

impl Hub {
    /// The subdomain label: the `vsrm` of `{org}.vsrm.visualstudio.com`.
    fn label(self) -> &'static str {
        match self {
            Hub::Releases => "vsrm",
            Hub::Entitlements => "vsaex",
            Hub::Extensions => "extmgmt",
        }
    }
}

/// The cloud origin for `org`, on the organization's own host or on one of its
/// hubs.
fn cloud_origin(org: &str, hub: Option<Hub>) -> Origin {
    let host = match hub {
        Some(hub) => format!("{org}.{}.visualstudio.com", hub.label()),
        None => format!("{org}.visualstudio.com"),
    };

    Origin {
        scheme: "https".to_owned(),
        host,
        port: None,
    }
}

/// The hub surfaces of one client: [`Client::hub`]'s view, whose verbs address
/// the hub's host instead of the organization's.
pub struct HubClient<'a> {
    client: &'a Client,
    hub: Hub,
}

/// The list unwrap both [`Client::list`] and [`HubClient::list`] use:
/// `{"value": [...]}` becomes the array, and a bare array passes through.
fn unwrap_value(value: Value) -> Value {
    match value {
        Value::Object(mut object) => match object.remove("value") {
            Some(value) => value,
            None => Value::Object(object),
        },
        other => other,
    }
}

impl HubClient<'_> {
    /// `GET` on the hub with the JSON body decoded.
    pub fn get(&self, path: &str, params: &[(String, String)]) -> Result<Value, AdoError> {
        self.client
            .send(Some(self.hub), "GET", path, params, None)?
            .json()
    }

    /// `GET` for a list endpoint on the hub, with the same unwrap
    /// [`Client::list`] applies.
    pub fn list(&self, path: &str, params: &[(String, String)]) -> Result<Value, AdoError> {
        Ok(unwrap_value(self.get(path, params)?))
    }

    /// `POST` with a JSON body on the hub.
    pub fn post(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.client
            .send(Some(self.hub), "POST", path, params, Some(body))?
            .json()
    }

    /// `PATCH` with a JSON body on the hub.
    pub fn patch(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.client
            .send(Some(self.hub), "PATCH", path, params, Some(body))?
            .json()
    }

    /// `DELETE` on the hub.
    pub fn delete(&self, path: &str, params: &[(String, String)]) -> Result<(), AdoError> {
        self.client
            .send(Some(self.hub), "DELETE", path, params, None)?
            .accepted()
    }
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

        // Ruling A3: the origins the credential may go to. The cloud base is the
        // organization's host plus its three hubs (D57); a self-hosted base is
        // parsed for its scheme/host/port, and a base that does not parse matches
        // no URL at all (the request would fail at build time anyway, so the
        // credential is never the risk).
        let origins: Vec<Origin> = match &base {
            Base::Cloud => {
                let mut origins = vec![cloud_origin(&credentials.org, None)];

                origins.extend(
                    [Hub::Releases, Hub::Entitlements, Hub::Extensions]
                        .map(|hub| cloud_origin(&credentials.org, Some(hub))),
                );

                origins
            }
            Base::Server(server) => server
                .parse::<http::Uri>()
                .ok()
                .and_then(|uri| {
                    Some(Origin {
                        scheme: uri.scheme_str()?.to_owned(),
                        host: uri.host()?.to_ascii_lowercase(),
                        port: uri.port_u16(),
                    })
                })
                .into_iter()
                .collect(),
        };

        Ok(Client {
            agent,
            base,
            org: credentials.org.clone(),
            auth: auth_header(credentials),
            origins,
        })
    }

    /// The absolute URL for `path` and `params`, with `api-version={API_VERSION}`
    /// merged first so a caller's own `api-version` wins.
    pub fn url_for(&self, path: &str, params: &[(String, String)]) -> String {
        self.url_for_on(None, path, params)
    }

    /// The absolute URL for `path` on `hub`: the same `api-version` merge
    /// [`Client::url_for`] does, addressed to the hub's own host on a cloud base
    /// and to the caller's server on a self-hosted one.
    pub fn url_for_hub(&self, hub: Hub, path: &str, params: &[(String, String)]) -> String {
        self.url_for_on(Some(hub), path, params)
    }

    /// This client's `hub` surface: the same credential and the same agent, with
    /// the verbs addressed to the hub's host.
    pub fn hub(&self, hub: Hub) -> HubClient<'_> {
        HubClient { client: self, hub }
    }

    fn url_for_on(&self, hub: Option<Hub>, path: &str, params: &[(String, String)]) -> String {
        let path = path.trim_start_matches('/');
        let query = encode_query(&with_api_version(params));

        self.absolute_url(hub, path, &query)
    }

    /// Whether `url`'s origin — scheme, host and explicit port — is one of the
    /// client's own (Ruling A3, D57). A relative or unparseable URL, and a client
    /// whose base did not parse, match nothing: the credential goes out only on a
    /// positive match. `http://host` and `http://host:80` are different origins
    /// here, which is the conservative direction for a credential.
    fn same_origin(&self, url: &str) -> bool {
        let Ok(uri) = url.parse::<http::Uri>() else {
            return false;
        };
        let (Some(scheme), Some(host)) = (uri.scheme_str(), uri.host()) else {
            return false;
        };

        self.origins.iter().any(|origin| {
            scheme.eq_ignore_ascii_case(&origin.scheme)
                && host.eq_ignore_ascii_case(&origin.host)
                && uri.port_u16() == origin.port
        })
    }

    /// The absolute URL for `path` with no query string at all: the connectionData
    /// lookup is the one captured endpoint that rejects `api-version`
    /// (`Client.get_raw_no_version/1`, `lib/ado_cli/client.ex:102-116`), so it must
    /// not reuse [`Client::url_for`]'s merge.
    fn url_for_unversioned(&self, path: &str) -> String {
        self.absolute_url(None, path.trim_start_matches('/'), "")
    }

    fn absolute_url(&self, hub: Option<Hub>, path: &str, query: &str) -> String {
        match &self.base {
            Base::Cloud => {
                let suffix = if query.is_empty() {
                    String::new()
                } else {
                    format!("?{query}")
                };

                format!(
                    "https://{}/{path}{suffix}",
                    cloud_origin(&self.org, hub).host
                )
            }
            Base::Server(server) => with_org(server, &self.org, path, query),
        }
    }

    /// `GET` with the JSON body decoded.
    pub fn get(&self, path: &str, params: &[(String, String)]) -> Result<Value, AdoError> {
        self.send(None, "GET", path, params, None)?.json()
    }

    /// `GET` a path with no `api-version` query parameter (and no other params).
    /// Only `/_apis/connectionData` needs it: the endpoint is the frozen CLI's
    /// `get_raw_no_version` target, and the captured request carries no query at
    /// all.
    pub fn get_without_version(&self, path: &str) -> Result<Value, AdoError> {
        let url = self.url_for_unversioned(path);
        let request = http::Request::builder()
            .method("GET")
            .uri(&url)
            .header(self.auth.0.as_str(), self.auth.1.as_str())
            .body(())
            .map_err(|error| build_failed(&error))?;
        let mut response = self
            .agent
            .run(request)
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

        Reply { status, body }.json()
    }

    /// `GET` for the list endpoints: `{"value": [...]}` unwraps to the array, and a
    /// bare array passes through untouched.
    pub fn list(&self, path: &str, params: &[(String, String)]) -> Result<Value, AdoError> {
        Ok(unwrap_value(self.get(path, params)?))
    }

    /// `GET` a download URL for its raw body. `url` is used **verbatim**: an
    /// absolute `resource.downloadUrl` carries its own query, so no `api-version`
    /// is merged in (D25). A relative path is resolved with [`Client::url_for`]
    /// first, which does merge the version.
    ///
    /// The credential is attached only when `url`'s origin is the client's own
    /// (Ruling A3): a server-supplied absolute URL on another host is fetched
    /// anonymously, so a credential cannot follow a URL the server chooses. A
    /// 401/403 on that path still classifies loudly, per spec §6.2.
    ///
    /// The status is classified before any body byte is handed over: a 2xx answers
    /// an open [`RawBody`], which streams without a size cap, and anything else
    /// classifies per spec §6.2 from a bounded read of the error body, decoded
    /// lossily so a non-text body still classifies by its status.
    pub fn get_raw(&self, url: &str) -> Result<RawBody, AdoError> {
        let mut builder = http::Request::builder().method("GET").uri(url);

        if self.same_origin(url) {
            builder = builder.header(self.auth.0.as_str(), self.auth.1.as_str());
        }

        let request = builder.body(()).map_err(|error| build_failed(&error))?;

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
        self.send(None, "POST", path, params, Some(body))?.json()
    }

    /// `PATCH` with a JSON body, returning the decoded response body.
    pub fn patch(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.send(None, "PATCH", path, params, Some(body))?.json()
    }

    /// `PUT` with a JSON body, returning the decoded response body.
    pub fn put(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.send(None, "PUT", path, params, Some(body))?.json()
    }

    /// `PUT` with one caller-supplied header — the frozen `Client.put/4`'s
    /// `extra_headers`, which only `wikis pages update` uses: the `If-Match`
    /// optimistic-concurrency guard built from the read's `eTag`.
    pub fn put_with_headers(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
        extra_headers: &[(&str, &str)],
    ) -> Result<Value, AdoError> {
        let payload = serde_json::to_vec(body).map_err(|error| encode_failed(&error))?;

        self.dispatch_with_headers(
            None,
            "PUT",
            path,
            params,
            Some(("application/json", payload)),
            extra_headers,
        )?
        .json()
    }

    /// `DELETE`: Azure answers 204 with no body, so a 2xx is the whole result.
    pub fn delete(&self, path: &str, params: &[(String, String)]) -> Result<(), AdoError> {
        self.send(None, "DELETE", path, params, None)?.accepted()
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
            None,
            "POST",
            path,
            params,
            Some(("application/octet-stream", body.to_vec())),
        )?
        .json()
    }

    /// `POST` with a JSON body and `application/json-patch+json`, the content
    /// type the frozen work-item create sends (captured): the work item API
    /// rejects a patch under `application/json`.
    pub fn post_json_patch(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.json_patch("POST", path, body, params)?.json()
    }

    /// `PATCH` with a JSON body and `application/json-patch+json`, the frozen
    /// `Client.patch/4` the work-item update sends (captured).
    pub fn patch_json_patch(
        &self,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Value, AdoError> {
        self.json_patch("PATCH", path, body, params)?.json()
    }

    fn json_patch(
        &self,
        method: &str,
        path: &str,
        body: &Value,
        params: &[(String, String)],
    ) -> Result<Reply, AdoError> {
        let payload = serde_json::to_vec(body).map_err(|error| encode_failed(&error))?;

        self.dispatch(
            None,
            method,
            path,
            params,
            Some((JSON_PATCH_CONTENT_TYPE, payload)),
        )
    }

    fn send(
        &self,
        hub: Option<Hub>,
        method: &str,
        path: &str,
        params: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<Reply, AdoError> {
        match body {
            Some(value) => {
                let payload = serde_json::to_vec(value).map_err(|error| encode_failed(&error))?;

                self.dispatch(
                    hub,
                    method,
                    path,
                    params,
                    Some(("application/json", payload)),
                )
            }
            None => self.dispatch(hub, method, path, params, None),
        }
    }

    /// One request and its response, before the status is classified.
    fn dispatch(
        &self,
        hub: Option<Hub>,
        method: &str,
        path: &str,
        params: &[(String, String)],
        body: Option<(&str, Vec<u8>)>,
    ) -> Result<Reply, AdoError> {
        self.dispatch_with_headers(hub, method, path, params, body, &[])
    }

    /// The same request with caller-supplied headers appended after the auth and
    /// content-type ones; only [`Client::put_with_headers`] passes any.
    fn dispatch_with_headers(
        &self,
        hub: Option<Hub>,
        method: &str,
        path: &str,
        params: &[(String, String)],
        body: Option<(&str, Vec<u8>)>,
        extra_headers: &[(&str, &str)],
    ) -> Result<Reply, AdoError> {
        let url = self.url_for_on(hub, path, params);
        let mut builder = http::Request::builder()
            .method(method)
            .uri(&url)
            .header(self.auth.0.as_str(), self.auth.1.as_str());

        for (name, value) in extra_headers {
            builder = builder.header(*name, *value);
        }

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
/// URL such as `https://server.test/tfs` keeps its path. An empty `query` adds no
/// `?` at all — the unversioned connectionData request carries no query string.
fn with_org(server: &str, org: &str, path: &str, query: &str) -> String {
    let suffix = if query.is_empty() {
        String::new()
    } else {
        format!("?{query}")
    };

    match server.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('/') {
            Some((host, tail)) => format!("{scheme}://{host}/{org}/{tail}/{path}{suffix}"),
            None => format!("{scheme}://{rest}/{org}/{path}{suffix}"),
        },
        None => format!("{server}/{org}/{path}{suffix}"),
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

    /// The connectionData lookup is the one captured request with no query at
    /// all, on both bases: the cloud host gets no `?api-version=`, and the
    /// self-hosted builder gets no dangling `?` either.
    #[test]
    fn an_unversioned_url_carries_no_query() {
        assert_eq!(
            client(None).url_for_unversioned("/_apis/connectionData"),
            "https://myorg.visualstudio.com/_apis/connectionData"
        );
        assert_eq!(
            client(Some("https://server.test/tfs/")).url_for_unversioned("_apis/connectionData"),
            "https://server.test/myorg/tfs/_apis/connectionData"
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

    /// Ruling A3: the origin comparison the credential guard uses. Scheme, host
    /// and the explicit port all participate; an unparseable or relative URL
    /// matches nothing.
    #[test]
    fn the_credential_origin_is_the_clients_own_base() {
        let cloud = client(None);
        assert!(cloud.same_origin("https://myorg.visualstudio.com/blob/x"));
        assert!(!cloud.same_origin("https://otherog.visualstudio.com/blob/x"));
        assert!(!cloud.same_origin("https://dev.azure.com/myorg/blob/x"));
        assert!(
            !cloud.same_origin("http://myorg.visualstudio.com/blob/x"),
            "the scheme is part of the origin"
        );
        assert!(
            !cloud.same_origin("/blob/x"),
            "a relative URL matches nothing"
        );
        assert!(!cloud.same_origin("not a url"));

        let server = client(Some("https://server.test/tfs"));
        assert!(server.same_origin("https://server.test/tfs/myorg/blob/x"));
        assert!(
            server.same_origin("https://SERVER.test/blob/x"),
            "hosts compare case-insensitively"
        );
        assert!(!server.same_origin("http://server.test/blob/x"));
        assert!(
            !server.same_origin("https://server.test:8443/blob/x"),
            "an explicit port is part of the origin"
        );

        let ported = client(Some("http://127.0.0.1:8080"));
        assert!(ported.same_origin("http://127.0.0.1:8080/blob/x"));
        assert!(!ported.same_origin("http://127.0.0.1:9090/blob/x"));
    }

    /// The hub surfaces live on their own host for a cloud organization: the
    /// classic Release API on `vsrm`, entitlements on `vsaex`, extension
    /// management on `extmgmt` (D57). The path stays the module's own — only the
    /// host the request is addressed to changes.
    #[test]
    fn hub_urls_use_the_orgs_hub_host_on_the_cloud_base() {
        let cloud = client(None);

        assert_eq!(
            cloud.url_for_hub(Hub::Releases, "/Alpha/_apis/release/releases", &[]),
            "https://myorg.vsrm.visualstudio.com/Alpha/_apis/release/releases?api-version=7.1"
        );
        assert_eq!(
            cloud.url_for_hub(Hub::Entitlements, "/_apis/userentitlements", &[]),
            "https://myorg.vsaex.visualstudio.com/_apis/userentitlements?api-version=7.1"
        );
        assert_eq!(
            cloud.url_for_hub(
                Hub::Extensions,
                "/_apis/extensionmanagement/installedextensions",
                &[]
            ),
            "https://myorg.extmgmt.visualstudio.com/_apis/extensionmanagement/installedextensions?api-version=7.1"
        );
    }

    /// Azure DevOps Server has no hub hosts: every surface stays on the
    /// collection the caller gave, with the org segment where it already was.
    #[test]
    fn hub_urls_stay_on_a_self_hosted_server() {
        assert_eq!(
            client(Some("https://server.test/tfs")).url_for_hub(
                Hub::Releases,
                "/Alpha/_apis/release/releases",
                &[("$top".to_owned(), "5".to_owned())]
            ),
            "https://server.test/myorg/tfs/Alpha/_apis/release/releases?api-version=7.1&%24top=5"
        );
    }

    /// Ruling A3 with the hubs: the credential may go to this organization's
    /// three hub hosts and nowhere else — not a sibling organization's hub, not
    /// the bare hub host, and not any other subdomain.
    #[test]
    fn the_credential_may_reach_the_orgs_hub_hosts() {
        let cloud = client(None);

        for hub in ["vsrm", "vsaex", "extmgmt"] {
            assert!(
                cloud.same_origin(&format!("https://myorg.{hub}.visualstudio.com/x")),
                "{hub} is one of this organization's hubs"
            );
        }
        assert!(
            !cloud.same_origin("https://myorg.notahub.visualstudio.com/x"),
            "an unknown subdomain is not a hub"
        );
        assert!(
            !cloud.same_origin("https://otherog.vsrm.visualstudio.com/x"),
            "another organization's hub is not this one's"
        );
        assert!(
            !cloud.same_origin("https://vsrm.dev.azure.com/myorg/x"),
            "the cloud base is the org host, not a hub host"
        );
        assert!(
            !client(Some("https://server.test/tfs"))
                .same_origin("https://myorg.vsrm.visualstudio.com/x"),
            "a self-hosted base has no cloud hubs"
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
