//! End-to-end tests for `ado pipelines list|show`: the REST surface the frozen
//! `lib/ado_cli/cli/pipelines.ex` builds — every method, path and query pair
//! verified against the frozen escript — the `--json` envelopes it emits, and the
//! human table/detail the module's formatters define.
//!
//! Both envelopes are pinned **byte-equal** to the captured oracle lines: every map
//! in the fixtures stays below Elixir's small-map threshold, so the term order the
//! oracle encodes is serde's sorted order, and each capture was confirmed
//! byte-identical to its `jq -S -c` form (W1-R12).
//!
//! Every test owns its environment: a `TempHome` for the config directory, a mock
//! server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT` credentials so no
//! credential resolution reaches the developer's keychain.

use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, fixture_bytes, stderr_of,
    stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";

/// The frozen escript's literal `ado pipelines list Alpha --json` line for the
/// `pipelines_list` fixture (captured from the 0.5.0 binary, W1-R12).
const ORACLE_LIST_JSON: &str = r#"{"ok":true,"result":[{"_links":{"runs":{"href":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/12/runs"},"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/12?revision=4"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=12"}},"configuration":{"path":"pipelines/ci.yml","repository":{"id":"9f1b7e0e-0001-4000-8000-000000000001","name":"Alpha.Core","type":"azureReposGit"},"type":"yaml"},"folder":"\\MyTeam\\Frontend","id":12,"name":"Alpha CI","queue":{"id":9,"name":"Azure Pipelines","pool":{"id":1,"isHosted":true,"name":"Azure Pipelines","poolType":"automation"},"url":"https://dev.azure.com/myorg/Alpha/_apis/build/queues/9"},"revision":4,"url":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/12?revision=4","variables":{"ENV":{"value":"staging"}}},{"_links":{"runs":{"href":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/7/runs"},"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/7?revision=1"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=7"}},"configuration":{"path":"azure-pipelines.yml","repository":{"id":"9f1b7e0e-0001-4000-8000-000000000001","name":"Alpha.Core","type":"azureReposGit"},"type":"yaml"},"folder":"\\","id":7,"name":"Alpha Nightly","queue":{"id":9,"name":"Azure Pipelines","pool":{"id":1,"isHosted":true,"name":"Azure Pipelines","poolType":"automation"},"url":"https://dev.azure.com/myorg/Alpha/_apis/build/queues/9"},"revision":1,"url":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/7?revision=1","variables":{}}]}"#;

/// The frozen escript's literal `ado pipelines show Alpha 12 --json` line for the
/// `pipelines_show` fixture.
const ORACLE_SHOW_JSON: &str = r#"{"ok":true,"result":{"_links":{"runs":{"href":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/12/runs"},"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/12?revision=4"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=12"}},"configuration":{"path":"pipelines/ci.yml","repository":{"id":"9f1b7e0e-0001-4000-8000-000000000001","name":"Alpha.Core","type":"azureReposGit"},"type":"yaml"},"folder":"\\MyTeam\\Frontend","id":12,"name":"Alpha CI","queue":{"id":9,"name":"Azure Pipelines","pool":{"id":1,"isHosted":true,"name":"Azure Pipelines","poolType":"automation"},"url":"https://dev.azure.com/myorg/Alpha/_apis/build/queues/9"},"revision":4,"url":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/12?revision=4","variables":{"DB_PASS":{"isSecret":true},"DEBUG":{"value":"false"},"ENV":{"value":"staging"}}}}"#;

/// The module's collection path, and the per-definition path `show` appends the
/// numeric id to.
const LIST_PATH: &str = "/myorg/Alpha/_apis/pipelines";
const SHOW_PATH: &str = "/myorg/Alpha/_apis/pipelines/12";

fn command(home: &TempHome, server: &MockServer, args: &[&str]) -> Command {
    let mut command = ado_cmd();
    home.apply(&mut command);
    command
        .env_remove("ADO_ORG")
        .env_remove("ADO_PAT")
        .env_remove("ADO_SERVER")
        .env("ADO_ORG", ORG)
        .env("ADO_PAT", "test-pat")
        .env("ADO_SERVER", server.base_url())
        .args(args);
    command
}

fn run(home: &TempHome, server: &MockServer, args: &[&str]) -> Output {
    command(home, server, args).output().expect("run ado")
}

fn fixture(name: &str) -> Value {
    let response = MockResponse::from_fixture(name);

    serde_json::from_slice(&response.body).expect("the fixture is JSON")
}

fn assert_success(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(output)
    );
}

fn pair(key: &str, value: &str) -> (String, String) {
    (key.to_owned(), value.to_owned())
}

fn api_version() -> (String, String) {
    pair("api-version", "7.1")
}

/// The one request every list flow starts with, answering the fixture when the
/// request carries `query` as wire-form pairs.
fn expect_list(server: &MockServer, query: &[(&str, &str)]) {
    server.expect_query(
        "GET",
        LIST_PATH,
        query,
        MockResponse::from_fixture("pipelines_list"),
    );
}

fn expect_show(server: &MockServer) {
    server.expect(
        "GET",
        SHOW_PATH,
        MockResponse::from_fixture("pipelines_show"),
    );
}

fn assert_list_request(request: &RecordedRequest, query: Vec<(String, String)>) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, LIST_PATH);
    assert_eq!(
        request.query_pairs(),
        query,
        "the list GET's wire-form pairs, in the client's merge order (D12 compares parsed pairs)"
    );
}

fn assert_no_table_bytes(stdout: &str) {
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
    for marker in ['\u{2500}', '\u{2501}', '\u{2502}', '\u{256d}'] {
        assert!(
            !stdout.contains(marker),
            "table or ANSI byte {marker:?} reached output: {stdout:?}"
        );
    }
}

#[test]
fn list_sends_the_pipelines_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(&server, &[("api-version", "7.1")]);

    let output = run(&home, &server, &["pipelines", "list", "Alpha", "--json"]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_LIST_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": fixture("pipelines_list")["value"].clone()}),
        "the value envelope carries the fixture's array under result"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one request per list");
    assert_list_request(&received[0], vec![api_version()]);
}

/// The module's own parameter order (`$top`, then `folder`), with the wire forms
/// the client encodes: `$` is `%24`, `/` is `%2F`.
#[test]
fn list_filters_reach_the_wire_in_the_modules_order() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(
        &server,
        &[
            ("%24top", "5"),
            ("api-version", "7.1"),
            ("folder", "MyTeam%2FFrontend"),
        ],
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "list",
            "Alpha",
            "--top",
            "5",
            "--folder",
            "MyTeam/Frontend",
            "--json",
        ],
    );

    assert_success(&output);
    assert_list_request(
        &server.received()[0],
        vec![
            api_version(),
            pair("%24top", "5"),
            pair("folder", "MyTeam%2FFrontend"),
        ],
    );
}

/// "Present means sent": Elixir's `Map.get` default applies only when the option is
/// absent, and `if value` is truthy for `0` and `""` — so an explicit zero top and an
/// explicit empty folder both reach the wire.
#[test]
fn list_present_empty_and_zero_options_reach_the_wire() {
    let home = TempHome::new();

    let top = MockServer::start();
    expect_list(&top, &[("%24top", "0"), ("api-version", "7.1")]);
    let output = run(
        &home,
        &top,
        &["pipelines", "list", "Alpha", "--top", "0", "--json"],
    );
    assert_success(&output);
    assert_list_request(&top.received()[0], vec![api_version(), pair("%24top", "0")]);

    let folder = MockServer::start();
    expect_list(&folder, &[("api-version", "7.1"), ("folder", "")]);
    let output = run(
        &home,
        &folder,
        &["pipelines", "list", "Alpha", "--folder", "", "--json"],
    );
    assert_success(&output);
    assert_list_request(
        &folder.received()[0],
        vec![api_version(), pair("folder", "")],
    );
}

/// The Elixir `--top` is `:integer` and the oracle parses `--top -1`, sending it as
/// `$top=-1`; clap needs `allow_negative_numbers(true)` to accept the same argv.
#[test]
fn list_negative_top_reaches_the_wire() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(&server, &[("%24top", "-1"), ("api-version", "7.1")]);

    let output = run(
        &home,
        &server,
        &["pipelines", "list", "Alpha", "--top", "-1", "--json"],
    );

    assert_success(&output);
    assert_list_request(
        &server.received()[0],
        vec![api_version(), pair("%24top", "-1")],
    );
}

