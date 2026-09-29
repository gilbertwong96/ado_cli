//! End-to-end tests for `ado logout`: its envelope, its exit codes, and what it
//! leaves on disk.
//!
//! Every test owns its environment — a `TempHome` with the `ADO_*` variables
//! removed unless the test sets them — so no test reads or writes the developer's
//! own config or home. The command still talks to the real credential store,
//! though, so the synthetic organization names (R52) are a collision guard, not
//! isolation: under a `TempHome` the keychain is unreachable (macOS resolves its
//! default keychain through `HOME`; CI Linux has no secret service), which is also
//! the production shape on a headless box, but a developer's live secret service
//! is reached and a delete attempt against those names is made.
//!
//! The oracle comparison is a *contract* comparison, not a file-bytes one: the
//! Elixir deletes its whole `~/.ado_cli/config.json` — the single file its token
//! lived in — while this build's `config.toml` is organization-scoped and never
//! holds the token (§7). The org-scoped removal, the kept settings, the failure
//! reporting and the message wording are the ruled differences (inventory D27);
//! the envelope *shape* is pinned to the oracle's.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use ado::commands::schema::find_node;
use ado_testkit::{MockResponse, MockServer, TempHome, ado_cmd, stderr_of, stdout_of};
use serde_json::{Value, json};

/// The frozen escript's `ado logout --json` envelope, captured with an isolated
/// `HOME` and a stored credential. The shape — `{"ok":true,"message":…}`, message
/// before ok, no `result` — is the oracle's and ours; the message is not (D27).
const ORACLE_LOGOUT_JSON: &str =
    r#"{"message":"Logged out. Credentials removed from ~/.ado_cli/config.json","ok":true}"#;

/// The oracle's plain output, ANSI-stripped: the message line plus the two blank
/// lines CliMate's `success` + `halt_success("")` rendering adds. Ours keeps the
/// message line only, like the other ported commands (D11's class).
const ORACLE_LOGOUT_PLAIN: &str =
    "Logged out. Credentials removed from ~/.ado_cli/config.json\n\n\n";

/// The message ours names the organization in, and the envelope it rides in.
fn logout_json(org: &str) -> String {
    format!(r#"{{"message":"Logged out. Credentials removed for '{org}'.","ok":true}}"#)
}

fn logout_plain(org: &str) -> String {
    format!("Logged out. Credentials removed for '{org}'.\n")
}

/// The message for a run nothing named an organization for.
const NOTHING_JSON: &str =
    r#"{"message":"Logged out. No stored credentials to remove.","ok":true}"#;
const NOTHING_PLAIN: &str = "Logged out. No stored credentials to remove.\n";

/// The organization the logout tests resolve. Synthetic for the same reason as
/// `cli_login.rs`'s: the keychain is unreachable under a `TempHome` and on CI, but a
/// developer's live secret service is not, and R26 forbids touching a real
/// credential (R52).
const ORG: &str = "ado-cli-test-org";

/// A port nothing listens on, pinned on the runs that follow a logout: they must
/// fail `auth_required`, and a regression that re-authenticated would send its
/// request here, where `ConnectionRefused` classifies as `network_error`
/// (`crates/ado-core/src/error.rs:154-178`, pinned at `:265-271`) — a code those
/// assertions reject. The dead server is what keeps a re-authenticated regression
/// from sending the synthetic PAT at Azure.
const DEAD_SERVER: &str = "http://127.0.0.1:1";

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

fn config_file(home: &TempHome) -> PathBuf {
    home.config_dir().join("config.toml")
}

fn credentials_file(home: &TempHome) -> PathBuf {
    home.config_dir().join("credentials.json")
}

/// The Elixir CLI's `~/.ado_cli/config.json`, inside the temp home.
fn legacy_file(home: &TempHome) -> PathBuf {
    home.path().join(".ado_cli").join("config.json")
}

/// A two-org configuration whose names are synthetic (R52): the first is the
/// default and holds the secret, the second is the non-secret setting a logout must
/// not destroy.
const TWO_ORG_CONFIG: &str = "default_org = \"ado-cli-test-org\"\nserver = \"https://ado.example.com\"\n\n[orgs.ado-cli-test-org]\nauth = \"pat\"\n\n[orgs.ado-cli-test-other]\nauth = \"browser\"\n";

/// The same configuration after the default organization is logged out.
const TWO_ORG_CONFIG_AFTER: &str =
    "server = \"https://ado.example.com\"\n\n[orgs.ado-cli-test-other]\nauth = \"browser\"\n";

const TWO_ORG_CREDENTIALS: &str = "{\n  \"ado-cli-test-org\": {\n    \"method\": \"pat\",\n    \"token\": \"ado-cli-test-org-token\"\n  },\n  \"ado-cli-test-other\": {\n    \"method\": \"browser\",\n    \"token\": \"ado-cli-test-other-token\"\n  }\n}";

const TWO_ORG_CREDENTIALS_AFTER: &str = "{\n  \"ado-cli-test-other\": {\n    \"method\": \"browser\",\n    \"token\": \"ado-cli-test-other-token\"\n  }\n}";

fn seed(home: &TempHome, config: &str, credentials: &str) {
    fs::write(config_file(home), config).expect("seed config.toml");
    fs::write(credentials_file(home), credentials).expect("seed credentials.json");
}

fn assert_success(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(output)
    );
}

