//! End-to-end tests for `ado repos list|show|branches`: the REST surface the
//! frozen `lib/ado_cli/cli/repos.ex` builds — every path and query pair verified
//! against the frozen escript — the `--json` envelopes it emits (pinned as
//! captured literals, W1-R12), and the human tables/detail the module's
//! formatters define.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a mock
//! server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT` credentials so no
//! credential resolution reaches the developer's keychain.

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";

/// The frozen escript's literal `ado repos list Alpha --json` line for the
/// `repos_list` fixture: the value envelope W1-R12 pins.
const ORACLE_REPOS_LIST_JSON: &str = r#"{"ok":true,"result":[{"defaultBranch":"refs/heads/main","id":"a1b2c3d4-5e6f-4a1b-8c9d-0e1f2a3b4c5d","isFork":false,"name":"Alpha.Core","project":{"id":"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c","name":"Alpha","state":"wellFormed","visibility":"private"},"remoteUrl":"https://myorg@dev.azure.com/myorg/Alpha/_git/Alpha.Core","size":204800,"sshUrl":"git@ssh.dev.azure.com:v3/myorg/Alpha/Alpha.Core","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/Alpha.Core","webUrl":"https://dev.azure.com/myorg/Alpha/_git/Alpha.Core"},{"defaultBranch":null,"id":"b2c3d4e5-6f70-4b1c-9d0e-1f2a3b4c5d6e","isFork":false,"name":"Alpha.Docs","project":{"id":"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c","name":"Alpha","state":"wellFormed","visibility":"private"},"remoteUrl":"https://myorg@dev.azure.com/myorg/Alpha/_git/Alpha.Docs","size":4096,"sshUrl":"git@ssh.dev.azure.com:v3/myorg/Alpha/Alpha.Docs","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/Alpha.Docs","webUrl":"https://dev.azure.com/myorg/Alpha/_git/Alpha.Docs"}]}"#;

/// The frozen escript's literal `ado repos show Alpha Alpha.Core --json` line.
const ORACLE_REPO_SHOW_JSON: &str = r#"{"ok":true,"result":{"defaultBranch":"refs/heads/main","id":"a1b2c3d4-5e6f-4a1b-8c9d-0e1f2a3b4c5d","isFork":false,"name":"Alpha.Core","project":{"id":"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c","name":"Alpha","state":"wellFormed","visibility":"private"},"remoteUrl":"https://myorg@dev.azure.com/myorg/Alpha/_git/Alpha.Core","size":204800,"sshUrl":"git@ssh.dev.azure.com:v3/myorg/Alpha/Alpha.Core","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/Alpha.Core","webUrl":"https://dev.azure.com/myorg/Alpha/_git/Alpha.Core"}}"#;

/// The frozen escript's literal `ado repos branches Alpha Alpha.Core --json`
/// line: the module's own `refs/heads/` filter drops the fixture's tag.
const ORACLE_BRANCHES_JSON: &str = r#"{"ok":true,"result":[{"creator":{"displayName":"Alice Example","id":"c1d2e3f4-0001-0001-0001-000000000001"},"name":"refs/heads/main","objectId":"1111111111111111111111111111111111111111","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/Alpha.Core/refs?filter=heads%2Fmain"},{"creator":{"displayName":"Bob Example","id":"c1d2e3f4-0002-0002-0002-000000000002"},"name":"refs/heads/feature/payments","objectId":"2222222222222222222222222222222222222222","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/Alpha.Core/refs?filter=heads%2Ffeature%2Fpayments"},{"creator":{"displayName":"Alice Example","id":"c1d2e3f4-0001-0001-0001-000000000001"},"name":"refs/heads/users/alice/experiment","objectId":"4444444444444444444444444444444444444444","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/Alpha.Core/refs?filter=heads%2Fusers%2Falice%2Fexperiment"}]}"#;

const REPOSITORIES: &str = "/myorg/Alpha/_apis/git/repositories";
const REPOSITORY: &str = "/myorg/Alpha/_apis/git/repositories/Alpha.Core";
const REFS: &str = "/myorg/Alpha/_apis/git/repositories/Alpha.Core/refs";

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

/// The one request the test's invocation sent, as raw wire-form query pairs.
fn received_query(server: &MockServer) -> Vec<(String, String)> {
    let received = server.received();

    assert_eq!(received.len(), 1, "expected exactly one request");
    received[0].query_pairs()
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
fn list_sends_the_project_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        REPOSITORIES,
        &[("api-version", "7.1")],
        MockResponse::from_fixture("repos_list"),
    );

    let output = run(&home, &server, &["repos", "list", "Alpha", "--json"]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_REPOS_LIST_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": fixture("repos_list")["value"].clone()}),
        "the value envelope carries the fixture's array under result"
    );
    assert_eq!(received_query(&server), vec![api_version()]);
    assert_eq!(server.received()[0].path, REPOSITORIES);
}

