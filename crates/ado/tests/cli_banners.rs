//! End-to-end tests for `ado banners show|set|delete`: the organization-scoped
//! `_apis/settings/entries/banners` surface the frozen
//! `lib/ado_cli/cli/banners.ex` builds, the put body, the two human views and the
//! error paths — including the `@<file>`/`-` message forms Ruling 4(b) repairs.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a
//! mock server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT`
//! credentials, so no credential resolution reaches the developer's keychain.
//! Every run scripts stdin explicitly, so no test can read a terminal.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";
const BANNERS: &str = "/myorg/_apis/settings/entries/banners";

fn command(home: &TempHome, server: &MockServer, org: &str, args: &[&str]) -> Command {
    let mut command = ado_cmd();
    home.apply(&mut command);
    command
        .env("ADO_ORG", org)
        .env("ADO_PAT", "test-pat")
        .env("ADO_SERVER", server.base_url())
        .args(args);
    command
}

fn run(home: &TempHome, server: &MockServer, args: &[&str]) -> Output {
    command(home, server, ORG, args)
        .stdin(Stdio::null())
        .output()
        .expect("run ado")
}

/// Runs the binary with `stdin` written to a pipe, never a terminal.
fn run_with_stdin(home: &TempHome, server: &MockServer, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = command(home, server, ORG, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ado");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(stdin)
        .expect("write the scripted stdin");
    child.wait_with_output().expect("wait for ado")
}

fn assert_success(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(output)
    );
}

fn envelope(output: &Output) -> Value {
    serde_json::from_str(&stdout_of(output)).expect("stdout is one JSON document")
}

fn requests(server: &MockServer) -> Vec<RecordedRequest> {
    server.received()
}

fn sent_body(request: &RecordedRequest) -> Value {
    serde_json::from_str(request.body.as_deref().expect("a request body"))
        .expect("the request body is JSON")
}

/// The captured settings entry (`banner_show.json`).
fn entry() -> Value {
    json!({"id": "banners", "value": {
        "message": "Scheduled maintenance tonight",
        "type": "warning",
        "level": "projectCollection"
    }})
}

fn usage_error(output: &Output, names: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(output)
    );
    assert!(
        stderr_of(output).contains(names),
        "the usage error names {names}: {}",
        stderr_of(output)
    );
}

// ── show ────────────────────────────────────────────────────────────────

#[test]
fn show_emits_the_value_object_and_the_org_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", BANNERS, MockResponse::json(200, entry()));

    let output = run(&home, &server, &["banners", "show", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": entry()["value"]}),
        "the entry's value member, not the whole settings entry"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, BANNERS);
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())]
    );
}

#[test]
fn show_renders_the_modules_three_labelled_fields() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", BANNERS, MockResponse::json(200, entry()));

    let output = run(&home, &server, &["banners", "show"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\nCurrent banner:\n",
            "  Message: Scheduled maintenance tonight\n",
            "  Type:    warning\n",
            "  Level:   projectCollection\n"
        ),
        "the module's layout (captured)"
    );
}

#[test]
fn show_renders_the_modules_defaults_for_missing_fields() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BANNERS,
        MockResponse::json(
            200,
            json!({"id": "banners", "value": {"message": "Heads up"}}),
        ),
    );

    let output = run(&home, &server, &["banners", "show"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\nCurrent banner:\n",
            "  Message: Heads up\n",
            "  Type:    info\n",
            "  Level:   projectCollection\n"
        ),
        "the module's `|| \"info\"`/`|| \"projectCollection\"` defaults"
    );
}

#[test]
fn show_prints_the_empty_value_sentence_for_an_empty_value() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BANNERS,
        MockResponse::json(200, json!({"id": "banners", "value": {}})),
    );

    let output = run(&home, &server, &["banners", "show"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "\nNo banner configured.\n");

    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BANNERS,
        MockResponse::json(200, json!({"id": "banners", "value": {}})),
    );

    let output = run(&home, &server, &["banners", "show", "--json"]);

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": {}}));
}

#[test]
fn show_treats_a_missing_entry_as_the_empty_banner_and_exits_zero() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BANNERS,
        MockResponse::json(
            404,
            json!({"message": "The settings entry does not exist."}),
        ),
    );

    let output = run(&home, &server, &["banners", "show"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "\nNo banner configured.\n");

    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BANNERS,
        MockResponse::json(
            404,
            json!({"message": "The settings entry does not exist."}),
        ),
    );

    let output = run(&home, &server, &["banners", "show", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": {}}),
        "the same empty-value document the present-but-empty entry answers"
    );
    assert!(stderr_of(&output).is_empty(), "no error line on this path");
}

