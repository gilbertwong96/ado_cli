//! End-to-end tests for `ado pipelines-builds list|show|tags list|definitions list`:
//! the REST surface the frozen `lib/ado_cli/cli/builds.ex` builds — every method,
//! path and query pair verified against the frozen escript — the `--json` envelopes
//! it emits, and the human table/detail the module's formatters define.
//!
//! The four envelopes are pinned **byte-equal** to the captured oracle lines: every
//! map in the fixtures stays below Elixir's small-map threshold, so the term order
//! the oracle encodes is serde's sorted order, and each capture was confirmed
//! byte-identical to its `jq -S -c` form (W1-R12).
//!
//! The hyphenated invocation (W1-1/D18) is this task's distinguishing surface:
//! `pipelines-builds` is the parseable spelling and the schema reports it, while the
//! space spelling is an unknown subcommand that never reaches the builds command.
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

/// The frozen escript's literal `ado pipelines-builds list Alpha --json` line for
/// the `builds_list` fixture (captured from the 0.5.0 binary, W1-R12).
const ORACLE_LIST_JSON: &str = r#"{"ok":true,"result":[{"_links":{"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/build/Builds/128"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/results?buildId=128"}},"buildNumber":"20260927.1","definition":{"id":5,"name":"Alpha CI","path":"\\","queueStatus":"enabled","revision":12,"type":"build","url":"https://dev.azure.com/myorg/Alpha/_apis/build/Definitions/5?revision=12"},"finishTime":"2026-09-27T09:18:44.55Z","id":128,"priority":"normal","queue":{"id":9,"name":"Azure Pipelines","pool":{"id":1,"isHosted":true,"name":"Azure Pipelines","poolType":"automation"},"url":"https://dev.azure.com/myorg/Alpha/_apis/build/Queues/9"},"queueTime":"2026-09-27T09:12:04.1533333Z","reason":"manual","requestedFor":{"displayName":"Alice Example","id":"9f1b7e0e-0001-4000-8000-000000000002","uniqueName":"alice@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/9f1b7e0e-0001-4000-8000-000000000002"},"result":"succeeded","sourceBranch":"refs/heads/main","sourceVersion":"3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a","startTime":"2026-09-27T09:12:31.02Z","status":"completed","url":"https://dev.azure.com/myorg/Alpha/_apis/build/Builds/128"},{"_links":{"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/build/Builds/127"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/results?buildId=127"}},"buildNumber":"20260926.3","definition":{"id":7,"name":"Alpha Nightly","path":"\\Nightly","queueStatus":"enabled","revision":4,"type":"build","url":"https://dev.azure.com/myorg/Alpha/_apis/build/Definitions/7?revision=4"},"finishTime":"2026-09-26T23:41:09.8Z","id":127,"priority":"normal","queue":{"id":9,"name":"Azure Pipelines","pool":{"id":1,"isHosted":true,"name":"Azure Pipelines","poolType":"automation"},"url":"https://dev.azure.com/myorg/Alpha/_apis/build/Queues/9"},"queueTime":"2026-09-26T23:35:12.7Z","reason":"schedule","requestedFor":{"displayName":"Alpha Nightly <alpha@example.test>","id":"9f1b7e0e-0001-4000-8000-000000000003","uniqueName":"alpha@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/9f1b7e0e-0001-4000-8000-000000000003"},"result":"failed","sourceBranch":"refs/heads/release/1.2","sourceVersion":"b7c6d5e4f3a29180876c5d4e3f2a1b0c9d8e7f6a","startTime":"2026-09-26T23:35:40.1Z","status":"completed","url":"https://dev.azure.com/myorg/Alpha/_apis/build/Builds/127"}]}"#;