#[test]
fn list_encodes_the_project_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/My%20Project/_apis/pipelines",
        &[("api-version", "7.1")],
        MockResponse::from_fixture("pipelines_list"),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "list", "My Project", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/pipelines",
        "a space is %20 in a path, not +"
    );
}

/// The stricter path encoder (D22) also escapes `/` and `?`, which Elixir leaves in
/// place (the `?` splits the URL into a query and the oracle's request lands on the
/// wrong path) while project names cannot contain them.
#[test]
fn list_encodes_path_separators_strictly() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/a%2Fb%3Fc/_apis/pipelines",
        &[("api-version", "7.1")],
        MockResponse::from_fixture("pipelines_list"),
    );

    let output = run(&home, &server, &["pipelines", "list", "a/b?c", "--json"]);

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/a%2Fb%3Fc/_apis/pipelines",
        "the stricter encoder escapes / and ? so a name cannot change the URL (D22)"
    );
}

#[test]
fn list_empty_answer_human_output_says_no_pipelines_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        LIST_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["pipelines", "list", "Alpha"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No pipelines found.\n");
    assert_eq!(server.received().len(), 1);
}

#[test]
fn list_empty_answer_json_is_the_empty_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        LIST_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["pipelines", "list", "Alpha", "--json"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");
    assert_eq!(server.received().len(), 1);
}

#[test]
fn list_json_emits_no_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(&server, &[("api-version", "7.1")]);

    let output = run(&home, &server, &["pipelines", "list", "Alpha", "--json"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let envelope: Value =
        serde_json::from_str(&stdout).expect("stdout is exactly one JSON document");

    assert_eq!(envelope["ok"], json!(true));
    assert_eq!(
        envelope["result"].as_array().map(Vec::len),
        Some(2),
        "the envelope is the value form"
    );
    assert!(
        envelope.get("count").is_none() && envelope.get("items").is_none(),
        "the count/items list form never ships (W1-R12): {stdout}"
    );
    assert!(
        !stdout.lines().any(|line| line.starts_with("ID")),
        "a table header reached --json output: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

/// The module's `print_pipelines_table/1`: ID, Name and Folder — the columns the
/// formatter writes, never the help text's wording.
#[test]
fn list_human_output_is_the_pipelines_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(&server, &[("api-version", "7.1")]);

    let output = run(&home, &server, &["pipelines", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        4,
        "a header row, a rule row and one row per pipeline: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header row: {stdout}");
    let name_at = lines[0].find("Name").expect("the Name header");
    let folder_at = lines[0].find("Folder").expect("the Folder header");
    assert!(
        name_at > 0 && folder_at > name_at,
        "the header order is ID, Name, Folder (the module's formatter): {stdout}"
    );
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].starts_with("12")
            && lines[2].contains("Alpha CI")
            && lines[2].ends_with("\\MyTeam\\Frontend"),
        "the first row: {stdout}"
    );
    assert!(
        lines[3].starts_with("7") && lines[3].contains("Alpha Nightly") && lines[3].ends_with('\\'),
        "the second row keeps the root folder's backslash: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn list_without_a_project_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["pipelines", "list", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("PROJECT"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn show_sends_the_pipeline_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_show(&server);

    let output = run(
        &home,
        &server,
        &["pipelines", "show", "Alpha", "12", "--json"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_SHOW_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": fixture("pipelines_show")}),
        "the value envelope carries the pipeline definition"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one request per show");
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, SHOW_PATH);
    assert_eq!(
        received[0].query_pairs(),
        vec![api_version()],
        "show carries no params beyond the version"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn show_encodes_the_project_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/My%20Project/_apis/pipelines/12",
        MockResponse::from_fixture("pipelines_show"),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "show", "My Project", "12", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/pipelines/12"
    );
}

/// The module's `print_pipeline_detail/1`, minus the colour: the labels and
/// fallbacks are the formatter's (60-character rule, optional Type/Path/Web lines,
/// trailing blank line).
#[test]
fn show_human_output_is_the_pipeline_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_show(&server);

    let output = run(&home, &server, &["pipelines", "show", "Alpha", "12"]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(
        stdout.starts_with("\nPipeline Details\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("\n{}\n", "─".repeat(60))),
        "the module's 60-character rule: {stdout}"
    );
    assert!(stdout.contains("  ID:     12\n"), "stdout: {stdout}");
    assert!(stdout.contains("  Name:   Alpha CI\n"), "stdout: {stdout}");
    assert!(
        stdout.contains("  Folder: \\MyTeam\\Frontend\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(
            "  URL:    https://dev.azure.com/myorg/Alpha/_apis/pipelines/12?revision=4\n"
        ),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("  Type:   yaml\n"), "stdout: {stdout}");
    assert!(
        stdout.contains("  Path:   pipelines/ci.yml\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(
            "  Web:    https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=12\n"
        ),
        "stdout: {stdout}"
    );
    assert!(stdout.ends_with("definitionId=12\n\n"), "stdout: {stdout}");
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
}

#[test]
fn show_404_reports_the_pipeline_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Alpha/_apis/pipelines/999",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "show", "Alpha", "999", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Pipeline #999 not found in project 'Alpha'")
    );
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(
        server.received().len(),
        1,
        "the 404 is answered for the definition that was asked for"
    );
}

#[test]
fn show_without_the_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["pipelines", "show", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("PIPELINE_ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn show_with_a_non_numeric_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["pipelines", "show", "Alpha", "abc", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("invalid value"),
        "the Elixir argument is :integer, so clap rejects a non-number: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn pipelines_without_a_subcommand_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["pipelines"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr_of(&output),
        "[Validation error] missing sub-command\n"
    );
    assert!(
        stdout_of(&output).is_empty(),
        "stdout: {}",
        stdout_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a missing subcommand sends no request"
    );
}

/// A closed stdout is a silent success: the table write returns EPIPE and the
/// binary exits 0 with no diagnostic (spec §6.3, R19).
#[test]
fn a_closed_stdout_is_a_silent_success() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(&server, &[("api-version", "7.1")]);

    let mut child = command(&home, &server, &["pipelines", "list", "Alpha"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ado");
    drop(child.stdout.take());

    let output = child.wait_with_output().expect("wait for ado");

    assert_eq!(server.received().len(), 1, "the table was rendered");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(output.stderr.is_empty(), "stderr: {}", stderr_of(&output));
}

// ── write paths: run, create, update, delete and the variable groups ─────
//
// Every path, query and body below is the request the frozen escript sent,
// captured against the standalone mock (spec §4.2); every response the tests
// serve is that capture's answer. The `--json` success output is this build's
// value/message envelope where the frozen CLI prints its human success line
// (D33). Stdin is always scripted, so no test can read a terminal (spec §4.1):
// `/dev/null` is EOF, and the captures show no prompt for any of these commands
// (R5) — the deletes proceed on EOF and on `n`.

use std::io::Write;

/// Runs the binary with `stdin` written to a pipe (never a terminal); an empty
/// slice is EOF.
fn run_with_stdin(home: &TempHome, server: &MockServer, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = command(home, server, args)
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

fn sent_body(request: &RecordedRequest) -> Value {
    serde_json::from_str(request.body.as_deref().expect("a request body"))
        .expect("the request body is JSON")
}

/// The success envelope, read from a mutation's stdout as its one document.
fn mutation_envelope(output: &Output) -> Value {
    assert_success(output);
    serde_json::from_str(&stdout_of(output)).expect("stdout is exactly one JSON document")
}

/// The captured `POST .../pipelines/12/runs` answer.
fn triggered_run() -> Value {
    json!({
        "id": 99,
        "name": "20260927.1",
        "status": "inProgress",
        "state": "inProgress",
        "createdDate": "2026-09-27T10:00:00.000Z",
        "url": "https://dev.azure.com/ado-harness/Alpha/_apis/pipelines/12/runs/99",
        "pipeline": {
            "id": 12,
            "name": "Alpha CI",
            "folder": "\\",
            "revision": 5,
            "url": "https://dev.azure.com/ado-harness/Alpha/_apis/pipelines/12",
        },
        "_links": {
            "web": {"href": "https://dev.azure.com/ado-harness/Alpha/_build/results?buildId=99"},
            "self": {"href": "https://dev.azure.com/ado-harness/Alpha/_apis/pipelines/12/runs/99"},
        },
    })
}

/// The captured `POST .../pipelines` answer.
fn created_pipeline() -> Value {
    json!({
        "id": 15,
        "name": "New CI",
        "folder": "/",
        "revision": 1,
        "configuration": {
            "type": "yaml",
            "path": "pipelines/new.yml",
            "repository": {"id": "Alpha.Core", "name": "Alpha.Core", "type": "azureReposGit"},
        },
        "url": "https://dev.azure.com/ado-harness/Alpha/_apis/pipelines/15?revision=1",
        "_links": {"web": {"href": "https://dev.azure.com/ado-harness/Alpha/_build/definition?definitionId=15"}},
    })
}

/// The captured `PATCH .../pipelines/12` answer.
fn updated_pipeline() -> Value {
    json!({
        "id": 12,
        "name": "Alpha CI (renamed)",
        "folder": "\\",
        "revision": 6,
        "configuration": {
            "type": "yaml",
            "path": "pipelines/renamed.yml",
            "repository": {"id": "Alpha.Core", "name": "Alpha.Core", "type": "azureReposGit"},
        },
        "url": "https://dev.azure.com/ado-harness/Alpha/_apis/pipelines/12?revision=6",
        "_links": {"web": {"href": "https://dev.azure.com/ado-harness/Alpha/_build/definition?definitionId=12"}},
    })
}

/// The captured `GET .../variablegroups` answer.
fn variable_groups() -> Value {
    json!({
        "count": 2,
        "value": [
            {
                "id": 5,
                "name": "prod-secrets",
                "description": "Production secrets for the deploy stage",
                "type": "Vsts",
                "variables": {
                    "DB_HOST": {"value": "db.example.com"},
                    "DB_PASS": {"isSecret": true},
                },
                "variableGroupProjectReferences": [
                    {"name": "prod-secrets", "projectReference": {"name": "Alpha"}},
                ],
            },
            {
                "id": 6,
                "name": "ci-shared",
                "type": "Vsts",
                "variables": {},
                "variableGroupProjectReferences": [
                    {"name": "ci-shared", "projectReference": {"name": "Alpha"}},
                ],
            },
        ],
    })
}

/// The captured `GET .../variablegroups/5` answer.
fn variable_group() -> Value {
    json!({
        "id": 5,
        "name": "prod-secrets",
        "description": "Production secrets for the deploy stage",
        "type": "Vsts",
        "variables": {
            "DB_HOST": {"value": "db.example.com"},
            "DB_PASS": {"isSecret": true},
        },
        "variableGroupProjectReferences": [
            {"name": "prod-secrets", "projectReference": {"name": "Alpha"}},
        ],
    })
}

/// The captured `POST .../variablegroups` answer.
fn created_variable_group() -> Value {
    json!({
        "id": 7,
        "name": "new-group",
        "description": "A new group",
        "type": "Vsts",
        "variables": {
            "DB_HOST": {"value": "db.example.com", "isSecret": false},
            "DB_PASS": {"value": "hunter2", "isSecret": true},
        },
        "variableGroupProjectReferences": [
            {"name": "new-group", "projectReference": {"name": "Alpha"}},
        ],
    })
}

/// The captured `PUT .../variablegroups/5` answer.
fn updated_variable_group() -> Value {
    json!({
        "id": 5,
        "name": "prod-secrets-renamed",
        "description": "Updated description",
        "type": "Vsts",
        "variables": {
            "DB_HOST": {"value": "db2.example.com"},
            "DB_PASS": {"isSecret": true},
            "API_KEY": {"value": "secret-value", "isSecret": true},
        },
        "variableGroupProjectReferences": [
            {"name": "prod-secrets-renamed", "projectReference": {"name": "Alpha"}},
        ],
    })
}

/// The captured `GET /_apis/projects` answer `vars delete` resolves against:
/// Alpha by name, and the id the DELETE's `projectIds` carries.
fn projects_lookup() -> Value {
    json!({
        "count": 1,
        "value": [
            {"id": "6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c", "name": "Alpha", "state": "wellFormed"},
        ],
    })
}

const RUN_PATH: &str = "/myorg/Alpha/_apis/pipelines/12/runs";
const PIPELINES_PATH: &str = "/myorg/Alpha/_apis/pipelines";
const PIPELINE_PATH: &str = "/myorg/Alpha/_apis/pipelines/12";
const PIPELINE_MISSING_PATH: &str = "/myorg/Alpha/_apis/pipelines/999";
const VARS_PATH: &str = "/myorg/Alpha/_apis/distributedtask/variablegroups";
const VARS_GROUP_PATH: &str = "/myorg/Alpha/_apis/distributedtask/variablegroups/5";
const VARS_MISSING_PATH: &str = "/myorg/Alpha/_apis/distributedtask/variablegroups/999";
const PROJECTS_LOOKUP_PATH: &str = "/myorg/_apis/projects";

#[test]
fn run_sends_the_captured_body_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", RUN_PATH, MockResponse::json(200, triggered_run()));

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "run",
            "Alpha",
            "12",
            "--branch",
            "feature/foo",
            "--variables",
            "ENV=staging,DEBUG=true",
            "--json",
        ],
        &[],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "result": triggered_run()}),
        "the created run is the value envelope (D33)"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "run sends one request");
    assert_eq!(received[0].method, "POST");
    assert_eq!(received[0].path, RUN_PATH);
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
    assert_eq!(
        sent_body(&received[0]),
        json!({
            "resources": {"repositories": {"self": {"refName": "refs/heads/feature/foo"}}},
            "variables": {"ENV": {"value": "staging"}, "DEBUG": {"value": "true"}},
        }),
        "the captured body: the ref with its refs/heads/ prefix, and each variable as a value object"
    );
}

