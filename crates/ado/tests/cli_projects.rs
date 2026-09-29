//! End-to-end tests for `ado projects list|show`: the REST surface the frozen
//! `lib/ado_cli/cli/projects.ex` builds — `show` exactly, `list` with the
//! `stateFilter`/`$top`/`$skip` its mapping table intends rather than the
//! `state`/`top`/`skip` its lookup miss sends (D19) — the `--json` envelopes, and
//! the human table/detail output.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a
//! mock server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT` credentials
//! so no credential resolution reaches the developer's keychain.

use std::process::{Command, Output, Stdio};

use ado_testkit::{MockResponse, MockServer, TempHome, ado_cmd, stderr_of, stdout_of};
use serde_json::{Value, json};

const ORG: &str = "myorg";

/// The frozen escript's literal `ado projects list --json` line for the
/// `projects_list` fixture, captured against the mock (W1-R12): the read
/// commands' list envelope is the value form, a bare array under `result` —
/// `Helpers.json_or_format`'s kind, not `count`/`items`.
const ORACLE_LIST_JSON: &str = r#"{"ok":true,"result":[{"description":"The first project","id":"6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c","lastUpdateTime":"2026-09-01T09:12:44.413Z","name":"Alpha","revision":12,"state":"wellFormed","url":"https://dev.azure.com/myorg/_apis/projects/6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c","visibility":"private"},{"description":"The second project","id":"b7c2d0a4-5e6f-4a1b-8c9d-0e1f2a3b4c5d","lastUpdateTime":"2026-09-10T15:02:10.117Z","name":"Beta","revision":4,"state":"wellFormed","url":"https://dev.azure.com/myorg/_apis/projects/b7c2d0a4-5e6f-4a1b-8c9d-0e1f2a3b4c5d","visibility":"public"}]}"#;

fn command(home: &TempHome, server: &MockServer, args: &[&str]) -> Command {
    let mut command = ado_cmd();
    home.apply(&mut command);
    command
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

/// The mapped names, not the frozen CLI's `state`/`top`/`skip`: the module's
/// mapping table and its help text intend `stateFilter`/`$top`/`$skip`, and
/// Azure ignores anything else (D19).
#[test]
fn list_sends_the_mapped_query_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/_apis/projects",
        &[
            ("api-version", "7.1"),
            ("stateFilter", "wellFormed"),
            ("%24top", "2"),
            ("%24skip", "1"),
        ],
        MockResponse::from_fixture("projects_list"),
    );

    let output = run(
        &home,
        &server,
        &[
            "projects",
            "list",
            "--state",
            "wellFormed",
            "--top",
            "2",
            "--skip",
            "1",
            "--json",
        ],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_LIST_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    let items = fixture("projects_list");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": items["value"].clone()}),
        "the parsed envelope carries the fixture's array under result"
    );
    assert_eq!(
        received_query(&server),
        vec![
            pair("api-version", "7.1"),
            pair("stateFilter", "wellFormed"),
            pair("%24top", "2"),
            pair("%24skip", "1"),
        ],
        "the mapped names, in wire form (D19)"
    );
    assert_eq!(server.received()[0].path, "/myorg/_apis/projects");
}

#[test]
fn list_without_options_sends_only_the_api_version() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/_apis/projects",
        &[("api-version", "7.1")],
        MockResponse::from_fixture("projects_list"),
    );

    let output = run(&home, &server, &["projects", "list", "--json"]);

    assert_success(&output);
    assert_eq!(
        received_query(&server),
        vec![pair("api-version", "7.1")],
        "absent options are not sent at all"
    );
}

/// The oracle sends every option that is present: Elixir's `if value` is truthy
/// for `0` and `""` (only `nil`/`false` are falsy), so `--top 0`, `--skip 0` and
/// `--state ''` all reach the wire.
#[test]
fn list_sends_present_zero_and_empty_options() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/_apis/projects",
        &[
            ("api-version", "7.1"),
            ("stateFilter", ""),
            ("%24top", "0"),
            ("%24skip", "0"),
        ],
        MockResponse::from_fixture("projects_list"),
    );

    let output = run(
        &home,
        &server,
        &[
            "projects", "list", "--state", "", "--top", "0", "--skip", "0", "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        received_query(&server),
        vec![
            pair("api-version", "7.1"),
            pair("stateFilter", ""),
            pair("%24top", "0"),
            pair("%24skip", "0"),
        ],
        "a present zero or empty option is sent, exactly as the oracle does"
    );
}

/// The oracle's `--top`/`--skip` are integers, so a negative value parses and
/// reaches the wire; clap needs `allow_negative_numbers` to match that.
#[test]
fn list_sends_negative_top_and_skip() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/_apis/projects",
        &[("api-version", "7.1"), ("%24top", "-1"), ("%24skip", "-1")],
        MockResponse::from_fixture("projects_list"),
    );

    let output = run(
        &home,
        &server,
        &["projects", "list", "--top", "-1", "--skip", "-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        received_query(&server),
        vec![
            pair("api-version", "7.1"),
            pair("%24top", "-1"),
            pair("%24skip", "-1"),
        ],
        "a negative value is a value, not a flag"
    );
}