#[test]
fn list_with_include_links_adds_the_module_param() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        REPOSITORIES,
        &[("api-version", "7.1"), ("includeLinks", "true")],
        MockResponse::from_fixture("repos_list"),
    );

    let output = run(
        &home,
        &server,
        &["repos", "list", "Alpha", "--include-links", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        received_query(&server),
        vec![api_version(), pair("includeLinks", "true")],
        "the module's include_links reaches the wire as includeLinks=true"
    );
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(
        envelope["result"],
        fixture("repos_list")["value"].clone(),
        "the links do not change the payload"
    );
}

/// The help text promises `--include-links` adds three columns; the module's
/// `print_repos_table/1` ignores the links entirely, and the module wins.
#[test]
fn list_include_links_adds_a_param_but_no_table_column() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        REPOSITORIES,
        &[("api-version", "7.1"), ("includeLinks", "true")],
        MockResponse::from_fixture("repos_list"),
    );

    let output = run(
        &home,
        &server,
        &["repos", "list", "Alpha", "--include-links"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        received_query(&server),
        vec![api_version(), pair("includeLinks", "true")],
        "the module's include_links reaches the wire as includeLinks=true"
    );
    assert_eq!(
        stdout.lines().count(),
        4,
        "the module's three columns, not six: {stdout}"
    );
    assert!(
        !stdout.contains("ssh.dev.azure.com") && !stdout.contains("https://dev.azure.com"),
        "a URL column reached the table: {stdout}"
    );
}

#[test]
fn list_encodes_the_project_name_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/My%20Project/_apis/git/repositories",
        &[("api-version", "7.1")],
        MockResponse::from_fixture("repos_list"),
    );

    let output = run(&home, &server, &["repos", "list", "My Project", "--json"]);

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/git/repositories",
        "URI.encode/1 encodes a space as %20, not +"
    );
}

#[test]
fn list_json_emits_no_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        REPOSITORIES,
        MockResponse::from_fixture("repos_list"),
    );

    let output = run(&home, &server, &["repos", "list", "Alpha", "--json"]);

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

#[test]
fn list_human_output_is_the_repository_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        REPOSITORIES,
        MockResponse::from_fixture("repos_list"),
    );

    let output = run(&home, &server, &["repos", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();
    let repos = fixture("repos_list");
    let first = &repos["value"][0];
    let second = &repos["value"][1];

    assert_eq!(
        lines.len(),
        4,
        "a header row, a rule row and one row per repository: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header row: {stdout}");
    let name_at = lines[0].find("Name").expect("the Name header");
    let branch_at = lines[0].find("Default Branch").expect("the branch header");
    assert!(
        name_at > 0 && branch_at > name_at,
        "the header order is ID, Name, Default Branch: {stdout}"
    );
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].contains(first["id"].as_str().expect("an id"))
            && lines[2].contains(first["name"].as_str().expect("a name"))
            && lines[2].ends_with("main"),
        "the first data row strips refs/heads/ from the branch: {stdout}"
    );
    assert!(
        lines[3].contains(second["name"].as_str().expect("a name")) && lines[3].ends_with("(none)"),
        "a null default branch reads (none): {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn list_human_output_says_no_repositories_found_when_empty() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        REPOSITORIES,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["repos", "list", "Alpha"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No repositories found.\n");
}

#[test]
fn list_404_is_the_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        REPOSITORIES,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(&home, &server, &["repos", "list", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn list_without_a_project_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["repos", "list", "--json"]);

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
fn show_sends_the_repository_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        REPOSITORY,
        &[("api-version", "7.1")],
        MockResponse::from_fixture("repo_show"),
    );

    let output = run(
        &home,
        &server,
        &["repos", "show", "Alpha", "Alpha.Core", "--json"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_REPO_SHOW_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": fixture("repo_show")}),
        "the value envelope carries the repository object"
    );
    assert_eq!(received_query(&server), vec![api_version()]);
    assert_eq!(server.received()[0].path, REPOSITORY);
}

#[test]
fn show_encodes_both_path_segments() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/My%20Project/_apis/git/repositories/Core%20Repo",
        &[("api-version", "7.1")],
        MockResponse::from_fixture("repo_show"),
    );

    let output = run(
        &home,
        &server,
        &["repos", "show", "My Project", "Core Repo", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/git/repositories/Core%20Repo",
        "both path segments are encoded like URI.encode/1"
    );
}