fn sorted_keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("a JSON object")
        .keys()
        .cloned()
        .collect()
}

/// The credential is gone from the store and the config entry that referenced it,
/// while the settings a logout never owned survive — and a later command can no
/// longer resolve a credential for the organization.
#[test]
fn logout_clears_the_credential_and_its_entry_but_keeps_the_other_settings() {
    let home = TempHome::new();
    seed(&home, TWO_ORG_CONFIG, TWO_ORG_CREDENTIALS);

    let output = run(&home, &["logout", "--json"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", logout_json(ORG)),
        "the oracle's envelope shape with the ruled message (D27)"
    );
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("a JSON document");
    assert_eq!(
        sorted_keys(&envelope),
        ["message", "ok"],
        "a message envelope, never a table or a value"
    );
    assert_ne!(
        stdout_of(&output).trim_end(),
        ORACLE_LOGOUT_JSON,
        "the message is ours (the oracle names its config file), the shape is its"
    );

    assert_eq!(
        fs::read_to_string(config_file(&home)).expect("config.toml"),
        TWO_ORG_CONFIG_AFTER,
        "the organization's entry and its default_org are gone; server and the other org stay"
    );
    assert_eq!(
        fs::read_to_string(credentials_file(&home)).expect("credentials.json"),
        TWO_ORG_CREDENTIALS_AFTER,
        "only the logged-out organization's credential is removed"
    );
    assert!(
        !fs::read_to_string(credentials_file(&home))
            .expect("credentials.json")
            .contains("ado-cli-test-org-token"),
        "the removed token is still on disk"
    );

    // The post-condition the command exists for: the next run cannot resolve the
    // credential that was logged out, and it fails before any request is built.
    let next = run(&home, &["projects", "list", "--org", ORG, "--json"]);

    assert_eq!(next.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&next)).expect("a JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("auth_required"));
}

/// A single-organization install lands exactly where the oracle does: after
/// `logout`, nothing is configured.
#[test]
fn logout_of_a_single_org_config_leaves_the_oracle_fresh_install_state() {
    let home = TempHome::new();
    let config = "default_org = \"ado-cli-test-org\"\n\n[orgs.ado-cli-test-org]\nauth = \"pat\"\n";
    let credentials = "{\n  \"ado-cli-test-org\": {\n    \"method\": \"pat\",\n    \"token\": \"ado-cli-test-org-token\"\n  }\n}";
    seed(&home, config, credentials);

    let output = run(&home, &["logout", "--json"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), format!("{}\n", logout_json(ORG)));
    assert!(
        !config_file(&home).exists(),
        "an emptied config.toml is removed, so whoami reports a fresh install"
    );
    assert!(
        !credentials_file(&home).exists(),
        "the last credential's file is removed"
    );

    let whoami = run(&home, &["whoami", "--json"]);

    assert_success(&whoami);
    let result =
        serde_json::from_str::<Value>(&stdout_of(&whoami)).expect("a JSON document")["result"]
            .clone();
    assert_eq!(result["configured"], json!(false));
    assert_eq!(result["org"], json!(null));
    assert_eq!(result["method"], json!(null));
    assert_eq!(result["authenticated"], json!(false));
    assert_eq!(result["server"], json!("dev.azure.com"));
}

/// Nothing stored, nothing to remove: the oracle exits 0 and says so, and so do we.
#[test]
fn logout_with_nothing_stored_still_exits_zero_and_says_so() {
    let home = TempHome::new();

    let json_output = run(&home, &["logout", "--json"]);

    assert_success(&json_output);
    assert_eq!(stdout_of(&json_output), format!("{NOTHING_JSON}\n"));
    assert!(
        stderr_of(&json_output).is_empty(),
        "stderr: {}",
        stderr_of(&json_output)
    );

    let plain = run(&home, &["logout"]);

    assert_success(&plain);
    assert_eq!(stdout_of(&plain), NOTHING_PLAIN);
    assert!(
        stderr_of(&plain).is_empty(),
        "stderr: {}",
        stderr_of(&plain)
    );
    assert_ne!(
        ORACLE_LOGOUT_PLAIN, NOTHING_PLAIN,
        "the oracle's message claims a removal it cannot know happened; ours says what it knows (D27)"
    );
    assert!(
        !config_file(&home).exists() && !credentials_file(&home).exists(),
        "a logout with nothing stored writes nothing"
    );
}

/// The second run has nothing left to remove and says so — the accurate wording
/// (D27c) — with the oracle's exit 0 both times.
#[test]
fn logout_twice_in_a_row_is_idempotent() {
    let home = TempHome::new();
    seed(
        &home,
        "default_org = \"ado-cli-test-org\"\n\n[orgs.ado-cli-test-org]\nauth = \"pat\"\n",
        "{\n  \"ado-cli-test-org\": {\n    \"method\": \"pat\",\n    \"token\": \"ado-cli-test-org-token\"\n  }\n}",
    );

    let mut first = command(&home, &["logout", "--json"]);
    first.env("ADO_ORG", ORG);
    let first = first.output().expect("run the first logout");

    let mut second = command(&home, &["logout", "--json"]);
    second.env("ADO_ORG", ORG);
    let second = second.output().expect("run the second logout");

    assert_success(&first);
    assert_success(&second);
    assert_eq!(
        stdout_of(&first),
        format!("{}\n", logout_json(ORG)),
        "the flag/env organization resolves and its credential is removed"
    );
    assert_eq!(
        stdout_of(&second),
        format!("{NOTHING_JSON}\n"),
        "the second run removed nothing, so it must not claim it did"
    );
    assert!(
        !config_file(&home).exists() && !credentials_file(&home).exists(),
        "the second run recreates nothing"
    );
}

/// A migrated Elixir install is logged out for real: the legacy file holds the
/// credential the import copied, and an absent `config.toml` is the import
/// marker — leaving the file behind would let the next command import the token
/// straight back (the oracle deleted that very file).
#[test]
fn a_migrated_install_is_not_re_imported_after_logout() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/ado-cli-test-org/_apis/projects",
        MockResponse::json(200, json!({"value": []})),
    );
    let legacy = legacy_file(&home);
    fs::create_dir_all(legacy.parent().expect("the legacy directory"))
        .expect("create the legacy directory");
    fs::write(
        &legacy,
        r#"{"org":"ado-cli-test-org","method":"pat","pat":"legacy-pat"}"#,
    )
    .expect("write the legacy file");

    // 1. The first client command imports the legacy install: credential in the
    // store, organization in the config marker.
    let mut importing = command(&home, &["projects", "list", "--org", ORG, "--json"]);
    importing.env("ADO_SERVER", server.base_url());
    let imported = importing.output().expect("run the importing list");

    assert_success(&imported);
    assert!(
        config_file(&home).exists(),
        "the import writes the config marker"
    );
    assert!(
        credentials_file(&home).exists(),
        "the import stores the credential"
    );
    let received = server.received();
    assert_eq!(
        received.len(),
        1,
        "the imported credential reached the wire"
    );
    assert_eq!(
        received[0].header("authorization"),
        Some("Basic OmxlZ2FjeS1wYXQ="),
        "Basic base64(':legacy-pat')"
    );

    // 2. Logout removes the store credential, the emptied config and the legacy
    // file that held the same secret.
    let output = run(&home, &["logout", "--json"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), format!("{}\n", logout_json(ORG)));
    assert!(
        !config_file(&home).exists(),
        "the emptied config is removed"
    );
    assert!(
        !credentials_file(&home).exists(),
        "the credential is removed"
    );
    assert!(
        !legacy.exists(),
        "the legacy file held the logged-out credential and must not survive"
    );

    // 3. The next command cannot import it back: no marker, no legacy file, no
    // credential. The dead server is what pins "no request": a regression that
    // re-authenticated would send one, and that attempt classifies as
    // `network_error` (`crates/ado-core/src/error.rs:154-178`, pinned at `:265-271`),
    // not the `auth_required` asserted below — so the observed code is the evidence
    // that nothing reached Azure with the synthetic PAT.
    let mut again = command(&home, &["projects", "list", "--org", ORG, "--json"]);
    again.env("ADO_SERVER", DEAD_SERVER);
    let again = again.output().expect("run the post-logout list");

    assert_eq!(again.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&again)).expect("a JSON document");
    assert_eq!(envelope["error"]["code"], json!("auth_required"));
    assert!(
        !config_file(&home).exists(),
        "no re-import recreates the marker"
    );
    assert!(
        !credentials_file(&home).exists(),
        "no re-import rebuilds the credential"
    );
    assert_eq!(
        server.received().len(),
        1,
        "only the importing run reached the server"
    );
}