/// `Map.get(parsed.options, :branch, "main")`: an absent branch is the default
/// `main`, and an absent `--variables` adds no `variables` key at all.
#[test]
fn run_without_options_sends_the_default_main_ref() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", RUN_PATH, MockResponse::json(200, triggered_run()));

    let output = run_with_stdin(&home, &server, &["pipelines", "run", "Alpha", "12"], &[]);

    assert_success(&output);
    let body = sent_body(&server.received()[0]);

    assert_eq!(
        body,
        json!({"resources": {"repositories": {"self": {"refName": "refs/heads/main"}}}}),
        "the frozen default branch"
    );
    assert!(
        body.get("variables").is_none(),
        "an absent --variables is an absent key, not an empty map: {body}"
    );
}

/// Captured: a pair without `=` is dropped, but a present `--variables` still
/// sends the key — with an empty object when every pair is dropped.
#[test]
fn run_drops_a_pair_without_an_equals_sign_and_keeps_the_variables_key() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", RUN_PATH, MockResponse::json(200, triggered_run()));

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "run", "Alpha", "12", "--variables", "NOEQUALS"],
        &[],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[0]),
        json!({
            "resources": {"repositories": {"self": {"refName": "refs/heads/main"}}},
            "variables": {},
        }),
        "the pair is dropped, the key stays (captured)"
    );
}

#[test]
fn run_human_output_is_the_run_summary() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", RUN_PATH, MockResponse::json(200, triggered_run()));

    let output = run_with_stdin(&home, &server, &["pipelines", "run", "Alpha", "12"], &[]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(
        stdout.starts_with("Pipeline run triggered!\n"),
        "stdout: {stdout}"
    );
    for line in [
        "  Run ID:   99\n",
        "  State:    inProgress\n",
        "  Pipeline: Alpha CI\n",
        "  URL:      https://dev.azure.com/ado-harness/Alpha/_build/results?buildId=99\n",
    ] {
        assert!(stdout.contains(line), "missing {line:?} in {stdout}");
    }
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
}

#[test]
fn create_sends_the_captured_body_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        PIPELINES_PATH,
        MockResponse::json(200, created_pipeline()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "create",
            "Alpha",
            "--name",
            "New CI",
            "--repo",
            "Alpha.Core",
            "--path",
            "pipelines/new.yml",
            "--json",
        ],
        &[],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "result": created_pipeline()}),
        "the created pipeline is the value envelope (D33)"
    );
    assert_eq!(
        sent_body(&server.received()[0]),
        json!({
            "name": "New CI",
            "folder": "/",
            "configuration": {
                "type": "yaml",
                "path": "pipelines/new.yml",
                "repository": {"id": "Alpha.Core", "name": "Alpha.Core", "type": "azureReposGit"},
            },
        }),
        "the captured body: the default folder '/' and the repo as both id and name"
    );
}

#[test]
fn create_with_a_folder_sends_it_verbatim() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        PIPELINES_PATH,
        MockResponse::json(200, created_pipeline()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "create",
            "Alpha",
            "--name",
            "New CI",
            "--repo",
            "Alpha.Core",
            "--path",
            "pipelines/new.yml",
            "--folder",
            "MyTeam/Frontend",
            "--json",
        ],
        &[],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[0])["folder"],
        json!("MyTeam/Frontend"),
        "the folder reaches the wire as given"
    );
}

