//! End-to-end tests for `ado logout`: its envelope, its exit codes, and what it
//! leaves on disk.
//!
//! Every test owns its environment — a `TempHome` with the `ADO_*` variables
//! removed unless the test sets them — so no test reads or writes the developer's
//! own config or home. Under a `TempHome` the OS keychain is unreachable (macOS
//! resolves its default keychain through `HOME`; CI Linux has no secret service),
//! so the credential file layer is the one the command actually rewrites, which is
//! also the production shape on a headless box.
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
use ado_testkit::{TempHome, ado_cmd, stderr_of, stdout_of};
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

/// A two-org configuration: `myorg` is the default and holds the secret; `other`
/// is the non-secret setting a logout must not destroy.
const TWO_ORG_CONFIG: &str = "default_org = \"myorg\"\nserver = \"https://ado.example.com\"\n\n[orgs.myorg]\nauth = \"pat\"\n\n[orgs.other]\nauth = \"browser\"\n";

/// The same configuration after `myorg` is logged out.
const TWO_ORG_CONFIG_AFTER: &str =
    "server = \"https://ado.example.com\"\n\n[orgs.other]\nauth = \"browser\"\n";

const TWO_ORG_CREDENTIALS: &str = "{\n  \"myorg\": {\n    \"method\": \"pat\",\n    \"token\": \"myorg-token\"\n  },\n  \"other\": {\n    \"method\": \"browser\",\n    \"token\": \"other-token\"\n  }\n}";

const TWO_ORG_CREDENTIALS_AFTER: &str =
    "{\n  \"other\": {\n    \"method\": \"browser\",\n    \"token\": \"other-token\"\n  }\n}";

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
        format!("{}\n", logout_json("myorg")),
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
            .contains("myorg-token"),
        "the removed token is still on disk"
    );

    // The post-condition the command exists for: the next run cannot resolve the
    // credential that was logged out, and it fails before any request is built.
    let next = run(&home, &["projects", "list", "--org", "myorg", "--json"]);

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
    let config = "default_org = \"myorg\"\n\n[orgs.myorg]\nauth = \"pat\"\n";
    let credentials =
        "{\n  \"myorg\": {\n    \"method\": \"pat\",\n    \"token\": \"myorg-token\"\n  }\n}";
    seed(&home, config, credentials);

    let output = run(&home, &["logout", "--json"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), format!("{}\n", logout_json("myorg")));
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

/// The second run behaves like the first's shape: exit 0, the same bytes, no
/// error — the oracle prints the same line twice, ours is idempotent too.
#[test]
fn logout_twice_in_a_row_is_idempotent() {
    let home = TempHome::new();
    seed(
        &home,
        "default_org = \"myorg\"\n\n[orgs.myorg]\nauth = \"pat\"\n",
        "{\n  \"myorg\": {\n    \"method\": \"pat\",\n    \"token\": \"myorg-token\"\n  }\n}",
    );

    let mut first = command(&home, &["logout", "--json"]);
    first.env("ADO_ORG", "myorg");
    let first = first.output().expect("run the first logout");

    let mut second = command(&home, &["logout", "--json"]);
    second.env("ADO_ORG", "myorg");
    let second = second.output().expect("run the second logout");

    assert_success(&first);
    assert_success(&second);
    assert_eq!(
        stdout_of(&first),
        format!("{}\n", logout_json("myorg")),
        "the flag/env organization resolves both runs"
    );
    assert_eq!(stdout_of(&second), stdout_of(&first), "idempotent");
    assert!(
        !config_file(&home).exists() && !credentials_file(&home).exists(),
        "the second run recreates nothing"
    );
}

/// Plain output names the organization and writes nothing to stderr.
#[test]
fn logout_plain_output_is_the_message_line() {
    let home = TempHome::new();
    seed(
        &home,
        "default_org = \"myorg\"\n\n[orgs.myorg]\nauth = \"pat\"\n",
        "{\n  \"myorg\": {\n    \"method\": \"pat\",\n    \"token\": \"myorg-token\"\n  }\n}",
    );

    let output = run(&home, &["logout"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), logout_plain("myorg"));
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
        "default_org = \"myorg\"\n\n[orgs.myorg]\nauth = \"pat\"\n",
        "{\n  \"myorg\": {\n    \"method\": \"pat\",\n    \"token\": \"myorg-token\"\n  }\n}",
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
            .contains("myorg-token"),
        "the credential is still on disk"
    );
    assert!(
        fs::read_to_string(config_file(&home))
            .expect("config.toml")
            .contains("[orgs.myorg]"),
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
