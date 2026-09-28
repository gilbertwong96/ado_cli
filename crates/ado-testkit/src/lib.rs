//! A dev-only testkit for the Rust rewrite: the mock Azure DevOps server, the
//! temporary home directory, and the fixture loader the test suites share.
//!
//! `axum` and `tokio` live in this crate and nowhere else; the shipped binary is
//! sync `ureq`. Fixtures are response bodies captured from Azure DevOps and the
//! existing Elixir test suite; Wave 1 adds the fixtures its commands consume.
//! `TempHome` redirects the home and config directories of a spawned process on
//! Linux and macOS; on Windows `dirs` reads the registry's known folders, so the
//! temp home cannot redirect them.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::Response;
use axum::routing::any;
use serde_json::Value;
use tokio::sync::oneshot;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// One response the mock is configured to send.
#[derive(Debug, Clone, PartialEq)]
pub struct MockResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

impl MockResponse {
    /// A JSON body, with `content-type: application/json`.
    pub fn json(status: u16, body: Value) -> MockResponse {
        MockResponse {
            status,
            body: body.to_string().into_bytes(),
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        }
    }

    /// A captured response body from `fixtures/<name>.json`, sent with status 200;
    /// pair it with [`with_status`](MockResponse::with_status) for the error fixtures.
    /// A `.json` fixture must read as UTF-8 text as well as parse as JSON — a
    /// corrupt capture fails loudly here rather than serving bytes.
    pub fn from_fixture(name: &str) -> MockResponse {
        let path = fixture_path(&format!("{name}.json"));
        let body = fs::read(&path)
            .unwrap_or_else(|error| panic!("cannot read fixture {}: {error}", path.display()));
        std::str::from_utf8(&body)
            .unwrap_or_else(|error| panic!("fixture {} is not UTF-8: {error}", path.display()));
        serde_json::from_slice::<Value>(&body)
            .unwrap_or_else(|error| panic!("fixture {} is not JSON: {error}", path.display()));

        MockResponse {
            status: 200,
            body,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        }
    }

    /// A raw byte body, with `content-type: application/octet-stream`.
    pub fn bytes(status: u16, body: Vec<u8>) -> MockResponse {
        MockResponse {
            status,
            body,
            headers: vec![(
                "content-type".to_owned(),
                "application/octet-stream".to_owned(),
            )],
        }
    }

    /// A raw fixture body from `fixtures/<name>`, served with status 200 and
    /// `application/octet-stream` — [`from_fixture`](MockResponse::from_fixture)'s
    /// sibling for downloads, where the body is bytes and not JSON.
    pub fn from_bytes_fixture(name: &str) -> MockResponse {
        let path = fixture_path(name);
        let body = fs::read(&path)
            .unwrap_or_else(|error| panic!("cannot read fixture {}: {error}", path.display()));

        MockResponse::bytes(200, body)
    }

    /// The same response with a different status.
    pub fn with_status(mut self, status: u16) -> MockResponse {
        self.status = status;
        self
    }
}

/// One request the mock received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub query: String,
    /// Header names are lowercased by the http crate.
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl RecordedRequest {
    /// The value of a header, matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The request's query as `(key, value)` pairs in the order it sent them, with
    /// no percent-decoding. Repeated keys are preserved and a bare `?flag` becomes
    /// `("flag", "")`.
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        parse_query(&self.query)
    }
}

/// A mock Azure DevOps server on an ephemeral `127.0.0.1` port, alive until dropped.
///
/// Expectations are single-shot and newest-first, like the Elixir
/// `AdoCli.TestServer`: the first request matching a method and path consumes it,
/// so a path can answer twice with two different responses. A query-aware
/// expectation is skipped unless the request carries every pair it requires — but
/// because newest wins, a query-blind expectation registered *after* a query-aware
/// one shadows it for every request. Register the query-blind fallback **first**
/// and the query-aware expectation last; that is the pagination shape, where
/// `%24top=10` consumes the specific expectation and every other page falls through
/// to the fallback. When a specific query must be *proven*, assert
/// [`received`](MockServer::received)'s
/// [`query_pairs`](RecordedRequest::query_pairs) rather than relying on the match.
pub struct MockServer {
    base_url: String,
    state: Arc<MockState>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    pub fn start() -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let address = listener.local_addr().expect("the bound address");

        let state = Arc::new(MockState::default());
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

