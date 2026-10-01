//! The browser OAuth 2.0 authorization-code flow with PKCE, in the shape
//! `lib/ado_cli/auth.ex` implements it (`login_browser/1`, `build_authorize_url/3`,
//! `listen_for_code/1`, `exchange_code_for_arm/3`, `save_browser_token/2`,
//! `auto_detect_org/1`).
//!
//! Four things are injected rather than hardcoded, so the whole flow can be driven
//! against local fakes: `identity_base` and `accounts_base`, the [`Opener`] (the
//! system browser) and the [`Callback`] (the loopback listener). The listener's
//! bound port is the `redirect_uri`; it binds port 0 and uses the port the OS gave
//! it, where the oracle finds a free port by opening and closing a socket first —
//! the same redirect URI, with the race removed rather than moved.
//!
//! The frozen listener waits 2 s of silence before it parses a request. This port
//! reads until the request is complete (the header block's end, plus a declared
//! body), and treats a peer that closes early the way it treats the read timeout:
//! whatever arrived is parsed. That is what makes a `form_post` body work at all —
//! the frozen `extract_body/1` uses a `:binary.match/2` byte offset as a
//! `String.slice/2` grapheme index, so it drops the `Content-Length` header it just
//! found and reads an empty body for every POST (captured in Task 10's report).

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::auth::identity::{
    agent, auth_failed, endpoint, get_bearer, post_form, reason, string_field,
};
use crate::auth::{device_code, org_required};
use crate::error::AdoError;

/// The accounts origin the org auto-detect asks when `--org` was absent
/// (`auto_detect_org/1`). The frozen CLI hardcodes it; this is the production value
/// of the seam.
pub const ACCOUNTS_BASE: &str = "https://app.vssps.visualstudio.com";

/// The reply page (`send_response/2`), 51 bytes. The oracle's own string, with the
/// header set it wraps it in.
pub const REPLY_PAGE: &str = "Authentication complete. You may close this window.";

/// The oracle's `:gen_tcp.accept(listener, 120_000)`: one accepted callback, or
/// "Browser login timed out or was cancelled.".
pub const ACCEPT_TIMEOUT: Duration = Duration::from_secs(120);

/// The oracle's `:gen_tcp.recv(socket, 0, 2000)`: a request that has not completed
/// by then is parsed as it stands.
pub const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// The largest header block the listener will buffer: 8 KiB without a `\r\n\r\n`
/// is not a browser callback, so the read stops and the bytes are parsed as they
/// stand (the frozen `recv_all/2` grows the block without a cap).
const MAX_HEADER_BYTES: usize = 8 * 1024;

/// The cap on the whole request, body included: a header declaring gigabytes of
/// `Content-Length` cannot grow the buffer past this before the read stops.
const MAX_REQUEST_BYTES: usize = 1 << 20;

/// The read phase's whole deadline. The frozen bounds each `recv` at two seconds
/// but lets a peer that dribbles a byte just under that hold the phase open
/// forever; this bounds the phase as one interval and parses what arrived.
const READ_DEADLINE: Duration = Duration::from_secs(10);

/// `@tenant`, the issuer the authorize URL and the code exchange use (the v2.0
/// paths, where the device flow's poll and the refresh exchange use the v1.0 ones).
const TENANT: &str = "organizations";

/// `@arm_resource` — the resource the PKCE authorization is for.
const ARM_RESOURCE: &str = "https://management.core.windows.net";

/// `@redirect_uri` — the loopback base every callback port is appended to.
const REDIRECT_BASE: &str = "http://localhost";

/// The CP1 claims blob (`~S'…'` in the frozen source: the spaces are literal).
const CLAIMS: &str = r#"{"access_token": {"xms_cc": {"values": ["CP1"]}}}"#;

/// The authorize URL's `scope`: ARM's `.default` plus the three OIDC scopes.
const SCOPE_SUFFIX: &str = "/.default offline_access openid profile";

/// The system browser. The one part of the flow a test cannot run for real, so it
/// is a trait: the flow prints the URL before it opens anything, and a failed
/// opener is not a failed login (the user can follow the printed URL).
pub trait Opener {
    fn open(&self, url: &str) -> Result<(), String>;
}

/// `open_browser/1`: `open` on macOS, `xdg-open` elsewhere on Unix, and
/// `cmd /c start` on Windows — the oracle's three, in its order.
pub struct SystemOpener;

impl Opener for SystemOpener {
    fn open(&self, url: &str) -> Result<(), String> {
        let (program, args): (&str, &[&str]) = if cfg!(target_os = "windows") {
            ("cmd", &["/c", "start"])
        } else if cfg!(target_os = "macos") {
            ("open", &[])
        } else {
            ("xdg-open", &[])
        };

        let status = Command::new(program)
            .args(args)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| error.to_string())?;

