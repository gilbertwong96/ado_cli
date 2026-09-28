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

/// The frozen escript's `ado login --method pat --org myorg --pat tok --json`
/// envelope, captured with an isolated `HOME`: our bytes differ only in
/// `credentials_saved_to` (the new config file, D11b) and in object key order
/// (serde_json's `BTreeMap` vs Elixir's map order, D1 — the comparison normalises
/// with `jq -S`). The shape, the method spelling and the `server: null` are the
/// oracle's.
fn oracle_success_json(config_file: &str) -> String {
    format!(
        r#"{{"ok":true,"result":{{"credentials_saved_to":"{config_file}","method":"pat","org":"myorg","server":null}}}}"#
    )
}

/// The oracle's two human lines, with the config file it names. The oracle's
/// message is byte-identical apart from that path.
fn oracle_success_plain(config_file: &str) -> String {
    format!("\n  Logged in to myorg via Pat.\n  Credentials saved to {config_file}\n")
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

/// The frozen escript's `--json` envelope for an unknown method. Ours differs by
/// the words only: `browser` is not shipped in this build, so the suggestion list
/// and the message name `pat` and `device`.
const ORACLE_UNKNOWN_METHOD_JSON: &str = r#"{"error":{"code":"validation_error","details":{"valid_methods":["browser","pat","device"]},"message":"Unknown method 'bogus'. Use 'browser', 'pat', or 'device'."},"ok":false}"#;

const UNKNOWN_METHOD_JSON: &str = r#"{"error":{"code":"validation_error","details":{"valid_methods":["pat","device"]},"message":"Unknown method 'bogus'. Use 'pat' or 'device'."},"ok":false}"#;

/// What the two shipped methods are, and nothing else: the message `browser` gets
/// names them (`--method browser` is Wave 3, so it is a validation error rather
/// than a silent no-op).
const BROWSER_JSON: &str = r#"{"error":{"code":"validation_error","details":{"valid_methods":["pat","device"]},"message":"Login method 'browser' is not available in this build. Use '--method pat' or '--method device'."},"ok":false}"#;

/// The PAT the success tests store; asserted absent from `config.toml`.
const PAT: &str = "pat-secret-token";

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
            "login", "--method", "pat", "--org", "myorg", "--pat", PAT, "--json",
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
    assert_eq!(config, expected_config("myorg", "pat"));
    assert!(
        !config.contains(PAT),
        "the token reached config.toml: {config}"
    );
    assert!(
        !config.contains("token"),
        "config.toml carries a token field: {config}"
    );

    let credentials = fs::read_to_string(credentials_file(&home)).expect("credentials.json");
    assert_eq!(credentials, expected_credentials("myorg", "pat", PAT));

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
        &["login", "--method", "pat", "--org", "myorg", "--pat", PAT],
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
        "/myorg/_apis/projects",
        MockResponse::from_fixture("projects_list"),
    );

    assert_success(&run(
        &home,
        &["login", "--method", "pat", "--org", "myorg", "--pat", PAT],
    ));

    let mut list = command(&home, &["projects", "list", "--json"]);
    list.env("ADO_SERVER", server.base_url());
    let output = list.output().expect("run ado projects list");

    assert_success(&output);
    let received = server.received();
    assert_eq!(received.len(), 1, "the list reached the mock once");
    assert_eq!(received[0].path, "/myorg/_apis/projects");
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
            "myorg",
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
        expected_config("myorg", "pat"),
        "the server is reported, not recorded"
    );

    let plain = run(
        &home,
        &[
            "login",
            "--method",
            "pat",
            "--org",
            "myorg",
            "--pat",
            PAT,
            "--server",
            "https://ado.example.com",
        ],
    );
    assert!(
        stdout_of(&plain).contains("  Logged in to myorg (https://ado.example.com) via Pat.\n")
    );
}

/// `resolve_method/1`: a PAT on `--pat` selects `pat` when `--method` is omitted —
/// the invocation the skills and `AGENTS.md` document.
#[cfg(not(target_os = "windows"))]
#[test]
fn the_pat_flag_infers_the_pat_method() {
    let home = TempHome::new();

    let output = run(&home, &["login", "--org", "myorg", "--pat", PAT, "--json"]);

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
    let mut command = command(
        &home,
        &["login", "--method", "pat", "--org", "myorg", "--json"],
    );
    command.env("ADO_PAT", PAT);

    let output = command.output().expect("run ado");

    assert_success(&output);
    assert_eq!(
        fs::read_to_string(credentials_file(&home)).expect("credentials.json"),
        expected_credentials("myorg", "pat", PAT)
    );
}

#[test]
fn login_without_a_pat_is_a_validation_error() {
    let home = TempHome::new();

    let json_output = run(
        &home,
        &["login", "--method", "pat", "--org", "myorg", "--json"],
    );

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

    let plain = run(&home, &["login", "--method", "pat", "--org", "myorg"]);

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
            "login", "--method", "pat", "--org", "myorg", "--pat", "", "--json",
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
/// `device_code` is one of them (`Unknown method 'device_code'`); ours rejects the
/// same set minus the browser it does not ship.
#[test]
fn an_unknown_method_is_a_validation_error() {
    let home = TempHome::new();

    let output = run(
        &home,
        &["login", "--method", "bogus", "--org", "myorg", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout_of(&output), format!("{UNKNOWN_METHOD_JSON}\n"));
    assert_ne!(
        stdout_of(&output),
        format!("{ORACLE_UNKNOWN_METHOD_JSON}\n"),
        "the oracle suggests browser, which this build does not ship"
    );

    let spelled = run(
        &home,
        &[
            "login",
            "--method",
            "device_code",
            "--org",
            "myorg",
            "--json",
        ],
    );

    assert_eq!(spelled.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&spelled)).expect("a JSON document");
    assert_eq!(
        envelope["error"]["message"],
        json!("Unknown method 'device_code'. Use 'pat' or 'device'.")
    );
    assert_nothing_written(&home);
}

/// Wave 1 ships `pat` and `device` only: `browser` is Wave 3, and so is the
/// no-`--method` default it would have selected. Both are validation errors that
/// name what ships — never a silent no-op.
#[test]
fn browser_login_is_not_shipped() {
    let home = TempHome::new();

    let explicit = run(
        &home,
        &["login", "--method", "browser", "--org", "myorg", "--json"],
    );

    assert_eq!(explicit.status.code(), Some(1));
    assert_eq!(stdout_of(&explicit), format!("{BROWSER_JSON}\n"));

    let inferred = run(&home, &["login", "--org", "myorg", "--json"]);

    assert_eq!(inferred.status.code(), Some(1));
    assert_eq!(
        stdout_of(&inferred),
        format!("{BROWSER_JSON}\n"),
        "no --method and no PAT is the browser method the oracle would have run"
    );
    assert!(
        stderr_of(&run(&home, &["login"])).contains("not available in this build"),
        "a bare `ado login` explains itself"
    );
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
