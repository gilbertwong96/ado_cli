//! End-to-end tests for `ado login`: the PAT method — its envelope, its exit
//! codes, and the credential it leaves on disk — and the validation surface both
//! methods share. The device-code flow itself is driven against a local fake
//! identity server in `crates/ado/src/commands/login.rs`, because the shipped
//! binary's identity endpoints are the real Microsoft ones: **no test in this suite
//! runs `login --method device` with an org**, since that would reach
//! `login.microsoftonline.com` (W1 adds no override for it).
//!
//! Every test owns its environment: a `TempHome` (which also means the OS keychain
//! is unreachable, so the credential file layer is the one that answers) with the
//! `ADO_*` variables removed unless the test sets them.

use std::fs;
use std::process::{Command, Output};

use ado_testkit::{MockResponse, MockServer, TempHome, ado_cmd, stderr_of, stdout_of};
use serde_json::{Value, json};

/// The frozen escript's `ado login --method pat --org <org> --pat <pat> --json`
/// envelope, captured with an isolated `HOME` and a throwaway organization: our
/// bytes differ only in `credentials_saved_to` (the new config file, D11b) and in
/// object key order (serde_json's `BTreeMap` vs Elixir's map order, D1 — the
/// comparison normalises with `jq -S`). The shape, the method spelling, the `org`
/// value and the `server: null` are the oracle's.
fn oracle_success_json(config_file: &str) -> String {
    format!(
        r#"{{"ok":true,"result":{{"credentials_saved_to":"{config_file}","method":"pat","org":"{ORG}","server":null}}}}"#
    )
}

/// The oracle's two human lines, with the config file it names. The oracle's
/// message is byte-identical apart from that path.
fn oracle_success_plain(config_file: &str) -> String {
    format!("\n  Logged in to {ORG} via Pat.\n  Credentials saved to {config_file}\n")
}

/// The organization-scoped path the next command builds from the stored login.
fn org_path(suffix: &str) -> String {
    format!("/{ORG}{suffix}")
}

/// The frozen escript's `--json` envelope for a missing `--pat`, captured with an
/// isolated `HOME`; byte-identical to ours.
const ORACLE_MISSING_PAT_JSON: &str = r#"{"error":{"code":"validation_error","details":{"env_var":"ADO_PAT","option":"--pat"},"message":"--pat is required for method=pat (or set ADO_PAT env var)"},"ok":false}"#;

/// The frozen escript's plain stderr line for the same invocation, minus the
/// colour the oracle wraps the label in (spec §8, R19).
const ORACLE_MISSING_PAT_PLAIN: &str =
    "[Validation error] --pat is required for method=pat (or set ADO_PAT env var)\n";

/// The frozen escript's `--json` envelope for a missing `--org`.
const ORACLE_MISSING_ORG_JSON: &str = r#"{"error":{"code":"validation_error","details":{"env_var":"ADO_ORG","option":"--org"},"message":"--org is required for method='pat' (or set ADO_ORG env var)"},"ok":false}"#;

/// The frozen escript's `--json` envelope for an unknown method. The two sides are
/// byte-identical since Wave 3's Task 10 shipped the browser method: the
/// suggestion list and the message name all three of the oracle's methods.
const ORACLE_UNKNOWN_METHOD_JSON: &str = r#"{"error":{"code":"validation_error","details":{"valid_methods":["browser","pat","device"]},"message":"Unknown method 'bogus'. Use 'browser', 'pat', or 'device'."},"ok":false}"#;

const UNKNOWN_METHOD_JSON: &str = ORACLE_UNKNOWN_METHOD_JSON;

/// The PAT the success tests store; asserted absent from `config.toml`.
const PAT: &str = "pat-secret-token";

/// The organization every test logs in to. Deliberately synthetic: `login` writes
/// the credential to the credential store, and on a developer machine with a live
/// secret service (a Linux desktop) that store *is* the real one — under `TempHome`
/// and on CI the keychain is unreachable, but a plausible name like `myorg` could
/// still collide with a real credential and overwrite it, which R26 forbids (R52).
const ORG: &str = "ado-cli-test-org";