/// The frozen escript's literal `ado pipelines-builds show Alpha 128 --json` line
/// for the `builds_show` fixture.
const ORACLE_SHOW_JSON: &str = r#"{"ok":true,"result":{"_links":{"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/build/Builds/128"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/results?buildId=128"}},"buildNumber":"20260927.1","definition":{"id":5,"name":"Alpha CI","path":"\\","queueStatus":"enabled","revision":12,"type":"build","url":"https://dev.azure.com/myorg/Alpha/_apis/build/Definitions/5?revision=12"},"finishTime":"2026-09-27T09:18:44.55Z","id":128,"priority":"normal","queue":{"id":9,"name":"Azure Pipelines","pool":{"id":1,"isHosted":true,"name":"Azure Pipelines","poolType":"automation"},"url":"https://dev.azure.com/myorg/Alpha/_apis/build/Queues/9"},"queueTime":"2026-09-27T09:12:04.1533333Z","reason":"manual","requestedFor":{"displayName":"Alice Example","id":"9f1b7e0e-0001-4000-8000-000000000002","uniqueName":"alice@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/9f1b7e0e-0001-4000-8000-000000000002"},"result":"succeeded","sourceBranch":"refs/heads/main","sourceVersion":"3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a","startTime":"2026-09-27T09:12:31.02Z","status":"completed","url":"https://dev.azure.com/myorg/Alpha/_apis/build/Builds/128"}}"#;

/// The frozen escript's literal `ado pipelines-builds tags list Alpha 128 --json`
/// line for the `builds_tags` fixture.
const ORACLE_TAGS_JSON: &str = r#"{"ok":true,"result":["release","prod","v1.2.3"]}"#;

/// The frozen escript's literal `ado pipelines-builds definitions list Alpha --json`
/// line for the `builds_definitions` fixture.
const ORACLE_DEFINITIONS_JSON: &str = r#"{"ok":true,"result":[{"_links":{"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/build/Definitions/5?revision=12"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=5"}},"createdDate":"2024-05-02T08:14:37.42Z","id":5,"name":"Alpha CI","path":"\\","project":{"id":"4f8b2a1c-0002-4000-8000-000000000001","name":"Alpha","revision":25,"state":"wellFormed","url":"https://dev.azure.com/myorg/_apis/projects/4f8b2a1c-0002-4000-8000-000000000001","visibility":"private"},"queue":{"id":9,"name":"Azure Pipelines","pool":{"id":1,"isHosted":true,"name":"Azure Pipelines","poolType":"automation"},"url":"https://dev.azure.com/myorg/Alpha/_apis/build/Queues/9"},"queueStatus":"enabled","repository":{"clean":"false","defaultBranch":"refs/heads/main","id":"9f1b7e0e-0001-4000-8000-000000000001","name":"Alpha.Core","type":"TfsGit","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/Alpha.Core"},"revision":12,"type":"build","url":"https://dev.azure.com/myorg/Alpha/_apis/build/Definitions/5?revision=12"},{"_links":{"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/build/Definitions/7?revision=4"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=7"}},"createdDate":"2025-01-13T16:02:11.9Z","id":7,"name":"Alpha Nightly","path":"\\Nightly","project":{"id":"4f8b2a1c-0002-4000-8000-000000000001","name":"Alpha","revision":25,"state":"wellFormed","url":"https://dev.azure.com/myorg/_apis/projects/4f8b2a1c-0002-4000-8000-000000000001","visibility":"private"},"queueStatus":"disabled","repository":{"clean":"false","defaultBranch":"refs/heads/main","id":"9f1b7e0e-0001-4000-8000-000000000001","name":"Alpha.Core","type":"TfsGit","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/Alpha.Core"},"revision":4,"type":"build","url":"https://dev.azure.com/myorg/Alpha/_apis/build/Definitions/7?revision=4"}]}"#;