        if status.success() {
            Ok(())
        } else {
            Err(format!("{program} exited with {status}"))
        }
    }
}

/// The authorization code and the `state` its callback carried (`nil` when the
/// callback carried none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorization {
    pub code: String,
    pub state: Option<String>,
}

/// One accepted callback: the loopback listener in production, a scripted answer in
/// the tests. `redirect_uri` is on the seam because the flow builds the authorize
/// URL *before* it waits, so the port has to be known first.
pub trait Callback {
    fn redirect_uri(&self) -> String;
    fn wait(&self) -> Result<Authorization, AdoError>;
}

/// `listen_for_code/1` + `wait_for_callback/1`: bind `127.0.0.1:0`, accept one
/// callback, reply with [`REPLY_PAGE`], and answer what the request carried. The
/// listener lives in an `Option` so [`LoopbackCallback::wait`] can take it: it is
/// dropped — closed — as soon as the callback is answered, like the oracle's
/// `after` clause, instead of living to the end of the flow.
pub struct LoopbackCallback {
    listener: Mutex<Option<TcpListener>>,
    port: u16,
    accept_timeout: Duration,
    read_timeout: Duration,
}

impl LoopbackCallback {
    pub fn bind() -> Result<LoopbackCallback, AdoError> {
        LoopbackCallback::bind_with(ACCEPT_TIMEOUT, READ_TIMEOUT)
    }

    fn bind_with(
        accept_timeout: Duration,
        read_timeout: Duration,
    ) -> Result<LoopbackCallback, AdoError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|error| {
            auth_failed(format!("Cannot listen for the browser callback: {error}"))
        })?;
        listener.set_nonblocking(true).map_err(|error| {
            auth_failed(format!("Cannot listen for the browser callback: {error}"))
        })?;
        let port = listener
            .local_addr()
            .map_err(|error| {
                auth_failed(format!("Cannot listen for the browser callback: {error}"))
            })?
            .port();

        Ok(LoopbackCallback {
            listener: Mutex::new(Some(listener)),
            port,
            accept_timeout,
            read_timeout,
        })
    }
}

impl Callback for LoopbackCallback {
    fn redirect_uri(&self) -> String {
        format!("{REDIRECT_BASE}:{}", self.port)
    }

    fn wait(&self) -> Result<Authorization, AdoError> {
        let Some(listener) = self.listener.lock().expect("the callback listener").take() else {
            return Err(auth_failed(
                "Cannot accept the browser callback: this listener has already answered",
            ));
        };
        let deadline = Instant::now() + self.accept_timeout;

        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(auth_failed("Browser login timed out or was cancelled."));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => {
                    return Err(auth_failed(format!(
                        "Cannot accept the browser callback: {error}"
                    )));
                }
            }
        };

        // A socket accepted from a non-blocking listener inherits the flag on
        // macOS, so the accepted stream is put back in blocking mode before it is
        // read.
        let _ = stream.set_nonblocking(false);
        let data = read_request(&mut stream, self.read_timeout, READ_DEADLINE);
        let result = parse_request(&data);
        let _ = stream.write_all(&reply_bytes());
        let _ = stream.flush();

        result
    }
}

/// The HTTP reply the oracle's `send_response/2` writes, byte for byte.
fn reply_bytes() -> Vec<u8> {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{REPLY_PAGE}",
        length = REPLY_PAGE.len(),
    )
    .into_bytes()
}