        MockServer {
            base_url: format!("http://{address}"),
            state,
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }

    /// The server's origin, e.g. `http://127.0.0.1:52341` — point `ADO_SERVER` at it.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Registers the response for `method` + `path`. The query string is not part of
    /// the match; [`expect_query`](MockServer::expect_query) is the query-aware
    /// variant and [`received`](MockServer::received) is where the raw query gets
    /// asserted.
    pub fn expect(&self, method: &str, path: &str, response: MockResponse) {
        self.expect_query(method, path, &[], response);
    }

    /// Registers the response for `method` + `path` when the request's query also
    /// carries every `(key, value)` pair in `query`. Pairs the request sends beyond
    /// those are ignored, the required pairs may arrive in any order, and a repeated
    /// key matches when any of its values is the required one. With an empty `query`
    /// this is [`expect`](MockServer::expect).
    ///
    /// Pairs are compared **as sent on the wire, undecoded**: the `ado-core` client
    /// percent-encodes every byte outside the unreserved set and writes a space as
    /// `+` (`encode_query` in `crates/ado-core/src/client.rs`), so the `$top` it
    /// sends arrives as `%24top` and must be expected as `("%24top", "10")` —
    /// `("$top", "10")` never matches. Assert fixture-derived tokens through
    /// [`received`](MockServer::received)'s
    /// [`query_pairs`](RecordedRequest::query_pairs) instead of an expectation when
    /// the spelling matters: a base64 `continuationToken` carrying `+`, `/` or `=` is
    /// sent as `%2B`, `%2F`, `%3D`, and `expect_query` only ever sees that encoded
    /// form.
    pub fn expect_query(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, &str)],
        response: MockResponse,
    ) {
        lock(&self.state.expectations).insert(
            0,
            Expectation {
                method: method.to_owned(),
                path: path.to_owned(),
                query: query
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                    .collect(),
                response,
            },
        );
    }

    /// Everything the server received so far, in arrival order.
    pub fn received(&self) -> Vec<RecordedRequest> {
        lock(&self.state.received).clone()
    }
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
struct MockState {
    expectations: Mutex<Vec<Expectation>>,
    received: Mutex<Vec<RecordedRequest>>,
}

struct Expectation {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    response: MockResponse,
}

impl Expectation {
    fn matches(&self, method: &str, path: &str, query: &str) -> bool {
        self.method.eq_ignore_ascii_case(method) && self.path == path && self.query_matches(query)
    }

    fn query_matches(&self, query: &str) -> bool {
        let sent = parse_query(query);

        self.query
            .iter()
            .all(|(key, value)| has_pair(&sent, key, value))
    }

    fn missing_query_pairs(&self, query: &str) -> Vec<String> {
        let sent = parse_query(query);

        self.query
            .iter()
            .filter(|(key, value)| !has_pair(&sent, key, value))
            .map(|(key, value)| format_pair(key, value))
            .collect()
    }
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => (pair.to_owned(), String::new()),
        })
        .collect()
}

fn has_pair(pairs: &[(String, String)], key: &str, value: &str) -> bool {
    pairs
        .iter()
        .any(|(sent_key, sent_value)| sent_key == key && sent_value == value)
}

fn format_pair(key: &str, value: &str) -> String {
    if value.is_empty() {
        key.to_owned()
    } else {
        format!("{key}={value}")
    }
}

async fn handle(State(state): State<Arc<MockState>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .expect("read the request body");

    lock(&state.received).push(RecordedRequest {
        method: parts.method.as_str().to_owned(),
        path: parts.uri.path().to_owned(),
        query: parts.uri.query().unwrap_or_default().to_owned(),
        headers: headers_of(&parts.headers),
        body: (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned()),
    });

    let query = parts.uri.query().unwrap_or_default();
    let matched = {
        let mut expectations = lock(&state.expectations);
        expectations
            .iter()
            .position(|expected| expected.matches(parts.method.as_str(), parts.uri.path(), query))
            .map(|index| expectations.remove(index).response)
    };

    match matched {
        Some(response) => respond(response),
        None => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header("content-type", "text/plain")
            .body(Body::from(no_expectation_message(
                &state,
                parts.method.as_str(),
                &parts.uri,
                query,
            )))
            .expect("a valid response"),
    }
}