/// The module's builds collection path; `show` appends `/<build_id>`, `tags list`
/// appends `/<build_id>/tags`.
const BUILDS_PATH: &str = "/myorg/Alpha/_apis/build/builds";
const SHOW_PATH: &str = "/myorg/Alpha/_apis/build/builds/128";
const TAGS_PATH: &str = "/myorg/Alpha/_apis/build/builds/128/tags";
const DEFINITIONS_PATH: &str = "/myorg/Alpha/_apis/build/definitions";

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
fn expect_builds(server: &MockServer, query: &[(&str, &str)]) {
    server.expect_query(
        "GET",
        BUILDS_PATH,
        query,
        MockResponse::from_fixture("builds_list"),
    );
}

fn expect_show(server: &MockServer) {
    server.expect("GET", SHOW_PATH, MockResponse::from_fixture("builds_show"));
}

fn expect_tags(server: &MockServer) {
    server.expect("GET", TAGS_PATH, MockResponse::from_fixture("builds_tags"));
}

fn expect_definitions(server: &MockServer) {
    server.expect(
        "GET",
        DEFINITIONS_PATH,
        MockResponse::from_fixture("builds_definitions"),
    );
}

fn assert_query(request: &RecordedRequest, path: &str, query: Vec<(String, String)>) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, path);
    assert_eq!(
        request.query_pairs(),
        query,
        "the GET's wire-form pairs, in the client's merge order (D12 compares parsed pairs)"
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
fn list_sends_the_builds_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_builds(&server, &[("api-version", "7.1")]);

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "list", "Alpha", "--json"],
    );

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
        json!({"ok": true, "result": fixture("builds_list")["value"].clone()}),
        "the value envelope carries the fixture's array under result"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one request per list");
    assert_query(&received[0], BUILDS_PATH, vec![api_version()]);
}

/// The module's own parameter order (`$top`, then `definitions`), with the wire
/// forms the client encodes: `$` is `%24`, `,` is `%2C`.
#[test]
fn list_filters_reach_the_wire_in_the_modules_order() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_builds(
        &server,
        &[
            ("%24top", "5"),
            ("api-version", "7.1"),
            ("definitions", "5%2C12"),
        ],
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-builds",
            "list",
            "Alpha",
            "--top",
            "5",
            "--definitions",
            "5,12",
            "--json",
        ],
    );

    assert_success(&output);
    assert_query(
        &server.received()[0],
        BUILDS_PATH,
        vec![
            api_version(),
            pair("%24top", "5"),
            pair("definitions", "5%2C12"),
        ],
    );
}

/// "Present means sent": Elixir's `Map.get` default applies only when the option is
/// absent, and `if value` is truthy for `0` and `""` — so an explicit zero top and an
/// explicit empty definitions string both reach the wire.
#[test]
fn list_present_empty_and_zero_options_reach_the_wire() {
    let home = TempHome::new();

    let top = MockServer::start();
    expect_builds(&top, &[("%24top", "0"), ("api-version", "7.1")]);
    let output = run(
        &home,
        &top,
        &["pipelines-builds", "list", "Alpha", "--top", "0", "--json"],
    );
    assert_success(&output);
    assert_query(
        &top.received()[0],
        BUILDS_PATH,
        vec![api_version(), pair("%24top", "0")],
    );

    let definitions = MockServer::start();
    expect_builds(&definitions, &[("api-version", "7.1"), ("definitions", "")]);
    let output = run(
        &home,
        &definitions,
        &[
            "pipelines-builds",
            "list",
            "Alpha",
            "--definitions",
            "",
            "--json",
        ],
    );
    assert_success(&output);
    assert_query(
        &definitions.received()[0],
        BUILDS_PATH,
        vec![api_version(), pair("definitions", "")],
    );
}

/// The Elixir `--top` is `:integer` and the oracle parses `--top -1`, sending it as
/// `$top=-1`; clap needs `allow_negative_numbers(true)` to accept the same argv.
#[test]
fn list_negative_top_reaches_the_wire() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_builds(&server, &[("%24top", "-1"), ("api-version", "7.1")]);

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "list", "Alpha", "--top", "-1", "--json"],
    );

    assert_success(&output);
    assert_query(
        &server.received()[0],
        BUILDS_PATH,
        vec![api_version(), pair("%24top", "-1")],
    );
}

