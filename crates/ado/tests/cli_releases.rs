//! End-to-end tests for `ado releases list|show`: the REST surface the frozen
//! `lib/ado_cli/cli/releases.ex` builds under `_apis/release/releases`, the
//! `--json` envelopes, the table and detail views, the query filters and the
//! error paths.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a
//! mock server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT`
//! credentials, so no credential resolution reaches the developer's keychain.

use std::process::{Command, Output};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";
const RELEASES: &str = "/myorg/Alpha/_apis/release/releases";

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

fn releases() -> Value {
    json!([
        {"id": 101, "name": "Release-101", "status": "active", "createdOn": "2026-09-20T12:34:56.789Z"},
        {"id": 100, "name": "Release-100", "status": "abandoned", "createdOn": "2026-09-19T01:02:03.000Z"},
        {"id": 99, "name": "Release-99", "status": "draft", "createdOn": "2026-09-18"}
    ])
}

fn release_show() -> Value {
    json!({
        "id": 101,
        "name": "Release-101",
        "status": "active",
        "releaseDefinition": {"id": 3, "name": "Alpha CD"},
        "createdOn": "2026-09-20T12:34:56.789Z",
        "createdBy": {"displayName": "Ada Lovelace"},
        "url": "https://dev.azure.com/myorg/Alpha/_apis/release/releases/101",
        "environments": [
            {"name": "Dev", "status": "succeeded"},
            {"name": "Prod", "status": "inProgress"},
            {"definitionEnvironmentId": 7, "status": "notStarted"},
            {"name": "Bare"}
        ]
    })
}

#[test]
fn list_emits_the_value_envelope_and_the_project_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        RELEASES,
        MockResponse::json(200, json!({"count": 3, "value": releases()})),
    );

    let output = run(&home, &server, &["releases", "list", "Alpha", "--json"]);

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": releases()}));

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, RELEASES);
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "no filter, no extra pair"
    );
}

#[test]
fn list_filters_send_top_definition_id_and_status() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        RELEASES,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &[
            "releases",
            "list",
            "Alpha",
            "--top",
            "5",
            "--definition-id",
            "3",
            "--status",
            "active",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("%24top".to_owned(), "5".to_owned()),
            ("definitionId".to_owned(), "3".to_owned()),
            ("statusFilter".to_owned(), "active".to_owned())
        ],
        "the module's three params, the schema's own names"
    );
}

#[test]
fn list_human_truncates_the_date_to_ten_bytes_and_keeps_short_ones() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        RELEASES,
        MockResponse::json(200, json!({"count": 3, "value": releases()})),
    );

    let output = run(&home, &server, &["releases", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    assert!(stdout.contains("2026-09-20"), "stdout: {stdout}");
    assert!(
        !stdout.contains("T12:34:56"),
        "the date is truncated: {stdout}"
    );
    assert!(
        stdout.contains("2026-09-18"),
        "a ten-byte date survives whole: {stdout}"
    );
    assert!(stdout.contains("abandoned"), "stdout: {stdout}");
    assert!(stdout.contains("Release-99"), "stdout: {stdout}");
}

#[test]
fn list_human_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        RELEASES,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["releases", "list", "Alpha"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No releases found.\n");
}

#[test]
fn list_404_and_500_keep_the_classified_envelope() {
    for (project, status) in [("Missing", 404), ("Broken", 500)] {
        let home = TempHome::new();
        let server = MockServer::start();
        let path = format!("/myorg/{project}/_apis/release/releases");
        server.expect(
            "GET",
            &path,
            MockResponse::json(status, json!({"message": "TF400813: nope."})),
        );

        let output = run(&home, &server, &["releases", "list", project, "--json"]);

        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            envelope(&output)["error"]["code"],
            json!(if status == 404 {
                "not_found"
            } else {
                "api_error"
            }),
            "status {status}"
        );
    }
}

#[test]
fn show_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{RELEASES}/101"),
        MockResponse::json(200, release_show()),
    );

    let output = run(
        &home,
        &server,
        &["releases", "show", "Alpha", "101", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": release_show()})
    );
    assert_eq!(requests(&server)[0].path, format!("{RELEASES}/101"));
}

#[test]
fn show_human_prints_the_module_detail_and_every_environment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{RELEASES}/101"),
        MockResponse::json(200, release_show()),
    );

    let output = run(&home, &server, &["releases", "show", "Alpha", "101"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "\nRelease Details\n\n{}\n  ID:          101\n  Name:        Release-101\n  Status:      active\n  Definition:  Alpha CD\n  Created On:  2026-09-20T12:34:56.789Z\n  Created By:  Ada Lovelace\n  URL:         https://dev.azure.com/myorg/Alpha/_apis/release/releases/101\n\n  Environments:\n    - Dev: succeeded\n    - Prod: inProgress\n    - 7: notStarted\n    - Bare: unknown\n\n",
            "─".repeat(60)
        ),
        "the module's detail: the box-drawing rule, the environment fallbacks (`7`, `unknown`), and the trailing blank line"
    );
}

#[test]
fn show_human_minimal_omits_the_optional_lines() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{RELEASES}/102"),
        MockResponse::json(
            200,
            json!({"id": 102, "name": "Release-102", "status": null}),
        ),
    );

    let output = run(&home, &server, &["releases", "show", "Alpha", "102"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "\nRelease Details\n\n{}\n  ID:          102\n  Name:        Release-102\n  Status:      \n  Created On:  \n  URL:         \n\n",
            "─".repeat(60)
        ),
        "an absent definition/creator/environments prints no line; a nil field prints empty"
    );
}

#[test]
fn show_404_keeps_the_module_wording_in_the_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{RELEASES}/999"),
        MockResponse::json(404, json!({"message": "TF400813: nope."})),
    );

    let output = run(
        &home,
        &server,
        &["releases", "show", "Alpha", "999", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(envelope(&output)["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Release #999 not found in project 'Alpha'")
    );
}

#[test]
fn show_without_an_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["releases", "show", "Alpha"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("RELEASE_ID"));
    assert!(requests(&server).is_empty());
}

#[test]
fn show_non_integer_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["releases", "show", "Alpha", "not-an-integer"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("RELEASE_ID") || stderr_of(&output).contains("integer"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(requests(&server).is_empty());
}

#[test]
fn list_underscore_definition_id_is_not_an_option() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["releases", "list", "Alpha", "--definition_id", "3"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("definition_id"),
        "the schema's spelling is not the runnable flag (the capture rejects it too): {}",
        stderr_of(&output)
    );
    assert!(requests(&server).is_empty());
}