fn command(home: &TempHome, args: &[&str]) -> Command {
    let mut command = ado_cmd();
    home.apply(&mut command);
    command
        .env_remove("ADO_ORG")
        .env_remove("ADO_PAT")
        .env_remove("ADO_SERVER")
        .args(args);
    command
}

fn run(home: &TempHome, args: &[&str]) -> Output {
    command(home, args).output().expect("run ado")
}

fn config_file(home: &TempHome) -> String {
    home.config_dir().join("config.toml").display().to_string()
}

fn credentials_file(home: &TempHome) -> std::path::PathBuf {
    home.config_dir().join("credentials.json")
}

/// The config file after a PAT login: `default_org` plus the organization's method
/// entry, and never the token (§7).
fn expected_config(org: &str, method: &str) -> String {
    format!("default_org = \"{org}\"\n\n[orgs.{org}]\nauth = \"{method}\"\n")
}

fn expected_credentials(org: &str, method: &str, token: &str) -> String {
    format!(
        "{{\n  \"{org}\": {{\n    \"method\": \"{method}\",\n    \"token\": \"{token}\"\n  }}\n}}"
    )
}

fn assert_success(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(output)
    );
}

/// Nothing a validation error can do may leave a login behind: no config file and
/// no credential file.
fn assert_nothing_written(home: &TempHome) {
    for path in [
        home.config_dir().join("config.toml"),
        credentials_file(home),
    ] {
        assert!(!path.exists(), "a failed login wrote {}", path.display());
    }
}

/// The keychain is unreachable under a `TempHome` (macOS resolves the default
/// keychain through `HOME`; Linux CI has no secret service), so the credential file
/// layer is what answers — which is also the evidence that no test wrote the real
/// keychain. On Windows the temp home cannot redirect the Credential Manager, so
/// the tests that store a credential are gated there.
#[cfg(not(target_os = "windows"))]
#[test]
fn pat_login_writes_the_credential_out_of_the_config_and_emits_the_oracle_envelope() {
    let home = TempHome::new();

    let output = run(
        &home,
        &[
            "login", "--method", "pat", "--org", ORG, "--pat", PAT, "--json",
        ],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    assert_eq!(
        stdout,
        format!("{}\n", oracle_success_json(&config_file(&home))),
        "the oracle's envelope, with this CLI's config file"
    );
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );

    let config = fs::read_to_string(home.config_dir().join("config.toml")).expect("config.toml");
    assert_eq!(config, expected_config(ORG, "pat"));
    assert!(
        !config.contains(PAT),
        "the token reached config.toml: {config}"
    );
    assert!(
        !config.contains("token"),
        "config.toml carries a token field: {config}"
    );

    let credentials = fs::read_to_string(credentials_file(&home)).expect("credentials.json");
    assert_eq!(credentials, expected_credentials(ORG, "pat", PAT));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(credentials_file(&home))
            .expect("the credential file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "credential file mode: {mode:o}");
    }
}

#[cfg(not(target_os = "windows"))]
#[test]
fn pat_login_plain_output_is_the_oracle_lines() {
    let home = TempHome::new();

    let output = run(
        &home,
        &["login", "--method", "pat", "--org", ORG, "--pat", PAT],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        oracle_success_plain(&config_file(&home))
    );
    assert!(
        !stdout_of(&output).contains('\u{1b}'),
        "ANSI in piped output: {:?}",
        stdout_of(&output)
    );
}

/// The self-review's whole point: the request path still finds the credential the
/// login stored — organization from `config.toml`'s `default_org`, token from the
/// store — with no `ADO_ORG`/`ADO_PAT` in the environment at all.
#[cfg(not(target_os = "windows"))]
#[test]
fn the_next_command_resolves_the_credential_the_login_stored() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &org_path("/_apis/projects"),
        MockResponse::from_fixture("projects_list"),
    );

    assert_success(&run(
        &home,
        &["login", "--method", "pat", "--org", ORG, "--pat", PAT],
    ));

    let mut list = command(&home, &["projects", "list", "--json"]);
    list.env("ADO_SERVER", server.base_url());
    let output = list.output().expect("run ado projects list");

    assert_success(&output);
    let received = server.received();
    assert_eq!(received.len(), 1, "the list reached the mock once");
    assert_eq!(received[0].path, org_path("/_apis/projects"));
    assert_eq!(
        received[0].header("authorization"),
        Some("Basic OnBhdC1zZWNyZXQtdG9rZW4="),
        "the stored token authenticates: Basic base64(':pat-secret-token')"
    );
}

