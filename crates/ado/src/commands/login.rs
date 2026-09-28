//! `ado login` — the PAT method and the device-code flow, ported from
//! `lib/ado_cli/cli/auth_commands.ex` (`login/1`, `resolve_method/1`,
//! `login_with_pat/3`, `login_with_device/2`, `login_success/4`) and
//! `lib/ado_cli/auth.ex` (`login_pat/2`, `request_device_code/1`,
//! `poll_for_token/4`, `exchange_and_save_device/2`).
//!
//! Both methods ship in Wave 1 (`browser` is Wave 3, and an unsupported method is a
//! validation error rather than a silent no-op). The storage is §7's clean slate:
//! the token goes to the credential store — the OS keychain, with
//! `credentials.json` behind it — and `config.toml` records the organization and the
//! method, never the token. That is the one deliberate difference from the oracle in
//! the success output: `credentials_saved_to` names the config file where the oracle
//! names the JSON file its own token lives in.

use std::io::Write;

use ado_core::auth::device_code::{self, DeviceCode};
use ado_core::config::AuthMethod;
use ado_core::env::{ENV_ORG, ENV_PAT, EnvSource};
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::json;

use crate::context::Context;
use crate::output::{Report, WriteFailure, write_bytes};

/// The `--method` values this build accepts. The oracle's list is these two plus
/// `browser`, which Wave 3 adds; the validation messages name what ships.
const METHODS: [&str; 2] = ["pat", "device"];