/// A legacy file naming another organization survives a logout by design and,
/// because that logout removes the emptied `config.toml` (the import marker), the
/// next client command imports it — the combination T12's re-review accepted (D27).
#[test]
fn a_logout_leaves_a_legacy_file_naming_another_org_for_the_next_import() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/ado-cli-test-other/_apis/projects",
        MockResponse::json(200, json!({"value": []})),
    );
    seed(
        &home,
        "default_org = \"ado-cli-test-org\"\n\n[orgs.ado-cli-test-org]\nauth = \"pat\"\n",
        "{\n  \"ado-cli-test-org\": {\n    \"method\": \"pat\",\n    \"token\": \"ado-cli-test-org-token\"\n  }\n}",
    );
    let legacy = legacy_file(&home);
    fs::create_dir_all(legacy.parent().expect("the legacy directory"))
        .expect("create the legacy directory");
    fs::write(
        &legacy,
        r#"{"org":"ado-cli-test-other","method":"pat","pat":"legacy-pat"}"#,
    )
    .expect("write the legacy file");

    let output = run(&home, &["logout", "--json"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), format!("{}\n", logout_json(ORG)));
    assert!(
        !config_file(&home).exists(),
        "the emptied config is removed, so the import marker is gone"
    );
    assert!(
        legacy.exists(),
        "the legacy file names another organization, so the logout leaves it"
    );

    let mut next = command(&home, &["projects", "list", "--json"]);
    next.env("ADO_SERVER", server.base_url());
    let next = next.output().expect("run the post-logout list");

    assert_success(&next);
    let received = server.received();
    assert_eq!(
        received.len(),
        1,
        "the next command imported the surviving legacy file"
    );
    assert_eq!(received[0].path, "/ado-cli-test-other/_apis/projects");
    assert_eq!(
        received[0].header("authorization"),
        Some("Basic OmxlZ2FjeS1wYXQ="),
        "Basic base64(':legacy-pat')"
    );
    assert!(
        config_file(&home).exists(),
        "the import leaves its marker behind"
    );
}