/// The oracle reports the server it was given and does not persist it
/// (`Auth.login_pat/2` saves org, method and pat only); ours reports it and leaves
/// the config's `server` alone, which stays `whoami`'s field (W1-R10).
#[cfg(not(target_os = "windows"))]
#[test]
fn pat_login_reports_the_server_it_was_given_without_persisting_it() {
    let home = TempHome::new();

    let output = run(
        &home,
        &[
            "login",
            "--method",
            "pat",
            "--org",
            ORG,
            "--pat",
            PAT,
            "--server",
            "https://ado.example.com",
            "--json",
        ],
    );

    assert_success(&output);
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("a JSON document");
    assert_eq!(
        envelope["result"]["server"],
        json!("https://ado.example.com")
    );
    assert_eq!(
        fs::read_to_string(home.config_dir().join("config.toml")).expect("config.toml"),
        expected_config(ORG, "pat"),
        "the server is reported, not recorded"
    );

    let plain = run(
        &home,
        &[
            "login",
            "--method",
            "pat",
            "--org",
            ORG,
            "--pat",
            PAT,
            "--server",
            "https://ado.example.com",
        ],
    );
    assert!(stdout_of(&plain).contains(&format!(
        "  Logged in to {ORG} (https://ado.example.com) via Pat.\n"
    )));
}