#[test]
fn show_human_output_is_the_repository_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", REPOSITORY, MockResponse::from_fixture("repo_show"));

    let output = run(&home, &server, &["repos", "show", "Alpha", "Alpha.Core"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let repo = fixture("repo_show");
    let string = |key: &str| repo[key].as_str().expect("a string field").to_owned();

    assert!(stdout.contains("Repository Details\n"), "stdout: {stdout}");
    assert!(
        stdout.contains(&format!("  ID:             {}", string("id"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  Name:           {}", string("name"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  Default Branch: {}", string("defaultBranch"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  Size:           {} bytes", repo["size"])),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  SSH URL:        {}", string("sshUrl"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  Web URL:        {}", string("webUrl"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "  Project:        {} ({})",
            repo["project"]["name"].as_str().expect("a project name"),
            repo["project"]["id"].as_str().expect("a project id")
        )),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
}

#[test]
fn show_without_the_repository_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["repos", "show", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("REPO_ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn show_404_reports_the_repository_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Alpha/_apis/git/repositories/Missing",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["repos", "show", "Alpha", "Missing", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Repository 'Missing' not found in project 'Alpha'")
    );
}

#[test]
fn branches_send_the_default_filter_and_emit_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        REFS,
        &[("api-version", "7.1"), ("filter", "heads%2F")],
        MockResponse::from_fixture("branches"),
    );

    let output = run(
        &home,
        &server,
        &["repos", "branches", "Alpha", "Alpha.Core", "--json"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_BRANCHES_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    let refs = fixture("branches");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": [
            refs["value"][0].clone(),
            refs["value"][1].clone(),
            refs["value"][3].clone(),
        ]}),
        "the module keeps refs/heads/* and drops the tag"
    );
    assert_eq!(
        received_query(&server),
        vec![api_version(), pair("filter", "heads%2F")],
        "the module's default filter is heads/, wire-encoded"
    );
    assert_eq!(server.received()[0].path, REFS);
}

#[test]
fn branches_with_filter_send_the_pattern() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        REFS,
        &[("api-version", "7.1"), ("filter", "feature")],
        MockResponse::from_fixture("branches"),
    );

    let output = run(
        &home,
        &server,
        &[
            "repos",
            "branches",
            "Alpha",
            "Alpha.Core",
            "--filter",
            "feature",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        received_query(&server),
        vec![api_version(), pair("filter", "feature")],
        "--filter replaces the default"
    );
}

/// The module reads `Map.get(options, :filter, "heads/")`, so an explicitly empty
/// `--filter` reaches the wire as `filter=` — the default applies only when the
/// option is absent.
#[test]
fn branches_with_an_empty_filter_send_an_empty_value() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        REFS,
        &[("api-version", "7.1"), ("filter", "")],
        MockResponse::from_fixture("branches"),
    );

    let output = run(
        &home,
        &server,
        &[
            "repos",
            "branches",
            "Alpha",
            "Alpha.Core",
            "--filter",
            "",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        received_query(&server),
        vec![api_version(), pair("filter", "")],
        "an explicit empty --filter is sent as filter="
    );
}

#[test]
fn branches_human_output_is_the_branch_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", REFS, MockResponse::from_fixture("branches"));

    let output = run(
        &home,
        &server,
        &["repos", "branches", "Alpha", "Alpha.Core"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();
    let refs = fixture("branches");

    assert_eq!(
        lines.len(),
        5,
        "a header row, a rule row and one row per head ref: {stdout}"
    );
    assert!(lines[0].starts_with("Name"), "header row: {stdout}");
    let object_at = lines[0].find("Object ID").expect("the Object ID header");
    assert!(
        lines[0].find("Name") == Some(0) && object_at > 0,
        "the header order is Name, Object ID: {stdout}"
    );
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].starts_with("main")
            && lines[2].contains(refs["value"][0]["objectId"].as_str().expect("an id")),
        "the first branch row: {stdout}"
    );
    assert!(
        lines[3].starts_with("feature/payments")
            && lines[3].contains(refs["value"][1]["objectId"].as_str().expect("an id")),
        "the second branch row: {stdout}"
    );
    assert!(
        lines[4].starts_with("users/alice/experiment")
            && lines[4].contains(refs["value"][3]["objectId"].as_str().expect("an id")),
        "the third branch row keeps its nested name: {stdout}"
    );
    assert!(
        !stdout.contains("refs/heads/") && !stdout.contains("v1.0.0"),
        "the refs prefix is stripped and the tag filtered: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn branches_human_output_says_no_branches_found_when_empty() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        REFS,
        MockResponse::json(
            200,
            json!({
                "count": 1,
                "value": [{"name": "refs/tags/v1.0.0", "objectId": "3333333333333333333333333333333333333333"}],
            }),
        ),
    );

    let output = run(
        &home,
        &server,
        &["repos", "branches", "Alpha", "Alpha.Core"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "No branches found.\n",
        "a payload of non-head refs filters down to nothing"
    );
}