#[test]
fn list_encodes_the_project_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/My%20Project/_apis/build/builds",
        &[("api-version", "7.1")],
        MockResponse::from_fixture("builds_list"),
    );

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "list", "My Project", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/build/builds",
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
        "/myorg/a%2Fb%3Fc/_apis/build/builds",
        &[("api-version", "7.1")],
        MockResponse::from_fixture("builds_list"),
    );

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "list", "a/b?c", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/a%2Fb%3Fc/_apis/build/builds",
        "the stricter encoder escapes / and ? so a name cannot change the URL (D22)"
    );
}

#[test]
fn list_empty_answer_human_output_says_no_builds_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BUILDS_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["pipelines-builds", "list", "Alpha"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No builds found.\n");
    assert_eq!(server.received().len(), 1);
}

#[test]
fn list_empty_answer_json_is_the_empty_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BUILDS_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "list", "Alpha", "--json"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");
    assert_eq!(server.received().len(), 1);
}

#[test]
fn list_json_emits_no_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_builds(&server, &[("api-version", "7.1")]);

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "list", "Alpha", "--json"],
    );

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

/// The module's `print_builds_table/1`: ID, Definition, Status, Result and Branch.
/// The renderer's own shape is regenerated surface (spec D9), so the header and the
/// cells are asserted, not the module's padding or its count line.
#[test]
fn list_human_output_is_the_builds_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_builds(&server, &[("api-version", "7.1")]);

    let output = run(&home, &server, &["pipelines-builds", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        4,
        "a header row, a rule row and one row per build: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header row: {stdout}");
    let definition_at = lines[0].find("Definition").expect("the Definition header");
    let status_at = lines[0].find("Status").expect("the Status header");
    let result_at = lines[0].find("Result").expect("the Result header");
    let branch_at = lines[0].find("Branch").expect("the Branch header");
    assert!(
        definition_at > 0
            && status_at > definition_at
            && result_at > status_at
            && branch_at > result_at,
        "the header order is ID, Definition, Status, Result, Branch (the module's formatter): {stdout}"
    );
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].starts_with("128")
            && lines[2].contains("Alpha CI")
            && lines[2].contains("completed")
            && lines[2].contains("succeeded")
            && lines[2].ends_with("refs/heads/main"),
        "the first row: {stdout}"
    );
    assert!(
        lines[3].starts_with("127")
            && lines[3].contains("Alpha Nightly")
            && lines[3].contains("failed")
            && lines[3].ends_with("refs/heads/release/1.2"),
        "the second row: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn list_without_a_project_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["pipelines-builds", "list", "--json"]);

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
fn show_sends_the_build_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_show(&server);

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "show", "Alpha", "128", "--json"],
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
        json!({"ok": true, "result": fixture("builds_show")}),
        "the value envelope carries the build"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one request per show");
    assert_query(&received[0], SHOW_PATH, vec![api_version()]);
    assert_no_table_bytes(&stdout);
}

#[test]
fn show_encodes_the_project_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/My%20Project/_apis/build/builds/128",
        MockResponse::from_fixture("builds_show"),
    );

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "show", "My Project", "128", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/build/builds/128"
    );
}