/// `resolve_method/1`: a PAT on `--pat` selects `pat` when `--method` is omitted —
/// the invocation the skills and `AGENTS.md` document.
#[cfg(not(target_os = "windows"))]
#[test]
fn the_pat_flag_infers_the_pat_method() {
    let home = TempHome::new();

    let output = run(&home, &["login", "--org", ORG, "--pat", PAT, "--json"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", oracle_success_json(&config_file(&home)))
    );
}

/// The same inference from the environment (`resolve_method/1` checks `ADO_PAT`).
#[cfg(not(target_os = "windows"))]
#[test]
fn the_environment_pat_infers_the_pat_method() {
    let home = TempHome::new();
    let mut command = command(&home, &["login", "--method", "pat", "--org", ORG, "--json"]);
    command.env("ADO_PAT", PAT);

    let output = command.output().expect("run ado");

    assert_success(&output);
    assert_eq!(
        fs::read_to_string(credentials_file(&home)).expect("credentials.json"),
        expected_credentials(ORG, "pat", PAT)
    );
}

#[test]
fn login_without_a_pat_is_a_validation_error() {
    let home = TempHome::new();

    let json_output = run(&home, &["login", "--method", "pat", "--org", ORG, "--json"]);

    assert_eq!(json_output.status.code(), Some(1));
    assert_eq!(
        stdout_of(&json_output),
        format!("{ORACLE_MISSING_PAT_JSON}\n"),
        "the oracle's exact bytes"
    );
    assert!(
        stderr_of(&json_output).is_empty(),
        "stderr: {}",
        stderr_of(&json_output)
    );

    let plain = run(&home, &["login", "--method", "pat", "--org", ORG]);

    assert_eq!(plain.status.code(), Some(1));
    assert_eq!(stderr_of(&plain), ORACLE_MISSING_PAT_PLAIN);
    assert!(
        stdout_of(&plain).is_empty(),
        "stdout: {}",
        stdout_of(&plain)
    );
    assert_nothing_written(&home);
}

#[test]
fn login_without_an_org_is_a_validation_error() {
    let home = TempHome::new();

    let json_output = run(&home, &["login", "--method", "pat", "--pat", PAT, "--json"]);

    assert_eq!(json_output.status.code(), Some(1));
    assert_eq!(
        stdout_of(&json_output),
        format!("{ORACLE_MISSING_ORG_JSON}\n")
    );

    let plain = run(&home, &["login", "--method", "pat", "--pat", PAT]);

    assert_eq!(plain.status.code(), Some(1));
    assert_eq!(
        stderr_of(&plain),
        "[Validation error] --org is required for method='pat' (or set ADO_ORG env var)\n"
    );
    assert_nothing_written(&home);
}

/// A blank flag value is not a value (`env::non_empty`, D16/W1-R3), so a blank
/// `--pat`/`--org` is the missing-flag error rather than, as in the oracle, a
/// stored blank.
#[test]
fn a_blank_value_is_not_a_value() {
    let home = TempHome::new();

    let blank_pat = run(
        &home,
        &[
            "login", "--method", "pat", "--org", ORG, "--pat", "", "--json",
        ],
    );

    assert_eq!(blank_pat.status.code(), Some(1));
    assert_eq!(
        stdout_of(&blank_pat),
        format!("{ORACLE_MISSING_PAT_JSON}\n")
    );

    let blank_org = run(
        &home,
        &[
            "login", "--method", "pat", "--org", "  ", "--pat", PAT, "--json",
        ],
    );

    assert_eq!(blank_org.status.code(), Some(1));
    assert_eq!(
        stdout_of(&blank_org),
        format!("{ORACLE_MISSING_ORG_JSON}\n")
    );
    assert_nothing_written(&home);
}

/// The oracle rejects any method that is not `browser`, `pat` or `device`, and
/// `device_code` is one of them (`Unknown method 'device_code'`); this build
/// rejects the same set and names the same three methods (Wave 3 shipped the
/// browser flow, so the words and `valid_methods` are the oracle's exactly).
#[test]
fn an_unknown_method_is_a_validation_error() {
    let home = TempHome::new();

    let output = run(
        &home,
        &["login", "--method", "bogus", "--org", ORG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout_of(&output), format!("{UNKNOWN_METHOD_JSON}\n"));

    let spelled = run(
        &home,
        &["login", "--method", "device_code", "--org", ORG, "--json"],
    );

    assert_eq!(spelled.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&spelled)).expect("a JSON document");
    assert_eq!(
        envelope["error"]["message"],
        json!("Unknown method 'device_code'. Use 'browser', 'pat', or 'device'.")
    );
    assert_eq!(
        envelope["error"]["details"]["valid_methods"],
        json!(["browser", "pat", "device"])
    );
    assert_nothing_written(&home);
}

/// The environment twin of the flag test above: `ProcessEnv` answers `Ok("")` for a
/// variable that is set but empty, so the same predicate has to run on `ADO_PAT` and
/// `ADO_ORG`. Before that filter, `ADO_PAT= ado login --method pat --org ORG` stored
/// an empty token and printed success, and `ADO_ORG=` stored a credential keyed by
/// `""` (D16: a blank value is not a value, in the environment as well as on a flag).
#[test]
fn blank_environment_values_are_not_values() {
    let home = TempHome::new();

    for blank in ["", "  "] {
        let mut blank_pat = command(&home, &["login", "--method", "pat", "--org", ORG, "--json"]);
        blank_pat.env("ADO_PAT", blank);
        let blank_pat = blank_pat.output().expect("run ado");

        assert_eq!(
            blank_pat.status.code(),
            Some(1),
            "ADO_PAT={blank:?} is not a value"
        );
        assert_eq!(
            stdout_of(&blank_pat),
            format!("{ORACLE_MISSING_PAT_JSON}\n"),
            "ADO_PAT={blank:?}"
        );

        let mut blank_org = command(&home, &["login", "--method", "pat", "--json"]);
        blank_org.env("ADO_ORG", blank).env("ADO_PAT", PAT);
        let blank_org = blank_org.output().expect("run ado");

        assert_eq!(
            blank_org.status.code(),
            Some(1),
            "ADO_ORG={blank:?} is not a value"
        );
        assert_eq!(
            stdout_of(&blank_org),
            format!("{ORACLE_MISSING_ORG_JSON}\n"),
            "ADO_ORG={blank:?}"
        );
    }

    assert_nothing_written(&home);
}

/// D26: the oracle's own guard exempts `device` from the `--org` requirement and
/// then writes a credential keyed by nothing (`org: nil`) with a token. Our store
/// and `config.toml` are per-organization, so an org-less device login has nowhere
/// to land: it is a validation error naming `--org`, and the flow never starts
/// (which is also why this suite may run it: no request leaves the process).
#[test]
fn device_login_without_an_org_is_a_validation_error() {
    let home = TempHome::new();

    let output = run(&home, &["login", "--method", "device", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            r#"{"error":{"code":"validation_error","details":{"env_var":"ADO_ORG","option":"--org"},"message":"--org is required for method='device' (or set ADO_ORG env var)"},"ok":false}"#
        )
    );

    let plain = run(&home, &["login", "--method", "device"]);

    assert_eq!(plain.status.code(), Some(1));
    assert_eq!(
        stderr_of(&plain),
        "[Validation error] --org is required for method='device' (or set ADO_ORG env var)\n"
    );
    assert_nothing_written(&home);
}

// ── the browser flow, through the real binary and a scripted opener ─────────
//
// The flow's identity and accounts origins are seams the *binary* does not
// expose (the frozen CLI hardcodes them and this build adds no override), so the
// end-to-end paths a test can drive are the ones that stop before a network
// call: the state comparison, a callback that carries an OAuth `error`, and the
// argv/default-method resolution. Everything the flow does is exercised for
// real — the loopback listener, the request it reads, the URL it prints, the
// `open` it calls — while `login.microsoftonline.com` and
// `app.vssps.visualstudio.com` are never contacted. The full exchange and the
// org auto-detect run against local fakes in
// `crates/ado/src/commands/login.rs`'s unit tests.

/// The `open` the flow calls, put in front of `PATH`. It records the authorize
/// URL it was handed and answers the callback that URL advertises. A matching
/// state would make the flow exchange the code at the real endpoint, so both
/// modes here stop before the exchange (a mismatched state, an OAuth error) —
/// which is also what makes them deterministic.
#[cfg(unix)]
const OPENER_SHIM: &str = r#"#!/usr/bin/env bash
set -u
printf '%s\n' "$1" >"$ADO_BROWSER_URL_FILE"
port=$(printf '%s' "$1" | sed -n 's/.*redirect_uri=http%3A%2F%2Flocalhost%3A\([0-9][0-9]*\).*/\1/p')
(
    for _ in $(seq 1 600); do
        case ${ADO_BROWSER_CALLBACK_MODE:-none} in
            mismatch) query='code=TEST-CODE&state=NOT-THE-STATE' ;;
            error) query='error=access_denied' ;;
            *) exit 0 ;;
        esac
        if exec 3<>"/dev/tcp/127.0.0.1/$port" 2>/dev/null; then
            printf 'GET /?%s HTTP/1.1\r\nHost: localhost\r\n\r\n' "$query" >&3
            exec 3<&- 3>&-
            exit 0
        fi
        sleep 0.05
    done
) >/dev/null 2>&1 &
exit 0
"#;