#[test]
fn create_conflict_is_the_conflict_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        PIPELINES_PATH,
        MockResponse::json(
            409,
            json!({"message": "A pipeline with the same name already exists."}),
        ),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "create",
            "Alpha",
            "--name",
            "New CI",
            "--repo",
            "Alpha.Core",
            "--path",
            "pipelines/new.yml",
            "--json",
        ],
        &[],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = mutation_envelope_failure(&output);
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(
        envelope["error"]["code"],
        json!("conflict"),
        "the captured 409 class (spec §4.3)"
    );
    assert_eq!(envelope["error"]["status"], json!(409));
}

/// The error envelope, read from stdout under `--json` (a failure still writes
/// one document there; D4's rule).
fn mutation_envelope_failure(output: &Output) -> Value {
    serde_json::from_str(&stdout_of(output)).expect("stdout is exactly one JSON document")
}

/// The not-found contract these commands share: exit 1, the `not_found` code and
/// status, and the module's own message. `details.body` is D24's upstream-bytes
/// row, asserted by the client's own tests, so it is not repeated here.
fn assert_not_found_envelope(output: &Output, message: &str) {
    assert_eq!(output.status.code(), Some(1));
    let envelope = mutation_envelope_failure(output);

    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(envelope["error"]["message"], json!(message));
}

/// R4's class, asserted here: the oracle's CliMate never validates the required
/// option and exits 0 with nothing on either stream; this build's clap makes it a
/// loud usage error before any credential or request (D5/D23).
#[test]
fn create_without_a_name_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "create",
            "Alpha",
            "--repo",
            "Alpha.Core",
            "--path",
            "pipelines/new.yml",
            "--json",
        ],
        &[],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("--name"),
        "the usage error names the missing option: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn create_human_output_is_the_success_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        PIPELINES_PATH,
        MockResponse::json(200, created_pipeline()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "create",
            "Alpha",
            "--name",
            "New CI",
            "--repo",
            "Alpha.Core",
            "--path",
            "pipelines/new.yml",
        ],
        &[],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Pipeline 'New CI' created (ID: 15).\n");
}

#[test]
fn update_sends_only_the_options_it_was_given() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        PIPELINE_PATH,
        MockResponse::json(200, updated_pipeline()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "update",
            "Alpha",
            "12",
            "--name",
            "Alpha CI (renamed)",
            "--path",
            "pipelines/renamed.yml",
            "--json",
        ],
        &[],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "result": updated_pipeline()}),
        "the updated pipeline is the value envelope (D33)"
    );

    let received = server.received();
    assert_eq!(received[0].method, "PATCH");
    assert_eq!(received[0].path, PIPELINE_PATH);
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
    assert_eq!(
        sent_body(&received[0]),
        json!({"name": "Alpha CI (renamed)", "configuration": {"path": "pipelines/renamed.yml"}}),
        "the captured body: the name and only the path inside a configuration object"
    );
}

/// The module's `Map.put(body, :configuration, Map.put(%{}, :path, path))`: a
/// `--path`-only update sends the configuration object and no name.
#[test]
fn update_with_only_the_path_omits_the_name() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        PIPELINE_PATH,
        MockResponse::json(200, updated_pipeline()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "update",
            "Alpha",
            "12",
            "--path",
            "pipelines/renamed.yml",
            "--json",
        ],
        &[],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[0]),
        json!({"configuration": {"path": "pipelines/renamed.yml"}}),
        "the captured body has no name key"
    );
}

/// The module's own guard runs before any request: no `--name` and no `--path` is
/// a validation error (captured: the oracle writes its message to stderr and
/// sends nothing).
#[test]
fn update_without_options_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "update", "Alpha", "12", "--json"],
        &[],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        mutation_envelope_failure(&output),
        json!({
            "ok": false,
            "error": {
                "code": "validation_error",
                "message": "At least one of --name or --path is required.",
            },
        })
    );
    assert!(
        server.received().is_empty(),
        "the guard runs before any request"
    );
}

#[test]
fn update_404_reports_the_pipeline_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        PIPELINE_MISSING_PATH,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "update",
            "Alpha",
            "999",
            "--name",
            "Renamed",
            "--json",
        ],
        &[],
    );

    assert_not_found_envelope(&output, "Pipeline #999 not found");
}

#[test]
fn update_human_output_is_pipeline_updated() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        PIPELINE_PATH,
        MockResponse::json(200, updated_pipeline()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "update",
            "Alpha",
            "12",
            "--name",
            "Alpha CI (renamed)",
        ],
        &[],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Pipeline updated.\n");
}

/// R5, as an executable case: the captured `pipelines delete` has no prompt and
/// no `--force`; EOF (an unscripted stdin) proceeds to the delete instead of
/// stopping at a question.
#[test]
fn delete_sends_the_delete_without_prompting() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        PIPELINE_PATH,
        MockResponse::json(200, json!({"id": 12})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "delete", "Alpha", "12", "--json"],
        &[],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "message": "Pipeline deleted."}),
        "the delete's message envelope (D33)"
    );

    let received = server.received();
    assert_eq!(
        received.len(),
        1,
        "EOF still sends the delete (R5: no prompt)"
    );
    assert_eq!(received[0].method, "DELETE");
    assert_eq!(received[0].path, PIPELINE_PATH);
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

/// Captured: `ado pipelines delete Alpha 12 --force` is `invalid option --force`
/// in the oracle too — this command never had the flag, so this build does not
/// add one (spec §4.4).
#[test]
fn delete_has_no_force_option() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "delete", "Alpha", "12", "--force"],
        &[],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("--force"),
        "the usage error names the unknown flag: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn delete_404_reports_the_pipeline_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        PIPELINE_MISSING_PATH,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "delete", "Alpha", "999", "--json"],
        &[],
    );

    assert_not_found_envelope(&output, "Pipeline #999 not found");
}

#[test]
fn vars_list_sends_top_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", VARS_PATH, MockResponse::json(200, variable_groups()));

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "vars",
            "list",
            "Alpha",
            "--top",
            "10",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("one document"),
        json!({"ok": true, "result": variable_groups()["value"].clone()}),
        "the value envelope carries the group array"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].path, VARS_PATH);
    assert_eq!(
        received[0].query_pairs(),
        vec![api_version(), pair("%24top", "10")]
    );
}

#[test]
fn vars_list_without_top_sends_only_the_api_version() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", VARS_PATH, MockResponse::json(200, variable_groups()));

    let output = run(
        &home,
        &server,
        &["pipelines", "vars", "list", "Alpha", "--json"],
    );

    assert_success(&output);
    assert_eq!(server.received()[0].query_pairs(), vec![api_version()]);
}

/// The list endpoints' envelope is the value form even when empty; the human
/// path keeps the module's own message.
#[test]
fn vars_list_empty_answer_is_an_empty_envelope_and_the_module_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        VARS_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "vars", "list", "Alpha", "--json"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");

    let human = MockServer::start();
    human.expect(
        "GET",
        VARS_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );
    let output = run(&home, &human, &["pipelines", "vars", "list", "Alpha"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No variable groups found.\n");
}

#[test]
fn vars_list_human_output_is_the_variable_groups_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", VARS_PATH, MockResponse::json(200, variable_groups()));

    let output = run(&home, &server, &["pipelines", "vars", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        4,
        "a header row, a rule row and one row per group: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header row: {stdout}");
    let name_at = lines[0].find("Name").expect("the Name header");
    let description_at = lines[0]
        .find("Description")
        .expect("the Description header");
    let count_at = lines[0].find("Variables").expect("the Variables header");
    assert!(
        name_at < description_at && description_at < count_at,
        "the module's column order — ID, Name, Description, Variables: {stdout}"
    );
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].starts_with('5') && lines[2].contains("prod-secrets") && lines[2].ends_with('2'),
        "the first row counts the group's two variables: {stdout}"
    );
    assert!(
        lines[3].starts_with('6') && lines[3].contains("ci-shared") && lines[3].ends_with('0'),
        "the second row counts zero: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn vars_show_sends_the_group_path_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        VARS_GROUP_PATH,
        MockResponse::json(200, variable_group()),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "vars", "show", "Alpha", "5", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", json!({"ok": true, "result": variable_group()}))
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, VARS_GROUP_PATH);
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