/// The request, as far as the browser got: the header block's end, plus a body of
/// the length a `Content-Length` header declares. A request without one is complete
/// once the headers are — which is what makes a GET instant, where the frozen waits
/// its two seconds every time. The read is bounded three ways the frozen is not:
/// `MAX_HEADER_BYTES`, `MAX_REQUEST_BYTES` and the read deadline.
fn read_request(
    stream: &mut TcpStream,
    read_timeout: Duration,
    read_deadline: Duration,
) -> Vec<u8> {
    let _ = stream.set_read_timeout(Some(read_timeout));
    let deadline = Instant::now() + read_deadline;
    let mut buffer = Vec::new();

    loop {
        let headers_done = header_end(&buffer).is_some();

        if Instant::now() >= deadline
            || buffer.len() >= MAX_REQUEST_BYTES
            || (!headers_done && buffer.len() >= MAX_HEADER_BYTES)
        {
            break;
        }

        let mut chunk = [0_u8; 1024];

        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                if request_is_complete(&buffer) {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    buffer
}

fn request_is_complete(buffer: &[u8]) -> bool {
    let Some(head_end) = header_end(buffer) else {
        return false;
    };

    match content_length(&buffer[..head_end]) {
        Some(length) => buffer.len() - head_end >= length,
        None => true,
    }
}

/// The index just past the header block's `\r\n\r\n`.
fn header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

/// The `Content-Length` a header block declares, matched case-insensitively the
/// way HTTP requires (the frozen's `:binary.match(data, "Content-Length: ")` is
/// case-sensitive, which is the second half of its POST bug). A header without a
/// usable number is no header.
fn content_length(head: &[u8]) -> Option<usize> {
    String::from_utf8_lossy(head)
        .split("\r\n")
        .skip(1)
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;

            if name.trim().eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
}

/// `extract_code_from_request/1` → `parse_request_line/1` + `extract_body/1` +
/// `oauth_params/3` + `extract_oauth_result/1`.
fn parse_request(data: &[u8]) -> Result<Authorization, AdoError> {
    let (head, body) = match header_end(data) {
        Some(index) => (&data[..index], &data[index..]),
        None => (data, &[][..]),
    };
    let head = String::from_utf8_lossy(head);
    let mut request_line = head.split("\r\n").next().unwrap_or_default().split(' ');
    let method = request_line.next().unwrap_or_default();
    let path = request_line.next().unwrap_or("/");

    let params = match method {
        // `oauth_params("POST", _path, body) when body != ""`.
        "POST" if !body.is_empty() => decode_query(&String::from_utf8_lossy(body)),
        "GET" => decode_query(path.split_once('?').map_or("", |(_, query)| query)),
        _ => Vec::new(),
    };

    let code = param(&params, "code");

    match (code, param(&params, "state"), param(&params, "error")) {
        (Some(code), state, _) => Ok(Authorization {
            code: code.to_owned(),
            state: state.map(str::to_owned),
        }),
        (None, _, Some(error)) => Err(auth_failed(format!("Authorization failed: {error}"))),
        (None, _, None) => Err(auth_failed("No authorization code received.")),
    }
}

fn param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

/// `URI.decode_query/1` for the shapes the callbacks carry: `&`-separated pairs,
/// `=` separating key from value (a pair without one has an empty value), `+` a
/// space, `%XX` a byte, and an invalid escape left alone.
fn decode_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (decode_form(key), decode_form(value)),
            None => (decode_form(pair), String::new()),
        })
        .collect()
}

fn decode_form(value: &str) -> String {
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

/// `generate_state/0`: 16 random bytes, base64url, unpadded.
pub fn random_state() -> String {
    random_token(16)
}

/// `generate_code_verifier/0`: 32 random bytes, base64url, unpadded (43 characters,
/// the RFC 7636 low bound).
fn random_verifier() -> String {
    random_token(32)
}

fn random_token(bytes: usize) -> String {
    let mut buffer = vec![0_u8; bytes];

    // `:crypto.strong_rand_bytes/1`'s job: these are the CSRF token, the PKCE
    // verifier and the nonce, so the randomness has to come from the OS.
    getrandom::fill(&mut buffer).expect("the OS random source");

    URL_SAFE_NO_PAD.encode(buffer)
}

/// `generate_code_challenge/1`: `base64url(SHA256(verifier))`, unpadded.
pub fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// `build_authorize_url/3`: the parameter order is `URI.encode_query/1` over the
/// frozen map — Erlang map term order, captured byte for byte and stable across the
/// oracle's runs — so it is a fixed list here rather than a map's iteration order.
pub fn authorize_url(
    identity_base: &str,
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    nonce: &str,
) -> String {
    let scope = format!("{ARM_RESOURCE}{SCOPE_SUFFIX}");
    let pairs = [
        ("scope", scope.as_str()),
        ("state", state),
        ("client_info", "1"),
        ("prompt", "select_account"),
        ("claims", CLAIMS),
        ("client_id", client_id),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("nonce", nonce),
        ("redirect_uri", redirect_uri),
        ("response_mode", "query"),
        ("response_type", "code"),
    ];
    let query: Vec<String> = pairs
        .iter()
        .map(|(key, value)| format!("{key}={}", encode_component(value)))
        .collect();

    format!(
        "{}/{TENANT}/oauth2/v2.0/authorize?{}",
        identity_base.trim_end_matches('/'),
        query.join("&")
    )
}

/// `URI.encode_www_form/1`: every byte outside RFC 3986's unreserved set
/// percent-encoded with uppercase hex, and a space as `+`.
fn encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            b' ' => encoded.push('+'),
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }

    encoded
}

/// Everything the browser flow needs that production knows and a test must be able
/// to replace.
pub struct BrowserFlow<'a> {
    /// `https://login.microsoftonline.com` in production.
    pub identity_base: &'a str,
    /// [`ACCOUNTS_BASE`] in production.
    pub accounts_base: &'a str,
    /// `ADO_OAUTH_CLIENT_ID`, else the Azure CLI public client (`CLIENT_ID`).
    pub client_id: &'a str,
    /// The `--org`/`ADO_ORG` hint. `None` means the org is auto-detected from the
    /// fresh token (and the flow cannot succeed without one — D26's extension).
    pub org: Option<&'a str>,
    pub opener: &'a dyn Opener,
    pub callback: &'a dyn Callback,
    /// Where the "Opening browser…" block goes. `json` picks the one
    /// `{"ok":true,"message":…}` line the other flows' announcements use.
    pub json: bool,
    pub announce: &'a mut dyn Write,
}

