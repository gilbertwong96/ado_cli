//! The standalone shape of the mock: a scenario file of routes plus a request log
//! a harness reads between the two CLI runs it compares. [`MockServer`] is the
//! in-process shape the integration suites drive; this is the one
//! `scripts/oracle-diff.sh --mock` starts as its own process.
//!
//! Matching is the same rule as [`MockServer::expect`](crate::MockServer::expect):
//! method (case-insensitive) and exact path, with the query ignored — so the
//! frozen CLI's `state`/`top`/`skip` and this build's `stateFilter`/`$top`/`$skip`
//! (D19) both reach the same route, and the difference is read from the request
//! log instead. Routes repeat: a case may hit the same path as often as it likes.
//!
//! A route may also declare the `query` pairs a request must carry. That is what
//! makes two GETs on one path distinguishable — the attachment download's metadata
//! GET and its raw-content GET, or the base and target fetches of one file — so a
//! route can answer each with the body it deserves. The pairs are compared **as
//! sent on the wire, undecoded** (like
//! [`MockServer::expect_query`](crate::MockServer::expect_query)), a value written
//! `*` matches any value, and a route that requires pairs is tried **before** one
//! that does not, whatever their order in the file: the specific route wins and the
//! query-blind route stays the fallback. A request that carries none of a route's
//! pairs falls through to the next route, and if none matches it is the usual 404.
//!
//! A route may also declare the `request_body` it expects a request to carry. That
//! is an **assertion, not part of the match**: the route still answers, and the
//! log line records `body_matched` (`true`/`false`, `null` when no expectation was
//! declared) so the harness can fail the case with the mismatch named. A string
//! expectation compares the bytes as sent; an object or array expectation parses
//! the body and compares structurally, so key order and whitespace cannot make an
//! otherwise-equal body fail — the body counterpart of the query's parsed-pairs
//! comparison.
//!
//! A route may serve a **sequence** instead of one body: `sequence` is an array of
//! response entries (`status`, `fixture` or `json`, `set`), and the route serves
//! one entry per matching request in order, the last entry repeating forever. That
//! is what a poll loop needs — `ci watch` reads a build until it is terminal, so
//! the same path must answer `inProgress` and then `completed` — and it is also
//! what makes a repeated request observable in the log even when two requests would
//! otherwise be indistinguishable. A route declares either the singular form or
//! `sequence`, never both. The cursor is per route, not per path: a request that
//! matches a different route cannot advance this one.
//!
//! [`MockServer`]: crate::MockServer

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::any;
use serde_json::Value;
use tokio::sync::oneshot;

use crate::{MockResponse, RecordedRequest, headers_of, lock, respond};

/// One route a scenario file declares.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub method: String,
    pub path: String,
    /// The query pairs the request must carry when the route declares any: the
    /// pairs as sent on the wire (undecoded, like `MockServer::expect_query`),
    /// with `*` as a value matching any value. Empty means the route is
    /// query-blind. See the module docs for the matching and precedence rules.
    pub query: Vec<(String, String)>,
    /// The responses this route serves, one per matching request, the last
    /// repeating: a plain route carries one entry, a `sequence` the entries it
    /// declares. See the module docs.
    pub responses: Vec<MockResponse>,
    /// The body the request must carry, when the route declares one: a string
    /// compares the bytes as sent, any other JSON value parses the request body
    /// and compares structurally. Recorded in the log as `body_matched`; see the
    /// module docs.
    pub request_body: Option<Value>,
}

impl Route {
    fn matches(&self, method: &str, path: &str, query: &[(String, String)]) -> bool {
        self.method.eq_ignore_ascii_case(method) && self.path == path && self.query_matches(query)
    }

    fn query_matches(&self, query: &[(String, String)]) -> bool {
        self.query
            .iter()
            .all(|(key, value)| required_pair_matches(query, key, value))
    }
}

/// Whether the request's pairs carry `key`, equal to `value` or to anything when
/// `value` is `*`.
fn required_pair_matches(query: &[(String, String)], key: &str, value: &str) -> bool {
    query
        .iter()
        .any(|(sent_key, sent_value)| sent_key == key && (value == "*" || sent_value == value))
}

/// The request's query as `(key, value)` pairs, exactly as sent: the same split
/// `MockServer::expect_query` matches against.
fn sent_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => (pair.to_owned(), String::new()),
        })
        .collect()
}

/// The route table a standalone mock serves, parsed from a scenario file.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    routes: Vec<Route>,
}

/// The origin placeholder a scenario can write wherever the CLI is expected to
/// name the mock's own URL (an absolute artifact `downloadUrl`, say): it expands
/// to the running server's origin once the port is known.
pub const BASE_PLACEHOLDER: &str = "{base}";