/// A migrated install that has never been imported has no config to name an
/// organization, so the legacy file's own organization is the target — the oracle
/// deletes that very file regardless of where an organization would come from.
/// Then the shape is genuinely empty: a second logout says so.
#[test]
fn a_legacy_only_install_is_logged_out_without_a_named_org() {
    let home = TempHome::new();
    let legacy = legacy_file(&home);
    fs::create_dir_all(legacy.parent().expect("the legacy directory"))
        .expect("create the legacy directory");
    fs::write(
        &legacy,
        r#"{"org":"ado-cli-test-org","method":"pat","pat":"legacy-pat"}"#,
    )
    .expect("write the legacy file");

    let output = run(&home, &["logout", "--json"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", logout_json(ORG)),
        "the legacy file names the organization being logged out, so the message may claim it"
    );
    assert!(
        !legacy.exists(),
        "the only copy of the credential is removed"
    );
    assert!(!config_file(&home).exists());
    assert!(!credentials_file(&home).exists());

    let mut next = command(&home, &["projects", "list", "--json"]);
    next.env("ADO_SERVER", DEAD_SERVER);
    let next = next.output().expect("run the post-logout list");

    assert_eq!(next.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&next)).expect("a JSON document");
    assert_eq!(envelope["error"]["code"], json!("auth_required"));
    assert!(
        !config_file(&home).exists(),
        "nothing was re-imported from a file that no longer exists"
    );
    assert!(!credentials_file(&home).exists());

    let second = run(&home, &["logout", "--json"]);

    assert_success(&second);
    assert_eq!(stdout_of(&second), format!("{NOTHING_JSON}\n"));
}

