//! End-to-end tests for `ado teams list|show|create|update|delete` and
//! `ado teams members list`: the REST surface the frozen
//! `lib/ado_cli/cli/teams.ex` built under `/{project}/_apis/teams` — which Azure
//! does not serve, so every command answered 404 live. These tests pin the
//! corrected `_apis/projects/{project}/teams` route (D58), the `--json`
//! envelopes, the human table and detail views, and the guard/error paths.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a
//! mock server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT`
//! credentials, so no credential resolution reaches the developer's keychain.

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";
const TEAMS: &str = "/myorg/_apis/projects/Alpha/teams";

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

/// Runs with `bytes` on stdin, so a command that prompts cannot proceed on the
/// developer's terminal (R8) and a refusal is observable.
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

/// The `value` array's two teams, the same fixture the capture served the oracle.
fn teams() -> Value {
    json!([
        {
            "id": "team-1",
            "name": "Alpha Team",
            "description": "The alpha team",
            "url": "https://dev.azure.com/myorg/_apis/projects/Alpha/teams/team-1",
            "identityUrl": "https://vssps.dev.azure.com/myorg/_apis/Identities/team-1",
            "projectName": "Alpha",
            "projectId": "proj-1"
        },
        {
            "id": "team-2",
            "name": "Beta Team",
            "description": "A description longer than thirty chars",
            "url": "https://dev.azure.com/myorg/_apis/projects/Alpha/teams/team-2",
            "identityUrl": "https://vssps.dev.azure.com/myorg/_apis/Identities/team-2",
            "projectName": "Alpha",
            "projectId": "proj-1"
        }
    ])
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
fn list_emits_the_value_envelope_and_the_project_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        TEAMS,
        MockResponse::json(200, json!({"count": 2, "value": teams()})),
    );

    let output = run(&home, &server, &["teams", "list", "Alpha", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": teams()}),
        "the frozen `json_or_format` value envelope: the unwrapped value array"
    );

    let request = request(&server);
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, TEAMS);
    assert_eq!(
        request.query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "the absent --top is not sent"
    );
}

#[test]
fn list_top_sends_the_top_query() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        TEAMS,
        MockResponse::json(200, json!({"count": 2, "value": teams()})),
    );

    let output = run(
        &home,
        &server,
        &["teams", "list", "Alpha", "--top", "5", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        request(&server).query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("%24top".to_owned(), "5".to_owned()),
        ],
        "the module's `$top`, percent-encoded on the wire"
    );
}

#[test]
fn list_renders_the_module_columns() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        TEAMS,
        MockResponse::json(200, json!({"count": 2, "value": teams()})),
    );

    let output = run(&home, &server, &["teams", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert!(lines[0].starts_with("ID"), "header line: {stdout}");
    assert!(lines[0].contains("Name"), "header line: {stdout}");
    assert!(lines[0].contains("Description"), "header line: {stdout}");
    assert!(
        lines[2].contains("team-1")
            && lines[2].contains("Alpha Team")
            && lines[2].contains("The alpha team"),
        "the first row carries the module's three fields: {stdout}"
    );
    assert!(
        lines[3].contains("team-2")
            && lines[3].contains("Beta Team")
            && lines[3].contains("A description longer than thirty chars"),
        "the second row keeps the whole description (this build's table, §8): {stdout}"
    );
}

#[test]
fn list_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        TEAMS,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["teams", "list", "Alpha"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No teams found.\n");
}

#[test]
fn list_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        TEAMS,
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(&home, &server, &["teams", "list", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({
            "ok": false,
            "error": {
                "code": "not_found",
                "status": 404,
                "message": "Resource not found. Check the project/repo/build ID and your permissions.",
                "details": {
                    "status": 404,
                    "body": "{\"message\":\"TF400813: The user is not authorized.\"}",
                },
            },
        }),
        "the list error path keeps the classified envelope (C2)"
    );
    assert!(
        stderr_of(&output).is_empty(),
        "the envelope is the whole answer under --json"
    );
}

#[test]
fn list_500_emits_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        TEAMS,
        MockResponse::json(
            500,
            json!({"message": "TF400898: An Internal Error Occurred."}),
        ),
    );

    let output = run(&home, &server, &["teams", "list", "Alpha", "--json"]);

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
fn show_gets_the_team_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    let team = teams()[0].clone();
    server.expect(
        "GET",
        &format!("{TEAMS}/team-1"),
        MockResponse::json(200, team.clone()),
    );

    let output = run(
        &home,
        &server,
        &["teams", "show", "Alpha", "team-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": team}));
    assert_eq!(request(&server).path, format!("{TEAMS}/team-1"));
}