/// A `TempHome` with the shim installed, and the command builder that points the
/// child at it.
#[cfg(unix)]
struct Browser {
    home: TempHome,
    shim: std::path::PathBuf,
    url_file: std::path::PathBuf,
}

#[cfg(unix)]
impl Browser {
    fn new() -> Browser {
        use std::os::unix::fs::PermissionsExt;

        let home = TempHome::new();
        let shim = home.path().join("bin");
        let url_file = home.path().join("authorize-url");
        fs::create_dir_all(&shim).expect("the shim directory");
        let opener = shim.join("open");
        fs::write(&opener, OPENER_SHIM).expect("the shim");
        fs::set_permissions(&opener, fs::Permissions::from_mode(0o755))
            .expect("the shim is runnable");

        Browser {
            home,
            shim,
            url_file,
        }
    }

    fn command(&self, mode: &str, args: &[&str]) -> Command {
        let mut command = command(&self.home, args);
        let path = std::env::var_os("PATH").unwrap_or_default();
        command
            .env(
                "PATH",
                format!("{}:{}", self.shim.display(), path.to_string_lossy()),
            )
            .env("ADO_BROWSER_URL_FILE", &self.url_file)
            .env("ADO_BROWSER_CALLBACK_MODE", mode);
        command
    }

    fn run(&self, mode: &str, args: &[&str]) -> Output {
        self.command(mode, args).output().expect("run ado")
    }