#[test]
fn vars_show_human_output_is_the_variable_group_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        VARS_GROUP_PATH,
        MockResponse::json(200, variable_group()),
    );

    let output = run(&home, &server, &["pipelines", "vars", "show", "Alpha", "5"]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(
        stdout.starts_with("\nVariable Group Details\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("\n{}\n", "─".repeat(60))),
        "the module's 60-character rule: {stdout}"
    );
    assert!(stdout.contains("  ID:          5\n"), "stdout: {stdout}");
    assert!(
        stdout.contains("  Name:        prod-secrets\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Description: Production secrets for the deploy stage\n"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("  Type:        Vsts\n"), "stdout: {stdout}");
    assert!(stdout.contains("\n  Variables:\n"), "stdout: {stdout}");
    assert!(stdout.contains("    DB_HOST\n"), "stdout: {stdout}");
    assert!(
        stdout.contains("    DB_PASS [secret]\n"),
        "a secret's value is never printed, only its marker: {stdout}"
    );
    assert!(
        !stdout.contains("db.example.com"),
        "no variable value is printed: {stdout}"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
}

#[test]
fn vars_show_404_reports_the_group_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        VARS_MISSING_PATH,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "vars", "show", "Alpha", "999", "--json"],
    );

    assert_not_found_envelope(&output, "Variable group #999 not found in project 'Alpha'");
}

#[test]
fn vars_create_sends_the_captured_body_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        VARS_PATH,
        MockResponse::json(200, created_variable_group()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "vars",
            "create",
            "Alpha",
            "--name",
            "new-group",
            "--description",
            "A new group",
            "--variables",
            "DB_HOST=db.example.com,DB_PASS=hunter2",
            "--secret",
            "DB_PASS",
            "--json",
        ],
        &[],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "result": created_variable_group()}),
        "the created group is the value envelope (D33)"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "POST");
    assert_eq!(received[0].path, VARS_PATH);
    assert_eq!(
        sent_body(&received[0]),
        json!({
            "name": "new-group",
            "type": "Vsts",
            "variableGroupProjectReferences": [
                {"name": "new-group", "projectReference": {"name": "Alpha"}},
            ],
            "description": "A new group",
            "variables": {
                "DB_HOST": {"value": "db.example.com", "isSecret": false},
                "DB_PASS": {"value": "hunter2", "isSecret": true},
            },
        }),
        "the captured body: the project reference by name, and the secret set marking one key"
    );
}

#[test]
fn vars_create_without_variables_omits_the_variables_key() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        VARS_PATH,
        MockResponse::json(200, created_variable_group()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "vars",
            "create",
            "Alpha",
            "--name",
            "new-group",
            "--json",
        ],
        &[],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[0]),
        json!({
            "name": "new-group",
            "type": "Vsts",
            "variableGroupProjectReferences": [
                {"name": "new-group", "projectReference": {"name": "Alpha"}},
            ],
        }),
        "captured: no --variables is no variables key, and --secret alone changes nothing"
    );
}

#[test]
fn vars_create_keeps_an_empty_variables_key_when_every_pair_is_dropped() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        VARS_PATH,
        MockResponse::json(200, created_variable_group()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "vars",
            "create",
            "Alpha",
            "--name",
            "new-group",
            "--variables",
            "NOEQUALS",
            "--json",
        ],
        &[],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[0])["variables"],
        json!({}),
        "captured: the pair is dropped and the empty map still ships"
    );
}

/// R4's class, asserted for this command too: the oracle exits 0 silently on the
/// missing required option; this build's clap is loud (D5/D23).
#[test]
fn vars_create_without_a_name_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "vars", "create", "Alpha", "--json"],
        &[],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("--name"),
        "the usage error names the missing option: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

/// The update is a GET of the existing group merged into a PUT: the passed keys
/// win, everything else is the existing body — including
/// `variableGroupProjectReferences`, which a rename does not rewrite.
#[test]
fn vars_update_merges_into_the_existing_group_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        VARS_GROUP_PATH,
        MockResponse::json(200, variable_group()),
    );
    server.expect(
        "PUT",
        VARS_GROUP_PATH,
        MockResponse::json(200, updated_variable_group()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "vars",
            "update",
            "Alpha",
            "5",
            "--name",
            "prod-secrets-renamed",
            "--description",
            "Updated description",
            "--variables",
            "DB_HOST=db2.example.com,API_KEY=secret-value",
            "--secret",
            "API_KEY",
            "--json",
        ],
        &[],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "result": updated_variable_group()}),
        "the updated group is the value envelope (D33)"
    );

    let received = server.received();
    assert_eq!(received.len(), 2, "update fetches, then puts");
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, VARS_GROUP_PATH);
    assert_eq!(received[1].method, "PUT");
    assert_eq!(received[1].path, VARS_GROUP_PATH);
    assert_eq!(
        sent_body(&received[1]),
        json!({
            "name": "prod-secrets-renamed",
            "type": "Vsts",
            "variableGroupProjectReferences": [
                {"name": "prod-secrets", "projectReference": {"name": "Alpha"}},
            ],
            "description": "Updated description",
            "variables": {
                "DB_HOST": {"value": "db2.example.com", "isSecret": false},
                "DB_PASS": {"isSecret": true},
                "API_KEY": {"value": "secret-value", "isSecret": true},
            },
        }),
        "the captured merge: DB_PASS keeps the existing shape, the passed keys are replaced/added, the references stay the existing ones"
    );
}

/// Captured: with no options at all the PUT echoes base + existing variables
/// unchanged (`add_variables_to_body` keeps them when `--variables` is absent).
#[test]
fn vars_update_without_options_echoes_the_existing_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        VARS_GROUP_PATH,
        MockResponse::json(200, variable_group()),
    );
    server.expect(
        "PUT",
        VARS_GROUP_PATH,
        MockResponse::json(200, updated_variable_group()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "vars", "update", "Alpha", "5", "--json"],
        &[],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[1]),
        json!({
            "name": "prod-secrets",
            "description": "Production secrets for the deploy stage",
            "type": "Vsts",
            "variableGroupProjectReferences": [
                {"name": "prod-secrets", "projectReference": {"name": "Alpha"}},
            ],
            "variables": {
                "DB_HOST": {"value": "db.example.com"},
                "DB_PASS": {"isSecret": true},
            },
        }),
        "the captured echo of the existing body"
    );
}

#[test]
fn vars_update_404_reports_the_group_not_found_message_and_sends_no_put() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        VARS_MISSING_PATH,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "vars",
            "update",
            "Alpha",
            "999",
            "--name",
            "Renamed",
            "--json",
        ],
        &[],
    );

    assert_not_found_envelope(&output, "Variable group #999 not found in project 'Alpha'");

    let received = server.received();
    assert_eq!(received.len(), 1, "a 404 on the fetch sends no PUT");
    assert_eq!(received[0].method, "GET");
}

/// Captured: `vars delete` resolves the project name through `GET
/// /_apis/projects` and carries the id as `projectIds`; no prompt stands in front
/// of either request (R5).
#[test]
fn vars_delete_resolves_the_project_id_and_sends_it() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        PROJECTS_LOOKUP_PATH,
        MockResponse::json(200, projects_lookup()),
    );
    server.expect(
        "DELETE",
        VARS_GROUP_PATH,
        MockResponse::json(200, json!({"id": 5})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "vars", "delete", "Alpha", "5", "--json"],
        &[],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "message": "Variable group #5 deleted."}),
        "the delete's message envelope (D33)"
    );

    let received = server.received();
    assert_eq!(
        received.len(),
        2,
        "EOF still sends both requests (R5: no prompt)"
    );
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, PROJECTS_LOOKUP_PATH);
    assert_eq!(received[1].method, "DELETE");
    assert_eq!(received[1].path, VARS_GROUP_PATH);
    assert_eq!(
        received[1].query_pairs(),
        vec![
            api_version(),
            pair("projectIds", "6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c"),
        ],
        "the resolved project id reaches the wire"
    );
}

/// Captured: a project the list does not name sends no `projectIds` at all.
#[test]
fn vars_delete_without_the_project_in_the_list_omits_the_project_ids() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        PROJECTS_LOOKUP_PATH,
        MockResponse::json(200, projects_lookup()),
    );
    server.expect(
        "DELETE",
        "/myorg/Beta/_apis/distributedtask/variablegroups/5",
        MockResponse::json(200, json!({"id": 5})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "vars", "delete", "Beta", "5", "--json"],
        &[],
    );

    assert_success(&output);
    let received = server.received();
    assert_eq!(received.len(), 2);
    assert_eq!(
        received[1].query_pairs(),
        vec![api_version()],
        "captured: a miss leaves the param off"
    );
}

