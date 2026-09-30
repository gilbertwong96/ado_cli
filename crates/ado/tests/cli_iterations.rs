//! End-to-end tests for `ado iterations list|show|create|update|delete`: the REST
//! surface the frozen `lib/ado_cli/cli/iterations.ex` builds under a team's
//! `work/teamsettings/iterations`, the `--json` envelopes, the human views, and
//! the date-option defect (D39) this build repairs.

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";
const ITERATIONS: &str = "/myorg/Alpha/Team/_apis/work/teamsettings/iterations";
const ITER_ID: &str = "aaaaaaaa-0001-0001-0001-000000000001";

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

fn run_with_stdin(home: &TempHome, server: &MockServer, bytes: &str, args: &[&str]) -> Output {
    let mut command = command(home, server, args);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn ado");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(bytes.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("run ado")
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

fn envelope(output: &Output) -> Value {
    serde_json::from_str(&stdout_of(output)).expect("stdout is one JSON document")
}

fn request(server: &MockServer) -> RecordedRequest {
    let received = server.received();

    assert_eq!(received.len(), 1, "expected exactly one request");
    received[0].clone()
}

fn body_of(request: &RecordedRequest) -> Value {
    serde_json::from_str(request.body.as_deref().expect("the request carried a body"))
        .expect("the body is JSON")
}

#[test]
fn list_emits_the_value_envelope_with_the_unwrapped_items() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ITERATIONS,
        MockResponse::from_fixture("iterations_list"),
    );

    let output = run(
        &home,
        &server,
        &["iterations", "list", "Alpha", "Team", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": fixture("iterations_list")["value"].clone()}),
        "`Client.list` unwraps `value`; the envelope is the value form (not count/items)"
    );

    let request = request(&server);
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, ITERATIONS);
    assert_eq!(
        request.query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())]
    );
}

#[test]
fn list_current_sends_the_timeframe_pair() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ITERATIONS,
        MockResponse::from_fixture("iterations_list"),
    );

    let output = run(
        &home,
        &server,
        &["iterations", "list", "Alpha", "Team", "--current", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        request(&server).query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("%24timeframe".to_owned(), "current".to_owned()),
        ],
        "the frozen CLI glues `?$timeframe=current` before the version, one broken pair (D25); this build sends the two pairs"
    );
}

#[test]
fn list_empty_says_no_iterations_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ITERATIONS,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["iterations", "list", "Alpha", "Team"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "No iterations found.\n",
        "the module's empty sentence"
    );
}

#[test]
fn list_renders_the_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ITERATIONS,
        MockResponse::from_fixture("iterations_list"),
    );

    let output = run(&home, &server, &["iterations", "list", "Alpha", "Team"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        4,
        "header, rule, one row per iteration: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header: {stdout}");
    let id_at = lines[0].find("ID").expect("the ID header");
    let name_at = lines[0].find("Name").expect("the Name header");
    let start_at = lines[0].find("Start").expect("the Start header");
    let finish_at = lines[0].find("Finish").expect("the Finish header");
    assert!(
        id_at < name_at && name_at < start_at && start_at < finish_at,
        "the header order is ID, Name, Start, Finish: {stdout}"
    );
    assert!(
        lines[2].contains(ITER_ID)
            && lines[2].contains("Sprint 24")
            && lines[2].contains("2026-01-15T00:00:00Z"),
        "the first row: {stdout}"
    );
}

#[test]
fn list_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ITERATIONS,
        MockResponse::json(404, json!({"message": "The team was not found."})),
    );

    let output = run(
        &home,
        &server,
        &["iterations", "list", "Alpha", "Team", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["code"],
        json!("not_found"),
        "the list error path keeps the classified envelope (C2)"
    );
}

#[test]
fn list_500_emits_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ITERATIONS,
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["iterations", "list", "Alpha", "Team", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
}

#[test]
fn show_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{ITERATIONS}/{ITER_ID}"),
        MockResponse::from_fixture("iterations_show"),
    );

    let output = run(
        &home,
        &server,
        &["iterations", "show", "Alpha", "Team", ITER_ID, "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": fixture("iterations_show")})
    );
    assert_eq!(request(&server).path, format!("{ITERATIONS}/{ITER_ID}"));
}

#[test]
fn show_renders_the_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{ITERATIONS}/{ITER_ID}"),
        MockResponse::from_fixture("iterations_show"),
    );

    let output = run(
        &home,
        &server,
        &["iterations", "show", "Alpha", "Team", ITER_ID],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(
        stdout.starts_with("\nIteration Details\n"),
        "the module's heading: {stdout:?}"
    );
    assert!(
        stdout.contains(&"─".repeat(60)),
        "the 60-character rule: {stdout:?}"
    );
    assert!(
        stdout.contains(&format!("  ID:    {ITER_ID}\n")),
        "{stdout}"
    );
    assert!(stdout.contains("  Name:  Sprint 24\n"), "{stdout}");
    assert!(stdout.contains("  Path:  Alpha\\Sprint 24\n"), "{stdout}");
    assert!(
        stdout.contains("  Start: 2026-01-15T00:00:00Z\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("  Finish: 2026-01-29T00:00:00Z\n"),
        "{stdout}"
    );
}

#[test]
fn show_404_reports_iteration_not_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{ITERATIONS}/missing-id"),
        MockResponse::json(404, json!({"message": "The iteration was not found."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "show",
            "Alpha",
            "Team",
            "missing-id",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({
            "ok": false,
            "error": {
                "code": "not_found",
                "status": 404,
                "message": "Iteration not found",
                "details": {
                    "status": 404,
                    "body": "{\"message\":\"The iteration was not found.\"}",
                },
            },
        }),
        "the module's own 404 wording"
    );
}

#[test]
fn create_posts_the_name_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        ITERATIONS,
        MockResponse::from_fixture("iterations_show"),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "create",
            "Alpha",
            "Team",
            "--name",
            "Sprint 24",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": fixture("iterations_show")})
    );

    let request = request(&server);
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, ITERATIONS);
    assert_eq!(
        body_of(&request),
        json!({"name": "Sprint 24"}),
        "names only: an absent date adds no attributes key"
    );
}