/// `default_org` naming another organization is a setting the logout never owned,
/// so the equality guard leaves it in place.
#[test]
fn logout_keeps_a_default_org_that_names_another_organization() {
    let home = TempHome::new();
    let config = "default_org = \"ado-cli-test-other\"\nserver = \"https://ado.example.com\"\n\n[orgs.ado-cli-test-org]\nauth = \"pat\"\n\n[orgs.ado-cli-test-other]\nauth = \"browser\"\n";
    seed(&home, config, TWO_ORG_CREDENTIALS);

    let output = run(&home, &["logout", "--org", ORG, "--json"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), format!("{}\n", logout_json(ORG)));
    assert_eq!(
        fs::read_to_string(config_file(&home)).expect("config.toml"),
        "default_org = \"ado-cli-test-other\"\nserver = \"https://ado.example.com\"\n\n[orgs.ado-cli-test-other]\nauth = \"browser\"\n",
        "default_org names another organization, so it survives untouched"
    );
    assert_eq!(
        fs::read_to_string(credentials_file(&home)).expect("credentials.json"),
        TWO_ORG_CREDENTIALS_AFTER
    );
}

/// Plain output names the organization and writes nothing to stderr.
#[test]
fn logout_plain_output_is_the_message_line() {
    let home = TempHome::new();
    seed(
        &home,
        "default_org = \"ado-cli-test-org\"\n\n[orgs.ado-cli-test-org]\nauth = \"pat\"\n",
        "{\n  \"ado-cli-test-org\": {\n    \"method\": \"pat\",\n    \"token\": \"ado-cli-test-org-token\"\n  }\n}",
    );

    let output = run(&home, &["logout"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), logout_plain(ORG));
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        !stdout_of(&output).contains('\u{1b}'),
        "ANSI in piped output: {:?}",
        stdout_of(&output)
    );
}