/// The module's `print_build_detail/1`, minus the colour: the labels and fallbacks
/// are the formatter's, and the rule is the module's **ASCII** 60 dashes — unlike
/// `pipelines`' box-drawing rule.
#[test]
fn show_human_output_is_the_build_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_show(&server);

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "show", "Alpha", "128"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(
        stdout.starts_with("\nBuild Details\n"),
        "the plain banner under the leading blank line: {stdout}"
    );
    assert!(
        stdout.contains(&format!("\n{}\n", "-".repeat(60))),
        "the module's ASCII 60-dash rule: {stdout}"
    );
    assert!(stdout.contains("  ID:         128\n"), "stdout: {stdout}");
    assert!(
        stdout.contains("  Definition: Alpha CI\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Status:     completed\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Result:     succeeded\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Branch:     refs/heads/main\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Requested:  Alice Example\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Queue:      2026-09-27T09:12:04.1533333Z\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(
            "  Web:        https://dev.azure.com/myorg/Alpha/_build/results?buildId=128\n"
        ),
        "stdout: {stdout}"
    );
    assert!(stdout.ends_with("buildId=128\n\n"), "stdout: {stdout}");
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
}

#[test]
fn show_404_reports_the_build_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Alpha/_apis/build/builds/999",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "show", "Alpha", "999", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Build #999 not found"),
        "the module's own 404 message"
    );
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );
    assert_eq!(server.received().len(), 1);
}

#[test]
fn show_without_the_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "show", "Alpha", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("BUILD_ID"),
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
        &["pipelines-builds", "show", "Alpha", "abc", "--json"],
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
fn tags_list_sends_the_tags_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_tags(&server);

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "tags", "list", "Alpha", "128", "--json"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_TAGS_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": fixture("builds_tags")}),
        "the value envelope carries the tag array"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one request per tags list");
    assert_query(&received[0], TAGS_PATH, vec![api_version()]);
    assert_no_table_bytes(&stdout);
}

#[test]
fn tags_list_encodes_the_project_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/My%20Project/_apis/build/builds/128/tags",
        MockResponse::from_fixture("builds_tags"),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-builds",
            "tags",
            "list",
            "My Project",
            "128",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/build/builds/128/tags"
    );
}

/// The module's `list_tags/1` human branch joins the names with a comma and a
/// space — not a table.
#[test]
fn tags_list_human_output_is_the_tag_list() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_tags(&server);

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "tags", "list", "Alpha", "128"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Tags: release, prod, v1.2.3\n");
    assert_no_table_bytes(&stdout_of(&output));
}

/// The module's own empty answer is "No tags." on the human path; under `--json` the
/// envelope is the value form, because the module's pre-`json_or_format` empty check
/// is the same JSON-contract violation D21 ruled on for `workitems` (the frozen
/// binary prints `No tags.` where the contract promises `{"ok":true,"result":[]}`).
#[test]
fn tags_list_empty_answers() {
    let home = TempHome::new();

    let human = MockServer::start();
    human.expect("GET", TAGS_PATH, MockResponse::json(200, json!([])));
    let output = run(
        &home,
        &human,
        &["pipelines-builds", "tags", "list", "Alpha", "128"],
    );
    assert_success(&output);
    assert_eq!(stdout_of(&output), "No tags.\n");
    assert_eq!(human.received().len(), 1);

    let json_server = MockServer::start();
    json_server.expect("GET", TAGS_PATH, MockResponse::json(200, json!([])));
    let output = run(
        &home,
        &json_server,
        &["pipelines-builds", "tags", "list", "Alpha", "128", "--json"],
    );
    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");
    assert_eq!(json_server.received().len(), 1);
}

#[test]
fn tags_list_without_the_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "tags", "list", "Alpha", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("BUILD_ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn definitions_list_sends_the_definitions_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_definitions(&server);

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "definitions", "list", "Alpha", "--json"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_DEFINITIONS_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": fixture("builds_definitions")["value"].clone()}),
        "the value envelope carries the fixture's array under result"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one request per definitions list");
    assert_query(&received[0], DEFINITIONS_PATH, vec![api_version()]);
    assert_no_table_bytes(&stdout);
}

#[test]
fn definitions_list_encodes_the_project_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/My%20Project/_apis/build/definitions",
        MockResponse::from_fixture("builds_definitions"),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-builds",
            "definitions",
            "list",
            "My Project",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/build/definitions"
    );
}