/// The 500 body when nothing matched: the request, plus the nearest expectation's
/// unmet query pairs when its method and path are the ones that matched.
fn no_expectation_message(state: &MockState, method: &str, uri: &Uri, query: &str) -> String {
    let missing = lock(&state.expectations)
        .iter()
        .find(|expected| {
            expected.method.eq_ignore_ascii_case(method) && expected.path == uri.path()
        })
        .map(|expected| expected.missing_query_pairs(query))
        .filter(|missing| !missing.is_empty());

    match missing {
        Some(missing) => format!(
            "no expectation for {method} {uri} (missing query pair{} {})",
            if missing.len() == 1 { "" } else { "s" },
            missing.join(", ")
        ),
        None => format!("no expectation for {method} {uri}"),
    }
}

fn respond(response: MockResponse) -> Response {
    let mut builder = Response::builder().status(response.status);
    for (name, value) in &response.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }

    builder
        .body(Body::from(response.body))
        .expect("a valid response")
}

fn headers_of(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect()
}

/// A throwaway home directory for the end-to-end tests: a spawned process gets
/// `HOME` and `XDG_CONFIG_HOME` (or `USERPROFILE` and `APPDATA`) pointing into it,
/// so the CLI under test never reads or writes the developer's real config.
/// The directory is removed when the guard drops.
pub struct TempHome {
    path: PathBuf,
    vars: Mutex<Vec<(String, String)>>,
}

impl TempHome {
    pub fn new() -> TempHome {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ado-testkit-home-{}-{unique}", std::process::id()));
        let home = TempHome {
            path,
            vars: Mutex::new(Vec::new()),
        };

        fs::create_dir_all(home.config_dir()).expect("create the temp home");

        home
    }

    /// The home directory itself.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The directory the CLI under test resolves as its config directory, created
    /// by [`TempHome::new`].
    pub fn config_dir(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.path()
                .join("Library")
                .join("Application Support")
                .join("ado")
        } else if cfg!(target_os = "windows") {
            app_data_dir(self.path()).join("ado")
        } else {
            self.path().join("ado")
        }
    }

    /// An extra environment variable for the spawned process. Chainable.
    pub fn env(&self, key: &str, value: &str) -> &TempHome {
        lock(&self.vars).push((key.to_owned(), value.to_owned()));
        self
    }

    /// Points a spawned command at this home: the home and config variables, then
    /// anything [`env`](TempHome::env) recorded.
    pub fn apply(&self, command: &mut Command) {
        for (key, value) in self.variables() {
            command.env(key, value);
        }
    }

    fn variables(&self) -> Vec<(String, String)> {
        let mut variables = if cfg!(target_os = "windows") {
            vec![
                ("USERPROFILE".to_owned(), self.path.display().to_string()),
                (
                    "APPDATA".to_owned(),
                    app_data_dir(self.path()).display().to_string(),
                ),
            ]
        } else {
            vec![
                ("HOME".to_owned(), self.path.display().to_string()),
                (
                    "XDG_CONFIG_HOME".to_owned(),
                    self.path.display().to_string(),
                ),
            ]
        };

        variables.extend(lock(&self.vars).iter().cloned());

        variables
    }
}

impl Default for TempHome {
    fn default() -> TempHome {
        TempHome::new()
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn app_data_dir(home: &Path) -> PathBuf {
    home.join("AppData").join("Roaming")
}

/// The built `ado` binary, for the end-to-end tests. Cargo sets `CARGO_BIN_EXE_ado`
/// for the integration tests of the binary's own crate; otherwise the test
/// executable's own location gives the target directory.
pub fn ado_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_ado") {
        return PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("the running test executable");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }

    path.join(format!("ado{}", std::env::consts::EXE_SUFFIX))
}

/// The built `ado` binary as an [`assert_cmd::Command`]: the plain spawn for suites
/// that only read an exit code and the two output streams, with `CARGO_BIN_EXE_ado`
/// resolution and `assert_cmd`'s assertion ergonomics. Panics if the binary has not
/// been built.
pub fn ado() -> assert_cmd::Command {
    assert_cmd::Command::cargo_bin("ado").expect("the ado binary is built")
}

/// The built `ado` binary as a [`Command`] the caller configures: apply a
/// [`TempHome`], point `ADO_SERVER` at a [`MockServer`], add arguments, then run it
/// with `output()`. Use this instead of [`ado`] when the suite owns the process
/// environment.
///
/// It does **not** strip ambient `ADO_ORG`, `ADO_PAT` or `ADO_SERVER` — unlike
/// `cli_whoami.rs`'s local helper — so a mock-server or credential suite must
/// `env_remove` or override them itself before spawning.
pub fn ado_cmd() -> Command {
    Command::new(ado_bin())
}

