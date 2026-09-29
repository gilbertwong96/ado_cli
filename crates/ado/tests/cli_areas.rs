//! End-to-end tests for `ado areas list|show|create|update|delete`: the REST
//! surface the frozen `lib/ado_cli/cli/areas.ex` builds under
//! `wit/classificationNodes/areas`, the `--json` envelopes, the human tree and
//! detail views, and the guard/error paths.
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
const AREAS: &str = "/myorg/Alpha/_apis/wit/classificationNodes/areas";

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
fn list_emits_the_value_envelope_with_the_root_node() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", AREAS, MockResponse::from_fixture("areas_root"));

    let output = run(&home, &server, &["areas", "list", "Alpha", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": fixture("areas_root")}),
        "the frozen `json_or_format` value envelope: the raw root node under result"
    );

    let request = request(&server);
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, AREAS);
    assert_eq!(
        request.query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "the absent --depth is not sent"
    );
}

#[test]
fn list_depth_sends_the_depth_query() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", AREAS, MockResponse::from_fixture("areas_root"));

    let output = run(
        &home,
        &server,
        &["areas", "list", "Alpha", "--depth", "2", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        request(&server).query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("%24depth".to_owned(), "2".to_owned()),
        ],
        "the module's `$depth`, percent-encoded on the wire"
    );
}

#[test]
fn list_renders_the_tree() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", AREAS, MockResponse::from_fixture("areas_root"));

    let output = run(&home, &server, &["areas", "list", "Alpha"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Alpha ▾\n  Team ▾\n    Feature\n  Sub\n",
        "the module's tree: two-space indent, ` ▾` exactly on nodes with children"
    );
}

#[test]
fn list_with_an_empty_child_list_renders_the_root_alone() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        AREAS,
        MockResponse::json(200, json!({"id": 10, "name": "Empty", "children": []})),
    );

    let output = run(&home, &server, &["areas", "list", "Alpha"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Empty\n",
        "an empty children array is a leaf, not a ` ▾`"
    );
}

#[test]
fn list_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        AREAS,
        MockResponse::json(404, json!({"message": "The area path was not found."})),
    );

    let output = run(&home, &server, &["areas", "list", "Alpha", "--json"]);

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
                    "body": "{\"message\":\"The area path was not found.\"}",
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
        AREAS,
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(&home, &server, &["areas", "list", "Alpha", "--json"]);

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
fn show_sends_the_encoded_area_path_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{AREAS}/Alpha%5CTeam"),
        MockResponse::from_fixture("areas_node"),
    );

    let output = run(
        &home,
        &server,
        &["areas", "show", "Alpha", "Alpha\\Team", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": fixture("areas_node")})
    );
    assert_eq!(request(&server).path, format!("{AREAS}/Alpha%5CTeam"));
}

#[test]
fn show_404_reports_the_area_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{AREAS}/Alpha%5CMissing"),
        MockResponse::json(404, json!({"message": "The area path was not found."})),
    );

    let output = run(
        &home,
        &server,
        &["areas", "show", "Alpha", "Alpha\\Missing", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({
            "ok": false,
            "error": {
                "code": "not_found",
                "status": 404,
                "message": "Area path 'Alpha\\Missing' not found",
                "details": {
                    "status": 404,
                    "body": "{\"message\":\"The area path was not found.\"}",
                },
            },
        }),
        "the module's own 404 wording, classified not_found (D4 for the oracle's stderr-only shape)"
    );
}

#[test]
fn show_renders_the_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{AREAS}/Alpha%5CTeam"),
        MockResponse::from_fixture("areas_node"),
    );

    let output = run(&home, &server, &["areas", "show", "Alpha", "Alpha\\Team"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\nArea Path Details\n\n",
            "------------------------------------------------------------\n",
            "  ID:        2\n",
            "  Name:      Team\n",
            "  Path:      \\Alpha\\Team\n",
            "  Structure: hierarchy\n",
            "  URL:       https://dev.azure.com/ado-harness/Alpha/_apis/wit/classificationNodes/Areas/Team\n",
        )
    );
}