impl Scenario {
    /// Parses a scenario: `{"responses": [{"method": …, "path": …}]}` where a
    /// response carries `status` (default 200) and either `fixture` (a file under
    /// `crates/ado-testkit/fixtures/`) or `json` (an inline body), plus optional
    /// `set` edits — `[{"pointer": "/value/0/resource/downloadUrl", "value": …}]` —
    /// applied to the body before it is served, an optional `query` object of the
    /// pairs the request must carry (a `"*"` value matches any value), and an
    /// optional `request_body` the request must carry (a string compares as sent;
    /// an object or array compares structurally). A route may carry `sequence`
    /// instead of `status`/`fixture`/`json`/`set`: an array of entries with those
    /// same fields, served one per matching request with the last repeating.
    pub fn from_json(text: &str) -> Result<Scenario, String> {
        let document: Value = serde_json::from_str(text)
            .map_err(|error| format!("the scenario is not JSON: {error}"))?;

        let responses = document
            .get("responses")
            .and_then(Value::as_array)
            .ok_or_else(|| "the scenario has no `responses` array".to_owned())?;

        let routes = responses
            .iter()
            .map(parse_route)
            .collect::<Result<Vec<Route>, String>>()?;

        if routes.is_empty() {
            return Err("the scenario has no routes".to_owned());
        }

        Ok(Scenario { routes })
    }

    pub fn load(path: &Path) -> Result<Scenario, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;

        Scenario::from_json(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// The same scenario with every [`BASE_PLACEHOLDER`] in a path or a body
    /// replaced by `base`.
    pub fn expand(&self, base: &str) -> Scenario {
        Scenario {
            routes: self
                .routes
                .iter()
                .map(|route| Route {
                    method: route.method.clone(),
                    path: route.path.replace(BASE_PLACEHOLDER, base),
                    query: route.query.clone(),
                    responses: route
                        .responses
                        .iter()
                        .map(|response| MockResponse {
                            status: response.status,
                            body: substitute_body(&response.body, base),
                            headers: response.headers.clone(),
                        })
                        .collect(),
                    request_body: route.request_body.clone(),
                })
                .collect(),
        }
    }
}

/// Replaces the placeholder in a body's strings, and only then reparses it: a body
/// without one keeps its captured bytes verbatim.
fn substitute_body(body: &[u8], base: &str) -> Vec<u8> {
    if !body
        .windows(BASE_PLACEHOLDER.len())
        .any(|window| window == BASE_PLACEHOLDER.as_bytes())
    {
        return body.to_vec();
    }

    match serde_json::from_slice::<Value>(body) {
        Ok(mut document) => {
            substitute_strings(&mut document, base);
            serde_json::to_vec(&document).unwrap_or_else(|_| body.to_vec())
        }
        Err(_) => body.to_vec(),
    }
}

fn substitute_strings(value: &mut Value, base: &str) {
    match value {
        Value::String(text) => *text = text.replace(BASE_PLACEHOLDER, base),
        Value::Array(items) => {
            for item in items {
                substitute_strings(item, base);
            }
        }
        Value::Object(entries) => {
            for entry in entries.values_mut() {
                substitute_strings(entry, base);
            }
        }
        _ => {}
    }
}

fn parse_route(value: &Value) -> Result<Route, String> {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("a route has no `method`: {value}"))?
        .to_owned();
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the {method} route has no `path`"))?
        .to_owned();
    let what = format!("the {method} {path} route");

    let responses = match value.get("sequence") {
        Some(entries) => parse_sequence(&what, value, entries)?,
        None => vec![parse_response(&what, value)?],
    };

    let request_body = match value.get("request_body") {
        Some(Value::String(text)) => Some(Value::String(text.clone())),
        Some(value @ (Value::Object(_) | Value::Array(_))) => Some(value.clone()),
        Some(other) => {
            return Err(format!(
                "the {method} {path} route's `request_body` must be a string, object or array: {other}"
            ));
        }
        None => None,
    };

    let query = match value.get("query") {
        Some(Value::Object(entries)) => entries
            .iter()
            .map(|(key, value)| match value.as_str() {
                Some(value) => Ok((key.clone(), value.to_owned())),
                None => Err(format!(
                    "the {method} {path} route's `query` value for '{key}' must be a string: {value}"
                )),
            })
            .collect::<Result<Vec<(String, String)>, String>>()?,
        Some(other) => {
            return Err(format!(
                "the {method} {path} route's `query` must be an object of pairs: {other}"
            ));
        }
        None => Vec::new(),
    };

    Ok(Route {
        method,
        path,
        query,
        responses,
        request_body,
    })
}