#[test]
fn branches_without_the_repository_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["repos", "branches", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("REPO_ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn repos_without_a_subcommand_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["repos"]);

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
}

/// A closed stdout is a silent success: the table write returns EPIPE and the
/// binary exits 0 with no diagnostic (spec §6.3, R19).
#[test]
fn a_closed_stdout_is_a_silent_success() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        REPOSITORIES,
        MockResponse::from_fixture("repos_list"),
    );

    let mut child = command(&home, &server, &["repos", "list", "Alpha"])
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

// ── write paths ──────────────────────────────────────────────────────────
//
// The mutations' REST surface — method, path, query and body, including the
// project-name lookup `repos create` performs first — is captured from the frozen
// escript against the mock (spec §4.2); their `--json` success output is this
// build's value/message envelope where the frozen CLI prints its human success
// line (D33). Every run scripts stdin explicitly, so a test can never read the
// developer's terminal (spec §4.1).

/// Runs the binary with `stdin` written to a pipe (never a terminal); an empty
/// slice is EOF, which is what the prompt's unanswered case means (D30).
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

/// The captured `POST …/repositories` answer the create tests serve.
fn created_repository() -> Value {
    json!({
        "id": "r1",
        "name": "NewRepo",
        "defaultBranch": "refs/heads/trunk",
        "sshUrl": "git@example.test:NewRepo",
        "webUrl": "https://example.test/NewRepo",
    })
}

fn assert_mutation_envelope(output: &Output, expected: &Value) {
    assert_success(output);
    let envelope: Value =
        serde_json::from_str(&stdout_of(output)).expect("stdout is exactly one JSON document");
    assert_eq!(&envelope, expected);
}

#[test]
fn create_resolves_the_project_id_from_the_list_and_sends_the_captured_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects",
        MockResponse::from_fixture("projects_list"),
    );
    server.expect(
        "POST",
        "/myorg/Alpha/_apis/git/repositories",
        MockResponse::json(200, created_repository()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "repos",
            "create",
            "Alpha",
            "NewRepo",
            "--default-branch",
            "trunk",
            "--json",
        ],
        &[],
    );

    assert_mutation_envelope(
        &output,
        &json!({"ok": true, "result": created_repository()}),
    );

    let received = server.received();
    assert_eq!(
        received.len(),
        2,
        "the module resolves the project name before it creates"
    );
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, "/myorg/_apis/projects");
    assert_eq!(received[1].method, "POST");
    assert_eq!(received[1].path, "/myorg/Alpha/_apis/git/repositories");
    assert_eq!(received[1].query_pairs(), vec![pair("api-version", "7.1")]);
    assert_eq!(
        sent_body(&received[1]),
        json!({
            "name": "NewRepo",
            "project": {"id": "6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c"},
            "defaultBranch": "refs/heads/trunk",
        }),
        "the captured body: the project name is resolved to the list's id, and the branch is a full ref"
    );
}

#[test]
fn create_without_the_default_branch_omits_it() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects",
        MockResponse::from_fixture("projects_list"),
    );
    server.expect(
        "POST",
        "/myorg/Alpha/_apis/git/repositories",
        MockResponse::json(200, created_repository()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["repos", "create", "Alpha", "Another", "--json"],
        &[],
    );

    assert_mutation_envelope(
        &output,
        &json!({"ok": true, "result": created_repository()}),
    );
    assert_eq!(
        sent_body(&server.received()[1]),
        json!({
            "name": "Another",
            "project": {"id": "6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c"},
        }),
        "the captured body without --default-branch"
    );
}

/// The frozen `Enum.find(projects, &(&1["name"] == project)` miss falls back to
/// the argument itself as the project id (captured with a project the list does
/// not hold).
#[test]
fn create_falls_back_to_the_argument_when_the_project_is_not_listed() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects",
        MockResponse::from_fixture("projects_list"),
    );
    server.expect(
        "POST",
        "/myorg/Bravo/_apis/git/repositories",
        MockResponse::json(200, created_repository()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["repos", "create", "Bravo", "Another", "--json"],
        &[],
    );

    assert_mutation_envelope(
        &output,
        &json!({"ok": true, "result": created_repository()}),
    );
    assert_eq!(
        sent_body(&server.received()[1])["project"]["id"],
        json!("Bravo"),
        "a project the list does not hold is used as the id"
    );
}

