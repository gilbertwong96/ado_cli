//! End-to-end tests for `ado test-coverage show`: the `_apis/test/codecoverage`
//! read the frozen `lib/ado_cli/cli/test_coverage.ex` builds, its `coverageData`
//! handling (a data path, an empty array, and a body with no key at all), the
//! bar chart's shape and colours, the `--json` envelopes and the error paths.
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

fn coverage_path(project: &str) -> String {
    format!("/{ORG}/{project}/_apis/test/codecoverage")
}

/// The frozen `test_results_test.exs` `test-coverage show` body, with a second
/// configuration and a second stat so the chart has more than one line.
fn coverage_data() -> Value {
    json!({"coverageData": [
        {"coverageStats": [
            {"label": "Lines", "total": 100, "covered": 85},
            {"label": "Branches", "total": 40, "covered": 12}
        ]},
        {"coverageStats": [
            {"label": "Lines", "total": 20, "covered": 20}
        ]}
    ]})
}

#[test]
fn show_emits_the_coverage_data_array() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &coverage_path("Alpha"),
        MockResponse::json(200, coverage_data()),
    );

    let output = run(
        &home,
        &server,
        &["test-coverage", "show", "Alpha", "42", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": coverage_data()["coverageData"]}),
        "the module's `data`, the coverageData array itself"
    );
}

#[test]
fn show_sends_the_build_id_pair() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &coverage_path("Alpha"),
        MockResponse::json(200, coverage_data()),
    );

    let output = run(
        &home,
        &server,
        &["test-coverage", "show", "Alpha", "42", "--json"],
    );

    assert_success(&output);
    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, coverage_path("Alpha"));
    assert_eq!(
        requests[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("buildId".to_owned(), "42".to_owned())
        ],
        "the module's `%{{\"buildId\" => build_id}}` pair"
    );
}

#[test]
fn show_human_draws_the_bar_chart() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &coverage_path("Alpha"),
        MockResponse::json(200, coverage_data()),
    );

    let output = run(&home, &server, &["test-coverage", "show", "Alpha", "42"]);

    assert_success(&output);
    let expected = format!(
        concat!(
            "\nCode Coverage for Build #42\n",
            "{}\n",
            "  Lines                   85.0% \u{1b}[32m{}\u{1b}[0m\n",
            "  Branches                30.0% \u{1b}[31m{}\u{1b}[0m\n",
            "  Lines                  100.0% \u{1b}[32m{}\u{1b}[0m\n",
            "\n",
        ),
        "─".repeat(70),
        "█".repeat(17) + &"░".repeat(3),
        "█".repeat(6) + &"░".repeat(14),
        "█".repeat(20),
    );

    assert_eq!(
        stdout_of(&output),
        expected,
        "the module's 20-cell bar, its 8-wide percentage and its three colours"
    );
}

#[test]
fn show_human_prints_the_header_for_an_empty_result() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &coverage_path("Empty"),
        MockResponse::json(200, json!({"coverageData": []})),
    );

    let output = run(&home, &server, &["test-coverage", "show", "Empty", "42"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("\nCode Coverage for Build #42\n{}\n\n", "─".repeat(70)),
        "an empty coverageData array still draws the header, with no rows"
    );
}

#[test]
fn show_json_emits_the_empty_value_envelope_for_an_empty_result() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &coverage_path("Empty"),
        MockResponse::json(200, json!({"coverageData": []})),
    );

    let output = run(
        &home,
        &server,
        &["test-coverage", "show", "Empty", "42", "--json"],
    );

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": []}));
}

#[test]
fn show_reports_no_coverage_data_for_a_body_without_the_key() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &coverage_path("NoCoverage"),
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["test-coverage", "show", "NoCoverage", "43"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "\nNo coverage data for build #43.\n\nDone.\n",
        "the module's show_no_coverage/1, its halt_success/1 message included"
    );
}

#[test]
fn show_json_emits_the_empty_value_envelope_without_the_key() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &coverage_path("NoCoverage"),
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["test-coverage", "show", "NoCoverage", "43", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": []}),
        "D21's rule: a read path whose frozen --json is prose answers the value envelope here"
    );
    assert!(
        !stdout_of(&output).contains("Done."),
        "no human line rides along with the document"
    );
}

#[test]
fn show_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &coverage_path("Missing"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["test-coverage", "show", "Missing", "42", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["details"]["body"],
        json!("{\"message\":\"TF400813: The user is not authorized.\"}"),
        "D24: the raw upstream body"
    );
}

#[test]
fn show_500_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &coverage_path("Broken"),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["test-coverage", "show", "Broken", "42", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
}

#[test]
fn show_encodes_the_project_as_one_segment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("/{ORG}/Alpha%2FBeta/_apis/test/codecoverage"),
        MockResponse::json(200, json!({"coverageData": []})),
    );

    let output = run(
        &home,
        &server,
        &["test-coverage", "show", "Alpha/Beta", "42", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].path,
        format!("/{ORG}/Alpha%2FBeta/_apis/test/codecoverage"),
        "the frozen path interpolates the project raw; this build escapes it (D22)"
    );
}