/// Captured: a failed lookup is swallowed and the delete proceeds without
/// `projectIds` (the module's `_ -> nil` branch).
#[test]
fn vars_delete_proceeds_when_the_project_lookup_fails() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        PROJECTS_LOOKUP_PATH,
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );
    server.expect(
        "DELETE",
        "/myorg/Beta/_apis/distributedtask/variablegroups/5",
        MockResponse::json(200, json!({"id": 5})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "vars", "delete", "Beta", "5", "--json"],
        &[],
    );

    assert_success(&output);
    assert_eq!(server.received()[1].query_pairs(), vec![api_version()]);
}

#[test]
fn vars_delete_404_reports_the_group_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        PROJECTS_LOOKUP_PATH,
        MockResponse::json(200, projects_lookup()),
    );
    server.expect(
        "DELETE",
        VARS_MISSING_PATH,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "vars", "delete", "Alpha", "999", "--json"],
        &[],
    );

    assert_not_found_envelope(&output, "Variable group #999 not found in project 'Alpha'");
    assert_eq!(server.received().len(), 2, "the delete was attempted");
}

/// Captured: `vars delete` has no `--force` either — it never prompts.
#[test]
fn vars_delete_has_no_force_option() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "vars", "delete", "Alpha", "5", "--force"],
        &[],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("--force"),
        "the usage error names the unknown flag: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

// ── Task 5: `pipelines variables` and `pipelines secure_files` ──────────
//
// Every path, query, body and human layout below is the frozen escript's,
// captured against the standalone mock before the port
// (`.superpowers/sdd/2026-09-27-wave-2-mutations/captures/task5/`): the two
// deletes were re-run with `n` on stdin against a mock that answers the GETs, and
// neither prompts (R5). `variables delete` proceeds; `secure_files delete`
// without `--force` is the ruled refusal — exit 1 here where the oracle exits 0,
// the same message on stderr, and nothing sent (D32).

use std::fs;

const VARIABLES_MISSING_PATH: &str = "/myorg/Alpha/_apis/pipelines/999";
const NO_VARIABLES_PIPELINE_PATH: &str = "/myorg/Alpha/_apis/pipelines/7";
const SECURE_FILES_PATH: &str = "/myorg/Alpha/_apis/distributedtask/securefiles";
const SECURE_FILE_ID: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
const SECURE_FILE_MISSING_ID: &str = "00000000-0000-4000-8000-000000000999";

/// The frozen escript's literal `ado pipelines variables list Alpha 12 --json`
/// line for the captured pipeline: one record per variable, key-sorted, with a
/// missing value or `isSecret` as `null`.
const ORACLE_VARIABLES_JSON: &str = r#"{"ok":true,"result":[{"isSecret":null,"key":"DB_HOST","value":"db.example.com"},{"isSecret":true,"key":"DB_PASS","value":null},{"isSecret":null,"key":"DEBUG","value":"false"}]}"#;

/// The captured `GET .../pipelines/12` answer.
fn pipeline_with_variables() -> Value {
    json!({
        "id": 12,
        "name": "Alpha CI",
        "folder": "\\",
        "revision": 4,
        "configuration": {
            "type": "yaml",
            "path": "pipelines/ci.yml",
            "variables": {
                "DB_HOST": {"value": "db.example.com"},
                "DB_PASS": {"isSecret": true},
                "DEBUG": {"value": "false"},
            },
        },
        "url": "https://dev.azure.com/ado-harness/Alpha/_apis/pipelines/12?revision=4",
    })
}

/// The captured `GET .../pipelines/7` answer: a `configuration` with no
/// `variables` map.
fn pipeline_without_variables() -> Value {
    json!({
        "id": 7,
        "name": "Alpha Nightly",
        "folder": "\\",
        "revision": 1,
        "configuration": {"type": "yaml", "path": "azure-pipelines.yml"},
        "url": "https://dev.azure.com/ado-harness/Alpha/_apis/pipelines/7?revision=1",
    })
}

/// The captured `GET .../securefiles` answer's two files.
fn secure_files_list() -> Value {
    json!({
        "count": 2,
        "value": [
            secure_file(),
            {
                "id": "9f1b7e0e-0001-4000-8000-000000000002",
                "name": "kubeconfig",
                "contentLength": 512,
                "createdOn": "2026-09-10T08:30:00.000Z",
                "modifiedOn": "2026-09-10T08:30:00.000Z",
                "createdBy": {"displayName": "Ada Lovelace"},
                "modifiedBy": {"displayName": "Ada Lovelace"},
            },
        ],
    })
}

/// The captured `GET .../securefiles/<guid>` and list entry.
fn secure_file() -> Value {
    json!({
        "id": SECURE_FILE_ID,
        "name": "prod-cert.pem",
        "contentLength": 2048,
        "createdOn": "2026-09-01T09:00:00.000Z",
        "modifiedOn": "2026-09-20T12:00:00.000Z",
        "createdBy": {"displayName": "Ada Lovelace", "id": "aaaaaaaa-0000-4000-8000-000000000001"},
        "modifiedBy": {"displayName": "Grace Hopper", "id": "aaaaaaaa-0000-4000-8000-000000000002"},
    })
}

/// The captured `POST .../securefiles` answer.
fn uploaded_secure_file() -> Value {
    json!({
        "id": "b7e2c9a0-1111-4222-8333-444455556666",
        "name": "cert.pem",
        "contentLength": 83,
        "createdOn": "2026-09-27T10:00:00.000Z",
        "modifiedOn": "2026-09-27T10:00:00.000Z",
        "createdBy": {"displayName": "Ada Lovelace"},
        "modifiedBy": {"displayName": "Ada Lovelace"},
    })
}

/// The UTF-8 upload fixture the harness scenario pins, written where the test can
/// name it: one file, so the harness's `request_body` pin and the test's byte
/// assertions cannot drift apart.
fn write_upload_fixture(home: &TempHome) -> (std::path::PathBuf, String) {
    let bytes = fixture_bytes("secure_file_upload.pem");
    let body = String::from_utf8(bytes).expect("the upload fixture is UTF-8");
    let path = home.path().join("cert.pem");

    fs::write(&path, &body).expect("write the upload fixture");

    (path, body)
}

fn expect_pipeline_show(server: &MockServer, path: &str, pipeline: Value) {
    server.expect("GET", path, MockResponse::json(200, pipeline));
}

#[test]
fn variables_list_sends_the_pipeline_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_pipeline_show(&server, SHOW_PATH, pipeline_with_variables());

    let output = run(
        &home,
        &server,
        &["pipelines", "variables", "list", "Alpha", "12", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{ORACLE_VARIABLES_JSON}\n"),
        "the oracle's exact bytes"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, SHOW_PATH);
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

#[test]
fn variables_list_human_output_is_the_key_value_secret_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_pipeline_show(&server, SHOW_PATH, pipeline_with_variables());

    let output = run(
        &home,
        &server,
        &["pipelines", "variables", "list", "Alpha", "12"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(stdout.contains("Key"), "stdout: {stdout}");
    assert!(stdout.contains("Value"), "stdout: {stdout}");
    assert!(stdout.contains("Secret"), "stdout: {stdout}");
    assert!(stdout.contains("DB_HOST"), "stdout: {stdout}");
    assert!(stdout.contains("db.example.com"), "stdout: {stdout}");
    assert!(stdout.contains("DB_PASS"), "stdout: {stdout}");
    assert!(stdout.contains("DEBUG"), "stdout: {stdout}");
    assert!(stdout.contains("yes"), "a secret reads yes: {stdout}");
    assert_no_table_bytes(&stdout);
}

#[test]
fn variables_list_without_a_variables_map_is_empty() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_pipeline_show(
        &server,
        NO_VARIABLES_PIPELINE_PATH,
        pipeline_without_variables(),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "variables", "list", "Alpha", "7", "--json"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");
}

#[test]
fn variables_list_without_a_variables_map_is_the_module_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_pipeline_show(
        &server,
        NO_VARIABLES_PIPELINE_PATH,
        pipeline_without_variables(),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "variables", "list", "Alpha", "7"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "No pipeline variables defined.\n",
        "the module's own empty-list message"
    );
}

#[test]
fn variables_list_404_reports_the_pipeline_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        VARIABLES_MISSING_PATH,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "variables", "list", "Alpha", "999", "--json"],
    );

    assert_not_found_envelope(&output, "Pipeline #999 not found");
}