#[test]
fn list_json_emits_no_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects",
        MockResponse::from_fixture("projects_list"),
    );

    let output = run(&home, &server, &["projects", "list", "--json"]);

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
    for marker in ['\u{2500}', '\u{2501}', '\u{2502}', '\u{1b}'] {
        assert!(
            !stdout.contains(marker),
            "table or ANSI byte {marker:?} reached --json output: {stdout:?}"
        );
    }
}

#[test]
fn list_human_output_is_the_project_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects",
        MockResponse::from_fixture("projects_list"),
    );

    let output = run(&home, &server, &["projects", "list"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();
    let projects = fixture("projects_list");
    let first = &projects["value"][0];
    let second = &projects["value"][1];

    assert_eq!(
        lines.len(),
        4,
        "a header row, a rule row and one row per project: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header row: {stdout}");
    let name_at = lines[0].find("Name").expect("the Name header");
    let state_at = lines[0].find("State").expect("the State header");
    assert!(
        name_at > 0 && state_at > name_at,
        "the header order is ID, Name, State: {stdout}"
    );
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].contains(first["id"].as_str().expect("an id"))
            && lines[2].contains(first["name"].as_str().expect("a name"))
            && lines[2].ends_with(first["state"].as_str().expect("a state")),
        "the first data row: {stdout}"
    );
    assert!(
        lines[3].contains(second["name"].as_str().expect("a name")),
        "the second data row: {stdout}"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
    for marker in ['\u{2500}', '\u{2501}', '\u{2502}'] {
        assert!(
            !stdout.contains(marker),
            "box drawing in piped output: {stdout:?}"
        );
    }
}

#[test]
fn list_human_output_says_no_projects_found_when_empty() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects",
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["projects", "list"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No projects found.\n");
}

#[test]
fn list_404_is_the_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(&home, &server, &["projects", "list", "--json"]);

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
fn show_sends_the_project_path_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/_apis/projects/Alpha",
        &[("api-version", "7.1")],
        MockResponse::from_fixture("projects_show"),
    );

    let output = run(&home, &server, &["projects", "show", "Alpha", "--json"]);

    assert_success(&output);
    let expected = json!({"ok": true, "result": fixture("projects_show")});
    assert_eq!(
        stdout_of(&output),
        format!("{expected}\n"),
        "the value envelope is the whole of stdout"
    );
    assert_eq!(received_query(&server), vec![pair("api-version", "7.1")]);
    assert_eq!(server.received()[0].path, "/myorg/_apis/projects/Alpha");
}

#[test]
fn show_with_capabilities_adds_the_capabilities_param() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/_apis/projects/Alpha",
        &[("api-version", "7.1"), ("includeCapabilities", "true")],
        MockResponse::from_fixture("projects_show"),
    );

    let output = run(
        &home,
        &server,
        &["projects", "show", "Alpha", "--capabilities", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        received_query(&server),
        vec![
            pair("api-version", "7.1"),
            pair("includeCapabilities", "true")
        ]
    );
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(
        envelope["result"]["capabilities"],
        fixture("projects_show")["capabilities"],
        "the capability map is what the fixture answered"
    );
}

#[test]
fn show_encodes_the_project_name_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/_apis/projects/My%20Project",
        &[("api-version", "7.1")],
        MockResponse::from_fixture("projects_show"),
    );

    let output = run(
        &home,
        &server,
        &["projects", "show", "My Project", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/_apis/projects/My%20Project",
        "URI.encode/1 encodes a space as %20, not +"
    );
}

#[test]
fn show_human_output_is_the_project_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects/Alpha",
        MockResponse::from_fixture("projects_show"),
    );

    let output = run(&home, &server, &["projects", "show", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let project = fixture("projects_show");
    let string = |key: &str| project[key].as_str().expect("a string field").to_owned();
    let team = &project["defaultTeam"];

    assert!(stdout.contains("Project Details\n"), "stdout: {stdout}");
    assert!(
        stdout.contains(&format!("  ID:          {}", string("id"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  Name:        {}", string("name"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  Description: {}", string("description"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  State:       {}", string("state"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  Visibility:  {}", string("visibility"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!("  URL:         {}", string("url"))),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "  Default Team: {} ({})",
            team["name"].as_str().expect("a team name"),
            team["id"].as_str().expect("a team id")
        )),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
}

#[test]
fn show_without_a_project_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["projects", "show", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("PROJECT_ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn show_404_reports_the_project_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/projects/Alpha",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(&home, &server, &["projects", "show", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Project 'Alpha' not found")
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
        "/myorg/_apis/projects",
        MockResponse::from_fixture("projects_list"),
    );

    let mut child = command(&home, &server, &["projects", "list"])
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

#[test]
fn projects_without_a_subcommand_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["projects"]);

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