/// The module's `print_definitions_table/1`: ID, Name and Queue, where the Queue
/// cell falls back to `queueStatus` when there is no queue name.
#[test]
fn definitions_list_human_output_is_the_definitions_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_definitions(&server);

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "definitions", "list", "Alpha"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        4,
        "a header row, a rule row and one row per definition: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header row: {stdout}");
    let name_at = lines[0].find("Name").expect("the Name header");
    let queue_at = lines[0].find("Queue").expect("the Queue header");
    assert!(
        name_at > 0 && queue_at > name_at,
        "the header order is ID, Name, Queue: {stdout}"
    );
    assert!(
        lines[2].starts_with("5")
            && lines[2].contains("Alpha CI")
            && lines[2].ends_with("Azure Pipelines"),
        "the first row: {stdout}"
    );
    assert!(
        lines[3].starts_with("7")
            && lines[3].contains("Alpha Nightly")
            && lines[3].ends_with("disabled"),
        "the second row uses queueStatus as the Queue cell: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn definitions_list_empty_answers() {
    let home = TempHome::new();

    let human = MockServer::start();
    human.expect(
        "GET",
        DEFINITIONS_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );
    let output = run(
        &home,
        &human,
        &["pipelines-builds", "definitions", "list", "Alpha"],
    );
    assert_success(&output);
    assert_eq!(stdout_of(&output), "No classic build definitions found.\n");
    assert_eq!(human.received().len(), 1);

    let json_server = MockServer::start();
    json_server.expect(
        "GET",
        DEFINITIONS_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );
    let output = run(
        &home,
        &json_server,
        &["pipelines-builds", "definitions", "list", "Alpha", "--json"],
    );
    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");
    assert_eq!(json_server.received().len(), 1);
}

#[test]
fn definitions_list_without_a_project_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["pipelines-builds", "definitions", "list", "--json"],
    );

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
fn pipelines_builds_without_a_subcommand_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["pipelines-builds"]);

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

#[test]
fn tags_without_a_subcommand_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["pipelines-builds", "tags"]);

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
    assert!(server.received().is_empty(), "no request");
}

/// W1-1/D18: the parseable spelling is the hyphenated one — it reaches the wire —
/// and the Elixir schema's space spelling does not run this command at all. The
/// oracle prints the `ado pipelines` group help on stdout plus `unknown sub-command
/// builds` on stderr, exit 1; the Rust binary answers with clap's usage error on
/// stderr, exit 1 (spec §8, usage-error presentation: D5). Both shapes mean the same
/// contract: the space spelling is not the builds command.
#[test]
fn the_hyphenated_spelling_runs_and_the_space_spelling_does_not() {
    let home = TempHome::new();

    let hyphenated = MockServer::start();
    expect_builds(&hyphenated, &[("api-version", "7.1")]);
    let output = run(
        &home,
        &hyphenated,
        &["pipelines-builds", "list", "Alpha", "--json"],
    );
    assert_success(&output);
    assert_eq!(stdout_of(&output), format!("{ORACLE_LIST_JSON}\n"));
    assert_eq!(
        hyphenated.received().len(),
        1,
        "the hyphenated spelling is the command that reaches the wire"
    );

    let spaced = MockServer::start();
    let output = run(
        &home,
        &spaced,
        &["pipelines", "builds", "list", "Alpha", "--json"],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "the space spelling is not the builds command"
    );
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for the unknown subcommand: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("'builds'"),
        "the usage error names the subcommand the space spelling tried to run: {}",
        stderr_of(&output)
    );
    assert!(
        spaced.received().is_empty(),
        "the space spelling sends no request"
    );
}

/// A closed stdout is a silent success: the table write returns EPIPE and the
/// binary exits 0 with no diagnostic (spec §6.3, R19).
#[test]
fn a_closed_stdout_is_a_silent_success() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_builds(&server, &[("api-version", "7.1")]);

    let mut child = command(&home, &server, &["pipelines-builds", "list", "Alpha"])
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