#[test]
fn variables_create_patches_the_existing_pipeline_with_the_new_variable() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_pipeline_show(&server, SHOW_PATH, pipeline_with_variables());
    server.expect(
        "PATCH",
        SHOW_PATH,
        MockResponse::json(200, json!({"id": 12, "revision": 5})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "variables",
            "create",
            "Alpha",
            "12",
            "--key",
            "ENV",
            "--value",
            "staging",
            "--json",
        ],
        &[],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "message": "Variable 'ENV' added."}),
        "the write's message envelope (D33)"
    );

    let received = server.received();
    assert_eq!(received.len(), 2, "the GET then the PATCH");
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[1].method, "PATCH");
    assert_eq!(received[1].path, SHOW_PATH);
    assert_eq!(
        sent_body(&received[1]),
        json!({
            "id": 12,
            "name": "Alpha CI",
            "folder": "\\",
            "revision": 4,
            "configuration": {
                "type": "yaml",
                "path": "pipelines/ci.yml",
                "variables": {
                    "DB_HOST": {"value": "db.example.com"},
                    "DB_PASS": {"isSecret": true},
                    "DEBUG": {"value": "false"},
                    "ENV": {"value": "staging", "isSecret": false},
                },
            },
            "url": "https://dev.azure.com/ado-harness/Alpha/_apis/pipelines/12?revision=4",
        }),
        "captured: the whole pipeline, with the new variable merged into configuration.variables"
    );
}

#[test]
fn variables_create_secret_marks_only_the_new_variable() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_pipeline_show(&server, SHOW_PATH, pipeline_with_variables());
    server.expect(
        "PATCH",
        SHOW_PATH,
        MockResponse::json(200, json!({"id": 12})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "variables",
            "create",
            "Alpha",
            "12",
            "--key",
            "API_KEY",
            "--value",
            "s3cret",
            "--secret",
            "--json",
        ],
        &[],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[1])["configuration"]["variables"]["API_KEY"],
        json!({"value": "s3cret", "isSecret": true}),
        "the boolean --secret marks this variable"
    );
}

#[test]
fn variables_create_without_required_options_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    for (args, expected) in [
        (
            vec!["pipelines", "variables", "create", "Alpha", "12", "--json"],
            "--key",
        ),
        (
            vec![
                "pipelines",
                "variables",
                "create",
                "Alpha",
                "12",
                "--key",
                "ENV",
                "--json",
            ],
            "--value",
        ),
    ] {
        let output = run_with_stdin(&home, &server, &args, &[]);

        assert_eq!(output.status.code(), Some(1), "args: {args:?}");
        assert!(
            stdout_of(&output).is_empty(),
            "no envelope for a usage error (R4/D34): {args:?}"
        );
        assert!(
            stderr_of(&output).contains(expected),
            "the usage error names the missing option {expected}: {}",
            stderr_of(&output)
        );
    }

    assert!(
        server.received().is_empty(),
        "a usage error sends no request (R4/D34)"
    );
}

#[test]
fn variables_delete_patches_the_existing_pipeline_without_the_key() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_pipeline_show(&server, SHOW_PATH, pipeline_with_variables());
    server.expect(
        "PATCH",
        SHOW_PATH,
        MockResponse::json(200, json!({"id": 12})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "variables",
            "delete",
            "Alpha",
            "12",
            "--key",
            "DEBUG",
            "--json",
        ],
        b"n\n",
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "message": "Variable 'DEBUG' removed."}),
        "the delete's message envelope (D33)"
    );

    let received = server.received();
    assert_eq!(
        received.len(),
        2,
        "stdin 'n' still sends both requests: this command does not prompt (R5)"
    );
    assert_eq!(
        sent_body(&received[1])["configuration"]["variables"],
        json!({
            "DB_HOST": {"value": "db.example.com"},
            "DB_PASS": {"isSecret": true},
        }),
        "captured: the existing map minus the named key"
    );
}

#[test]
fn variables_delete_without_a_key_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["pipelines", "variables", "delete", "Alpha", "12", "--json"],
        &[],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "captured: the oracle exits 0 silently (R4/D34)"
    );
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error"
    );
    assert!(
        stderr_of(&output).contains("--key"),
        "the usage error names the missing option: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn variables_delete_has_no_force_option() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "variables",
            "delete",
            "Alpha",
            "12",
            "--key",
            "DEBUG",
            "--force",
        ],
        &[],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "captured: no --force on this command"
    );
    assert!(
        stderr_of(&output).contains("--force"),
        "the usage error names the unknown flag: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn variables_delete_404_sends_no_patch() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        VARIABLES_MISSING_PATH,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "variables",
            "delete",
            "Alpha",
            "999",
            "--key",
            "DEBUG",
            "--json",
        ],
        &[],
    );

    assert_not_found_envelope(&output, "Pipeline #999 not found");
    assert_eq!(server.received().len(), 1, "the 404 stops before the PATCH");
}

#[test]
fn secure_files_list_sends_the_top_param_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        SECURE_FILES_PATH,
        MockResponse::json(200, secure_files_list()),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "list",
            "Alpha",
            "--top",
            "10",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "result": secure_files_list()["value"].clone()})
        ),
        "the value envelope carries the API's file objects"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].path, SECURE_FILES_PATH);
    assert_eq!(
        received[0].query_pairs(),
        vec![api_version(), pair("%24top", "10")],
        "the module's $top, present means sent"
    );
}

#[test]
fn secure_files_list_without_top_sends_no_top_param() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        SECURE_FILES_PATH,
        MockResponse::json(200, secure_files_list()),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "secure_files", "list", "Alpha", "--json"],
    );

    assert_success(&output);
    assert_eq!(server.received()[0].query_pairs(), vec![api_version()]);
}

#[test]
fn secure_files_list_human_output_is_the_id_name_size_modified_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        SECURE_FILES_PATH,
        MockResponse::json(200, secure_files_list()),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "secure_files", "list", "Alpha"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    for header in ["ID", "Name", "Size", "Modified"] {
        assert!(stdout.contains(header), "the {header} column: {stdout}");
    }
    assert!(stdout.contains(SECURE_FILE_ID), "stdout: {stdout}");
    assert!(stdout.contains("prod-cert.pem"), "stdout: {stdout}");
    assert!(stdout.contains("2048"), "stdout: {stdout}");
    assert!(stdout.contains("kubeconfig"), "stdout: {stdout}");
    assert_no_table_bytes(&stdout);
}

#[test]
fn secure_files_list_empty_is_the_module_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        SECURE_FILES_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "secure_files", "list", "Alpha", "--json"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");
}

#[test]
fn secure_files_show_sends_the_guid_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{SECURE_FILES_PATH}/{SECURE_FILE_ID}"),
        MockResponse::json(200, secure_file()),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "show",
            "Alpha",
            SECURE_FILE_ID,
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", json!({"ok": true, "result": secure_file()})),
        "the value envelope carries the file object"
    );

    let received = server.received();
    assert_eq!(
        received[0].path,
        format!("{SECURE_FILES_PATH}/{SECURE_FILE_ID}")
    );
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

#[test]
fn secure_files_show_human_output_is_the_detail_block() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{SECURE_FILES_PATH}/{SECURE_FILE_ID}"),
        MockResponse::json(200, secure_file()),
    );

    let output = run(
        &home,
        &server,
        &["pipelines", "secure_files", "show", "Alpha", SECURE_FILE_ID],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(stdout.contains("Secure File Details"), "stdout: {stdout}");
    assert!(
        stdout.contains(&format!("\n{}\n", "─".repeat(60))),
        "the module's 60-character rule: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  ID:        {SECURE_FILE_ID}\n")),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Name:      prod-cert.pem\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Size:      2048 bytes\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Created:   2026-09-01T09:00:00.000Z by Ada Lovelace\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Modified:  2026-09-20T12:00:00.000Z by Grace Hopper\n"),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
}

#[test]
fn secure_files_show_404_reports_the_file_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{SECURE_FILES_PATH}/{SECURE_FILE_MISSING_ID}"),
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "show",
            "Alpha",
            SECURE_FILE_MISSING_ID,
            "--json",
        ],
    );

    assert_not_found_envelope(
        &output,
        &format!("Secure file {SECURE_FILE_MISSING_ID} not found in project 'Alpha'"),
    );
}

