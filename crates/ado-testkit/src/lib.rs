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
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::any;
use serde_json::Value;
use tokio::sync::oneshot;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// One response the mock is configured to send.
#[derive(Debug, Clone, PartialEq)]
pub struct MockResponse {
    pub status: u16,
    pub body: String,
    pub headers: Vec<(String, String)>,
}

impl MockResponse {
    /// A JSON body, with `content-type: application/json`.
    pub fn json(status: u16, body: Value) -> MockResponse {
        MockResponse {
            status,
            body: body.to_string(),
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        }
    }

    /// A captured response body from `fixtures/<name>.json`, sent with status 200;
    /// pair it with [`with_status`](MockResponse::with_status) for the error fixtures.
    pub fn from_fixture(name: &str) -> MockResponse {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(format!("{name}.json"));
        let body = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read fixture {}: {error}", path.display()));
        let value = serde_json::from_str(&body)
            .unwrap_or_else(|error| panic!("fixture {} is not JSON: {error}", path.display()));

        MockResponse::json(200, value)
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
}

/// A mock Azure DevOps server on an ephemeral `127.0.0.1` port, alive until dropped.
///
/// Expectations are single-shot and newest-first, like the Elixir
/// `AdoCli.TestServer`: the first request matching a method and path consumes it,
/// so a path can answer twice with two different responses.
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
    /// the match; [`received`](MockServer::received) is where it gets asserted.
    pub fn expect(&self, method: &str, path: &str, response: MockResponse) {
        lock(&self.state.expectations).insert(
            0,
            Expectation {
                method: method.to_owned(),
                path: path.to_owned(),
                response,
            },
        );
    }

    /// Everything the server received so far, in arrival order.
    pub fn received(&self) -> Vec<RecordedRequest> {
        lock(&self.state.received).clone()
    }
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
    response: MockResponse,
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

    let matched = {
        let mut expectations = lock(&state.expectations);
        expectations
            .iter()
            .position(|expected| {
                expected.method.eq_ignore_ascii_case(parts.method.as_str())
                    && expected.path == parts.uri.path()
            })
            .map(|index| expectations.remove(index).response)
    };

    match matched {
        Some(response) => respond(response),
        None => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header("content-type", "text/plain")
            .body(Body::from(format!(
                "no expectation for {} {}",
                parts.method, parts.uri
            )))
            .expect("a valid response"),
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_load_and_are_json() {
        let projects = MockResponse::from_fixture("projects_list");
        assert_eq!(projects.status, 200);
        assert_eq!(
            projects.headers,
            vec![("content-type".to_owned(), "application/json".to_owned())]
        );
        let body: Value = serde_json::from_str(&projects.body).expect("valid JSON");
        assert_eq!(body["value"].as_array().expect("a value array").len(), 2);

        let error = MockResponse::from_fixture("error_404").with_status(404);
        assert_eq!(error.status, 404);
        let body: Value = serde_json::from_str(&error.body).expect("valid JSON");
        assert!(
            body["message"]
                .as_str()
                .expect("a message")
                .contains("TF400813"),
            "body: {}",
            error.body
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