/// What a browser login did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserLogin {
    /// The exchange succeeded: the credential is the caller's to store under `org`.
    Authenticated { org: String, token: String },
    /// stdout was closed before the URL could be shown, so the flow stopped where
    /// the device path stops when its code cannot be shown: a silent success (spec
    /// §6.3, R19), and no browser was opened.
    NotShown,
}

/// `login_browser/1`. Answers the organization the login was for: the hint, or the
/// one auto-detect adopted.
pub fn login(flow: &mut BrowserFlow<'_>) -> Result<BrowserLogin, AdoError> {
    let state = random_state();
    let verifier = random_verifier();
    let nonce = random_state();
    let challenge = code_challenge(&verifier);
    let redirect_uri = flow.callback.redirect_uri();
    let url = authorize_url(
        flow.identity_base,
        flow.client_id,
        &redirect_uri,
        &challenge,
        &state,
        &nonce,
    );

    if let Announcement::NotShown = announce_opening(flow, &url)? {
        return Ok(BrowserLogin::NotShown);
    }

    // A failed opener is not a failed login: the URL is already printed, so the
    // user can open it themselves.
    let _ = flow.opener.open(&url);

    let Authorization {
        code,
        state: returned,
    } = flow.callback.wait()?;

    if returned.as_deref() != Some(state.as_str()) {
        return Err(auth_failed("State mismatch — possible CSRF attack."));
    }

    let refresh = exchange_code_for_arm(
        flow.identity_base,
        flow.client_id,
        &code,
        &redirect_uri,
        &verifier,
    )?;
    let token = device_code::exchange(flow.identity_base, flow.client_id, refresh.as_deref())?;

    let org = match flow.org {
        Some(org) => org.to_owned(),
        None => detect_org(flow, &token)?,
    };

    Ok(BrowserLogin::Authenticated { org, token })
}

/// Whether the flow's opening announcement reached its reader.
enum Announcement {
    Shown,
    NotShown,
}

/// The oracle's three announcement lines and the URL the browser should open. The
/// URL is the one thing a user cannot reconstruct, so it is printed before the
/// opener runs and a closed stdout stops the flow there, like the device path's
/// code.
fn announce_opening(flow: &mut BrowserFlow<'_>, url: &str) -> Result<Announcement, AdoError> {
    let org_hint = match flow.org {
        Some(org) => format!(" to sign in to {org}"),
        None => String::new(),
    };
    let text = if flow.json {
        let envelope = serde_json::to_string(&crate::envelope::ok_message(&format!(
            "Opening browser{org_hint}...\n  {url}"
        )))
        .map_err(|error| AdoError::validation(error.to_string()))?;

        format!("{envelope}\n")
    } else {
        // The URL is on its own line: the oracle prints the two-space indent with
        // `CLI.write/1`, whose CliMate implementation is `IO.puts` and therefore
        // ends the line. The captured bytes are the contract (Task 10's
        // pre-harness comparison), not the source's intent.
        format!("\nOpening browser{org_hint}...\n  \n{url}\n\n")
    };

    match flow.announce.write_all(text.as_bytes()) {
        Ok(()) => Ok(Announcement::Shown),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(Announcement::NotShown),
        Err(error) => Err(AdoError::validation(error.to_string())),
    }
}

/// `auto_detect_org/1`: the account's organizations from the fresh token. Zero
/// organizations — or a lookup that failed — has nothing to key a credential to, so
/// it is the D26 refusal after the oracle's own lines; several are printed and not
/// adopted, likewise. One is adopted and printed.
fn detect_org(flow: &mut BrowserFlow<'_>, token: &str) -> Result<String, AdoError> {
    match list_accounts(flow.accounts_base, token) {
        Ok(accounts) if accounts.len() == 1 => {
            let org = accounts.into_iter().next().expect("one account");
            announce_text(flow, &format!("Detected org: {org}\n"));

            Ok(org)
        }
        Ok(accounts) if accounts.len() > 1 => {
            let names = accounts.join(", ");
            announce_text(
                flow,
                &format!(
                    "Multiple organizations found: {names}\nRe-run with --org <name> to pick one.\n"
                ),
            );

            Err(org_required("browser"))
        }
        // The oracle's `save_browser_token/2` prints exactly these two lines for a
        // login it could not key — for an empty account list and for a lookup that
        // failed alike — and then succeeds with no organization. This build cannot
        // record that login, so it refuses instead (D26's extension), and the token
        // is deliberately not stored even though the exchange succeeded.
        _ => {
            announce_text(
                flow,
                "No Azure DevOps organizations were found for this account.\nSet your org with: export ADO_ORG=<your-org>\n",
            );

            Err(org_required("browser"))
        }
    }
}

