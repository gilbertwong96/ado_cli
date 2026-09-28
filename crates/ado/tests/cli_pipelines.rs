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
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
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

    serde_json::from_str(&response.body).expect("the fixture is JSON")
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