/// The responses a route serves: its singular form is a one-entry sequence.
fn parse_sequence(what: &str, route: &Value, entries: &Value) -> Result<Vec<MockResponse>, String> {
    for singular in ["status", "fixture", "json", "set"] {
        if route.get(singular).is_some() {
            return Err(format!("{what} has both `sequence` and `{singular}`"));
        }
    }

    let entries = entries
        .as_array()
        .ok_or_else(|| format!("{what}'s `sequence` is not an array"))?;

    if entries.is_empty() {
        return Err(format!("{what}'s `sequence` has no entries"));
    }

    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| parse_response(&format!("{what}'s sequence entry {index}"), entry))
        .collect()
}

/// One response a route serves: `status` (default 200), `fixture` xor `json`, and
/// the `set` edits applied to that body.
fn parse_response(what: &str, value: &Value) -> Result<MockResponse, String> {
    let status = value.get("status").and_then(Value::as_u64).unwrap_or(200) as u16;

    let (body, content_type) = match (value.get("fixture"), value.get("json")) {
        (Some(fixture), None) => {
            let name = fixture
                .as_str()
                .ok_or_else(|| format!("{what}'s `fixture` is not a string"))?;

            fixture_body(name)?
        }
        (None, Some(inline)) => (
            serde_json::to_vec(inline)
                .map_err(|error| format!("cannot encode the body: {error}"))?,
            "application/json",
        ),
        (None, None) => return Err(format!("{what} has neither `fixture` nor `json`")),
        (Some(_), Some(_)) => return Err(format!("{what} has both `fixture` and `json`")),
    };

    let body = match value.get("set") {
        Some(edits) => apply_edits(&body, edits).map_err(|error| format!("{what}: {error}"))?,
        None => body,
    };

    Ok(MockResponse {
        status,
        body,
        headers: vec![("content-type".to_owned(), content_type.to_owned())],
    })
}

/// Whether a request's raw bytes satisfy a route's `request_body` expectation.
fn body_matches(expected: &Value, body: &[u8]) -> bool {
    match expected {
        Value::String(text) => String::from_utf8_lossy(body) == text.as_str(),
        _ => match serde_json::from_slice::<Value>(body) {
            Ok(actual) => &actual == expected,
            Err(_) => false,
        },
    }
}

/// A fixture served with the content type its bytes deserve: a JSON fixture as JSON,
/// anything else (the artifact zip) as opaque bytes.
fn fixture_body(name: &str) -> Result<(Vec<u8>, &'static str), String> {
    let path = fixture_path(name);
    let body = std::fs::read(&path)
        .map_err(|error| format!("cannot read fixture {}: {error}", path.display()))?;

    let content_type = if serde_json::from_slice::<Value>(&body).is_ok() {
        "application/json"
    } else {
        "application/octet-stream"
    };

    Ok((body, content_type))
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn apply_edits(body: &[u8], edits: &Value) -> Result<Vec<u8>, String> {
    let mut document: Value = serde_json::from_slice(body)
        .map_err(|error| format!("`set` needs a JSON body: {error}"))?;

    for edit in edits
        .as_array()
        .ok_or_else(|| "`set` is not an array".to_owned())?
    {
        let pointer = edit
            .get("pointer")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("a `set` edit has no `pointer`: {edit}"))?;
        let value = edit
            .get("value")
            .ok_or_else(|| format!("the {pointer} edit has no `value`"))?;

        match document.pointer_mut(pointer) {
            Some(slot) => *slot = value.clone(),
            None => return Err(format!("the pointer {pointer} matches nothing in the body")),
        }
    }

    serde_json::to_vec(&document).map_err(|error| format!("cannot encode the edited body: {error}"))
}

/// A scenario-driven mock server, alive until dropped.
pub struct StandaloneMock {
    base_url: String,
    state: Arc<StandaloneState>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

struct StandaloneState {
    routes: Vec<Route>,
    /// How many requests each route has answered: the cursor into its `responses`.
    served: Mutex<Vec<usize>>,
    requests: Mutex<Vec<RecordedRequest>>,
    log: Mutex<Option<File>>,
}

impl StandaloneMock {
    /// Starts the mock on `port` (0 takes an ephemeral one). With `record`, every
    /// request is appended to that file as one JSON object per line and flushed as
    /// it arrives, so a harness can read the log the moment a CLI run exits — and
    /// can split it per run by counting lines before and after.
    pub fn start(scenario: Scenario, record: Option<&Path>, port: u16) -> StandaloneMock {
        let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind the mock port");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let address = listener.local_addr().expect("the bound address");
        let base_url = format!("http://{address}");

        let log = record.map(|path| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap_or_else(|error| {
                    panic!("cannot open the request log {}: {error}", path.display())
                })
        });

