//! `ado login` — the PAT method, the device-code flow and the browser OAuth flow,
//! ported from `lib/ado_cli/cli/auth_commands.ex` (`login/1`, `resolve_method/1`,
//! `login_with_pat/3`, `login_with_device/2`, `login_with_browser/2`,
//! `login_success/4`) and `lib/ado_cli/auth.ex` (`login_pat/2`,
//! `request_device_code/1`, `poll_for_token/4`, `exchange_and_save_device/2`,
//! `login_browser/1`).
//!
//! The storage is §7's clean slate: the token goes to the credential store — the OS
//! keychain, with `credentials.json` behind it — and `config.toml` records the
//! organization and the method, never the token. That is the one deliberate
//! difference from the oracle in the success output: `credentials_saved_to` names
//! the config file where the oracle names the JSON file its own token lives in.
//!
//! `ADO_OAUTH_CLIENT_ID` is read here, once, and threaded into whichever flow runs
//! (spec §4.7): the OAuth app is part of the auth contract, so it is not a constant
//! inside `ado-core`.

use std::io::Write;

use ado_core::auth::browser::{self, BrowserLogin};
use ado_core::auth::device_code::{self, DeviceCode};
use ado_core::auth::org_required;
use ado_core::config::AuthMethod;
use ado_core::env::{ENV_OAUTH_CLIENT_ID, ENV_ORG, ENV_PAT, ENV_SERVER, EnvSource, non_empty};
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::json;

use crate::context::Context;
use crate::output::{Report, WriteFailure, write_bytes};

/// The `--method` values this build accepts — the oracle's list, in the oracle's
/// order (`valid_methods: ["browser", "pat", "device"]`).
const METHODS: [&str; 3] = ["browser", "pat", "device"];

/// The parts of the interactive flows production fixes and a test replaces. The
/// origins are parameters for the same reason the device flow's already was: the
/// frozen CLI hardcodes them and this build adds no environment override, so a
/// test's only way to run the flow is to hand it a local fake.
pub struct LoginSeams<'a> {
    pub identity_base: &'a str,
    pub accounts_base: &'a str,
    pub opener: &'a dyn browser::Opener,
    /// `None` binds the real loopback listener.
    pub callback: Option<&'a dyn browser::Callback>,
}