/// A process's stdout decoded as UTF-8, lossily.
pub fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A process's stderr decoded as UTF-8, lossily.
pub fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    use serde_json::json;

    use super::*;

    /// A raw HTTP/1.1 GET, so the unit tests exercise the mock exactly as a client does.
    fn http_get(server: &MockServer, target: &str) -> (u16, String) {
        let address = server
            .base_url()
            .strip_prefix("http://")
            .expect("an http origin");
        let mut stream = TcpStream::connect(address).expect("connect to the mock server");
        write!(
            stream,
            "GET {target} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n"
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

    #[test]
    fn expect_matches_without_regard_to_the_query() {
        let server = MockServer::start();
        server.expect("GET", "/x", MockResponse::json(200, json!({"any": true})));

        let (status, body) = http_get(&server, "/x?$top=10&$skip=5");

        assert_eq!(status, 200);
        assert_eq!(body, r#"{"any":true}"#);
    }

    #[test]
    fn expect_query_matches_when_every_pair_is_present_in_any_order() {
        let server = MockServer::start();
        server.expect_query(
            "GET",
            "/_apis/projects",
            &[("$top", "10"), ("api-version", "7.1")],
            MockResponse::json(200, json!({"matched": "query"})),
        );

        let (status, body) = http_get(&server, "/_apis/projects?api-version=7.1&$top=10");

        assert_eq!(status, 200);
        assert_eq!(body, r#"{"matched":"query"}"#);
    }

    #[test]
    fn expect_query_ignores_pairs_it_does_not_require() {
        let server = MockServer::start();
        server.expect_query(
            "GET",
            "/_apis/projects",
            &[("$top", "10")],
            MockResponse::json(200, json!({"matched": "query"})),
        );

        let (status, body) = http_get(
            &server,
            "/_apis/projects?$top=10&$skip=50&stateFilter=wellFormed",
        );

        assert_eq!(status, 200);
        assert_eq!(body, r#"{"matched":"query"}"#);
    }

    /// The client's wire form is what the matcher sees: `$top` arrives as
    /// `%24top`, a space as `+`, and a decoded spelling never matches.
    #[test]
    fn expect_query_matches_the_wire_form_the_client_sends() {
        let server = MockServer::start();
        server.expect_query(
            "GET",
            "/_apis/projects",
            &[("%24top", "10"), ("search", "a+b%2Fc")],
            MockResponse::json(200, json!({"matched": "wire"})),
        );

        let (status, body) = http_get(&server, "/_apis/projects?%24top=10&search=a+b%2Fc");

        assert_eq!(status, 200);
        assert_eq!(body, r#"{"matched":"wire"}"#);

        let decoded = MockServer::start();
        decoded.expect_query(
            "GET",
            "/_apis/projects",
            &[("$top", "10")],
            MockResponse::json(200, json!({"matched": "decoded"})),
        );

        let (status, body) = http_get(&decoded, "/_apis/projects?%24top=10");

        assert_eq!(status, 500, "the decoded spelling must not match");
        assert!(body.contains("missing query pair $top=10"), "body: {body}");
    }

    #[test]
    fn expect_query_names_the_pair_the_request_is_missing() {
        let server = MockServer::start();
        server.expect_query(
            "GET",
            "/_apis/projects",
            &[("$top", "10")],
            MockResponse::json(200, json!({"matched": "query"})),
        );

        let (status, body) = http_get(&server, "/_apis/projects?$top=50");

        assert_eq!(status, 500);
        assert!(
            body.contains("GET /_apis/projects?$top=50"),
            "the message does not name the request: {body}"
        );
        assert!(
            body.contains("missing query pair $top=10"),
            "the message does not name the missing pair: {body}"
        );
    }

    #[test]
    fn expect_query_matches_any_occurrence_of_a_repeated_key() {
        let server = MockServer::start();
        server.expect_query(
            "GET",
            "/x",
            &[("tag", "b")],
            MockResponse::json(200, json!({"matched": "query"})),
        );

        let (status, body) = http_get(&server, "/x?tag=a&tag=b");

        assert_eq!(status, 200);
        assert_eq!(body, r#"{"matched":"query"}"#);
    }

    #[test]
    fn a_query_miss_falls_through_to_an_older_expectation() {
        let server = MockServer::start();
        server.expect(
            "GET",
            "/x",
            MockResponse::json(200, json!({"which": "any"})),
        );
        server.expect_query(
            "GET",
            "/x",
            &[("$top", "10")],
            MockResponse::json(200, json!({"which": "ten"})),
        );

        let (status, body) = http_get(&server, "/x?$top=5");

        assert_eq!(status, 200);
        assert_eq!(body, r#"{"which":"any"}"#);
    }

    #[test]
    fn query_pairs_preserves_repeated_keys_and_empty_values() {
        let server = MockServer::start();
        server.expect("GET", "/x", MockResponse::json(200, json!({})));

        let _ = http_get(&server, "/x?tag=a&tag=b&flag&empty=&a=b=c");

        let received = server.received();
        assert_eq!(received[0].query, "tag=a&tag=b&flag&empty=&a=b=c");
        assert_eq!(
            received[0].query_pairs(),
            vec![
                ("tag".to_owned(), "a".to_owned()),
                ("tag".to_owned(), "b".to_owned()),
                ("flag".to_owned(), String::new()),
                ("empty".to_owned(), String::new()),
                ("a".to_owned(), "b=c".to_owned()),
            ]
        );
    }

    #[test]
    fn query_pairs_is_empty_without_a_query() {
        let request = RecordedRequest {
            method: "GET".to_owned(),
            path: "/x".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: None,
        };

        assert_eq!(request.query_pairs(), Vec::new());
    }

    #[test]
    fn fixtures_load_and_are_json() {
        let projects = MockResponse::from_fixture("projects_list");
        assert_eq!(projects.status, 200);
        assert_eq!(
            projects.headers,
            vec![("content-type".to_owned(), "application/json".to_owned())]
        );
        let body: Value = serde_json::from_slice(&projects.body).expect("valid JSON");
        assert_eq!(body["value"].as_array().expect("a value array").len(), 2);

        let error = MockResponse::from_fixture("error_404").with_status(404);
        assert_eq!(error.status, 404);
        let body: Value = serde_json::from_slice(&error.body).expect("valid JSON");
        assert!(
            body["message"]
                .as_str()
                .expect("a message")
                .contains("TF400813"),
            "body: {}",
            String::from_utf8_lossy(&error.body)
        );
    }

    #[test]
    fn from_fixture_rejects_non_utf8_bytes() {
        let path = fixture_path("not-utf8.json");
        fs::write(&path, [0xff, 0xfe, 0x00]).expect("write a corrupt fixture");

        let panic = std::panic::catch_unwind(|| MockResponse::from_fixture("not-utf8"));

        fs::remove_file(&path).expect("remove the corrupt fixture");
        let message = panic.expect_err("a non-UTF-8 fixture panics");
        let message = message.downcast_ref::<String>().expect("the panic message");
        assert!(
            message.contains("is not UTF-8"),
            "the panic names the guarantee: {message}"
        );
    }

    #[test]
    fn a_bytes_fixture_keeps_its_bytes() {
        let zip = MockResponse::from_bytes_fixture("artifacts_download.zip");

        assert_eq!(zip.status, 200);
        assert_eq!(
            zip.headers,
            vec![(
                "content-type".to_owned(),
                "application/octet-stream".to_owned()
            )]
        );
        assert!(
            zip.body.starts_with(b"PK\x03\x04"),
            "a real zip fixture, not text: {:?}",
            zip.body
        );
        assert!(
            std::str::from_utf8(&zip.body).is_err(),
            "the fixture is not valid UTF-8, so only a byte-preserving body can serve it"
        );
    }

    #[test]
    fn config_dir_lives_under_the_temp_home() {
        let home = TempHome::new();
        let config_dir = home.config_dir();

        assert!(config_dir.starts_with(home.path()));
        assert!(config_dir.ends_with("ado"));
        assert!(config_dir.is_dir(), "new() creates the config directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_exports_the_home_config_dir_and_recorded_variables() {
        let home = TempHome::new();
        home.env("ADO_ORG", "myorg");
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "printf '%s|%s|%s' \"$HOME\" \"$XDG_CONFIG_HOME\" \"$ADO_ORG\"",
        ]);

        home.apply(&mut command);

        let output = command.output().expect("run sh");
        let path = home.path().display();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("{path}|{path}|myorg")
        );
    }
}