#[test]
fn show_500_is_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BANNERS,
        MockResponse::json(500, json!({"message": "TF400813: unavailable"})),
    );

    let output = run(&home, &server, &["banners", "show", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
    assert_eq!(
        envelope["error"]["message"],
        json!("Azure DevOps server error. Retry later.")
    );
}

#[test]
fn show_rejects_an_extra_positional() {
    let home = TempHome::new();
    let server = MockServer::start();

    usage_error(&run(&home, &server, &["banners", "show", "Extra"]), "Extra");
    assert!(requests(&server).is_empty(), "a usage error sends nothing");
}

// ── set ─────────────────────────────────────────────────────────────────

#[test]
fn set_puts_the_value_object_with_the_modules_two_defaults() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("PUT", BANNERS, MockResponse::json(200, entry()));

    let output = run(
        &home,
        &server,
        &[
            "banners",
            "set",
            "--message",
            "Maintenance tonight",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0]),
        json!({"value": {
            "message": "Maintenance tonight",
            "type": "info",
            "level": "projectCollection"
        }}),
        "the module's body with its two `Map.get` defaults"
    );
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Banner set: \"Maintenance tonight\""}),
        "the message envelope (D33's message class)"
    );
}

#[test]
fn set_prints_the_modules_line_in_human_mode() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("PUT", BANNERS, MockResponse::json(200, entry()));

    let output = run(
        &home,
        &server,
        &["banners", "set", "--message", "Maintenance tonight"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Banner set: \"Maintenance tonight\"\n");
}

#[test]
fn set_sends_the_given_type_and_level() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("PUT", BANNERS, MockResponse::json(200, entry()));

    let output = run(
        &home,
        &server,
        &[
            "banners",
            "set",
            "--message",
            "Maintenance tonight",
            "--type",
            "warning",
            "--level",
            "project",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0])["value"],
        json!({"message": "Maintenance tonight", "type": "warning", "level": "project"})
    );
}

#[test]
fn set_sends_a_present_empty_type_as_the_empty_string() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("PUT", BANNERS, MockResponse::json(200, entry()));

    let output = run(
        &home,
        &server,
        &[
            "banners",
            "set",
            "--message",
            "Heads up",
            "--type",
            "",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0])["value"]["type"],
        json!(""),
        "a present empty option is not the absent default"
    );
}

#[test]
fn set_without_the_message_option_is_a_loud_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["banners", "set", "--json"]);

    usage_error(&output, "message");
    assert!(
        requests(&server).is_empty(),
        "no request, unlike D34's silence"
    );
}

#[test]
fn set_reads_an_at_file_message_trimmed() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("PUT", BANNERS, MockResponse::json(200, entry()));
    let path = home.path().join("banner-message.txt");
    std::fs::write(&path, "Message from a file.\n\n").expect("write the message file");

    let output = run(
        &home,
        &server,
        &[
            "banners",
            "set",
            "--message",
            &format!("@{}", path.display()),
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0])["value"]["message"],
        json!("Message from a file."),
        "the file's content, trimmed (Ruling 4(b))"
    );
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Banner set: \"Message from a file.\""}),
        "the success line names the resolved message"
    );
}

#[test]
fn set_reads_a_dash_message_from_stdin_trimmed() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("PUT", BANNERS, MockResponse::json(200, entry()));

    let output = run_with_stdin(
        &home,
        &server,
        &["banners", "set", "--message", "-", "--json"],
        b"From stdin.\n",
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0])["value"]["message"],
        json!("From stdin."),
        "stdin, trimmed (Ruling 4(b))"
    );
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Banner set: \"From stdin.\""})
    );
}

#[test]
fn set_with_a_missing_message_file_is_a_validation_error_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["banners", "set", "--message", "@nope.txt", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .starts_with("Cannot read message file \"nope.txt\": "),
        "the connections-convention wording names the file: {}",
        envelope["error"]["message"]
    );
    assert!(requests(&server).is_empty(), "no request is built");
}

#[test]
fn set_404_is_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        BANNERS,
        MockResponse::json(
            404,
            json!({"message": "The settings entry does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["banners", "set", "--message", "Heads up", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
}

// ── delete ──────────────────────────────────────────────────────────────

#[test]
fn delete_sends_the_org_path_and_prints_the_modules_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("DELETE", BANNERS, MockResponse::json(200, json!({})));

    let output = run(&home, &server, &["banners", "delete", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Banner removed."})
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].path, BANNERS);

    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("DELETE", BANNERS, MockResponse::json(200, json!({})));

    let output = run(&home, &server, &["banners", "delete"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Banner removed.\n");
}

#[test]
fn delete_404_is_the_modules_wording_with_no_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        BANNERS,
        MockResponse::json(
            404,
            json!({"message": "The settings entry does not exist."}),
        ),
    );

    let output = run(&home, &server, &["banners", "delete", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("No banner to delete."),
        "the module's own wording, in the error envelope this build always writes (D4)"
    );
    assert!(
        stderr_of(&output).is_empty(),
        "under --json the envelope is the whole output: {}",
        stderr_of(&output)
    );
}

#[test]
fn delete_500_is_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        BANNERS,
        MockResponse::json(500, json!({"message": "TF400813: unavailable"})),
    );

    let output = run(&home, &server, &["banners", "delete", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(envelope(&output)["error"]["code"], json!("api_error"));
}

#[test]
fn the_group_without_a_subcommand_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    usage_error(&run(&home, &server, &["banners"]), "sub-command");
}