impl<'a> LoginSeams<'a> {
    /// Production: the real origins, the system browser and a freshly bound
    /// loopback listener.
    pub fn production(identity_base: &'a str) -> LoginSeams<'a> {
        LoginSeams {
            identity_base,
            accounts_base: browser::ACCOUNTS_BASE,
            opener: &browser::SystemOpener,
            callback: None,
        }
    }
}

/// `ado login`.
///
/// `identity_base` is the identity origin the interactive flows talk to —
/// [`device_code::IDENTITY_BASE`] in production, a local fake in the tests, because
/// the flow's endpoints are otherwise not redirectable and this build adds no
/// environment override for them. `announce` receives the device code and its URL
/// after the code is requested and before the first poll, which is the only moment
/// the user can read them, and the browser flow's authorize URL before it opens
/// anything.
pub fn run(
    context: &mut Context,
    method: Option<&str>,
    identity_base: &str,
    announce: &mut dyn Write,
) -> Result<Report, AdoError> {
    run_with_seams(
        context,
        method,
        &LoginSeams::production(identity_base),
        announce,
    )
}

/// [`run`] with the flows' injected parts exposed to the caller.
pub fn run_with_seams(
    context: &mut Context,
    method: Option<&str>,
    seams: &LoginSeams<'_>,
    announce: &mut dyn Write,
) -> Result<Report, AdoError> {
    let (org, pat, server, client_id) = {
        let env = context.env();

        // `FlagEnv::set` drops a blank flag, but the environment behind it answers
        // `Ok("")` for a set-but-empty variable, so the same predicate has to run
        // here: a blank `ADO_ORG`/`ADO_PAT` is not a value (D16), exactly as
        // `credentials::resolve` already treats those variables.
        let value = |name: &str| env.get(name).filter(|value| non_empty(value));

        // `set_server/1` reads the flag, then `ADO_SERVER`, and nothing else: the
        // config's `server` is `whoami`'s display field (W1-R10), so a login never
        // reports one this invocation did not name.
        //
        // `ADO_OAUTH_CLIENT_ID` overrides the OAuth app for both interactive flows
        // (spec §4.7); a blank value is not a value (D16), so it falls back to the
        // Azure CLI public client. The frozen escript cannot honour this at runtime
        // at all — its module attribute is evaluated at compile time (captured in
        // the Task 10 report) — which is why this build reads it here.
        (
            value(ENV_ORG),
            value(ENV_PAT),
            value(ENV_SERVER),
            value(ENV_OAUTH_CLIENT_ID).unwrap_or_else(|| device_code::CLIENT_ID.to_owned()),
        )
    };

    match resolve_method(method, pat.as_deref())? {
        LoginMethod::Pat => login_with_pat(context, org, pat, server.as_deref()),
        LoginMethod::Device => {
            login_with_device(context, org, seams.identity_base, &client_id, announce)
        }
        LoginMethod::Browser => login_with_browser(context, org, seams, &client_id, announce),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginMethod {
    Browser,
    Pat,
    Device,
}

/// `resolve_method/1`: an explicit `--method` wins; otherwise a PAT on `--pat` or
/// `ADO_PAT` means `pat`, and anything else is the browser flow — the oracle's
/// default, and this build's since Wave 3.
fn resolve_method(method: Option<&str>, pat: Option<&str>) -> Result<LoginMethod, AdoError> {
    match method {
        Some("browser") => Ok(LoginMethod::Browser),
        Some("pat") => Ok(LoginMethod::Pat),
        Some("device") => Ok(LoginMethod::Device),
        Some(other) => Err(unknown_method(other)),
        None if pat.is_some() => Ok(LoginMethod::Pat),
        None => Ok(LoginMethod::Browser),
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
/// **DevOps** access token, and store the DevOps one. The oracle's device path has no
/// server to report (`login_success(parsed, org_name, "device", nil)`), so this one
/// passes none.
fn login_with_device(
    context: &mut Context,
    org: Option<String>,
    identity_base: &str,
    client_id: &str,
    announce: &mut dyn Write,
) -> Result<Report, AdoError> {
    // D26: the oracle's guard exempts `device` from the `--org` requirement and then
    // writes a credential keyed by nothing (`org: nil`); our store and
    // `config.toml` are per-organization, so the org is required before the flow
    // starts — a dead end refused rather than ported.
    let org = org.ok_or_else(|| org_required("device"))?;
    let device = device_code::request(identity_base, client_id).map_err(login_failed)?;

    if let Err(failure) = announce_device_code(context, &device, announce) {
        return match failure {
            // A closed stdout cannot show the code, so the flow stops before polling:
            // a silent success, like every other pipe failure (spec §6.3, R19), and
            // no minutes of polling for a caller that is not reading.
            WriteFailure::BrokenPipe => Ok(Report::Text(String::new())),
            WriteFailure::Other(message) => Err(AdoError::validation(message)),
        };
    }

    let refresh = device_code::poll(identity_base, client_id, &device).map_err(login_failed)?;
    let token = device_code::exchange(identity_base, client_id, refresh.as_deref())
        .map_err(login_failed)?;

    context
        .save_login(&org, AuthMethod::Device, &token)
        .map_err(login_failed)?;

    Ok(success(context, &org, "device", None))
}

/// `login_with_browser/2` → `login_browser/1`: the PKCE authorization against ARM,
/// the loopback callback, the state check, the code exchange, the ARM→DevOps
/// exchange — and, when `--org` was absent, the organization the account resolves
/// to. The oracle's browser path has no server to report
/// (`login_success(parsed, org_name, "browser", nil)`), so this one passes none.
///
/// Unlike `device`, the org is *not* required up front: the browser flow's
/// auto-detect is what resolves it, so an unruly account is refused only after the
/// exchange (D26's extension — the flow itself announces the reason, and the token
/// is deliberately not stored).
fn login_with_browser(
    context: &mut Context,
    org: Option<String>,
    seams: &LoginSeams<'_>,
    client_id: &str,
    announce: &mut dyn Write,
) -> Result<Report, AdoError> {
    let listener;
    let callback: &dyn browser::Callback = match seams.callback {
        Some(callback) => callback,
        None => {
            listener = browser::LoopbackCallback::bind().map_err(login_failed)?;
            &listener
        }
    };
    let mut flow = browser::BrowserFlow {
        identity_base: seams.identity_base,
        accounts_base: seams.accounts_base,
        client_id,
        org: org.as_deref(),
        opener: seams.opener,
        callback,
        json: context.json(),
        announce,
    };

    match browser::login(&mut flow).map_err(browser_failed)? {
        BrowserLogin::NotShown => Ok(Report::Text(String::new())),
        BrowserLogin::Authenticated { org, token } => {
            context
                .save_login(&org, AuthMethod::Browser, &token)
                .map_err(login_failed)?;

            Ok(success(context, &org, "browser", None))
        }
    }
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
/// `credentials_saved_to` — and its two human lines. Two values are ours rather than
/// the oracle's: `server` is the flag/env one the caller resolved (the oracle's
/// `set_server/1` reads the same two, and its device path passes `nil`), and
/// `credentials_saved_to` names this CLI's config file, where the oracle names the
/// JSON file that holds its token (§7, D11b's file).
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

/// The module's `--pat is required for method=pat (or set ADO_PAT env var)`.
fn pat_required() -> AdoError {
    AdoError {
        code: ErrorCode::ValidationError,
        status: None,
        message: "--pat is required for method=pat (or set ADO_PAT env var)".to_owned(),
        details: Some(json!({"option": "--pat", "env_var": "ADO_PAT"})),
    }
}

/// `dispatch_login/4`'s fallback, byte for byte (`valid_methods` is the oracle's
/// three-name list).
fn unknown_method(method: &str) -> AdoError {
    AdoError {
        code: ErrorCode::ValidationError,
        status: None,
        message: format!("Unknown method '{method}'. Use 'browser', 'pat', or 'device'."),
        details: Some(json!({"valid_methods": METHODS})),
    }
}

/// The browser flow's failures, split by what they are: its own are the oracle's
/// `auth_required`-prefixed ones, while a refusal to key the login is the same
/// `validation_error` the other methods' `--org` guard emits before their flow starts
/// (D26) — prefixed by nothing, because the oracle never prefixes it either.
fn browser_failed(error: AdoError) -> AdoError {
    if error.code == ErrorCode::AuthRequired {
        login_failed(error)
    } else {
        error
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
    use ado_core::credentials::{InMemoryStore, SecretStore, StoreCall, Stored};
    use ado_core::env::{ENV_SERVER, MapEnv};
    use ado_testkit::{MockResponse, MockServer, TempHome};
    use serde_json::{Value, json};

    use super::*;
    use crate::args::GlobalOpts;
    use ado_core::auth::identity;

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
        headers: Vec<(String, String)>,
        at: Instant,
    }

    impl Received {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        }

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
                    let Some((path, form, headers)) = read_request(&mut stream) else {
                        continue;
                    };

                    let answer = {
                        let mut state = lock(&thread_state);
                        state.requests.push(Received {
                            path,
                            form,
                            headers,
                            at,
                        });
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

    /// One HTTP/1.1 request: its path, its form body and its headers.
    type Request = (String, Vec<(String, String)>, Vec<(String, String)>);

    /// `ureq` sends `content-length`, so the body is read exactly.
    fn read_request(stream: &mut TcpStream) -> Option<Request> {
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

        let request_line = head.lines().next()?;
        let path = request_line.split_whitespace().nth(1)?.to_owned();
        let end = (header_end + content_length).min(buffer.len());
        let body = String::from_utf8_lossy(&buffer[header_end..end]).into_owned();
        let headers = head
            .lines()
            .skip(1)
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;

                Some((name.trim().to_owned(), value.trim().to_owned()))
            })
            .collect();

        Some((path, parse_form(&body), headers))
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

    /// The PAT path's `server` is the flag/env one and nothing else: a server
    /// recorded in `config.toml` is `whoami`'s display field (W1-R10), and
    /// `set_server/1` — the function this mirrors — reads `--server` and `ADO_SERVER`
    /// only. The recorded value survives the login untouched.
    #[test]
    fn the_config_server_is_not_the_login_envelope_server() {
        let home = TempHome::new();
        let mut context = test_context(
            opts(Some("myorg"), Some("pat-token"), None, true),
            MapEnv::new(),
            InMemoryStore::new(),
            &home,
        )
        .with_config(Config {
            server: Some("https://config.test".to_owned()),
            ..Config::default()
        });

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
            json!(null),
            "the config's server is not an envelope value"
        );
        assert!(
            config_text(&home).contains("server = \"https://config.test\""),
            "the login preserves the setting it does not own: {}",
            config_text(&home)
        );
    }

    /// The oracle's device path reports no server at all
    /// (`login_success(parsed, org, "device", nil)`), even when this invocation named
    /// one — the value belongs to the PAT path.
    #[test]
    fn the_device_path_reports_no_server_even_when_one_is_named() {
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
            opts(None, None, Some("https://flag.test"), true),
            org_env("myorg").set(ENV_SERVER, "https://env.test"),
            InMemoryStore::new(),
            &home,
        );

        let report = run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut Vec::new(),
        )
        .expect("the device login");

        let Report::Json(envelope) = report else {
            panic!("the json context reports an envelope");
        };
        assert_eq!(
            envelope["result"]["server"],
            json!(null),
            "the oracle's device path passes nil"
        );
        assert!(envelope["result"]["credentials_saved_to"].is_string());
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

    /// A blank environment value is not a value (D16): `ADO_PAT=`/`ADO_ORG=` are the
    /// missing-option errors, not a stored empty credential. The flag twins are
    /// `cli_login.rs`'s `a_blank_value_is_not_a_value`.
    #[test]
    fn a_blank_environment_value_is_not_a_value() {
        let home = TempHome::new();
        let store = InMemoryStore::new();

        for blank in ["", "  "] {
            let mut blank_pat = test_context(
                opts(Some("myorg"), None, None, true),
                MapEnv::new().set(ENV_PAT, blank),
                store.clone(),
                &home,
            );

            let error = run(
                &mut blank_pat,
                Some("pat"),
                device_code::IDENTITY_BASE,
                &mut Vec::new(),
            )
            .expect_err("a blank ADO_PAT is not a value");

            assert_eq!(error.message, pat_required().message, "{blank:?}");

            let mut blank_org = test_context(
                opts(None, Some("pat-token"), None, true),
                MapEnv::new().set(ENV_ORG, blank),
                store.clone(),
                &home,
            );

            let error = run(
                &mut blank_org,
                Some("pat"),
                device_code::IDENTITY_BASE,
                &mut Vec::new(),
            )
            .expect_err("a blank ADO_ORG is not a value");

            assert_eq!(error.message, org_required("pat").message, "{blank:?}");
        }

        assert!(
            store.calls().is_empty(),
            "nothing is stored: {:?}",
            store.calls()
        );
        assert!(
            !home
                .config_dir()
                .join(ado_core::config::CONFIG_FILE)
                .exists(),
            "nothing is recorded"
        );
    }

    /// The checklist item "replaced, not shadowed": a second login for the same
    /// organization overwrites the credential, so the newer token is the one the
    /// next command resolves.
    #[test]
    fn a_second_login_replaces_the_credential() {
        let home = TempHome::new();
        let store = InMemoryStore::new();

        for token in ["first-token", "second-token"] {
            let mut context = test_context(
                opts(Some("myorg"), Some(token), None, true),
                MapEnv::new(),
                store.clone(),
                &home,
            );

            run(
                &mut context,
                Some("pat"),
                device_code::IDENTITY_BASE,
                &mut Vec::new(),
            )
            .expect("the login");
        }

        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Pat, "second-token")),
            "the store is keyed by organization, so the later token replaces the earlier one"
        );
        assert_eq!(
            store
                .calls()
                .iter()
                .filter(|call| matches!(call, StoreCall::Set(_)))
                .count(),
            2,
            "both logins wrote: {:?}",
            store.calls()
        );
        assert!(
            !config_text(&home).contains("first-token"),
            "config.toml: {}",
            config_text(&home)
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
        assert_eq!(
            message,
            "To sign in, use a web browser to open: https://microsoft.com/devicelogin and enter the code: USER-CODE",
            "the oracle's `verification_url`, not the RFC fallback (`-rfc`)"
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
    /// `interval + 5` seconds rather than the server's interval — and that increase
    /// is the interval the *next* poll waits as well, which is why this script spends
    /// two five-second waits: the cadence is observed, not asserted.
    #[test]
    fn slow_down_waits_the_increment_and_keeps_it() {
        let home = TempHome::new();
        let fake = FakeIdentity::start(
            vec![
                device_code_reply(0),
                slow_down(),
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

        run(
            &mut context,
            Some("device"),
            fake.base_url(),
            &mut Vec::new(),
        )
        .expect("slow_down is not terminal");

        let polls = fake.polls();
        assert_eq!(
            polls.len(),
            3,
            "slow_down, then a pending the grant follows"
        );

        let after_slow_down = polls[1].after(&polls[0]);
        assert!(
            after_slow_down >= Duration::from_millis(4900),
            "slow_down must wait the server's interval plus the oracle's five seconds: {after_slow_down:?}"
        );

        let after_pending = polls[2].after(&polls[1]);
        assert!(
            after_pending >= Duration::from_millis(4900),
            "slow_down's increase is the interval every later poll waits, not a one-off:\
             `handle_token_error/4` recurses with `interval + 5`: {after_pending:?}"
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
            (Some("browser"), LoginMethod::Browser),
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
            resolve_method(None, None),
            Ok(LoginMethod::Browser),
            "no --method and no PAT is the oracle's default: the browser flow"
        );
        assert_eq!(
            resolve_method(Some("device_code"), None)
                .expect_err("the oracle rejects this spelling too")
                .message,
            "Unknown method 'device_code'. Use 'browser', 'pat', or 'device'."
        );
        assert_eq!(
            unknown_method("bogus").details,
            Some(json!({"valid_methods": ["browser", "pat", "device"]})),
            "the oracle's own three-name list"
        );
    }

    // ── the browser flow ──────────────────────────────────────────────────

    /// What the scripted callback answers. The state the flow generated is only
    /// visible in the authorize URL the opener was handed, so the scripted opener
    /// records it and the callback replays it (or a different one).
    #[derive(Debug, Clone, PartialEq)]
    enum CallbackReply {
        Code,
        MismatchedState,
        NoState,
        NoCode,
        OAuthError(String),
        Timeout,
    }

    /// The scripted browser seams in one type: [`browser::Opener`] records the URL
    /// (and can fail, like a machine with no browser), and [`browser::Callback`]
    /// answers what the case scripted.
    struct ScriptedBrowser {
        seen_url: Mutex<Option<String>>,
        reply: CallbackReply,
        opener_fails: bool,
    }

    impl ScriptedBrowser {
        fn new(reply: CallbackReply) -> ScriptedBrowser {
            ScriptedBrowser {
                seen_url: Mutex::new(None),
                reply,
                opener_fails: false,
            }
        }

        fn with_failing_opener(reply: CallbackReply) -> ScriptedBrowser {
            ScriptedBrowser {
                opener_fails: true,
                ..ScriptedBrowser::new(reply)
            }
        }

        /// The authorize URL the flow printed and handed to the opener.
        fn url(&self) -> String {
            lock(&self.seen_url)
                .clone()
                .expect("the flow opens the URL it printed")
        }

        fn state(&self) -> Option<String> {
            state_of(&self.url())
        }

        /// Whether the flow opened anything at all.
        fn opened(&self) -> Option<String> {
            lock(&self.seen_url).clone()
        }

        fn redirect_uri(&self) -> String {
            let url = self.url();
            let redirect = url
                .split_once("redirect_uri=")
                .expect("the authorize URL carries a redirect_uri")
                .1
                .split('&')
                .next()
                .expect("a value")
                .to_owned();

            decode_component(&redirect)
        }
    }

    impl browser::Opener for ScriptedBrowser {
        fn open(&self, url: &str) -> Result<(), String> {
            *lock(&self.seen_url) = Some(url.to_owned());

            if self.opener_fails {
                Err("no browser".to_owned())
            } else {
                Ok(())
            }
        }
    }

    impl browser::Callback for ScriptedBrowser {
        fn redirect_uri(&self) -> String {
            "http://localhost:41234".to_owned()
        }

        fn wait(&self) -> Result<browser::Authorization, AdoError> {
            let state = self.state();

            match &self.reply {
                CallbackReply::Code => Ok(browser::Authorization {
                    code: "AUTH-CODE".to_owned(),
                    state,
                }),
                CallbackReply::MismatchedState => Ok(browser::Authorization {
                    code: "AUTH-CODE".to_owned(),
                    state: Some("NOT-THE-STATE".to_owned()),
                }),
                CallbackReply::NoState => Ok(browser::Authorization {
                    code: "AUTH-CODE".to_owned(),
                    state: None,
                }),
                CallbackReply::NoCode => {
                    Err(identity::auth_failed("No authorization code received."))
                }
                CallbackReply::OAuthError(error) => Err(identity::auth_failed(format!(
                    "Authorization failed: {error}"
                ))),
                CallbackReply::Timeout => Err(identity::auth_failed(
                    "Browser login timed out or was cancelled.",
                )),
            }
        }
    }

    /// The generated `state` from the authorize URL: the flow's own value, read back
    /// by the test so the callback can echo it.
    fn state_of(url: &str) -> Option<String> {
        let rest = url.split_once("&state=")?.1;
        let encoded = rest.split('&').next()?;

        Some(decode_component(encoded))
    }

    /// The PKCE challenge the URL advertised, derived from the verifier the exchange
    /// actually sent: the wire's own relationship, computed by the test (with
    /// `sha2` and `base64`, the crates the product itself does not use for this).
    fn challenge_of(verifier: &str) -> String {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        use sha2::{Digest, Sha256};

        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    }

    fn code_challenge_of(url: &str) -> String {
        let rest = url
            .split_once("code_challenge=")
            .expect("the authorize URL carries a challenge")
            .1;
        let encoded = rest.split('&').next().expect("a value");

        decode_component(encoded)
    }

    /// A login against the scripted seams: the identity fake answers the two
    /// exchanges, the accounts fake the auto-detect.
    fn run_scripted(
        context: &mut Context,
        identity: &FakeIdentity,
        accounts: &FakeIdentity,
        browser: &ScriptedBrowser,
        announce: &mut dyn Write,
    ) -> Result<Report, AdoError> {
        let seams = LoginSeams {
            identity_base: identity.base_url(),
            accounts_base: accounts.base_url(),
            opener: browser,
            callback: Some(browser),
        };

        run_with_seams(context, Some("browser"), &seams, announce)
    }

    /// The two exchanges' replies: the ARM grant (access + refresh) and the DevOps
    /// token.
    fn browser_replies(token: &str) -> Vec<Reply> {
        vec![
            reply(
                200,
                json!({"access_token": "arm-token", "refresh_token": "refresh-token"}),
            ),
            devops_token(token),
        ]
    }

    fn accounts_reply(value: Value) -> Reply {
        reply(200, value)
    }

    /// The whole flow, end to end: PKCE, the authorize URL, the code exchange, the
    /// ARM→DevOps exchange, the credential and the oracle's envelope. The code
    /// exchange's `code_verifier` is checked against the URL's `code_challenge` —
    /// the one relationship a port can get wrong without any byte differing.
    #[test]
    fn the_browser_flow_exchanges_the_code_and_stores_the_devops_token() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(browser_replies("devops-token"), None);
        let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
        let store = InMemoryStore::new();
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut announce = Vec::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );

        let report = run_scripted(&mut context, &identity, &accounts, &browser, &mut announce)
            .expect("the browser login");

        assert_eq!(
            report,
            Report::Json(json!({
                "ok": true,
                "result": {
                    "org": "myorg",
                    "method": "browser",
                    "server": null,
                    "credentials_saved_to": config_file(&home),
                }
            })),
            "the oracle's value envelope for the browser path"
        );
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Browser, "devops-token")),
            "the DevOps token is stored, not the ARM one or the refresh token"
        );
        assert_eq!(
            config_text(&home),
            "default_org = \"myorg\"\n\n[orgs.myorg]\nauth = \"browser\"\n"
        );

        let url = browser.url();
        assert!(
            url.starts_with(&format!(
                "{}/organizations/oauth2/v2.0/authorize?",
                identity.base_url()
            )),
            "the authorize URL is built on the injected identity origin: {url}"
        );

        // The `--json` announcement is one document, and it carries the URL the
        // opener was handed (the human block is pinned by its own case).
        let announced = String::from_utf8(announce).expect("utf-8");
        let message: Value = serde_json::from_str(announced.trim_end()).expect("one document");
        assert_eq!(
            message["message"],
            json!(format!("Opening browser to sign in to myorg...\n  {url}"))
        );

        let received = identity.received();
        assert_eq!(
            received.len(),
            2,
            "the code exchange and the refresh exchange"
        );
        let exchange = &received[0];
        assert_eq!(exchange.path, "/organizations/oauth2/v2.0/token");
        assert_eq!(
            exchange.form_value("grant_type"),
            Some("authorization_code")
        );
        assert_eq!(exchange.form_value("code"), Some("AUTH-CODE"));
        assert_eq!(
            exchange.form_value("redirect_uri"),
            Some("http://localhost:41234")
        );
        assert_eq!(
            exchange.form_value("client_id"),
            Some(CLIENT_ID),
            "the Azure CLI public client"
        );
        assert_eq!(
            challenge_of(exchange.form_value("code_verifier").expect("a verifier")),
            code_challenge_of(&url),
            "the verifier sent is the one the URL's challenge commits to"
        );
        assert_eq!(
            browser.redirect_uri(),
            "http://localhost:41234",
            "the redirect_uri in the URL and the one in the exchange agree"
        );

        let refresh = &received[1];
        assert_eq!(refresh.form_value("grant_type"), Some("refresh_token"));
        assert_eq!(refresh.form_value("refresh_token"), Some("refresh-token"));
        assert_eq!(refresh.form_value("resource"), Some(DEVOPS_RESOURCE));
        assert!(
            accounts.received().is_empty(),
            "an explicit --org never asks the accounts endpoint"
        );
    }

    /// The oracle's browser success lines: `login_success/4`'s two, with this build's
    /// config file. The oracle's extra `Authenticated successfully as X.` line is not
    /// reproduced — the device path already dropped its twin (W1's record).
    #[test]
    fn the_browser_human_lines_are_the_oracles() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(browser_replies("devops-token"), None);
        let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut announce = Vec::new();
        let mut context = test_context(
            opts(None, None, None, false),
            org_env("myorg"),
            InMemoryStore::new(),
            &home,
        );

        let report = run_scripted(&mut context, &identity, &accounts, &browser, &mut announce)
            .expect("the browser login");

        assert_eq!(
            report,
            Report::Text(format!(
                "\n  Logged in to myorg via Browser.\n  Credentials saved to {}",
                config_file(&home)
            ))
        );
        assert_eq!(
            String::from_utf8(announce).expect("utf-8"),
            format!(
                "\nOpening browser to sign in to myorg...\n  \n{}\n\n",
                browser.url()
            ),
            "the oracle's captured bytes: the two-space indent is its own line"
        );
    }