#[test]
fn show_renders_the_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{TEAMS}/team-1"),
        MockResponse::json(200, teams()[0].clone()),
    );

    let output = run(&home, &server, &["teams", "show", "Alpha", "team-1"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\nTeam Details\n\n",
            "────────────────────────────────────────────────────────────\n",
            "  ID:          team-1\n",
            "  Name:        Alpha Team\n",
            "  Description: The alpha team\n",
            "  URL:         https://dev.azure.com/myorg/_apis/projects/Alpha/teams/team-1\n",
            "\n",
        ),
        "the module's detail, with the trailing blank line its `writeln(\"\")` writes"
    );
}

#[test]
fn show_prints_none_for_a_missing_description_and_an_empty_absent_url() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{TEAMS}/team-1"),
        MockResponse::json(200, json!({"id": "team-1", "name": "Alpha Team"})),
    );

    let output = run(&home, &server, &["teams", "show", "Alpha", "team-1"]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(
        stdout.contains("  Description: (none)\n"),
        "the module's `description || \"(none)\"`: {stdout}"
    );
    assert!(
        stdout.contains("  URL:         \n"),
        "a missing url interpolates as the empty string: {stdout}"
    );
}

#[test]
fn show_404_reports_the_team_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{TEAMS}/missing-id"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["teams", "show", "Alpha", "missing-id", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({
            "ok": false,
            "error": {
                "code": "not_found",
                "status": 404,
                "message": "Team 'missing-id' not found",
                "details": {
                    "status": 404,
                    "body": "{\"message\":\"TF400813: The user is not authorized.\"}",
                },
            },
        }),
        "the module's own 404 wording (D4 for the oracle's stderr-only shape)"
    );
}

#[test]
fn show_encodes_an_email_id_strictly() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{TEAMS}/ada%40example.com"),
        MockResponse::json(200, json!({"id": "ada@example.com", "name": "Ada Team"})),
    );

    let output = run(
        &home,
        &server,
        &["teams", "show", "Alpha", "ada@example.com"],
    );

    assert_success(&output);
    assert_eq!(
        request(&server).path,
        format!("{TEAMS}/ada%40example.com"),
        "the stricter segment encoding (D22): the oracle's URI.encode leaves @ alone"
    );
}

#[test]
fn create_posts_the_name_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        TEAMS,
        MockResponse::json(200, json!({"id": "team-9", "name": "Team"})),
    );

    let output = run(
        &home,
        &server,
        &["teams", "create", "Alpha", "--name", "Team", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": {"id": "team-9", "name": "Team"}}),
        "the created team under the value envelope"
    );

    let request = request(&server);
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, TEAMS);
    assert_eq!(
        body_of(&request),
        json!({"name": "Team"}),
        "no description key when --description is absent"
    );
}

#[test]
fn create_with_description_adds_it_to_the_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        TEAMS,
        MockResponse::json(200, json!({"id": "team-10", "name": "Beta"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "teams",
            "create",
            "Alpha",
            "--name",
            "Beta",
            "--description",
            "A team",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Team 'Beta' created (ID: team-10).\n",
        "the response's name and id (D33 is the --json difference)"
    );
    assert_eq!(
        body_of(&request(&server)),
        json!({"name": "Beta", "description": "A team"})
    );
}

#[test]
fn create_without_name_is_a_usage_error_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["teams", "create", "Alpha", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a missing required option is loud here (D34)"
    );
    assert!(
        stderr_of(&output).contains("--name"),
        "stderr names the missing option: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty(), "nothing was sent");
}

#[test]
fn update_patches_only_the_given_options() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &format!("{TEAMS}/team-1"),
        MockResponse::json(200, json!({"id": "team-1", "name": "Renamed"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "teams", "update", "Alpha", "team-1", "--name", "Renamed", "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": {"id": "team-1", "name": "Renamed"}})
    );

    let request = request(&server);
    assert_eq!(request.method, "PATCH");
    assert_eq!(request.path, format!("{TEAMS}/team-1"));
    assert_eq!(
        body_of(&request),
        json!({"name": "Renamed"}),
        "only the given field"
    );
}