        let state = Arc::new(StandaloneState {
            served: Mutex::new(vec![0; scenario.routes().len()]),
            routes: scenario.expand(&base_url).routes,
            requests: Mutex::new(Vec::new()),
            log: Mutex::new(log),
        });
        let router = Router::new()
            .fallback(any(handle))
            .with_state(state.clone());
        let (shutdown, shutdown_rx) = oneshot::channel();

        let thread = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the mock server runtime");
            runtime.block_on(async move {
                let listener =
                    tokio::net::TcpListener::from_std(listener).expect("a tokio listener");
                tokio::spawn(async move {
                    let _ = axum::serve(listener, router).await;
                });
                let _ = shutdown_rx.await;
            });
        });

        StandaloneMock {
            base_url,
            state,
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }

    /// The server's origin, e.g. `http://127.0.0.1:52341` — point `ADO_SERVER` at it.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Everything the server received so far, in arrival order.
    pub fn received(&self) -> Vec<RecordedRequest> {
        lock(&self.state.requests).clone()
    }
}

impl StandaloneState {
    /// The response the route at `index` serves next: its cursor walks the route's
    /// sequence and stops on the last entry.
    fn serve(&self, index: usize) -> MockResponse {
        let responses = &self.routes[index].responses;
        let served = {
            let mut cursors = lock(&self.served);
            let served = cursors[index];
            cursors[index] = served.saturating_add(1);
            served
        };

        responses[served.min(responses.len() - 1)].clone()
    }

    fn record(&self, request: &RecordedRequest, matched: bool, body_matched: Option<bool>) {
        lock(&self.requests).push(request.clone());

        let mut log = lock(&self.log);
        if let Some(file) = log.as_mut() {
            let line = serde_json::json!({
                "method": request.method,
                "path": request.path,
                "query": request.query,
                "body": request.body,
                "matched": matched,
                "body_matched": body_matched,
            });

            writeln!(file, "{line}")
                .and_then(|()| file.flush())
                .unwrap_or_else(|error| panic!("cannot write the request log: {error}"));
        }
    }
}