/// The oracle reports success even when the deletion failed (captured with a
/// read-only `~/.ado_cli`: the config survives, exit 0). Ours reports the failure
/// and leaves the surviving credential visible, because a success line over a
/// credential that is still on disk is the defect this rewrite exists to remove
/// (D27).
#[cfg(unix)]
#[test]
fn a_failing_credential_layer_reports_a_failure_and_keeps_the_credential() {
    let home = TempHome::new();
    seed(
        &home,
        "default_org = \"ado-cli-test-org\"\n\n[orgs.ado-cli-test-org]\nauth = \"pat\"\n",
        "{\n  \"ado-cli-test-org\": {\n    \"method\": \"pat\",\n    \"token\": \"ado-cli-test-org-token\"\n  }\n}",
    );
    fs::set_permissions(home.config_dir(), fs::Permissions::from_mode(0o555))
        .expect("make the config dir read-only");

    if fs::write(home.config_dir().join("probe"), b"probe").is_ok() {
        // Running as root: the mode bits do not apply, so there is nothing to assert.
        fs::remove_file(home.config_dir().join("probe")).expect("remove the probe");
        fs::set_permissions(home.config_dir(), fs::Permissions::from_mode(0o755))
            .expect("restore the config dir");
        return;
    }

    let json_output = run(&home, &["logout", "--json"]);
    let plain = run(&home, &["logout"]);
    fs::set_permissions(home.config_dir(), fs::Permissions::from_mode(0o755))
        .expect("restore the config dir");

    assert_eq!(json_output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&json_output)).expect("a JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("credentials.json"),
        "the envelope does not name the layer that failed: {}",
        stdout_of(&json_output)
    );

    assert_eq!(plain.status.code(), Some(1));
    assert!(
        stderr_of(&plain).starts_with("[Validation error] "),
        "stderr: {}",
        stderr_of(&plain)
    );
    assert!(
        stderr_of(&plain).contains("credentials.json"),
        "stderr: {}",
        stderr_of(&plain)
    );
    assert!(
        stdout_of(&plain).is_empty(),
        "stdout: {}",
        stdout_of(&plain)
    );

    assert!(
        fs::read_to_string(credentials_file(&home))
            .expect("the credential file survives the failed delete")
            .contains("ado-cli-test-org-token"),
        "the credential is still on disk"
    );
    assert!(
        fs::read_to_string(config_file(&home))
            .expect("config.toml")
            .contains("[orgs.ado-cli-test-org]"),
        "a failed credential removal leaves the config entry alone"
    );
}

/// The oracle's node: `Remove stored credentials.`, no arguments, no subcommands.
/// Our clap tree adds the five globals to every node (D2, consolidated as D23), so
/// only the node's own option set is empty on both sides.
#[test]
fn the_schema_node_is_the_oracles() {
    let logout = find_node("logout").expect("the logout node");

    assert_eq!(logout["name"], json!("ado logout"));
    assert_eq!(
        logout["doc"],
        json!("Remove stored credentials."),
        "the oracle's doc, verbatim (captured: ado schema logout --json)"
    );
    assert_eq!(logout["arguments"], json!([]));
    assert_eq!(logout["subcommands"], json!([]));

    let options = logout["options"].as_array().expect("the option array");
    let mut names = options
        .iter()
        .map(|option| option["name"].as_str().expect("an option name"))
        .collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(
        names,
        ["json", "org", "pat", "server", "verbose"],
        "the globals clap copies into every node; logout has no local option"
    );
    assert!(
        options
            .iter()
            .all(|option| option["required"] == json!(false)),
        "logout requires no flag"
    );
}