#[test]
fn update_description_alone_patches_the_description() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &format!("{TEAMS}/team-2"),
        MockResponse::json(200, json!({"id": "team-2", "name": "Beta Team"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "teams",
            "update",
            "Alpha",
            "team-2",
            "--description",
            "New desc",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Team 'Beta Team' updated.\n",
        "the response's name (D33 is the --json difference)"
    );
    assert_eq!(
        body_of(&request(&server)),
        json!({"description": "New desc"})
    );
}

#[test]
fn update_without_options_is_the_module_guard_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["teams", "update", "Alpha", "team-1", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({
            "ok": false,
            "error": {
                "code": "validation_error",
                "message": "At least one of --name or --description is required.",
            },
        }),
        "the module's guard precedes any request (the oracle writes it to stderr, D4)"
    );
    assert!(server.received().is_empty(), "nothing was sent");
}

#[test]
fn update_404_reports_the_team_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &format!("{TEAMS}/missing-id"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "teams",
            "update",
            "Alpha",
            "missing-id",
            "--name",
            "Renamed",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(envelope(&output)["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Team 'missing-id' not found")
    );
}

#[test]
fn delete_sends_the_delete_without_a_prompt() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{TEAMS}/team-1"),
        MockResponse::json(204, json!({})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        "n\n",
        &["teams", "delete", "Alpha", "team-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Team 'team-1' deleted."}),
        "the message envelope where the oracle prints its human line under --json (D33)"
    );

    let request = request(&server);
    assert_eq!(request.method, "DELETE");
    assert_eq!(
        request.path,
        format!("{TEAMS}/team-1"),
        "the request proceeded: `teams delete` never prompts (R1/R5)"
    );
    assert!(request.body.is_none(), "the DELETE carries no body");
    assert!(
        stderr_of(&output).is_empty(),
        "no prompt reached stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn delete_404_reports_the_team_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{TEAMS}/missing-id"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["teams", "delete", "Alpha", "missing-id", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Team 'missing-id' not found")
    );
}

#[test]
fn members_list_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    let members = json!([
        {
            "id": "member-1",
            "identity": {"displayName": "Ada Lovelace", "uniqueName": "ada@example.com", "id": "id-1"},
            "isTeamAdmin": false
        }
    ]);
    server.expect(
        "GET",
        &format!("{TEAMS}/team-1/members"),
        MockResponse::json(200, json!({"count": 1, "value": members})),
    );

    let output = run(
        &home,
        &server,
        &["teams", "members", "list", "Alpha", "team-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": members}),
        "the members list nests one level deeper but keeps the value envelope"
    );
    assert_eq!(request(&server).path, format!("{TEAMS}/team-1/members"));
}

#[test]
fn members_list_renders_the_module_columns() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{TEAMS}/team-1/members"),
        MockResponse::json(
            200,
            json!({"count": 1, "value": [
                {
                    "id": "member-1",
                    "identity": {"displayName": "Ada Lovelace", "uniqueName": "ada@example.com"}
                }
            ]}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["teams", "members", "list", "Alpha", "team-1"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert!(lines[0].starts_with("ID"), "header line: {stdout}");
    assert!(lines[0].contains("Display Name"), "header line: {stdout}");
    assert!(lines[0].contains("Unique Name"), "header line: {stdout}");
    assert!(
        lines[2].contains("member-1")
            && lines[2].contains("Ada Lovelace")
            && lines[2].contains("ada@example.com"),
        "the row carries identity.displayName and identity.uniqueName: {stdout}"
    );
}

#[test]
fn members_list_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{TEAMS}/team-1/members"),
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["teams", "members", "list", "Alpha", "team-1"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No members found.\n");
}