#[test]
fn create_with_dates_posts_the_attributes_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        ITERATIONS,
        MockResponse::from_fixture("iterations_show"),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "create",
            "Alpha",
            "Team",
            "--name",
            "Sprint 24",
            "--start-date",
            "2026-03-01",
            "--finish-date",
            "2026-03-14",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server)),
        json!({
            "name": "Sprint 24",
            "attributes": {"startDate": "2026-03-01", "finishDate": "2026-03-14"},
        }),
        "the body the frozen `put_in/3` intended and crashed before sending (D39)"
    );
}

#[test]
fn create_with_one_date_posts_only_that_attribute() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        ITERATIONS,
        MockResponse::from_fixture("iterations_show"),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "create",
            "Alpha",
            "Team",
            "--name",
            "Sprint 24",
            "--start-date",
            "2026-03-01",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server)),
        json!({"name": "Sprint 24", "attributes": {"startDate": "2026-03-01"}})
    );
}

#[test]
fn create_with_a_finish_date_posts_only_that_attribute() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        ITERATIONS,
        MockResponse::from_fixture("iterations_show"),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "create",
            "Alpha",
            "Team",
            "--name",
            "Sprint 24",
            "--finish-date",
            "2026-03-14",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server)),
        json!({"name": "Sprint 24", "attributes": {"finishDate": "2026-03-14"}}),
        "the finish-only half of the D39 repair, whose capture T16 added"
    );
}

#[test]
fn create_without_name_is_a_usage_error_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["iterations", "create", "Alpha", "Team", "--json"],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "a missing required option is loud here (D34)"
    );
    assert!(stderr_of(&output).contains("--name"));
    assert!(server.received().is_empty());
}

#[test]
fn update_patches_only_the_given_name() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &format!("{ITERATIONS}/{ITER_ID}"),
        MockResponse::from_fixture("iterations_show"),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "update",
            "Alpha",
            "Team",
            ITER_ID,
            "--name",
            "Sprint 24b",
            "--json",
        ],
    );

    assert_success(&output);
    let request = request(&server);

    assert_eq!(request.method, "PATCH");
    assert_eq!(request.path, format!("{ITERATIONS}/{ITER_ID}"));
    assert_eq!(body_of(&request), json!({"name": "Sprint 24b"}));
}

#[test]
fn update_with_dates_posts_the_attributes_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &format!("{ITERATIONS}/{ITER_ID}"),
        MockResponse::from_fixture("iterations_show"),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "update",
            "Alpha",
            "Team",
            ITER_ID,
            "--start-date",
            "2026-03-01",
            "--finish-date",
            "2026-03-14",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server)),
        json!({"attributes": {"startDate": "2026-03-01", "finishDate": "2026-03-14"}}),
        "the same D39 repair on the update path"
    );
}

#[test]
fn update_with_a_start_date_posts_only_that_attribute() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &format!("{ITERATIONS}/{ITER_ID}"),
        MockResponse::from_fixture("iterations_show"),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "update",
            "Alpha",
            "Team",
            ITER_ID,
            "--start-date",
            "2026-03-01",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server)),
        json!({"attributes": {"startDate": "2026-03-01"}}),
        "the start-only half of the D39 repair, whose capture T16 added"
    );
}

#[test]
fn update_without_options_is_the_modules_guard() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["iterations", "update", "Alpha", "Team", ITER_ID, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({
            "ok": false,
            "error": {
                "code": "validation_error",
                "message": "At least one option is required.",
            },
        }),
        "the module's guard wording; the oracle writes it to stderr with no envelope (D4)"
    );
    assert!(
        server.received().is_empty(),
        "the guard precedes the request"
    );
}

#[test]
fn update_404_reports_iteration_not_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &format!("{ITERATIONS}/missing-id"),
        MockResponse::json(404, json!({"message": "The iteration was not found."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "update",
            "Alpha",
            "Team",
            "missing-id",
            "--name",
            "X",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Iteration not found")
    );
}

#[test]
fn delete_sends_the_delete_without_a_prompt() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{ITERATIONS}/{ITER_ID}"),
        MockResponse::json(204, json!({})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        "n\n",
        &["iterations", "delete", "Alpha", "Team", ITER_ID, "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Iteration deleted."}),
        "the message envelope where the oracle prints its human line under --json (D33)"
    );

    let request = request(&server);

    assert_eq!(request.method, "DELETE");
    assert_eq!(
        request.path,
        format!("{ITERATIONS}/{ITER_ID}"),
        "the request proceeded: `iterations delete` never prompts (R1)"
    );
    assert!(request.body.is_none());
}

#[test]
fn delete_404_reports_iteration_not_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{ITERATIONS}/missing-id"),
        MockResponse::json(404, json!({"message": "The iteration was not found."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "iterations",
            "delete",
            "Alpha",
            "Team",
            "missing-id",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Iteration not found")
    );
}