#[test]
fn secure_files_upload_posts_the_raw_bytes_with_the_name_query() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        SECURE_FILES_PATH,
        MockResponse::json(200, uploaded_secure_file()),
    );
    let (path, bytes) = write_upload_fixture(&home);

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "upload",
            "Alpha",
            "cert.pem",
            "--file",
            path.to_str().expect("a UTF-8 path"),
            "--json",
        ],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "message": "Secure file 'cert.pem' uploaded (ID: b7e2c9a0-1111-4222-8333-444455556666, 83 bytes)."}),
        "the upload's message envelope (D33)"
    );

    let received = server.received();
    assert_eq!(
        received.len(),
        1,
        "an upload with no --allow-exists is one request"
    );
    assert_eq!(received[0].method, "POST");
    assert_eq!(received[0].path, SECURE_FILES_PATH);
    assert_eq!(
        received[0].query_pairs(),
        vec![api_version(), pair("name", "cert.pem")],
        "captured: the name is a query parameter"
    );
    assert_eq!(
        received[0].header("content-type"),
        Some("application/octet-stream"),
        "captured: the body is not JSON"
    );
    assert_eq!(
        received[0].body.as_deref(),
        Some(bytes.as_str()),
        "the bytes are the fixture's, not a JSON encoding of them"
    );
    assert_eq!(bytes.len(), 83, "the byte count the success line reports");
}

#[test]
fn secure_files_upload_without_file_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "upload",
            "Alpha",
            "cert.pem",
            "--json",
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "captured: the oracle exits 0 silently (R4/D34)"
    );
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error"
    );
    assert!(
        stderr_of(&output).contains("--file"),
        "the usage error names the missing option: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn secure_files_upload_of_a_missing_file_is_the_file_not_found_error() {
    let home = TempHome::new();
    let server = MockServer::start();
    let path = home.path().join("absent.pem");

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "upload",
            "Alpha",
            "cert.pem",
            "--file",
            path.to_str().expect("a UTF-8 path"),
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = mutation_envelope_failure(&output);
    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!(format!("File not found: {}", path.display())),
        "the module's message names the path as given"
    );
    assert!(server.received().is_empty(), "no request without a file");
}

#[test]
fn secure_files_upload_allow_exists_deletes_the_named_file_first() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        SECURE_FILES_PATH,
        MockResponse::json(200, secure_files_list()),
    );
    server.expect(
        "DELETE",
        &format!("{SECURE_FILES_PATH}/{SECURE_FILE_ID}"),
        MockResponse::json(200, json!({"id": SECURE_FILE_ID})),
    );
    server.expect(
        "POST",
        SECURE_FILES_PATH,
        MockResponse::json(200, uploaded_secure_file()),
    );
    let (path, _) = write_upload_fixture(&home);

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "upload",
            "Alpha",
            "prod-cert.pem",
            "--file",
            path.to_str().expect("a UTF-8 path"),
            "--allow-exists",
            "--json",
        ],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "message": "Deleted existing 'prod-cert.pem' (id: f47ac10b-58cc-4372-a567-0e02b2c3d479).\nSecure file 'cert.pem' uploaded (ID: b7e2c9a0-1111-4222-8333-444455556666, 83 bytes)."}),
        "the replace path carries both captured lines (D33)"
    );

    let received = server.received();
    assert_eq!(received.len(), 3, "lookup, delete, upload");
    assert_eq!(received[0].method, "GET");
    assert_eq!(
        received[0].query_pairs(),
        vec![api_version(), pair("namePattern", "prod-cert.pem")],
        "captured: the lookup filters by namePattern"
    );
    assert_eq!(received[1].method, "DELETE");
    assert_eq!(
        received[1].path,
        format!("{SECURE_FILES_PATH}/{SECURE_FILE_ID}")
    );
    assert_eq!(received[2].method, "POST");
}

#[test]
fn secure_files_upload_allow_exists_proceeds_when_the_lookup_finds_nothing() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        SECURE_FILES_PATH,
        MockResponse::json(200, json!({"count": 1, "value": [secure_file()]})),
    );
    server.expect(
        "POST",
        SECURE_FILES_PATH,
        MockResponse::json(200, uploaded_secure_file()),
    );
    let (path, _) = write_upload_fixture(&home);

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "upload",
            "Alpha",
            "new-cert.pem",
            "--file",
            path.to_str().expect("a UTF-8 path"),
            "--allow-exists",
            "--json",
        ],
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "message": "Secure file 'cert.pem' uploaded (ID: b7e2c9a0-1111-4222-8333-444455556666, 83 bytes)."}),
        "no replace line when nothing matched"
    );

    let received = server.received();
    assert_eq!(received.len(), 2, "the lookup and the upload only");
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[1].method, "POST");
}

#[test]
fn secure_files_upload_allow_exists_proceeds_when_the_lookup_fails() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        SECURE_FILES_PATH,
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );
    server.expect(
        "POST",
        SECURE_FILES_PATH,
        MockResponse::json(200, uploaded_secure_file()),
    );
    let (path, _) = write_upload_fixture(&home);

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "upload",
            "Alpha",
            "cert.pem",
            "--file",
            path.to_str().expect("a UTF-8 path"),
            "--allow-exists",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        server.received().len(),
        2,
        "captured: the module's failed-lookup fallback uploads anyway"
    );
}

#[test]
fn secure_files_upload_conflict_reports_the_replace_hint() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        SECURE_FILES_PATH,
        MockResponse::json(
            409,
            json!({"message": "A secure file with name 'cert.pem' already exists."}),
        ),
    );
    let (path, _) = write_upload_fixture(&home);

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "upload",
            "Alpha",
            "cert.pem",
            "--file",
            path.to_str().expect("a UTF-8 path"),
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = mutation_envelope_failure(&output);
    assert_eq!(envelope["error"]["code"], json!("conflict"));
    assert_eq!(envelope["error"]["status"], json!(409));
    assert_eq!(
        envelope["error"]["message"],
        json!(
            "A secure file named 'cert.pem' already exists. Re-run with --allow-exists to replace it, or use a different name. (A secure file with name 'cert.pem' already exists.)"
        ),
        "the captured 409 wording, with the API's message in parentheses"
    );
}

#[test]
fn secure_files_upload_allow_exists_delete_failure_stops_before_the_upload() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        SECURE_FILES_PATH,
        MockResponse::json(200, secure_files_list()),
    );
    server.expect(
        "DELETE",
        &format!("{SECURE_FILES_PATH}/{SECURE_FILE_ID}"),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );
    let (path, _) = write_upload_fixture(&home);

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "upload",
            "Alpha",
            "prod-cert.pem",
            "--file",
            path.to_str().expect("a UTF-8 path"),
            "--allow-exists",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = mutation_envelope_failure(&output);
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(
        server.received().len(),
        2,
        "captured: a failed replace reports and sends no upload"
    );
}

#[test]
fn secure_files_delete_without_force_refuses_and_sends_nothing() {
    let home = TempHome::new();
    let server = MockServer::start();

    for json in [false, true] {
        let mut args = vec![
            "pipelines",
            "secure_files",
            "delete",
            "Alpha",
            SECURE_FILE_ID,
        ];
        if json {
            args.push("--json");
        }

        let output = run_with_stdin(&home, &server, &args, b"n\n");

        assert_eq!(
            output.status.code(),
            Some(1),
            "captured: the oracle prints the guard and exits 0; a refusal is never a success (D32), json={json}"
        );
        assert!(
            stdout_of(&output).is_empty(),
            "nothing on stdout, so a --json run cannot read a result (json={json})"
        );
        assert!(
            stderr_of(&output).contains("Pass --force to confirm."),
            "the oracle's own guard wording is the refusal (json={json}): {}",
            stderr_of(&output)
        );
    }

    assert!(
        server.received().is_empty(),
        "captured: the oracle sends nothing from the guard path"
    );
}

#[test]
fn secure_files_delete_force_sends_the_delete_and_emits_the_message_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{SECURE_FILES_PATH}/{SECURE_FILE_ID}"),
        MockResponse::json(200, json!({"id": SECURE_FILE_ID})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "delete",
            "Alpha",
            SECURE_FILE_ID,
            "--force",
            "--json",
        ],
        b"n\n",
    );

    assert_eq!(
        mutation_envelope(&output),
        json!({"ok": true, "message": format!("Secure file {SECURE_FILE_ID} deleted.")}),
        "the delete's message envelope (D33), and stdin 'n' does not stop it (R5)"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "DELETE");
    assert_eq!(
        received[0].path,
        format!("{SECURE_FILES_PATH}/{SECURE_FILE_ID}")
    );
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

#[test]
fn secure_files_delete_force_404_reports_the_file_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{SECURE_FILES_PATH}/{SECURE_FILE_MISSING_ID}"),
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines",
            "secure_files",
            "delete",
            "Alpha",
            SECURE_FILE_MISSING_ID,
            "--force",
            "--json",
        ],
    );

    assert_not_found_envelope(
        &output,
        &format!("Secure file {SECURE_FILE_MISSING_ID} not found in project 'Alpha'"),
    );
}