    /// The authorize URL the flow handed to the opener.
    fn url(&self) -> String {
        fs::read_to_string(&self.url_file)
            .expect("the opener was called and recorded a URL")
            .trim()
            .to_owned()
    }
}

/// The authorize URL's query, decoded pair by pair and in the order the flow
/// wrote them: `+` is a space, `%XX` is a byte.
#[cfg(unix)]
fn query_pairs(url: &str) -> Vec<(String, String)> {
    let query = url.split_once('?').expect("a query string").1;

    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));

            (decode(key), decode(value))
        })
        .collect()
}

#[cfg(unix)]
fn decode(value: &str) -> String {
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

#[cfg(unix)]
fn value<'a>(pairs: &'a [(String, String)], key: &str) -> &'a str {
    pairs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("the authorize URL has no `{key}`"))
}

/// The oracle's authorize URL, captured byte for byte from the frozen escript
/// (Task 10's `browser-mismatch`): the host, the path, and — the part a port
/// gets wrong by accident — `URI.encode_query/1` over an Elixir map, i.e. this
/// exact parameter order.
#[cfg(unix)]
const AUTHORIZE_PREFIX: &str =
    "https://login.microsoftonline.com/organizations/oauth2/v2.0/authorize?";

#[cfg(unix)]
const AUTHORIZE_KEYS: [&str; 12] = [
    "scope",
    "state",
    "client_info",
    "prompt",
    "claims",
    "client_id",
    "code_challenge",
    "code_challenge_method",
    "nonce",
    "redirect_uri",
    "response_mode",
    "response_type",
];

/// The whole URL: host, path, parameter order and values. The random parts (the
/// state, the PKCE challenge, the nonce, the port) are asserted by shape — the
/// flow generates them — and everything else is the oracle's bytes.
#[cfg(unix)]
#[test]
fn the_printed_authorize_url_is_the_oracles() {
    let browser = Browser::new();
    let output = browser.run(
        "mismatch",
        &["login", "--method", "browser", "--org", ORG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let url = browser.url();
    assert!(
        url.starts_with(AUTHORIZE_PREFIX),
        "the oracle's host and path: {url}"
    );

    let pairs = query_pairs(&url);
    let keys: Vec<&str> = pairs.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys, AUTHORIZE_KEYS,
        "the oracle's parameter order (`URI.encode_query/1` over an Elixir map)"
    );
    assert_eq!(
        value(&pairs, "scope"),
        "https://management.core.windows.net/.default offline_access openid profile"
    );
    assert_eq!(value(&pairs, "response_type"), "code");
    assert_eq!(value(&pairs, "response_mode"), "query");
    assert_eq!(value(&pairs, "prompt"), "select_account");
    assert_eq!(value(&pairs, "client_info"), "1");
    assert_eq!(
        value(&pairs, "claims"),
        r#"{"access_token": {"xms_cc": {"values": ["CP1"]}}}"#,
        "the CP1 claims blob, spaces and all"
    );
    assert_eq!(
        value(&pairs, "client_id"),
        "04b07795-8ddb-461a-bbee-02f9e1bf7b46",
        "the Azure CLI public client"
    );
    assert_eq!(value(&pairs, "code_challenge_method"), "S256");
    assert_eq!(
        value(&pairs, "code_challenge").len(),
        43,
        "base64url(SHA256(verifier)), unpadded: {}",
        value(&pairs, "code_challenge")
    );
    assert_eq!(
        value(&pairs, "state").len(),
        22,
        "base64url(16 bytes), unpadded"
    );
    assert_eq!(
        value(&pairs, "nonce").len(),
        22,
        "base64url(16 bytes), unpadded"
    );
    let redirect = value(&pairs, "redirect_uri");
    assert!(
        redirect.starts_with("http://localhost:"),
        "the loopback redirect the flow bound: {redirect}"
    );
    assert!(
        redirect["http://localhost:".len()..].parse::<u16>().is_ok(),
        "the redirect's port is the one the listener bound: {redirect}"
    );

    // The URL is printed before the browser is opened, and the opener is handed
    // the same string.
    let stdout = stdout_of(&output);
    let message: Value = serde_json::from_str(stdout.lines().next().expect("a first line"))
        .expect("the announcement is one JSON document");
    assert_eq!(message["ok"], json!(true));
    assert_eq!(
        message["message"],
        json!(format!("Opening browser to sign in to {ORG}...\n  {url}")),
        "the URL reaches stdout under --json, and it is the one the opener got"
    );
}

/// `ADO_OAUTH_CLIENT_ID` is the identity app the flow talks to (spec §4.7): the
/// oracle's *escript* ignores it at runtime — its module attribute is evaluated
/// at compile time — but the documented behaviour, and this build's, is a runtime
/// read.
#[cfg(unix)]
#[test]
fn the_oauth_client_id_override_reaches_the_authorize_url() {
    let browser = Browser::new();
    let override_id = "11111111-2222-3333-4444-555555555555";

    let mut command = browser.command("mismatch", &["login", "--method", "browser", "--org", ORG]);
    command.env("ADO_OAUTH_CLIENT_ID", override_id);
    let output = command.output().expect("run ado");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        value(&query_pairs(&browser.url()), "client_id"),
        override_id
    );

    for blank in ["", "  "] {
        let mut command =
            browser.command("mismatch", &["login", "--method", "browser", "--org", ORG]);
        command.env("ADO_OAUTH_CLIENT_ID", blank);
        command.output().expect("run ado");

        assert_eq!(
            value(&query_pairs(&browser.url()), "client_id"),
            "04b07795-8ddb-461a-bbee-02f9e1bf7b46",
            "a blank override is not a value (D16): {blank:?}"
        );
    }
}