/// The frozen `case Client.list("/_apis/projects") do {:ok, …} -> … ; _ -> project`
/// swallows a failed lookup and proceeds with the argument — captured with a
/// 500 on the list, the create still sent.
#[test]
fn create_falls_back_to_the_argument_when_the_project_list_fails() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects",
        MockResponse::from_fixture("error_404").with_status(500),
    );
    server.expect(
        "POST",
        "/myorg/Bravo/_apis/git/repositories",
        MockResponse::json(200, created_repository()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["repos", "create", "Bravo", "Another", "--json"],
        &[],
    );

    assert_mutation_envelope(
        &output,
        &json!({"ok": true, "result": created_repository()}),
    );
    assert_eq!(
        server.received().len(),
        2,
        "the lookup failure is not fatal"
    );
    assert_eq!(
        sent_body(&server.received()[1])["project"]["id"],
        json!("Bravo")
    );
}

#[test]
fn delete_with_force_sends_the_delete_and_does_not_prompt() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        REPOSITORY,
        MockResponse::json(200, json!({"id": "a1"})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "repos",
            "delete",
            "Alpha",
            "Alpha.Core",
            "--force",
            "--json",
        ],
        &[],
    );

    assert_mutation_envelope(
        &output,
        &json!({"ok": true, "message": "Repository 'Alpha.Core' deleted from 'Alpha'."}),
    );
    assert!(
        stderr_of(&output).is_empty(),
        "--force skips the prompt entirely: {}",
        stderr_of(&output)
    );
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "DELETE");
    assert_eq!(received[0].path, REPOSITORY);
    assert_eq!(received[0].query_pairs(), vec![pair("api-version", "7.1")]);
    assert!(received[0].body.is_none(), "the delete carries no body");
}

#[test]
fn delete_answered_yes_prompts_on_stderr_and_deletes() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        REPOSITORY,
        MockResponse::json(200, json!({"id": "a1"})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["repos", "delete", "Alpha", "Alpha.Core", "--json"],
        b"y\n",
    );

    assert_mutation_envelope(
        &output,
        &json!({"ok": true, "message": "Repository 'Alpha.Core' deleted from 'Alpha'."}),
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("Delete repository 'Alpha/Alpha.Core'? This cannot be undone. [y/N] "),
        "the question is on stderr (D31): {stderr}"
    );
    assert!(
        !stdout_of(&output).contains("[y/N]"),
        "no prompt may reach stdout: {}",
        stdout_of(&output)
    );
    assert_eq!(server.received().len(), 1, "the confirmed delete was sent");
}

#[test]
fn delete_answered_no_refuses_and_sends_nothing() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["repos", "delete", "Alpha", "Alpha.Core"],
        b"n\n",
    );

    assert_eq!(output.status.code(), Some(1), "a refusal exits 1 (D32)");
    assert!(
        stdout_of(&output).is_empty(),
        "a refusal writes no document: {}",
        stdout_of(&output)
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("Delete repository 'Alpha/Alpha.Core'? This cannot be undone. [y/N] "),
        "the question is on stderr: {stderr}"
    );
    assert!(
        stderr.contains("Aborted."),
        "this build's refusal is on stderr: {stderr}"
    );
    assert!(server.received().is_empty(), "a refusal sends nothing");
}

#[test]
fn delete_at_eof_refuses_and_sends_nothing() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["repos", "delete", "Alpha", "Alpha.Core"],
        &[],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "an unanswered question is a refusal, not the frozen CLI's silent exit 0 (D30)"
    );
    assert!(stdout_of(&output).is_empty());
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("Delete repository 'Alpha/Alpha.Core'? This cannot be undone. [y/N] ")
            && stderr.contains("Aborted."),
        "EOF behaves exactly like an answered no: {stderr}"
    );
    assert!(server.received().is_empty(), "a refusal sends nothing");
}

#[test]
fn delete_refusal_under_json_emits_no_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["repos", "delete", "Alpha", "Alpha.Core", "--json"],
        b"n\n",
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "a refusal is not an API failure and has no envelope; stdout stays empty: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("Aborted."),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn delete_404_reports_the_repository_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        "/myorg/Alpha/_apis/git/repositories/Missing",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["repos", "delete", "Alpha", "Missing", "--force", "--json"],
        &[],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Repository 'Missing' not found in project 'Alpha'")
    );
}

#[test]
fn delete_without_the_repository_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["repos", "delete", "Alpha", "--force", "--json"],
        &[],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_of(&output).is_empty());
    assert!(
        stderr_of(&output).contains("REPO_ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}