/// `ado login`.
///
/// `identity_base` is the identity origin the device flow talks to —
/// [`device_code::IDENTITY_BASE`] in production, a local fake in the tests, because
/// the flow's endpoints are otherwise not redirectable and this build adds no
/// environment override for them. `announce` receives the device code and its URL
/// after the code is requested and before the first poll, which is the only moment
/// the user can read them.
pub fn run(
    context: &mut Context,
    method: Option<&str>,
    identity_base: &str,
    announce: &mut dyn Write,
) -> Result<Report, AdoError> {
    let (org, pat) = {
        let env = context.env();
        (env.get(ENV_ORG), env.get(ENV_PAT))
    };
    let server = context.server();

    match resolve_method(method, pat.as_deref())? {
        LoginMethod::Pat => login_with_pat(context, org, pat, server.as_deref()),
        LoginMethod::Device => {
            login_with_device(context, org, identity_base, server.as_deref(), announce)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginMethod {
    Pat,
    Device,
}

/// `resolve_method/1`: an explicit `--method` wins; otherwise a PAT on `--pat` or
/// `ADO_PAT` means `pat`, and anything else is the browser flow the oracle would have
/// run — which this build does not ship.
fn resolve_method(method: Option<&str>, pat: Option<&str>) -> Result<LoginMethod, AdoError> {
    match method {
        Some("pat") => Ok(LoginMethod::Pat),
        Some("device") => Ok(LoginMethod::Device),
        Some("browser") => Err(browser_not_shipped()),
        Some(other) => Err(unknown_method(other)),
        None if pat.is_some() => Ok(LoginMethod::Pat),
        None => Err(browser_not_shipped()),
    }
}

/// `login_with_pat/3`: store the token, record the organization and method, report.
fn login_with_pat(
    context: &mut Context,
    org: Option<String>,
    pat: Option<String>,
    server: Option<&str>,
) -> Result<Report, AdoError> {
    let org = org.ok_or_else(|| org_required("pat"))?;
    let pat = pat.ok_or_else(pat_required)?;

    context
        .save_login(&org, AuthMethod::Pat, &pat)
        .map_err(login_failed)?;

    Ok(success(context, &org, "pat", server))
}

/// `login_with_device/2` → `login_device_code/1` → `exchange_and_save_device/2`:
/// request the code, show it, poll for the ARM refresh token, exchange that for a
/// **DevOps** access token, and store the DevOps one.
fn login_with_device(
    context: &mut Context,
    org: Option<String>,
    identity_base: &str,
    server: Option<&str>,
    announce: &mut dyn Write,
) -> Result<Report, AdoError> {
    // D26: the oracle's guard exempts `device` from the `--org` requirement and then
    // writes a credential keyed by nothing (`org: nil`); our store and
    // `config.toml` are per-organization, so the org is required before the flow
    // starts — a dead end refused rather than ported.
    let org = org.ok_or_else(|| org_required("device"))?;
    let device = device_code::request(identity_base).map_err(login_failed)?;

    if let Err(failure) = announce_device_code(context, &device, announce) {
        return match failure {
            // A closed stdout cannot show the code, so the flow stops before polling:
            // a silent success, like every other pipe failure (spec §6.3, R19), and
            // no minutes of polling for a caller that is not reading.
            WriteFailure::BrokenPipe => Ok(Report::Text(String::new())),
            WriteFailure::Other(message) => Err(AdoError::validation(message)),
        };
    }

    let refresh = device_code::poll(identity_base, &device).map_err(login_failed)?;
    let token = device_code::exchange(identity_base, refresh.as_deref()).map_err(login_failed)?;

    context
        .save_login(&org, AuthMethod::Device, &token)
        .map_err(login_failed)?;

    Ok(success(context, &org, "device", server))
}

/// The module's device-code instructions: one `{"ok":true,"message":…}` line under
/// `--json`, so an agent can read the code and the URL from stdout, and the ported
/// block on the human path (the oracle's colour is dropped, spec §8). The oracle
/// writes that block even under `--json`, leaving its stdout mixed prose and JSON;
/// ours keeps the JSON path parseable.
fn announce_device_code(
    context: &Context,
    device: &DeviceCode,
    out: &mut dyn Write,
) -> Result<(), WriteFailure> {
    let text = if context.json() {
        let message = format!(
            "To sign in, use a web browser to open: {} and enter the code: {}",
            device.verification_uri, device.user_code
        );
        let envelope = serde_json::to_string(&ok_message(&message))
            .map_err(|error| WriteFailure::Other(error.to_string()))?;

        format!("{envelope}\n")
    } else {
        format!(
            "\nTo sign in, use a web browser to open:\n  {}\n\nAnd enter the code: {}\n\n",
            device.verification_uri, device.user_code
        )
    };

    write_bytes(out, text.as_bytes())
}

/// `login_success/4`: the oracle's value envelope — `org`, `method`, `server` and
/// `credentials_saved_to` — and its two human lines. `credentials_saved_to` names
/// this CLI's config file, where the oracle names the JSON file that holds its token
/// (§7, D11b's file); every other part of the output is the oracle's bytes.
fn success(context: &Context, org: &str, method: &str, server: Option<&str>) -> Report {
    let saved_to = context
        .config_file()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "(not set)".to_owned());
    let server_label = match server {
        Some(server) => format!(" ({server})"),
        None => String::new(),
    };
    let capitalized = capitalize(method);

    context.json_or_report(
        ok_value(json!({
            "org": org,
            "method": method,
            "server": server,
            "credentials_saved_to": saved_to,
        })),
        || {
            Report::Text(format!(
                "\n  Logged in to {org}{server_label} via {capitalized}.\n  Credentials saved to {saved_to}"
            ))
        },
    )
}

/// `String.capitalize/1` for the two lowercase method names.
fn capitalize(method: &str) -> String {
    let mut characters = method.chars();

    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

/// The module's `--org is required for method='<method>' (or set ADO_ORG env var)`
/// and its `details` pair, captured byte-for-byte for `pat` and reused for `device`,
/// whose org the oracle's guard exempted (D26).
fn org_required(method: &str) -> AdoError {
    AdoError {
        code: ErrorCode::ValidationError,
        status: None,
        message: format!("--org is required for method='{method}' (or set ADO_ORG env var)"),
        details: Some(json!({"option": "--org", "env_var": "ADO_ORG"})),
    }
}

/// The module's `--pat is required for method=pat (or set ADO_PAT env var)`.
fn pat_required() -> AdoError {
    AdoError {
        code: ErrorCode::ValidationError,
        status: None,
        message: "--pat is required for method=pat (or set ADO_PAT env var)".to_owned(),
        details: Some(json!({"option": "--pat", "env_var": "ADO_PAT"})),
    }
}

/// `dispatch_login/4`'s fallback. The oracle suggests `browser` as well; this build
/// ships `pat` and `device`, so the message and `valid_methods` name those.
fn unknown_method(method: &str) -> AdoError {
    AdoError {
        code: ErrorCode::ValidationError,
        status: None,
        message: format!("Unknown method '{method}'. Use 'pat' or 'device'."),
        details: Some(json!({"valid_methods": METHODS})),
    }
}

/// Wave 3's browser flow — and what the oracle would have run for `--method browser`
/// or for no `--method` and no PAT. It is a validation error that says what ships,
/// never a silent no-op.
fn browser_not_shipped() -> AdoError {
    AdoError {
        code: ErrorCode::ValidationError,
        status: None,
        message:
            "Login method 'browser' is not available in this build. Use '--method pat' or '--method device'."
                .to_owned(),
        details: Some(json!({"valid_methods": METHODS})),
    }
}

/// The module's `Output.error(parsed, "auth_required", "Login failed: #{reason}")`:
/// the message is prefixed and `details.reason` carries the underlying one, which the
/// oracle comparison treats as opaque (D12). The code stays the failure's own, so a
/// transport failure remains a `network_error` where the oracle's one error branch
/// called everything `auth_required`.
fn login_failed(error: AdoError) -> AdoError {
    let message = format!("Login failed: {}", error.message);

    AdoError {
        code: error.code,
        status: error.status,
        message,
        details: Some(json!({ "reason": error.message })),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::{self, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use ado_core::auth::device_code::{ARM_RESOURCE, CLIENT_ID, DEVOPS_RESOURCE};
    use ado_core::config::{Config, OrgEntry};
    use ado_core::credentials::{InMemoryStore, SecretStore, Stored};
    use ado_core::env::{ENV_SERVER, MapEnv};
    use ado_testkit::{MockResponse, MockServer, TempHome};
    use serde_json::{Value, json};

    use super::*;
    use crate::args::GlobalOpts;

    fn opts(org: Option<&str>, pat: Option<&str>, server: Option<&str>, json: bool) -> GlobalOpts {
        GlobalOpts {
            org: org.map(str::to_owned),
            pat: pat.map(str::to_owned),
            server: server.map(str::to_owned),
            verbose: false,
            json,
        }
    }

    /// A context whose environment, store and config path the test owns: no test
    /// reads the process environment, the OS keychain or the developer's files.
    fn test_context(
        opts: GlobalOpts,
        env: MapEnv,
        store: impl SecretStore + 'static,
        home: &TempHome,
    ) -> Context {
        Context::for_test(opts, env, store, home)
    }

    fn org_env(org: &str) -> MapEnv {
        MapEnv::new().set(ENV_ORG, org)
    }

    fn config_file(home: &TempHome) -> String {
        home.config_dir()
            .join(ado_core::config::CONFIG_FILE)
            .display()
            .to_string()
    }

    fn config_text(home: &TempHome) -> String {
        std::fs::read_to_string(home.config_dir().join(ado_core::config::CONFIG_FILE))
            .expect("config.toml")
    }

    fn stored(method: AuthMethod, token: &str) -> Stored {
        Stored {
            method,
            token: token.to_owned(),
        }
    }

    // ── the local identity server ─────────────────────────────────────────

    /// One scripted answer.
    #[derive(Clone, Debug)]
    struct Reply {
        status: u16,
        body: Value,
    }

    fn reply(status: u16, body: Value) -> Reply {
        Reply { status, body }
    }

    /// One request the fake received, with the form body it carried and the moment it
    /// arrived — the timestamps are what the polling-interval assertions read.
    #[derive(Clone, Debug)]
    struct Received {
        path: String,
        form: Vec<(String, String)>,
        at: Instant,
    }

    impl Received {
        fn form_value(&self, key: &str) -> Option<&str> {
            self.form
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        }

        fn grant(&self) -> Option<&str> {
            self.form_value("grant_type")
        }

        /// How long after `earlier` this request arrived.
        fn after(&self, earlier: &Received) -> Duration {
            self.at.duration_since(earlier.at)
        }
    }

    /// A local identity server: a plain `TcpListener`, because the ADO mock in
    /// `ado-testkit` models Azure DevOps and the shipped binary's identity endpoints
    /// are the real Microsoft ones — a test must never reach them.
    struct FakeIdentity {
        base_url: String,
        state: Arc<Mutex<FakeState>>,
        shutdown: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    struct FakeState {
        replies: VecDeque<Reply>,
        default: Option<Reply>,
        requests: Vec<Received>,
    }

    impl FakeIdentity {
        /// Serves `replies` in order; every request after the script runs out gets
        /// `default`, or a 500 when there is none.
        fn start(replies: Vec<Reply>, default: Option<Reply>) -> FakeIdentity {
            let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
            listener
                .set_nonblocking(true)
                .expect("a non-blocking listener");
            let address = listener.local_addr().expect("the bound address");
            let state = Arc::new(Mutex::new(FakeState {
                replies: replies.into(),
                default,
                requests: Vec::new(),
            }));
            let shutdown = Arc::new(AtomicBool::new(false));
            let thread_state = state.clone();
            let thread_shutdown = shutdown.clone();

            let thread = thread::spawn(move || {
                while !thread_shutdown.load(Ordering::Relaxed) {
                    let mut stream = match listener.accept() {
                        Ok((stream, _)) => stream,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(_) => break,
                    };
                    // A socket accepted from a non-blocking listener inherits the
                    // flag on macOS, so the accepted stream is put back in blocking
                    // mode before the request is read.
                    stream.set_nonblocking(false).expect("a blocking stream");
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .expect("a read timeout");

                    let at = Instant::now();
                    let Some((path, form)) = read_request(&mut stream) else {
                        continue;
                    };

                    let answer = {
                        let mut state = lock(&thread_state);
                        state.requests.push(Received { path, form, at });
                        state.replies.pop_front().or_else(|| state.default.clone())
                    };

                    match answer {
                        Some(reply) => respond(&mut stream, &reply),
                        None => respond(
                            &mut stream,
                            &reply(500, json!({"error": "no scripted reply"})),
                        ),
                    }
                }
            });

            FakeIdentity {
                base_url: format!("http://{address}"),
                state,
                shutdown,
                thread: Some(thread),
            }
        }

        fn base_url(&self) -> &str {
            &self.base_url
        }

        fn received(&self) -> Vec<Received> {
            lock(&self.state).requests.clone()
        }

        /// The token-endpoint polls: the device-code request and the exchange are the
        /// other two paths.
        fn polls(&self) -> Vec<Received> {
            self.received()
                .into_iter()
                .filter(|request| {
                    request.path.ends_with("/oauth2/token")
                        && request.grant() != Some("refresh_token")
                })
                .collect()
        }
    }

    impl Drop for FakeIdentity {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// One HTTP/1.1 request: its path and its form body decoded. `ureq` sends
    /// `content-length`, so the body is read exactly.
    fn read_request(stream: &mut TcpStream) -> Option<(String, Vec<(String, String)>)> {
        let mut buffer = Vec::new();
        let header_end = loop {
            if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
            if !read_more(stream, &mut buffer)? {
                return None;
            }
        };

        let head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
        let mut content_length = 0;
        for line in head.lines() {
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }

        while buffer.len() < header_end + content_length {
            if !read_more(stream, &mut buffer)? {
                break;
            }
        }

        let path = head.lines().next()?.split_whitespace().nth(1)?.to_owned();
        let end = (header_end + content_length).min(buffer.len());
        let body = String::from_utf8_lossy(&buffer[header_end..end]).into_owned();

        Some((path, parse_form(&body)))
    }

    fn read_more(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> Option<bool> {
        let mut chunk = [0_u8; 512];

        match stream.read(&mut chunk) {
            Ok(0) => Some(false),
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                Some(true)
            }
            Err(_) => None,
        }
    }

    fn parse_form(body: &str) -> Vec<(String, String)> {
        body.split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| match pair.split_once('=') {
                Some((key, value)) => (decode_component(key), decode_component(value)),
                None => (decode_component(pair), String::new()),
            })
            .collect()
    }

    /// The inverse of the client's form encoding, so the pairs the tests assert are
    /// the values the flow sent.
    fn decode_component(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut decoded = Vec::with_capacity(bytes.len());
        let mut index = 0;

        while index < bytes.len() {
            match bytes[index] {
                b'+' => {
                    decoded.push(b' ');
                    index += 1;
                }
                b'%' if index + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                        .ok()
                        .and_then(|hex| u8::from_str_radix(hex, 16).ok());

                    match hex {
                        Some(byte) => {
                            decoded.push(byte);
                            index += 3;
                        }
                        None => {
                            decoded.push(bytes[index]);
                            index += 1;
                        }
                    }
                }
                byte => {
                    decoded.push(byte);
                    index += 1;
                }
            }
        }

        String::from_utf8_lossy(&decoded).into_owned()
    }

    fn respond(stream: &mut TcpStream, reply: &Reply) {
        let body = reply.body.to_string();
        let reason = if reply.status == 200 { "OK" } else { "Error" };
        let head = format!(
            "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {length}\r\nconnection: close\r\n\r\n",
            status = reply.status,
            length = body.len(),
        );

        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body.as_bytes());
        let _ = stream.flush();
    }

    /// The device-code answer: both verification spellings, with different values so
    /// the test proves which one the flow reads (`verification_url`, the oracle's),
    /// and the interval the server asks for.
    fn device_code_reply(interval: u64) -> Reply {
        reply(
            200,
            json!({
                "device_code": "DEVICE-CODE",
                "user_code": "USER-CODE",
                "verification_url": "https://microsoft.com/devicelogin",
                "verification_uri": "https://microsoft.com/devicelogin-rfc",
                "interval": interval,
            }),
        )
    }

    fn pending() -> Reply {
        reply(400, json!({"error": "authorization_pending"}))
    }

    fn slow_down() -> Reply {
        reply(400, json!({"error": "slow_down"}))
    }

    fn granted() -> Reply {
        reply(
            200,
            json!({"access_token": "arm-token", "refresh_token": "refresh-token"}),
        )
    }

    fn granted_without_refresh() -> Reply {
        reply(200, json!({"access_token": "arm-token"}))
    }

    fn devops_token(token: &str) -> Reply {
        reply(200, json!({"access_token": token}))
    }

    /// A writer whose every write fails the way a closed pipe does.
    struct ClosedStdout;

    impl Write for ClosedStdout {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    // ── the PAT method ────────────────────────────────────────────────────

    #[test]
    fn a_pat_login_records_the_org_and_method_and_emits_the_oracle_envelope() {
        let home = TempHome::new();
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(Some("myorg"), Some("pat-token"), None, true),
            MapEnv::new(),
            store.clone(),
            &home,
        );

        let report = run(
            &mut context,
            Some("pat"),
            device_code::IDENTITY_BASE,
            &mut Vec::new(),
        )
        .expect("the pat login");

        assert_eq!(
            report,
            Report::Json(json!({
                "ok": true,
                "result": {
                    "org": "myorg",
                    "method": "pat",
                    "server": null,
                    "credentials_saved_to": config_file(&home),
                }
            })),
            "the oracle's value envelope, with this CLI's config file"
        );
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Pat, "pat-token"))
        );
        assert_eq!(
            config_text(&home),
            "default_org = \"myorg\"\n\n[orgs.myorg]\nauth = \"pat\"\n"
        );
        assert!(
            !config_text(&home).contains("pat-token"),
            "the token reached config.toml"
        );
    }

    #[test]
    fn a_pat_login_keeps_the_settings_it_did_not_write() {
        let home = TempHome::new();
        let store = InMemoryStore::new();
        let config = Config {
            server: Some("https://ado.example.com".to_owned()),
            orgs: std::collections::BTreeMap::from([(
                "other".to_owned(),
                OrgEntry {
                    auth: AuthMethod::Browser,
                },
            )]),
            ..Config::default()
        };
        let mut context = test_context(
            opts(Some("myorg"), Some("pat-token"), None, true),
            MapEnv::new(),
            store.clone(),
            &home,
        )
        .with_config(config);

        run(
            &mut context,
            Some("pat"),
            device_code::IDENTITY_BASE,
            &mut Vec::new(),
        )
        .expect("the pat login");

        let written = config_text(&home);
        assert!(written.contains("server = \"https://ado.example.com\""));
        assert!(written.contains("[orgs.other]\nauth = \"browser\""));
        assert!(written.contains("default_org = \"myorg\""));
    }

    /// §6.6: `--pat` outranks `ADO_PAT`, and both outrank nothing else.
    #[test]
    fn the_flag_pat_beats_the_environment_pat() {
        let home = TempHome::new();
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(Some("myorg"), Some("flag-pat"), None, true),
            MapEnv::new().set(ENV_PAT, "env-pat"),
            store.clone(),
            &home,
        );

        run(
            &mut context,
            Some("pat"),
            device_code::IDENTITY_BASE,
            &mut Vec::new(),
        )
        .expect("the pat login");

        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Pat, "flag-pat"))
        );
    }

    /// `login` resolves its options the way the module does — `--org` or `ADO_ORG` —
    /// so a stored `default_org` does not stand in for the flag.
    #[test]
    fn the_config_default_org_does_not_satisfy_the_org_requirement() {
        let home = TempHome::new();
        let config = Config {
            default_org: Some("myorg".to_owned()),
            ..Config::default()
        };
        let mut context = test_context(
            opts(None, Some("pat-token"), None, true),
            MapEnv::new(),
            InMemoryStore::new(),
            &home,
        )
        .with_config(config);

        let error = run(
            &mut context,
            Some("pat"),
            device_code::IDENTITY_BASE,
            &mut Vec::new(),
        )
        .expect_err("the org comes from the flag or the environment");

        assert_eq!(error.code, ErrorCode::ValidationError);
        assert_eq!(
            error.details,
            Some(json!({"option": "--org", "env_var": "ADO_ORG"}))
        );
        assert!(
            !home
                .config_dir()
                .join(ado_core::config::CONFIG_FILE)
                .exists(),
            "a validation error writes nothing"
        );
    }

    /// A store that cannot answer is the oracle's own failure mode (it could not
    /// write its config): "Login failed: …", with the code the store classified, and
    /// no config file — the login is not recorded without a credential behind it.
    #[test]
    fn a_store_failure_is_reported_and_leaves_no_config() {
        let home = TempHome::new();
        let store = InMemoryStore::unavailable();
        let mut context = test_context(
            opts(Some("myorg"), Some("pat-token"), None, true),
            MapEnv::new(),
            store.clone(),
            &home,
        );

        let error = run(
            &mut context,
            Some("pat"),
            device_code::IDENTITY_BASE,
            &mut Vec::new(),
        )
        .expect_err("the store is unavailable");

        assert_eq!(error.code, ErrorCode::ValidationError);
        assert_eq!(error.message, "Login failed: Credential store unavailable.");
        assert_eq!(
            error.details,
            Some(json!({"reason": "Credential store unavailable."}))
        );
        assert!(
            !home
                .config_dir()
                .join(ado_core::config::CONFIG_FILE)
                .exists()
        );
    }

    /// What the login was for: the next command's client resolves the organization
    /// from the config the login wrote and the token from the store (the T2 gap).
    #[test]
    fn the_client_resolves_the_login_it_stored() {
        let home = TempHome::new();
        let server = MockServer::start();
        server.expect(
            "GET",
            "/myorg/_apis/projects",
            MockResponse::json(200, json!({"value": []})),
        );
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(Some("myorg"), Some("pat-token"), None, true),
            MapEnv::new().set(ENV_SERVER, server.base_url()),
            store.clone(),
            &home,
        );

        run(
            &mut context,
            Some("pat"),
            device_code::IDENTITY_BASE,
            &mut Vec::new(),
        )
        .expect("the pat login");

        // A fresh context, like the next process: no flags and no `ADO_ORG`, so the
        // organization has to come from the config the login wrote (the file's own
        // contents are pinned above; the binary suite drives this same chain through
        // two processes, with `config.toml` loaded from disk).
        let mut next = test_context(
            opts(None, None, None, true),
            MapEnv::new().set(ENV_SERVER, server.base_url()),
            store,
            &home,
        )
        .with_config(Config {
            default_org: Some("myorg".to_owned()),
            orgs: std::collections::BTreeMap::from([(
                "myorg".to_owned(),
                OrgEntry {
                    auth: AuthMethod::Pat,
                },
            )]),
            ..Config::default()
        });
        next.client()
            .expect("the stored login resolves")
            .get("/_apis/projects", &[])
            .expect("the mock answers");

        let received = server.received();
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].path, "/myorg/_apis/projects",
            "the organization comes from config.toml's default_org"
        );
        assert_eq!(
            received[0].header("authorization"),
            Some("Basic OnBhdC10b2tlbg=="),
            "Basic base64(':pat-token') — the token comes from the store"
        );
    }

    #[test]
    fn the_server_is_resolved_but_not_recorded() {
        let home = TempHome::new();
        let mut context = test_context(
            opts(
                Some("myorg"),
                Some("pat-token"),
                Some("https://flag.test"),
                true,
            ),
            MapEnv::new().set(ENV_SERVER, "https://env.test"),
            InMemoryStore::new(),
            &home,
        );

        let report = run(
            &mut context,
            Some("pat"),
            device_code::IDENTITY_BASE,
            &mut Vec::new(),
        )
        .expect("the pat login");

        let Report::Json(envelope) = report else {
            panic!("the json context reports an envelope");
        };
        assert_eq!(
            envelope["result"]["server"],
            json!("https://flag.test"),
            "the flag server outranks ADO_SERVER, as the oracle's set_server/1 does"
        );
        assert!(
            !config_text(&home).contains("server"),
            "the oracle does not persist the server either (W1-R10: it stays whoami's field)"
        );
    }

    #[test]
    fn the_pat_human_lines_are_the_oracles() {
        let home = TempHome::new();
        let mut context = test_context(
            opts(
                Some("myorg"),
                Some("pat-token"),
                Some("https://ado.test"),
                false,
            ),
            MapEnv::new(),
            InMemoryStore::new(),
            &home,
        );

        let report = run(
            &mut context,
            Some("pat"),
            device_code::IDENTITY_BASE,
            &mut Vec::new(),
        )
        .expect("the pat login");

        assert_eq!(
            report,
            Report::Text(format!(
                "\n  Logged in to myorg (https://ado.test) via Pat.\n  Credentials saved to {}",
                config_file(&home)
            ))
        );
    }

    // ── the device-code flow ──────────────────────────────────────────────

    /// The happy path, end to end: the code the server returns is announced before
    /// the first poll; the polling waits the server's interval between attempts; and
    /// what lands in the store is the token the *exchange* returned, not the ARM one.
    #[test]
    fn the_device_flow_polls_at_the_servers_interval_and_stores_the_devops_token() {
        let home = TempHome::new();
        let fake = FakeIdentity::start(
            vec![
                device_code_reply(1),
                pending(),
                pending(),
                granted(),
                devops_token("devops-token"),
            ],
            None,
        );
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );
        let mut announced = Vec::new();

        let report = run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut announced,
        )
        .expect("the device login");

        let announced = String::from_utf8(announced).expect("utf-8");
        let message: Value = serde_json::from_str(announced.trim_end()).expect("one JSON line");
        assert_eq!(message["ok"], json!(true));
        let message = message["message"].as_str().expect("a message");
        assert!(message.contains("USER-CODE"), "message: {message}");
        assert!(
            message.contains("https://microsoft.com/devicelogin"),
            "the oracle's `verification_url`, not the RFC fallback: {message}"
        );

        assert_eq!(
            report,
            Report::Json(json!({
                "ok": true,
                "result": {
                    "org": "myorg",
                    "method": "device",
                    "server": null,
                    "credentials_saved_to": config_file(&home),
                }
            }))
        );
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Device, "devops-token")),
            "the DevOps token is stored, not the ARM token or the refresh token"
        );
        assert_eq!(
            config_text(&home),
            "default_org = \"myorg\"\n\n[orgs.myorg]\nauth = \"device\"\n"
        );

        // The server's `interval` is honoured between polls: `pending` waits it, and
        // the first poll follows the device-code request immediately, as the oracle's
        // `poll_for_token/4` does.
        let polls = fake.polls();
        assert_eq!(polls.len(), 3, "two pendings and the grant");
        for window in polls.windows(2) {
            let waited = window[1].after(&window[0]);
            assert!(
                waited >= Duration::from_millis(950),
                "the server's 1s interval was not honoured: {waited:?}"
            );
        }

        let received = fake.received();
        assert_eq!(received[0].path, "/organizations/oauth2/devicecode");
        assert_eq!(received[0].form_value("client_id"), Some(CLIENT_ID));
        assert_eq!(
            received[0].form_value("resource"),
            Some(ARM_RESOURCE),
            "the device code is requested for ARM, which accepts MSAs"
        );
        assert_eq!(received[1].path, "/organizations/oauth2/token");
        assert_eq!(
            received[1].form_value("grant_type"),
            Some("urn:ietf:params:oauth:grant-type:device_code")
        );
        assert_eq!(received[1].form_value("device_code"), Some("DEVICE-CODE"));
        assert_eq!(
            received.last().expect("the exchange").path,
            "/organizations/oauth2/token"
        );
        assert_eq!(
            received
                .last()
                .expect("the exchange")
                .form_value("resource"),
            Some(DEVOPS_RESOURCE),
            "the exchange asks for the DevOps resource"
        );
        assert_eq!(
            received
                .last()
                .expect("the exchange")
                .form_value("refresh_token"),
            Some("refresh-token")
        );
    }

    #[test]
    fn the_device_human_path_prints_the_code_and_the_url() {
        let home = TempHome::new();
        let fake = FakeIdentity::start(
            vec![
                device_code_reply(0),
                granted(),
                devops_token("devops-token"),
            ],
            None,
        );
        let mut context = test_context(
            opts(None, None, None, false),
            org_env("myorg"),
            InMemoryStore::new(),
            &home,
        );
        let mut announced = Vec::new();

        let report = run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut announced,
        )
        .expect("the device login");

        assert_eq!(
            String::from_utf8(announced).expect("utf-8"),
            "\nTo sign in, use a web browser to open:\n  https://microsoft.com/devicelogin\n\nAnd enter the code: USER-CODE\n\n"
        );
        assert_eq!(
            report,
            Report::Text(format!(
                "\n  Logged in to myorg via Device.\n  Credentials saved to {}",
                config_file(&home)
            ))
        );
    }

    /// `slow_down` is not terminal: the flow keeps going, after waiting the oracle's
    /// `interval + 5` seconds rather than the server's interval.
    #[test]
    fn slow_down_waits_the_increment_and_keeps_going() {
        let home = TempHome::new();
        let fake = FakeIdentity::start(
            vec![
                device_code_reply(0),
                slow_down(),
                granted(),
                devops_token("devops-token"),
            ],
            None,
        );
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );

        run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut Vec::new(),
        )
        .expect("slow_down is not terminal");

        let polls = fake.polls();
        assert_eq!(polls.len(), 2);
        let waited = polls[1].after(&polls[0]);
        assert!(
            waited >= Duration::from_millis(4900),
            "slow_down must wait the server's interval plus the oracle's five seconds: {waited:?}"
        );
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Device, "devops-token"))
        );
    }

    /// A terminal error is terminal: one poll, a stable code, nothing stored.
    #[test]
    fn a_terminal_device_error_stores_nothing() {
        let home = TempHome::new();
        let fake = FakeIdentity::start(
            vec![
                device_code_reply(0),
                reply(400, json!({"error": "expired_token"})),
            ],
            Some(granted()),
        );
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );

        let error = run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut Vec::new(),
        )
        .expect_err("the code expired");

        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert_eq!(
            error.message,
            "Login failed: Device code expired. Please try again."
        );
        assert_eq!(fake.polls().len(), 1, "a terminal error is not retried");
        assert!(store.calls().is_empty(), "calls: {:?}", store.calls());
        assert!(
            !home
                .config_dir()
                .join(ado_core::config::CONFIG_FILE)
                .exists()
        );
    }

    /// Both spellings of "the user said no" are terminal (RFC 8628 says
    /// `access_denied`; the identity endpoint says `authorization_declined`).
    #[test]
    fn a_declined_authorization_is_terminal() {
        for declined in ["authorization_declined", "access_denied"] {
            let home = TempHome::new();
            let fake = FakeIdentity::start(
                vec![device_code_reply(0), reply(400, json!({"error": declined}))],
                None,
            );
            let store = InMemoryStore::new();
            let mut context = test_context(
                opts(None, None, None, true),
                org_env("myorg"),
                store.clone(),
                &home,
            );

            let error = run(
                &mut context,
                Some("device"),
                fake.base_url(),
                &mut Vec::new(),
            )
            .expect_err(declined);

            assert_eq!(error.code, ErrorCode::AuthRequired, "{declined}");
            assert_eq!(
                error.message, "Login failed: Authorization declined by user.",
                "{declined}"
            );
            assert_eq!(fake.polls().len(), 1, "{declined}");
            assert!(store.calls().is_empty(), "{declined}");
        }
    }

    /// A server that never grants cannot make the run loop forever: the oracle's
    /// `attempts > 120` guard is the flow's ceiling.
    #[test]
    fn an_unhelpful_server_does_not_poll_forever() {
        let home = TempHome::new();
        let fake = FakeIdentity::start(vec![device_code_reply(0)], Some(pending()));
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );

        let error = run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut Vec::new(),
        )
        .expect_err("the server never grants");

        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert_eq!(
            error.message,
            "Login failed: Authentication timed out. Please try again."
        );
        assert_eq!(
            fake.polls().len(),
            121,
            "the oracle polls at most 121 times (`attempts > 120`)"
        );
        assert!(store.calls().is_empty());
    }

    /// The ARM→DevOps exchange tries `organizations` and falls back to `consumers`,
    /// and the token it returns from the fallback is the one that is stored.
    #[test]
    fn the_exchange_falls_back_to_the_msa_tenant() {
        let home = TempHome::new();
        let fake = FakeIdentity::start(
            vec![
                device_code_reply(0),
                granted(),
                reply(400, json!({"error": "invalid_grant"})),
                devops_token("devops-token"),
            ],
            None,
        );
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );

        run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut Vec::new(),
        )
        .expect("the consumers fallback answers");

        let exchanges = fake
            .received()
            .into_iter()
            .filter(|request| request.grant() == Some("refresh_token"))
            .collect::<Vec<_>>();
        assert_eq!(
            exchanges
                .iter()
                .map(|request| request.path.as_str())
                .collect::<Vec<_>>(),
            ["/organizations/oauth2/token", "/consumers/oauth2/token"]
        );
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Device, "devops-token"))
        );
    }

    /// A grant with no refresh token cannot be exchanged; the oracle has a message
    /// for exactly this case.
    #[test]
    fn a_grant_without_a_refresh_token_is_terminal() {
        let home = TempHome::new();
        let fake = FakeIdentity::start(vec![device_code_reply(0), granted_without_refresh()], None);
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );

        let error = run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut Vec::new(),
        )
        .expect_err("no refresh token");

        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert_eq!(
            error.message,
            "Login failed: No refresh token available for DevOps exchange"
        );
        assert_eq!(
            fake.received().len(),
            2,
            "no exchange is attempted without a refresh token"
        );
        assert!(store.calls().is_empty());
    }

    /// A device-code request the identity server refuses, and a 200 it answers
    /// incompletely: both are stable errors carrying the server's own words.
    #[test]
    fn a_bad_device_code_response_is_a_stable_error() {
        let refused = FakeIdentity::start(
            vec![reply(
                400,
                json!({"error": "invalid_request", "error_description": "AADSTS50000: nope"}),
            )],
            None,
        );
        let home = TempHome::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            InMemoryStore::new(),
            &home,
        );

        let error = run(
            &mut context,
            Some("device"),
            refused.base_url(),
            &mut Vec::new(),
        )
        .expect_err("the request was refused");

        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert_eq!(
            error.message,
            "Login failed: Device code request failed (HTTP 400): AADSTS50000: nope"
        );

        let incomplete =
            FakeIdentity::start(vec![reply(200, json!({"device_code": "only"}))], None);
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            InMemoryStore::new(),
            &home,
        );

        let error = run(
            &mut context,
            Some("device"),
            incomplete.base_url(),
            &mut Vec::new(),
        )
        .expect_err("the response is missing fields");

        assert_eq!(
            error.message,
            "Login failed: Device code response is missing a required field."
        );
        assert!(
            incomplete.polls().is_empty(),
            "an unusable device code is never polled"
        );
    }

    /// A transport failure keeps the §6.2 classification and gains the module's
    /// prefix; nothing is stored.
    #[test]
    fn a_transport_failure_is_a_network_error() {
        let home = TempHome::new();
        let store = InMemoryStore::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );

        let error = run(
            &mut context,
            Some("device"),
            "http://127.0.0.1:1",
            &mut Vec::new(),
        )
        .expect_err("nothing listens on port 1");

        assert_eq!(error.code, ErrorCode::NetworkError);
        assert_eq!(
            error.message,
            "Login failed: Connection refused. Is the server reachable?"
        );
        assert!(store.calls().is_empty());
    }

    /// A closed stdout cannot show the code, so the flow stops before the first
    /// poll: a silent success, and no minutes of polling for nobody.
    #[test]
    fn a_closed_stdout_stops_the_flow_before_the_first_poll() {
        let home = TempHome::new();
        let fake = FakeIdentity::start(vec![device_code_reply(0), granted()], None);
        let mut context = test_context(
            opts(None, None, None, false),
            org_env("myorg"),
            InMemoryStore::new(),
            &home,
        );

        let report = run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut ClosedStdout,
        )
        .expect("a closed stdout is a silent success");

        assert_eq!(report, Report::Text(String::new()));
        assert_eq!(
            fake.received().len(),
            1,
            "only the device-code request went out"
        );
    }

    #[test]
    fn the_methods_are_resolved_like_the_module() {
        for (method, expected) in [
            (Some("pat"), LoginMethod::Pat),
            (Some("device"), LoginMethod::Device),
        ] {
            assert_eq!(resolve_method(method, None), Ok(expected), "{method:?}");
        }

        assert_eq!(
            resolve_method(None, Some("pat-token")),
            Ok(LoginMethod::Pat),
            "a PAT infers the method"
        );
        assert_eq!(
            resolve_method(None, None)
                .expect_err("browser is not shipped")
                .message,
            browser_not_shipped().message
        );
        assert_eq!(
            resolve_method(Some("browser"), None)
                .expect_err("browser is not shipped")
                .message,
            browser_not_shipped().message
        );
        assert_eq!(
            resolve_method(Some("device_code"), None)
                .expect_err("the oracle rejects this spelling too")
                .message,
            "Unknown method 'device_code'. Use 'pat' or 'device'."
        );
    }
}