/// The informational lines: best effort, because the login itself is already done
/// by the time they are written. A closed stdout must not turn a completed
/// exchange into a failure.
fn announce_text(flow: &mut BrowserFlow<'_>, text: &str) {
    let text = if flow.json {
        let Ok(envelope) = serde_json::to_string(&crate::envelope::ok_message(text.trim_end()))
        else {
            return;
        };

        format!("{envelope}\n")
    } else {
        text.to_owned()
    };

    let _ = flow.announce.write_all(text.as_bytes());
}

/// `list_accounts/1` + `parse_accounts_body/1` + `extract_account_names/1`. Every
/// failure — a transport error, a non-200, a body that is neither `{"value": […]}` nor
/// a top-level array — is the caller's `{:error, _}`: no organizations.
fn list_accounts(accounts_base: &str, token: &str) -> Result<Vec<String>, AdoError> {
    let agent = agent();
    let (status, body) = get_bearer(&agent, &endpoint(accounts_base, "_apis/accounts"), token)?;

    if status != 200 {
        return Err(auth_failed(format!(
            "Accounts request failed (HTTP {status})"
        )));
    }

    let value = serde_json::from_str::<Value>(&body)
        .map_err(|error| auth_failed(format!("Could not parse the accounts response: {error}")))?;

    // The accounts API answers a top-level array, not the usual `{"value": […]}`.
    let accounts = match value {
        Value::Object(mut map) => match map.remove("value") {
            Some(Value::Array(accounts)) => accounts,
            _ => return Err(auth_failed("The accounts response has no value list.")),
        },
        Value::Array(accounts) => accounts,
        _ => return Err(auth_failed("The accounts response is not a list.")),
    };

    Ok(accounts
        .iter()
        .filter_map(account_name)
        .filter(|name| !name.is_empty())
        .collect())
}