impl Drop for StandaloneMock {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn handle(State(state): State<Arc<StandaloneState>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .expect("read the request body");

    let recorded = RecordedRequest {
        method: parts.method.as_str().to_owned(),
        path: parts.uri.path().to_owned(),
        query: parts.uri.query().unwrap_or_default().to_owned(),
        headers: headers_of(&parts.headers),
        body: (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned()),
    };

    let matched_index = {
        let pairs = sent_pairs(&recorded.query);

        state
            .routes
            .iter()
            .position(|route| {
                !route.query.is_empty() && route.matches(&recorded.method, &recorded.path, &pairs)
            })
            .or_else(|| {
                state.routes.iter().position(|route| {
                    route.query.is_empty()
                        && route.matches(&recorded.method, &recorded.path, &pairs)
                })
            })
    };

    let body_matched = matched_index.and_then(|index| {
        state.routes[index]
            .request_body
            .as_ref()
            .map(|expected| body_matches(expected, &bytes))
    });

    state.record(&recorded, matched_index.is_some(), body_matched);

    match matched_index.map(|index| state.serve(index)) {
        Some(response) => respond(response),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "message": format!("no route for {} {}", recorded.method, recorded.path),
                })
                .to_string(),
            ))
            .expect("a valid response"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    use serde_json::json;

    use super::*;

    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// A raw HTTP/1.1 request with a body, so the tests exercise the mock exactly
    /// as a client does.
    fn http_request(
        mock: &StandaloneMock,
        method: &str,
        target: &str,
        body: &str,
    ) -> (u16, String) {
        let address = mock
            .base_url()
            .strip_prefix("http://")
            .expect("an http origin");
        let mut stream = TcpStream::connect(address).expect("connect to the mock");
        write!(
            stream,
            "{method} {target} HTTP/1.1\r\nhost: localhost\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("write the request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read the response");

        let (head, body) = response
            .split_once("\r\n\r\n")
            .expect("a response head and body");
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .expect("a status line")
            .parse()
            .expect("a numeric status");

        (status, body.to_owned())
    }

    /// A bodyless GET, the shape most tests use.
    fn http_get(mock: &StandaloneMock, target: &str) -> (u16, String) {
        http_request(mock, "GET", target, "")
    }

    fn scenario(text: &str) -> Scenario {
        Scenario::from_json(text).expect("a valid scenario")
    }

    fn scratch(name: &str) -> PathBuf {
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ado-standalone-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    fn request_log(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .expect("the request log")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a JSON line"))
            .collect()
    }

    #[test]
    fn a_scenario_loads_a_fixture_and_an_inline_body() {
        let parsed = scenario(
            r#"{"responses": [
                {"method": "GET", "path": "/a", "fixture": "projects_list.json"},
                {"method": "POST", "path": "/b", "status": 404, "json": {"message": "gone"}}
            ]}"#,
        );

        assert_eq!(parsed.routes().len(), 2);
        assert_eq!(parsed.routes()[0].method, "GET");
        assert_eq!(parsed.routes()[0].path, "/a");
        assert_eq!(parsed.routes()[0].responses[0].status, 200);
        assert_eq!(
            parsed.routes()[0].responses[0].headers,
            vec![("content-type".to_owned(), "application/json".to_owned())]
        );
        assert_eq!(parsed.routes()[1].responses[0].status, 404);
        assert_eq!(
            parsed.routes()[1].responses[0].body,
            br#"{"message":"gone"}"#
        );
    }

    #[test]
    fn a_scenario_rejects_a_response_with_no_body() {
        let error = Scenario::from_json(r#"{"responses": [{"method": "GET", "path": "/a"}]}"#)
            .expect_err("a bodyless route is invalid");

        assert!(
            error.contains("neither `fixture` nor `json`"),
            "error: {error}"
        );
    }

    #[test]
    fn a_scenario_reports_a_missing_responses_array() {
        let error = Scenario::from_json("{}").expect_err("no responses");

        assert!(error.contains("no `responses` array"), "error: {error}");
    }

    #[test]
    fn set_edits_replace_a_nested_value() {
        let parsed = scenario(
            r#"{"responses": [{
                "method": "GET",
                "path": "/artifacts",
                "fixture": "artifacts_list.json",
                "set": [{
                    "pointer": "/value/0/resource/downloadUrl",
                    "value": "{base}/blob/drop.zip"
                }]
            }]}"#,
        );

        let body: Value =
            serde_json::from_slice(&parsed.routes()[0].responses[0].body).expect("JSON");
        assert_eq!(
            body["value"][0]["resource"]["downloadUrl"],
            json!("{base}/blob/drop.zip")
        );
        assert_eq!(
            body["value"][1]["name"],
            json!("TestResults"),
            "the rest is untouched"
        );
    }

    #[test]
    fn set_reports_a_pointer_that_matches_nothing() {
        let error = Scenario::from_json(
            r#"{"responses": [{"method": "GET", "path": "/a", "json": {}, "set": [
                {"pointer": "/missing", "value": 1}
            ]}]}"#,
        )
        .expect_err("a bad pointer is invalid");

        assert!(error.contains("matches nothing"), "error: {error}");
    }

    #[test]
    fn a_route_can_require_a_json_request_body() {
        let path = scratch("log-body");
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [{
                    "method": "POST",
                    "path": "/x",
                    "request_body": {"query": "SELECT 1"},
                    "json": {"ok": true}
                }]}"#,
            ),
            Some(&path),
            0,
        );

        let (status, _) = http_request(&mock, "POST", "/x", r#"{"query":"SELECT 1"}"#);

        assert_eq!(status, 200);
        assert_eq!(request_log(&path)[0]["body_matched"], json!(true));

        drop(mock);
        std::fs::remove_file(&path).expect("remove the log");
    }

    #[test]
    fn a_body_mismatch_is_recorded_not_routed_away() {
        let path = scratch("log-body-mismatch");
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [{
                    "method": "POST",
                    "path": "/x",
                    "request_body": {"query": "SELECT 1"},
                    "json": {"ok": true}
                }]}"#,
            ),
            Some(&path),
            0,
        );

        let (status, _) = http_request(&mock, "POST", "/x", r#"{"query":"SELECT 2"}"#);

        assert_eq!(
            status, 200,
            "the route still answers; the log carries the verdict"
        );
        assert_eq!(request_log(&path)[0]["body_matched"], json!(false));

        drop(mock);
        std::fs::remove_file(&path).expect("remove the log");
    }

    #[test]
    fn a_json_request_body_ignores_key_order_and_whitespace() {
        let path = scratch("log-body-order");
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [{
                    "method": "POST",
                    "path": "/x",
                    "request_body": {"b": 2, "a": 1},
                    "json": {}
                }]}"#,
            ),
            Some(&path),
            0,
        );

        let (status, _) = http_request(&mock, "POST", "/x", "{\n  \"a\": 1,\n  \"b\": 2\n}");

        assert_eq!(status, 200);
        assert_eq!(request_log(&path)[0]["body_matched"], json!(true));

        drop(mock);
        std::fs::remove_file(&path).expect("remove the log");
    }

    #[test]
    fn a_json_request_body_compares_array_order_positionally() {
        let path = scratch("log-body-array-order");
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [{
                    "method": "PATCH",
                    "path": "/x",
                    "request_body": [
                        {"op": "replace", "path": "/fields/System.Tags", "value": "a,b"},
                        {"op": "add", "path": "/fields/System.Title", "value": "T"},
                        {"op": "add", "path": "/fields/System.AssignedTo", "value": "bob"}
                    ],
                    "json": {}
                }]}"#,
            ),
            Some(&path),
            0,
        );

        // update's captured pin is replace-tags-first; create's is add-ops-first.
        let _ = http_request(
            &mock,
            "PATCH",
            "/x",
            r#"[{"op":"add","path":"/fields/System.Title","value":"T"},{"op":"add","path":"/fields/System.AssignedTo","value":"bob"},{"op":"replace","path":"/fields/System.Tags","value":"a,b"}]"#,
        );
        let _ = http_request(
            &mock,
            "PATCH",
            "/x",
            r#"[{"value":"a,b","path":"/fields/System.Tags","op":"replace"},{"value":"T","path":"/fields/System.Title","op":"add"},{"value":"bob","path":"/fields/System.AssignedTo","op":"add"}]"#,
        );

        let lines = request_log(&path);
        assert_eq!(
            lines[0]["body_matched"],
            json!(false),
            "the same ops in create's order must not satisfy update's pin"
        );
        assert_eq!(
            lines[1]["body_matched"],
            json!(true),
            "key order inside an op object stays ignored"
        );

        drop(mock);
        std::fs::remove_file(&path).expect("remove the log");
    }

    #[test]
    fn a_string_request_body_compares_the_bytes_as_sent() {
        let path = scratch("log-body-string");
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [{
                    "method": "PUT",
                    "path": "/x",
                    "request_body": "raw-bytes",
                    "json": {}
                }]}"#,
            ),
            Some(&path),
            0,
        );

        let _ = http_request(&mock, "PUT", "/x", "raw-bytes");
        let _ = http_request(&mock, "PUT", "/x", "raw-byte");

        let lines = request_log(&path);
        assert_eq!(lines[0]["body_matched"], json!(true));
        assert_eq!(lines[1]["body_matched"], json!(false));

        drop(mock);
        std::fs::remove_file(&path).expect("remove the log");
    }

    #[test]
    fn a_request_body_that_is_not_a_string_object_or_array_is_rejected() {
        let error = Scenario::from_json(
            r#"{"responses": [{"method": "POST", "path": "/x", "json": {}, "request_body": 12}]}"#,
        )
        .expect_err("a numeric expectation is invalid");

        assert!(error.contains("`request_body`"), "error: {error}");
    }

    #[test]
    fn expand_rewrites_the_placeholder_in_paths_and_bodies() {
        let parsed = scenario(
            r#"{"responses": [{
                "method": "GET",
                "path": "/{base}/blob",
                "json": {"url": "{base}/blob"}
            }]}"#,
        )
        .expand("http://127.0.0.1:9999");

        assert_eq!(parsed.routes()[0].path, "/http://127.0.0.1:9999/blob");
        assert_eq!(
            parsed.routes()[0].responses[0].body,
            br#"{"url":"http://127.0.0.1:9999/blob"}"#
        );
    }

    #[test]
    fn the_mock_serves_a_route_and_records_the_request() {
        let mock = StandaloneMock::start(
            scenario(r#"{"responses": [{"method": "GET", "path": "/x", "json": {"ok": true}}]}"#),
            None,
            0,
        );

        let (status, body) = http_get(&mock, "/x?$top=10");

        assert_eq!(status, 200);
        assert_eq!(body, r#"{"ok":true}"#);
        assert_eq!(mock.received().len(), 1);
        assert_eq!(mock.received()[0].path, "/x");
        assert_eq!(mock.received()[0].query, "$top=10");
        assert_eq!(mock.received()[0].method, "GET");
    }

    #[test]
    fn the_mock_answers_an_unrouted_request_with_404() {
        let mock = StandaloneMock::start(
            scenario(r#"{"responses": [{"method": "GET", "path": "/x", "json": {}}]}"#),
            None,
            0,
        );

        let (status, body) = http_get(&mock, "/nope");

        assert_eq!(status, 404);
        assert!(body.contains("no route for GET /nope"), "body: {body}");
    }

    #[test]
    fn the_mock_answers_the_same_route_twice() {
        let mock = StandaloneMock::start(
            scenario(r#"{"responses": [{"method": "GET", "path": "/x", "json": {"ok": true}}]}"#),
            None,
            0,
        );

        assert_eq!(http_get(&mock, "/x").0, 200);
        assert_eq!(http_get(&mock, "/x").0, 200);
        assert_eq!(mock.received().len(), 2);
    }

    #[test]
    fn a_two_entry_sequence_answers_the_same_path_twice_differently() {
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [{
                    "method": "GET",
                    "path": "/builds/1",
                    "sequence": [
                        {"json": {"status": "inProgress"}},
                        {"json": {"status": "completed"}}
                    ]
                }]}"#,
            ),
            None,
            0,
        );

        assert_eq!(
            http_get(&mock, "/builds/1"),
            (200, r#"{"status":"inProgress"}"#.to_owned())
        );
        assert_eq!(
            http_get(&mock, "/builds/1"),
            (200, r#"{"status":"completed"}"#.to_owned())
        );
        assert_eq!(
            http_get(&mock, "/builds/1"),
            (200, r#"{"status":"completed"}"#.to_owned()),
            "the last entry repeats"
        );
    }

    #[test]
    fn a_one_entry_sequence_repeats() {
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [{
                    "method": "GET",
                    "path": "/x",
                    "sequence": [{"json": {"ok": true}}]
                }]}"#,
            ),
            None,
            0,
        );

        assert_eq!(http_get(&mock, "/x"), (200, r#"{"ok":true}"#.to_owned()));
        assert_eq!(http_get(&mock, "/x"), (200, r#"{"ok":true}"#.to_owned()));
    }

    #[test]
    fn a_sequence_entry_carries_its_own_status_and_set() {
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [{
                    "method": "POST",
                    "path": "/x",
                    "sequence": [
                        {"status": 409, "json": {"message": "busy"}},
                        {"json": {"id": 1}, "set": [{"pointer": "/id", "value": 2}]}
                    ]
                }]}"#,
            ),
            None,
            0,
        );

        assert_eq!(
            http_request(&mock, "POST", "/x", ""),
            (409, r#"{"message":"busy"}"#.to_owned())
        );
        assert_eq!(
            http_request(&mock, "POST", "/x", ""),
            (200, r#"{"id":2}"#.to_owned())
        );
    }

    #[test]
    fn a_sequence_leaves_other_paths_to_their_routes_and_the_404() {
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [
                    {"method": "GET", "path": "/x", "sequence": [{"json": {"served": 1}}, {"json": {"served": 2}}]},
                    {"method": "GET", "path": "/y", "json": {"served": "y"}}
                ]}"#,
            ),
            None,
            0,
        );

        assert_eq!(http_get(&mock, "/y"), (200, r#"{"served":"y"}"#.to_owned()));
        assert_eq!(http_get(&mock, "/x"), (200, r#"{"served":1}"#.to_owned()));

        let (status, body) = http_get(&mock, "/nope");
        assert_eq!(status, 404, "an unmatched request is still a 404");
        assert!(body.contains("no route for GET /nope"), "body: {body}");

        assert_eq!(
            http_get(&mock, "/x"),
            (200, r#"{"served":2}"#.to_owned()),
            "the other routes' requests did not advance this sequence"
        );
    }

    #[test]
    fn the_request_log_keeps_matched_per_request_while_a_sequence_runs() {
        let path = scratch("log-sequence");
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [{
                    "method": "GET",
                    "path": "/x",
                    "sequence": [{"json": {"served": 1}}, {"json": {"served": 2}}]
                }]}"#,
            ),
            Some(&path),
            0,
        );

        let _ = http_get(&mock, "/x");
        let _ = http_get(&mock, "/nope");
        let _ = http_get(&mock, "/x");

        let lines = request_log(&path);
        assert_eq!(lines[0]["matched"], json!(true));
        assert_eq!(lines[1]["matched"], json!(false));
        assert_eq!(lines[2]["matched"], json!(true));

        drop(mock);
        std::fs::remove_file(&path).expect("remove the log");
    }

    #[test]
    fn a_sequence_and_a_singular_body_are_rejected_together() {
        let error = Scenario::from_json(
            r#"{"responses": [{
                "method": "GET",
                "path": "/x",
                "json": {"ok": true},
                "sequence": [{"json": {"ok": true}}]
            }]}"#,
        )
        .expect_err("both forms on one route are invalid");

        assert!(
            error.contains("both `sequence` and `json`"),
            "error: {error}"
        );
    }

    #[test]
    fn a_sequence_entry_without_a_body_is_rejected() {
        let error = Scenario::from_json(
            r#"{"responses": [{
                "method": "GET",
                "path": "/x",
                "sequence": [{"status": 200}]
            }]}"#,
        )
        .expect_err("a bodyless entry is invalid");

        assert!(
            error.contains("sequence entry 0 has neither `fixture` nor `json`"),
            "error: {error}"
        );
    }

    #[test]
    fn an_empty_sequence_is_rejected() {
        let error = Scenario::from_json(
            r#"{"responses": [{"method": "GET", "path": "/x", "sequence": []}]}"#,
        )
        .expect_err("an empty sequence is invalid");

        assert!(
            error.contains("`sequence` has no entries"),
            "error: {error}"
        );
    }

    #[test]
    fn expand_rewrites_the_placeholder_in_a_sequence_too() {
        let parsed = scenario(
            r#"{"responses": [{
                "method": "GET",
                "path": "/x",
                "sequence": [{"json": {"url": "{base}/first"}}, {"json": {"url": "{base}/second"}}]
            }]}"#,
        )
        .expand("http://127.0.0.1:9999");

        assert_eq!(
            parsed.routes()[0].responses[0].body,
            br#"{"url":"http://127.0.0.1:9999/first"}"#
        );
        assert_eq!(
            parsed.routes()[0].responses[1].body,
            br#"{"url":"http://127.0.0.1:9999/second"}"#
        );
    }

    #[test]
    fn a_route_can_require_query_pairs() {
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [
                    {"method": "GET", "path": "/items", "query": {"version": "aaaa1111"}, "json": {"rev": "base"}},
                    {"method": "GET", "path": "/items", "query": {"version": "cccc3333"}, "json": {"rev": "target"}}
                ]}"#,
            ),
            None,
            0,
        );

        let (status, body) = http_get(
            &mock,
            "/items?api-version=7.1&path=%2Fa.ex&version=aaaa1111&versionType=commit",
        );
        assert_eq!(status, 200);
        assert_eq!(body, r#"{"rev":"base"}"#);

        let (status, body) = http_get(&mock, "/items?version=cccc3333");
        assert_eq!(status, 200);
        assert_eq!(body, r#"{"rev":"target"}"#);

        let (status, body) = http_get(&mock, "/items?version=dddd4444");
        assert_eq!(status, 404, "a version no route declares has no route");
        assert!(body.contains("no route for GET /items"), "body: {body}");
    }

    #[test]
    fn a_query_requiring_route_is_tried_before_a_query_blind_one() {
        // The blind route is declared first: the specific one still wins, so one
        // path can answer the metadata GET and the raw GET differently.
        let mock = StandaloneMock::start(
            scenario(
                r#"{"responses": [
                    {"method": "GET", "path": "/attachments/att-1", "json": {"id": "att-1"}},
                    {"method": "GET", "path": "/attachments/att-1", "query": {"fileName": "*"}, "json": {"raw": true}}
                ]}"#,
            ),
            None,
            0,
        );

        let (status, body) = http_get(&mock, "/attachments/att-1?api-version=7.1");
        assert_eq!(status, 200);
        assert_eq!(body, r#"{"id":"att-1"}"#);

        let (status, body) = http_get(&mock, "/attachments/att-1?api-version=7.1&fileName=out.bin");
        assert_eq!(status, 200);
        assert_eq!(body, r#"{"raw":true}"#);

        // The frozen CLI's spelling of the same pair: the version glued into the
        // name's value, one pair, compared as sent.
        let (status, body) = http_get(&mock, "/attachments/att-1?fileName=out.bin?api-version=7.1");
        assert_eq!(status, 200);
        assert_eq!(body, r#"{"raw":true}"#);
    }

    #[test]
    fn a_scenario_rejects_a_non_string_query_value() {
        let error = Scenario::from_json(
            r#"{"responses": [{"method": "GET", "path": "/x", "json": {}, "query": {"version": 12}}]}"#,
        )
        .expect_err("a numeric query value is invalid");

        assert!(
            error.contains("`query` value for 'version'"),
            "error: {error}"
        );
    }

    #[test]
    fn the_mock_writes_one_json_line_per_request_flushed() {
        let path = scratch("log");
        let mock = StandaloneMock::start(
            scenario(r#"{"responses": [{"method": "GET", "path": "/x", "json": {"ok": true}}]}"#),
            Some(&path),
            0,
        );

        let _ = http_get(&mock, "/x?a=b");
        let _ = http_get(&mock, "/nope");

        let lines = request_log(&path);
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            json!({"method": "GET", "path": "/x", "query": "a=b", "body": null, "matched": true, "body_matched": null})
        );
        assert_eq!(
            lines[1],
            json!({"method": "GET", "path": "/nope", "query": "", "body": null, "matched": false, "body_matched": null})
        );

        drop(mock);
        std::fs::remove_file(&path).expect("remove the log");
    }

    #[test]
    fn the_mock_binds_the_port_it_is_given() {
        let mock = StandaloneMock::start(
            scenario(r#"{"responses": [{"method": "GET", "path": "/x", "json": {}}]}"#),
            None,
            0,
        );

        let port: u16 = mock
            .base_url()
            .rsplit(':')
            .next()
            .expect("a port")
            .parse()
            .expect("a numeric port");

        assert_ne!(port, 0);
        assert_eq!(mock.base_url(), format!("http://127.0.0.1:{port}"));
    }
}