#[test]
fn show_falls_back_to_hierarchy_and_omits_an_absent_url() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{AREAS}/Alpha%5CTeam"),
        MockResponse::json(
            200,
            json!({"id": 2, "name": "Team", "path": "\\Alpha\\Team"}),
        ),
    );

    let output = run(&home, &server, &["areas", "show", "Alpha", "Alpha\\Team"]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(
        stdout.contains("  Structure: hierarchy\n"),
        "the module's `structureType || \"hierarchy\"`: {stdout}"
    );
    assert!(
        !stdout.contains("  URL:"),
        "an absent url prints no line: {stdout}"
    );
}

#[test]
fn create_posts_the_name_body_at_the_root() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", AREAS, MockResponse::from_fixture("areas_node"));

    let output = run(
        &home,
        &server,
        &["areas", "create", "Alpha", "--name", "Team", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": fixture("areas_node")}),
        "the created node under the value envelope"
    );

    let request = request(&server);
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, AREAS);
    assert_eq!(body_of(&request), json!({"name": "Team"}));
}

#[test]
fn create_with_parent_nests_under_the_encoded_parent_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        &format!("{AREAS}/Alpha%5CTeam"),
        MockResponse::from_fixture("areas_node"),
    );

    let output = run(
        &home,
        &server,
        &[
            "areas",
            "create",
            "Alpha",
            "--name",
            "Team",
            "--parent",
            "Alpha\\Team",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Area 'Team' created (ID: 2).\n",
        "the module's human success line (D33 is the --json difference)"
    );

    let request = request(&server);
    assert_eq!(request.path, format!("{AREAS}/Alpha%5CTeam"));
    assert_eq!(body_of(&request), json!({"name": "Team"}));
}

#[test]
fn create_encodes_a_slash_in_the_parent_rather_than_letting_it_split_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        &format!("{AREAS}/Alpha%2FTeam"),
        MockResponse::from_fixture("areas_node"),
    );

    let output = run(
        &home,
        &server,
        &[
            "areas",
            "create",
            "Alpha",
            "--name",
            "Team",
            "--parent",
            "Alpha/Team",
        ],
    );

    assert_success(&output);
    assert_eq!(
        request(&server).path,
        format!("{AREAS}/Alpha%2FTeam"),
        "the stricter segment encoding (D22): the oracle's URI.encode leaves / alone"
    );
}

#[test]
fn create_without_name_is_a_usage_error_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["areas", "create", "Alpha", "--json"]);

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
fn update_patches_the_new_name() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &format!("{AREAS}/Alpha%5CTeam"),
        MockResponse::from_fixture("areas_node"),
    );

    let output = run(
        &home,
        &server,
        &[
            "areas",
            "update",
            "Alpha",
            "Alpha\\Team",
            "--name",
            "Team",
            "--json",
        ],
    );

    assert_success(&output);
    let request = request(&server);

    assert_eq!(request.method, "PATCH");
    assert_eq!(request.path, format!("{AREAS}/Alpha%5CTeam"));
    assert_eq!(body_of(&request), json!({"name": "Team"}));
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": fixture("areas_node")})
    );
}

#[test]
fn update_without_name_is_a_usage_error_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["areas", "update", "Alpha", "Alpha\\Team", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("--name"));
    assert!(server.received().is_empty());
}

#[test]
fn delete_sends_the_delete_without_a_prompt() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{AREAS}/Alpha%5COld"),
        MockResponse::json(204, json!({})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        "n\n",
        &["areas", "delete", "Alpha", "Alpha\\Old", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Area 'Alpha\\Old' deleted."}),
        "the message envelope where the oracle prints its human line under --json (D33)"
    );

    let request = request(&server);
    assert_eq!(request.method, "DELETE");
    assert_eq!(
        request.path,
        format!("{AREAS}/Alpha%5COld"),
        "the request proceeded: `areas delete` never prompts (R1)"
    );
    assert!(request.body.is_none(), "the DELETE carries no body");
}

#[test]
fn delete_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{AREAS}/Alpha%5CMissing"),
        MockResponse::json(404, json!({"message": "The area path was not found."})),
    );

    let output = run(
        &home,
        &server,
        &["areas", "delete", "Alpha", "Alpha\\Missing", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(envelope(&output)["error"]["code"], json!("not_found"));
}