/// The state is compared before the code is used (`login_browser/1`'s `^state`
/// match), so a callback that carries the wrong state stops the flow with the
/// oracle's sentence and exchanges nothing.
#[cfg(unix)]
#[test]
fn a_mismatched_state_stops_the_browser_flow() {
    let browser = Browser::new();
    let output = browser.run(
        "mismatch",
        &["login", "--method", "browser", "--org", ORG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let stdout = stdout_of(&output);
    let mut lines = stdout.lines();
    let _announcement = lines.next().expect("the URL announcement");
    let envelope: Value =
        serde_json::from_str(lines.next().expect("the error envelope")).expect("a JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("auth_required"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Login failed: State mismatch — possible CSRF attack.")
    );
    assert_eq!(
        envelope["error"]["details"]["reason"],
        json!("State mismatch — possible CSRF attack.")
    );
    assert_nothing_written(&browser.home);

    let plain = browser.run("mismatch", &["login", "--method", "browser", "--org", ORG]);
    assert_eq!(plain.status.code(), Some(1));
    assert_eq!(
        stderr_of(&plain),
        "[Auth required] Login failed: State mismatch — possible CSRF attack.\n"
    );
    assert_nothing_written(&browser.home);
}

/// No `--method` and no PAT is the browser method (`resolve_method/1`), and an
/// OAuth `error` in the callback is the oracle's sentence — again before the
/// exchange, so no endpoint is contacted.
#[cfg(unix)]
#[test]
fn login_without_a_method_resolves_to_the_browser_flow() {
    let browser = Browser::new();
    let output = browser.run("error", &["login", "--org", ORG, "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = stdout_of(&output);
    let envelope: Value = serde_json::from_str(stdout.lines().last().expect("the envelope"))
        .expect("a JSON document");
    assert_eq!(
        envelope["error"]["message"],
        json!("Login failed: Authorization failed: access_denied")
    );
    assert!(
        !browser.url().is_empty(),
        "the flow ran: the opener was handed the authorize URL"
    );
    assert_nothing_written(&browser.home);
}

/// An explicit `--method browser` is accepted, and an org-less browser login is
/// *not* refused up front the way `device` is (D26): the flow starts, because
/// auto-detection is what resolves the organization (recorded in the inventory).
#[cfg(unix)]
#[test]
fn an_org_less_browser_login_starts_the_flow() {
    let browser = Browser::new();
    let output = browser.run("error", &["login", "--method", "browser"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr_of(&output),
        "[Auth required] Login failed: Authorization failed: access_denied\n"
    );

    let pairs = query_pairs(&browser.url());
    assert_eq!(value(&pairs, "response_type"), "code");
    assert_nothing_written(&browser.home);
}