/// `a["AccountName"] || a["accountName"] || a["accountUri"] || ""`: the first key
/// that is not `nil`/`false` wins even when it is `""` (Elixir's `||`, where `""` is
/// truthy), and the caller drops the empty ones.
fn account_name(account: &Value) -> Option<String> {
    ["AccountName", "accountName", "accountUri"]
        .iter()
        .find_map(|key| {
            account
                .get(*key)
                .filter(|value| !value.is_null() && **value != Value::Bool(false))
        })
        .map(|value| match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
}

/// `exchange_code_for_arm/3`: the authorization code for an ARM access token and a
/// refresh token. Only the refresh token is answered — the access token is the
/// oracle's discarded `_arm_token` — and a grant without one is the shared
/// exchange's own error.
fn exchange_code_for_arm(
    identity_base: &str,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<Option<String>, AdoError> {
    let agent = agent();
    let (status, body) = post_form(
        &agent,
        &endpoint(identity_base, &format!("{TENANT}/oauth2/v2.0/token")),
        &[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ],
    )?;

    if status != 200 {
        return Err(auth_failed(format!(
            "ARM token exchange failed (HTTP {status}): {}",
            reason(&body)
        )));
    }

    let value = serde_json::from_str::<Value>(&body).unwrap_or(Value::Null);

    match string_field(&value, "access_token") {
        Some(_) => Ok(value
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_owned)),
        None => Err(auth_failed("Invalid ARM token response")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    /// The frozen module's constants, verbatim (`lib/ado_cli/auth.ex`): the paths,
    /// the resource, the redirect base, the claims blob and the two timeouts.
    #[test]
    fn the_constants_are_the_elixir_modules() {
        assert_eq!(ACCOUNTS_BASE, "https://app.vssps.visualstudio.com");
        assert_eq!(TENANT, "organizations");
        assert_eq!(ARM_RESOURCE, "https://management.core.windows.net");
        assert_eq!(REDIRECT_BASE, "http://localhost");
        assert_eq!(SCOPE_SUFFIX, "/.default offline_access openid profile");
        assert_eq!(ACCEPT_TIMEOUT, Duration::from_secs(120));
        assert_eq!(READ_TIMEOUT, Duration::from_secs(2));
        assert_eq!(REPLY_PAGE.len(), 51);
    }

    /// RFC 7636 appendix B's vector: the derivation is the spec's, not just this
    /// build's.
    #[test]
    fn the_code_challenge_is_rfc_7636s_vector() {
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    /// The captured oracle URL, with the random parts substituted: the port writes
    /// the same bytes, in the same parameter order.
    #[test]
    fn the_authorize_url_is_the_captured_one() {
        let url = authorize_url(
            "https://login.microsoftonline.com",
            "04b07795-8ddb-461a-bbee-02f9e1bf7b46",
            "http://localhost:63922",
            "7ILv3pA2yDpY2J6wX0l7Va4zUmV-Op7yooJmfjEL7S8",
            "4AQC9WmIu89Okbq9PgnCOg",
            "3W9zjaQuK1Lg-EjHI5thCw",
        );

        assert_eq!(
            url,
            concat!(
                "https://login.microsoftonline.com/organizations/oauth2/v2.0/authorize?",
                "scope=https%3A%2F%2Fmanagement.core.windows.net%2F.default+offline_access+openid+profile",
                "&state=4AQC9WmIu89Okbq9PgnCOg",
                "&client_info=1",
                "&prompt=select_account",
                "&claims=%7B%22access_token%22%3A+%7B%22xms_cc%22%3A+%7B%22values%22%3A+%5B%22CP1%22%5D%7D%7D%7D",
                "&client_id=04b07795-8ddb-461a-bbee-02f9e1bf7b46",
                "&code_challenge=7ILv3pA2yDpY2J6wX0l7Va4zUmV-Op7yooJmfjEL7S8",
                "&code_challenge_method=S256",
                "&nonce=3W9zjaQuK1Lg-EjHI5thCw",
                "&redirect_uri=http%3A%2F%2Flocalhost%3A63922",
                "&response_mode=query",
                "&response_type=code",
            )
        );
    }

    #[test]
    fn the_form_encoding_is_uri_encode_www_form() {
        assert_eq!(encode_component("a b"), "a+b");
        assert_eq!(encode_component("~*-._"), "~%2A-._");
        assert_eq!(
            encode_component(r#"{"access_token": {"xms_cc": {"values": ["CP1"]}}}"#),
            "%7B%22access_token%22%3A+%7B%22xms_cc%22%3A+%7B%22values%22%3A+%5B%22CP1%22%5D%7D%7D%7D"
        );
        assert_eq!(
            encode_component("http://localhost:1"),
            "http%3A%2F%2Flocalhost%3A1"
        );
    }

    #[test]
    fn the_generated_tokens_are_unpadded_base64url() {
        for state in [random_state(), random_state(), random_state()] {
            assert_eq!(state.len(), 22);
            assert!(
                state
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{state}"
            );
        }

        let challenge = code_challenge(&random_verifier());
        assert_eq!(challenge.len(), 43);
        assert!(!challenge.contains('='));
    }

    #[test]
    fn decoding_is_uri_decode_query() {
        assert_eq!(
            decode_query("code=A%2Fb&state=x-y_z"),
            vec![
                ("code".to_owned(), "A/b".to_owned()),
                ("state".to_owned(), "x-y_z".to_owned())
            ]
        );
        assert_eq!(
            decode_query("code=A+B"),
            vec![("code".to_owned(), "A B".to_owned())],
            "a form's space"
        );
        assert_eq!(
            decode_query("code=%zz&x"),
            vec![
                ("code".to_owned(), "%zz".to_owned()),
                ("x".to_owned(), String::new())
            ],
            "an invalid escape is left alone; a pair without `=` has an empty value"
        );
    }

    // ── the request parser ────────────────────────────────────────────────

    fn get(query: &str) -> Result<Authorization, AdoError> {
        parse_request(format!("GET /{query} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
    }

    #[test]
    fn a_get_callback_carries_the_code_and_state() {
        assert_eq!(
            get("?code=the-code&state=the-state").expect("a code"),
            Authorization {
                code: "the-code".to_owned(),
                state: Some("the-state".to_owned())
            }
        );
    }

    #[test]
    fn a_post_callback_carries_the_form_body() {
        let request = "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 30\r\n\
                       Content-Type: application/x-www-form-urlencoded\r\n\r\n\
                       code=the-code&state=the-state";

        assert_eq!(
            parse_request(request.as_bytes()).expect("a code"),
            Authorization {
                code: "the-code".to_owned(),
                state: Some("the-state".to_owned())
            },
            "the form_post body, which the frozen parser never reads"
        );
    }

    #[test]
    fn the_content_length_header_is_case_insensitive() {
        let request = "POST / HTTP/1.1\r\nHost: localhost\r\ncontent-length: 30\r\n\r\n\
                       code=the-code&state=the-state";

        assert_eq!(
            parse_request(request.as_bytes()).expect("a code").code,
            "the-code"
        );
    }

    #[test]
    fn a_code_without_a_state_is_still_a_code() {
        assert_eq!(
            get("?code=the-code").expect("a code"),
            Authorization {
                code: "the-code".to_owned(),
                state: None
            }
        );
    }

    #[test]
    fn the_oauth_error_and_the_missing_code_are_the_oracles_sentences() {
        assert_eq!(
            get("?error=access_denied").expect_err("an error").message,
            "Authorization failed: access_denied"
        );
        assert_eq!(
            get("?state=the-state").expect_err("no code").message,
            "No authorization code received."
        );
        assert_eq!(
            get("").expect_err("no code").message,
            "No authorization code received."
        );
        assert_eq!(
            parse_request(b"GARBAGE LINE\r\n\r\n")
                .expect_err("not HTTP")
                .message,
            "No authorization code received.",
            "a request that is not HTTP is parsed as it stands, never a panic"
        );
    }

    // ── the listeners ─────────────────────────────────────────────────────

    /// The reply bytes, header lines and body. The oracle's captured reply is these
    /// bytes plus one trailing `\n` (150 against 149), past its own declared
    /// `Content-Length: 51` — no HTTP client can observe it (D50).
    #[test]
    fn the_reply_is_the_oracles() {
        assert_eq!(
            String::from_utf8(reply_bytes()).expect("utf-8"),
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Length: 51\r\n\
             Connection: close\r\n\
             \r\n\
             Authentication complete. You may close this window."
        );
    }

    /// One accepted callback, over a real socket: the port is the redirect URI, the
    /// request is read, the reply is written, and the answer is what the request
    /// carried.
    #[test]
    fn the_listener_answers_a_get_and_replies() {
        let callback = LoopbackCallback::bind().expect("a listener");
        let redirect = callback.redirect_uri();
        assert!(redirect.starts_with("http://localhost:"));

        let waiter = std::thread::spawn(move || callback.wait());

        let reply = send_request(
            &redirect,
            b"GET /?code=the-code&state=the-state HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );

        assert_eq!(
            waiter.join().expect("the listener thread").expect("a code"),
            Authorization {
                code: "the-code".to_owned(),
                state: Some("the-state".to_owned())
            }
        );
        assert_eq!(
            String::from_utf8_lossy(&reply),
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 51\r\nConnection: close\r\n\r\nAuthentication complete. You may close this window."
        );
    }

    #[test]
    fn the_listener_reads_a_form_post_body() {
        let callback = LoopbackCallback::bind().expect("a listener");
        let redirect = callback.redirect_uri();
        let waiter = std::thread::spawn(move || callback.wait());

        send_request(
            &redirect,
            b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 30\r\n\r\ncode=the-code&state=the-state",
        );

        assert_eq!(
            waiter
                .join()
                .expect("the listener thread")
                .expect("a code")
                .code,
            "the-code"
        );
    }

    /// A body shorter than its `Content-Length`: the read timeout ends the wait and
    /// what arrived is parsed (the frozen's `recv_all/2` would have raised on the
    /// close instead).
    #[test]
    fn the_listener_uses_a_short_body() {
        let callback =
            LoopbackCallback::bind_with(Duration::from_millis(200), Duration::from_millis(200))
                .expect("a listener");
        let redirect = callback.redirect_uri();
        let waiter = std::thread::spawn(move || callback.wait());

        send_request(
            &redirect,
            b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 90\r\n\r\ncode=the-code&state=SHORT",
        );

        assert_eq!(
            waiter.join().expect("the listener thread").expect("a code"),
            Authorization {
                code: "the-code".to_owned(),
                state: Some("SHORT".to_owned())
            },
            "the short body is parsed as it stands"
        );
    }

    /// The accept timeout is the oracle's sentence, and it is the port's own clock
    /// (the constant above is 120 s; the test's hundred milliseconds is the seam).
    #[test]
    fn the_listener_times_out_with_the_oracles_sentence() {
        let callback = LoopbackCallback::bind_with(Duration::from_millis(100), READ_TIMEOUT)
            .expect("a listener");

        let error = callback.wait().expect_err("nobody called back");

        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert_eq!(error.message, "Browser login timed out or was cancelled.");
    }

    /// M-6: the listener is closed as soon as the callback is answered (the
    /// oracle's `after` clause), so a second connection is refused rather than
    /// queueing unanswered for the rest of the flow.
    #[test]
    fn the_listener_closes_once_the_callback_is_answered() {
        let callback = LoopbackCallback::bind().expect("a listener");
        let redirect = callback.redirect_uri();
        let port: u16 = redirect
            .trim_start_matches("http://localhost:")
            .parse()
            .expect("a port");
        let waiter = std::thread::spawn(move || callback.wait());

        send_request(
            &redirect,
            b"GET /?code=the-code&state=the-state HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        waiter.join().expect("the listener thread").expect("a code");

        assert!(
            TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err(),
            "the listener is closed once the callback is answered"
        );
    }

    /// M-7's header cap: a header block past `MAX_HEADER_BYTES` is not a browser
    /// callback, so the read stops there instead of growing without a bound.
    #[test]
    fn a_header_block_past_the_cap_is_parsed_as_it_stands() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a listener");
        let port = listener.local_addr().expect("the address").port();
        let writer = std::thread::spawn(move || {
            let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");
            write_until_blocked(&mut stream, &vec![b'a'; MAX_HEADER_BYTES + 64 * 1024]);
        });

        let (mut stream, _) = listener.accept().expect("accept");
        let started = Instant::now();
        let data = read_request(&mut stream, READ_TIMEOUT, READ_DEADLINE);
        let elapsed = started.elapsed();

        assert!(
            data.len() >= MAX_HEADER_BYTES && data.len() < MAX_HEADER_BYTES + 1024,
            "the read stopped at the header cap, not at the peer's close ({} bytes)",
            data.len()
        );
        assert!(
            elapsed < READ_TIMEOUT,
            "the cap ended the read, not the per-read timeout ({elapsed:?})"
        );

        drop(stream);
        writer.join().expect("the writer thread");
    }

    /// M-7's total cap: a declared `Content-Length` of gigabytes cannot grow the
    /// buffer past `MAX_REQUEST_BYTES`.
    #[test]
    fn a_declared_huge_body_stops_at_the_request_cap() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a listener");
        let port = listener.local_addr().expect("the address").port();
        let writer = std::thread::spawn(move || {
            let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");
            let header = "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4000000000\r\n\r\n";
            let _ = stream.write_all(header.as_bytes());
            write_until_blocked(&mut stream, &vec![b'b'; MAX_REQUEST_BYTES + 64 * 1024]);
        });

        let (mut stream, _) = listener.accept().expect("accept");
        let started = Instant::now();
        let data = read_request(&mut stream, READ_TIMEOUT, READ_DEADLINE);
        let elapsed = started.elapsed();

        assert!(
            data.len() >= MAX_REQUEST_BYTES && data.len() < MAX_REQUEST_BYTES + 1024,
            "the read stopped at the request cap ({} bytes)",
            data.len()
        );
        assert!(
            elapsed < READ_TIMEOUT,
            "the cap ended the read, not the per-read timeout ({elapsed:?})"
        );

        drop(stream);
        writer.join().expect("the writer thread");
    }

    /// M-7's deadline: a peer that dribbles just under the per-read timeout cannot
    /// hold the read phase open forever; the deadline ends it and the bytes so far
    /// are parsed.
    #[test]
    fn a_dribbling_peer_stops_at_the_read_deadline() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a listener");
        let port = listener.local_addr().expect("the address").port();
        let writer = std::thread::spawn(move || {
            let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");

            for _ in 0..50 {
                if stream.write_all(b"G").is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });

        let (mut stream, _) = listener.accept().expect("accept");
        let started = Instant::now();
        let data = read_request(&mut stream, READ_TIMEOUT, Duration::from_millis(200));
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_millis(800),
            "the deadline ended the read, not the dribbler's fifty bytes ({elapsed:?})"
        );
        assert!(
            data.len() < 50,
            "the read stopped before the dribbler finished ({} bytes)",
            data.len()
        );

        drop(stream);
        writer.join().expect("the writer thread");
    }

    /// A second `wait` on the same listener is refused: the listener was closed
    /// when the first callback was answered (M-6).
    #[test]
    fn a_second_wait_is_refused() {
        let callback = LoopbackCallback::bind().expect("a listener");
        let redirect = callback.redirect_uri();
        let client = std::thread::spawn(move || {
            send_request(
                &redirect,
                b"GET /?code=the-code&state=the-state HTTP/1.1\r\nHost: localhost\r\n\r\n",
            );
        });

        callback.wait().expect("the first callback");
        client.join().expect("the client thread");

        let error = callback
            .wait()
            .expect_err("the listener has already answered");
        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert_eq!(
            error.message,
            "Cannot accept the browser callback: this listener has already answered"
        );
    }

    /// The shared writer for the two cap tests: non-blocking, so a reader that
    /// stops at a cap does not deadlock the test, and the bytes already sent stay
    /// in the socket when the stream drops.
    fn write_until_blocked(stream: &mut TcpStream, bytes: &[u8]) {
        stream.set_nonblocking(true).expect("non-blocking");
        let mut written = 0;

        while written < bytes.len() {
            match stream.write(&bytes[written..]) {
                Ok(count) => written += count,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(_) => return,
            }
        }
    }

    fn send_request(redirect: &str, request: &[u8]) -> Vec<u8> {
        let port: u16 = redirect
            .trim_start_matches("http://localhost:")
            .parse()
            .expect("a port");
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");
        stream.write_all(request).expect("write");
        stream.flush().expect("flush");

        let mut reply = Vec::new();
        let _ = stream.read_to_end(&mut reply);

        reply
    }
}