    /// `--json`: the URL reaches stdout as one parseable document, the way the device
    /// path's code does.
    #[test]
    fn the_json_announcement_is_one_message_line_with_the_url() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(browser_replies("devops-token"), None);
        let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut announce = Vec::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            InMemoryStore::new(),
            &home,
        );

        run_scripted(&mut context, &identity, &accounts, &browser, &mut announce)
            .expect("the browser login");

        let announced = String::from_utf8(announce).expect("utf-8");
        let message: Value = serde_json::from_str(announced.trim_end()).expect("one JSON document");
        assert_eq!(message["ok"], json!(true));
        assert_eq!(
            message["message"],
            json!(format!(
                "Opening browser to sign in to myorg...\n  {}",
                browser.url()
            ))
        );
    }

    /// Auto-detect, the one-account branch: the org is adopted, printed, and the
    /// credential is stored under it. The accounts request carries the fresh DevOps
    /// token as a bearer.
    #[test]
    fn the_browser_flow_adopts_the_only_detected_org() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(browser_replies("devops-token"), None);
        let accounts = FakeIdentity::start(
            vec![accounts_reply(
                json!({"value": [{"AccountName": "detected-org"}]}),
            )],
            None,
        );
        let store = InMemoryStore::new();
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut announce = Vec::new();
        let mut context = test_context(
            opts(None, None, None, true),
            MapEnv::new(),
            store.clone(),
            &home,
        );

        let report = run_scripted(&mut context, &identity, &accounts, &browser, &mut announce)
            .expect("the org was detected");

        let Report::Json(envelope) = report else {
            panic!("the json context reports an envelope");
        };
        assert_eq!(envelope["result"]["org"], json!("detected-org"));
        assert_eq!(
            store.get("detected-org").expect("get"),
            Some(stored(AuthMethod::Browser, "devops-token"))
        );
        let announced = String::from_utf8(announce).expect("utf-8");
        let detected: Value = serde_json::from_str(announced.lines().last().expect("a line"))
            .expect("the detected-org line is one JSON document");
        assert_eq!(
            detected["message"],
            json!("Detected org: detected-org"),
            "the oracle prints the org it adopted"
        );

        let received = accounts.received();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].path, "/_apis/accounts");
        assert_eq!(
            received[0].header("authorization"),
            Some("Bearer devops-token"),
            "the accounts call authenticates with the fresh DevOps token"
        );
    }

    /// The accounts document the API really returns is a top-level array, and a name
    /// may come from `accountUri` when neither `AccountName` spelling is there — the
    /// oracle's `extract_account_names/1` fallback chain.
    #[test]
    fn the_accounts_names_fall_back_and_the_body_may_be_an_array() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(browser_replies("devops-token"), None);
        let accounts = FakeIdentity::start(
            vec![accounts_reply(
                json!([{"accountUri": "https://dev.azure.com/x"}]),
            )],
            None,
        );
        let store = InMemoryStore::new();
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut context = test_context(
            opts(None, None, None, true),
            MapEnv::new(),
            store.clone(),
            &home,
        );

        run_scripted(
            &mut context,
            &identity,
            &accounts,
            &browser,
            &mut Vec::new(),
        )
        .expect("the accountUri is a name");

        assert_eq!(
            store.get("https://dev.azure.com/x").expect("get"),
            Some(stored(AuthMethod::Browser, "devops-token"))
        );
    }

    /// The zero-account branch, after the oracle's own lines: D26's refusal, and
    /// nothing stored although the exchange succeeded.
    #[test]
    fn the_browser_flow_refuses_when_no_org_can_be_adopted() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(browser_replies("devops-token"), None);
        let accounts = FakeIdentity::start(vec![accounts_reply(json!({"value": []}))], None);
        let store = InMemoryStore::new();
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut announce = Vec::new();
        let mut context = test_context(
            opts(None, None, None, false),
            MapEnv::new(),
            store.clone(),
            &home,
        );

        let error = run_scripted(&mut context, &identity, &accounts, &browser, &mut announce)
            .expect_err("no organization to key the credential to");

        assert_eq!(error.code, ErrorCode::ValidationError);
        assert_eq!(
            error.message,
            "--org is required for method='browser' (or set ADO_ORG env var)"
        );
        assert_eq!(
            error.details,
            Some(json!({"option": "--org", "env_var": "ADO_ORG"}))
        );
        assert_eq!(
            String::from_utf8(announce).expect("utf-8"),
            format!(
                "\nOpening browser...\n  \n{}\n\nNo Azure DevOps organizations were found for this account.\nSet your org with: export ADO_ORG=<your-org>\n",
                browser.url()
            ),
            "the oracle's own lines, then the refusal (this build's, not the oracle's \
             `Authenticated successfully.`)"
        );
        assert!(store.calls().is_empty(), "calls: {:?}", store.calls());
        assert!(
            !home
                .config_dir()
                .join(ado_core::config::CONFIG_FILE)
                .exists(),
            "a refused login writes nothing"
        );
    }

    /// Several accounts are printed and not adopted: the oracle's two lines, then the
    /// same refusal.
    #[test]
    fn several_detected_orgs_are_printed_and_refused() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(browser_replies("devops-token"), None);
        let accounts = FakeIdentity::start(
            vec![accounts_reply(
                json!({"value": [{"AccountName": "one"}, {"accountName": "two"}]}),
            )],
            None,
        );
        let store = InMemoryStore::new();
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut announce = Vec::new();
        let mut context = test_context(
            opts(None, None, None, false),
            MapEnv::new(),
            store.clone(),
            &home,
        );

        let error = run_scripted(&mut context, &identity, &accounts, &browser, &mut announce)
            .expect_err("several organizations, none adopted");

        assert_eq!(error.code, ErrorCode::ValidationError);
        let announced = String::from_utf8(announce).expect("utf-8");
        assert!(
            announced.contains(
                "Multiple organizations found: one, two\nRe-run with --org <name> to pick one.\n"
            ),
            "{announced}"
        );
        assert!(store.calls().is_empty());
    }

    /// A lookup that fails is the oracle's `{:error, _} -> nil`: the same lines and
    /// the same refusal as an empty account list (the oracle prints those two lines
    /// for both).
    #[test]
    fn a_failed_accounts_lookup_is_no_org() {
        for failure in [
            reply(500, json!({"message": "boom"})),
            reply(200, json!({})),
        ] {
            let home = TempHome::new();
            let identity = FakeIdentity::start(browser_replies("devops-token"), None);
            let accounts = FakeIdentity::start(vec![failure], None);
            let store = InMemoryStore::new();
            let browser = ScriptedBrowser::new(CallbackReply::Code);
            let mut announce = Vec::new();
            let mut context = test_context(
                opts(None, None, None, false),
                MapEnv::new(),
                store.clone(),
                &home,
            );

            let error = run_scripted(&mut context, &identity, &accounts, &browser, &mut announce)
                .expect_err("no organizations");

            assert_eq!(error.code, ErrorCode::ValidationError);
            assert!(
                String::from_utf8(announce)
                    .expect("utf-8")
                    .contains("No Azure DevOps organizations were found for this account.")
            );
            assert!(store.calls().is_empty());
        }
    }

    /// The state check happens before the code is used: a mismatched (or missing)
    /// state stops the flow with the oracle's sentence and contacts nothing.
    #[test]
    fn a_state_that_is_not_the_flows_own_stops_before_the_exchange() {
        for answer in [CallbackReply::MismatchedState, CallbackReply::NoState] {
            let home = TempHome::new();
            let identity = FakeIdentity::start(browser_replies("devops-token"), None);
            let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
            let store = InMemoryStore::new();
            let browser = ScriptedBrowser::new(answer);
            let mut context = test_context(
                opts(None, None, None, true),
                org_env("myorg"),
                store.clone(),
                &home,
            );

            let error = run_scripted(
                &mut context,
                &identity,
                &accounts,
                &browser,
                &mut Vec::new(),
            )
            .expect_err("the state does not match");

            assert_eq!(error.code, ErrorCode::AuthRequired);
            assert_eq!(
                error.message,
                "Login failed: State mismatch — possible CSRF attack."
            );
            assert_eq!(
                error.details,
                Some(json!({"reason": "State mismatch — possible CSRF attack."}))
            );
            assert!(
                identity.received().is_empty(),
                "no request is made with a code that failed the state check"
            );
            assert!(store.calls().is_empty());
        }
    }

    /// The callback's own failures are the oracle's sentences, prefixed by
    /// `login_failed/1` and carrying `auth_required`.
    #[test]
    fn the_callback_failures_are_the_oracles_sentences() {
        for (answer, expected) in [
            (CallbackReply::NoCode, "No authorization code received."),
            (
                CallbackReply::OAuthError("access_denied".to_owned()),
                "Authorization failed: access_denied",
            ),
            (
                CallbackReply::Timeout,
                "Browser login timed out or was cancelled.",
            ),
        ] {
            let home = TempHome::new();
            let identity = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
            let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
            let browser = ScriptedBrowser::new(answer.clone());
            let mut context = test_context(
                opts(None, None, None, true),
                org_env("myorg"),
                InMemoryStore::new(),
                &home,
            );

            let error = run_scripted(
                &mut context,
                &identity,
                &accounts,
                &browser,
                &mut Vec::new(),
            )
            .expect_err("the callback failed");

            assert_eq!(error.code, ErrorCode::AuthRequired, "{answer:?}");
            assert_eq!(
                error.message,
                format!("Login failed: {expected}"),
                "{answer:?}"
            );
            assert_eq!(
                error.details,
                Some(json!({"reason": expected})),
                "{answer:?}"
            );
        }
    }

    /// A machine with no browser is not a failed login: the URL is printed, and the
    /// flow waits for the callback regardless.
    #[test]
    fn a_failed_opener_is_not_a_failed_login() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(browser_replies("devops-token"), None);
        let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
        let store = InMemoryStore::new();
        let browser = ScriptedBrowser::with_failing_opener(CallbackReply::Code);
        let mut announce = Vec::new();
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );

        run_scripted(&mut context, &identity, &accounts, &browser, &mut announce)
            .expect("an opener that fails is not a login that fails");

        assert!(browser.opened().is_some(), "the opener was still called");
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Browser, "devops-token"))
        );
        assert!(
            String::from_utf8(announce)
                .expect("utf-8")
                .contains("Opening browser to sign in to myorg..."),
            "the URL is printed whether or not the browser opened"
        );
    }

    /// A closed stdout cannot show the URL, so the flow stops before opening
    /// anything: a silent success, like the device path's code (spec §6.3, R19).
    #[test]
    fn a_closed_stdout_stops_the_browser_flow_before_the_opener() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
        let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut context = test_context(
            opts(None, None, None, false),
            org_env("myorg"),
            InMemoryStore::new(),
            &home,
        );

        let report = run_scripted(
            &mut context,
            &identity,
            &accounts,
            &browser,
            &mut ClosedStdout,
        )
        .expect("a closed stdout is a silent success");

        assert_eq!(report, Report::Text(String::new()));
        assert!(
            browser.opened().is_none(),
            "no browser is opened when the URL cannot be shown"
        );
        assert!(identity.received().is_empty());
    }

    /// The ARM→DevOps exchange tries `organizations` and falls back to `consumers`,
    /// exactly as the device path's does (the frozen reuses
    /// `exchange_refresh_for_devops/2` for both).
    #[test]
    fn the_browser_exchange_falls_back_to_the_msa_tenant() {
        let home = TempHome::new();
        let identity = FakeIdentity::start(
            vec![
                reply(
                    200,
                    json!({"access_token": "arm-token", "refresh_token": "refresh-token"}),
                ),
                reply(400, json!({"error": "invalid_grant"})),
                devops_token("devops-token"),
            ],
            None,
        );
        let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
        let store = InMemoryStore::new();
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            store.clone(),
            &home,
        );

        run_scripted(
            &mut context,
            &identity,
            &accounts,
            &browser,
            &mut Vec::new(),
        )
        .expect("the consumers fallback answers");

        let exchanges: Vec<String> = identity
            .received()
            .into_iter()
            .filter(|request| request.grant() == Some("refresh_token"))
            .map(|request| request.path)
            .collect();
        assert_eq!(
            exchanges,
            ["/organizations/oauth2/token", "/consumers/oauth2/token"]
        );
        assert_eq!(
            store.get("myorg").expect("get"),
            Some(stored(AuthMethod::Browser, "devops-token"))
        );
    }

    /// The ARM exchange's refusals: the oracle's message with the server's own words,
    /// and its `Invalid ARM token response` for a 200 that carries no access token.
    #[test]
    fn a_refused_code_exchange_is_the_oracles_message() {
        let refused = FakeIdentity::start(
            vec![reply(
                400,
                json!({"error": "invalid_grant", "error_description": "AADSTS9002313: Invalid request."}),
            )],
            None,
        );
        let home = TempHome::new();
        let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            InMemoryStore::new(),
            &home,
        );

        let error = run_scripted(&mut context, &refused, &accounts, &browser, &mut Vec::new())
            .expect_err("the code was refused");

        assert_eq!(
            error.message,
            "Login failed: ARM token exchange failed (HTTP 400): AADSTS9002313: Invalid request."
        );

        let empty = FakeIdentity::start(vec![reply(200, json!({"token_type": "Bearer"}))], None);
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg"),
            InMemoryStore::new(),
            &home,
        );

        let error = run_scripted(&mut context, &empty, &accounts, &browser, &mut Vec::new())
            .expect_err("no access token");

        assert_eq!(error.message, "Login failed: Invalid ARM token response");
    }

    /// `ADO_OAUTH_CLIENT_ID` is threaded into both interactive flows (spec §4.7): the
    /// authorize URL, the code exchange, the device-code request and its poll all
    /// carry it.
    #[test]
    fn the_oauth_client_id_override_reaches_both_flows() {
        let home = TempHome::new();
        let override_id = "11111111-2222-3333-4444-555555555555";
        let identity = FakeIdentity::start(browser_replies("devops-token"), None);
        let accounts = FakeIdentity::start(Vec::new(), Some(reply(500, json!({}))));
        let store = InMemoryStore::new();
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg").set(ENV_OAUTH_CLIENT_ID, override_id),
            store.clone(),
            &home,
        );

        run_scripted(
            &mut context,
            &identity,
            &accounts,
            &browser,
            &mut Vec::new(),
        )
        .expect("the browser login");

        let url = browser.url();
        assert!(url.contains(&format!("client_id={override_id}")), "{url}");
        assert_eq!(
            identity.received()[0].form_value("client_id"),
            Some(override_id)
        );

        // The device flow reads the same variable.
        let device_fake = FakeIdentity::start(
            vec![
                device_code_reply(0),
                granted(),
                devops_token("devops-token"),
            ],
            None,
        );
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg").set(ENV_OAUTH_CLIENT_ID, override_id),
            store,
            &home,
        );

        run(
            &mut context,
            Some("device"),
            device_fake.base_url(),
            &mut Vec::new(),
        )
        .expect("the device login");

        let received = device_fake.received();
        assert_eq!(
            received[0].form_value("client_id"),
            Some(override_id),
            "the device-code request"
        );
        assert_eq!(
            received[1].form_value("client_id"),
            Some(override_id),
            "the poll"
        );
        assert_eq!(
            received
                .last()
                .expect("the exchange")
                .form_value("client_id"),
            Some(override_id),
            "the ARM→DevOps exchange"
        );

        // A blank override is not a value (D16).
        let blank = FakeIdentity::start(browser_replies("devops-token"), None);
        let browser = ScriptedBrowser::new(CallbackReply::Code);
        let mut context = test_context(
            opts(None, None, None, true),
            org_env("myorg").set(ENV_OAUTH_CLIENT_ID, "  "),
            InMemoryStore::new(),
            &home,
        );

        run_scripted(&mut context, &blank, &accounts, &browser, &mut Vec::new())
            .expect("the blank override falls back");

        assert!(browser.url().contains(&format!("client_id={CLIENT_ID}")));
        assert_eq!(blank.received()[0].form_value("client_id"), Some(CLIENT_ID));
    }
}
