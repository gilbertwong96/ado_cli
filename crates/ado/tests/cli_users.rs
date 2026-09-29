//! End-to-end tests for `ado users list|show|add|remove`: the organization-scoped
//! `_apis/userentitlements` surface the frozen `lib/ado_cli/cli/users.ex` builds,
//! the `--json` envelopes, the human table and detail views, and the
//! guard/error paths. `users` takes no project argument — every path is
//! org-scoped — and `remove` never prompts (R1/R5).
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
const USERS: &str = "/myorg/_apis/userentitlements";

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

/// The entitlement of `id` with the captured field set.
fn entitlement(id: &str, name: &str, email: &str, license: &str) -> Value {
    json!({
        "id": id,
        "user": {
            "descriptor": format!("aad.{name}"),
            "displayName": name,
            "principalName": email,
            "subjectKind": "user",
            "origin": "aad",
        },
        "accessLevel": {
            "accountLicenseType": license,
            "licensingSource": "account",
            "status": "active",
        },
        "lastAccessedDate": "2026-09-20T10:00:00.00Z",
        "dateCreated": "2026-01-05T08:30:00.00Z",
    })
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
fn list_is_organization_scoped_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    let users = json!([
        entitlement("user-1", "Ada Lovelace", "ada@example.com", "express"),
        entitlement("user-2", "Grace Hopper", "grace@example.com", "stakeholder"),
    ]);
    server.expect(
        "GET",
        USERS,
        MockResponse::json(200, json!({"count": 2, "value": users})),
    );

    let output = run(&home, &server, &["users", "list", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": users}),
        "the value envelope carries the unwrapped entitlements"
    );

    let request = request(&server);
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.path, USERS,
        "no project segment: every users path is org-scoped"
    );
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
        USERS,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["users", "list", "--top", "5", "--json"]);

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
        USERS,
        MockResponse::json(
            200,
            json!({"count": 1, "value": [
                entitlement("user-1", "Ada Lovelace", "ada@example.com", "express")
            ]}),
        ),
    );

    let output = run(&home, &server, &["users", "list"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert!(lines[0].starts_with("ID"), "header line: {stdout}");
    assert!(lines[0].contains("Email"), "header line: {stdout}");
    assert!(lines[0].contains("License"), "header line: {stdout}");
    assert!(
        lines[2].contains("user-1")
            && lines[2].contains("ada@example.com")
            && lines[2].contains("express"),
        "the row carries the module's three fields: {stdout}"
    );
}

#[test]
fn list_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        USERS,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["users", "list"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No users found.\n");
}

#[test]
fn list_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        USERS,
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(&home, &server, &["users", "list", "--json"]);

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
        USERS,
        MockResponse::json(
            500,
            json!({"message": "TF400898: An Internal Error Occurred."}),
        ),
    );

    let output = run(&home, &server, &["users", "list", "--json"]);

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
fn show_gets_the_user_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    let user = entitlement("user-1", "Ada Lovelace", "ada@example.com", "express");
    server.expect(
        "GET",
        &format!("{USERS}/user-1"),
        MockResponse::json(200, user.clone()),
    );

    let output = run(&home, &server, &["users", "show", "user-1", "--json"]);

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": user}));
    assert_eq!(request(&server).path, format!("{USERS}/user-1"));
}

#[test]
fn show_renders_the_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{USERS}/user-1"),
        MockResponse::json(
            200,
            entitlement("user-1", "Ada Lovelace", "ada@example.com", "express"),
        ),
    );

    let output = run(&home, &server, &["users", "show", "user-1"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\nUser Details\n\n",
            "────────────────────────────────────────────────────────────\n",
            "  ID:      user-1\n",
            "  Email:   ada@example.com\n",
            "  Name:    Ada Lovelace\n",
            "  License: express\n",
            "  Status:  active\n",
        )
    );
}

#[test]
fn show_404_reports_the_user_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{USERS}/missing-id"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(&home, &server, &["users", "show", "missing-id", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({
            "ok": false,
            "error": {
                "code": "not_found",
                "status": 404,
                "message": "User 'missing-id' not found",
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
        &format!("{USERS}/ada%40example.com"),
        MockResponse::json(
            200,
            entitlement("user-1", "Ada Lovelace", "ada@example.com", "express"),
        ),
    );

    let output = run(&home, &server, &["users", "show", "ada@example.com"]);

    assert_success(&output);
    assert_eq!(
        request(&server).path,
        format!("{USERS}/ada%40example.com"),
        "the stricter segment encoding (D22): the oracle's URI.encode leaves @ alone"
    );
}

#[test]
fn add_posts_the_express_body_by_default() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        USERS,
        MockResponse::json(
            200,
            entitlement("user-9", "Ada Lovelace", "ada@example.com", "express"),
        ),
    );

    let output = run(
        &home,
        &server,
        &["users", "add", "--email", "ada@example.com", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({
            "ok": true,
            "result": entitlement("user-9", "Ada Lovelace", "ada@example.com", "express"),
        }),
        "the created entitlement under the value envelope (D33 for the oracle's human line)"
    );

    let request = request(&server);
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, USERS);
    assert_eq!(
        body_of(&request),
        json!({
            "accessLevel": {"accountLicenseType": "express"},
            "user": {"principalName": "ada@example.com", "subjectKind": "user"},
        }),
        "the captured body: express is the default license"
    );
}

#[test]
fn add_license_overrides_the_default() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        USERS,
        MockResponse::json(
            200,
            entitlement(
                "user-10",
                "Grace Hopper",
                "grace@example.com",
                "stakeholder",
            ),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "users",
            "add",
            "--email",
            "grace@example.com",
            "--license",
            "stakeholder",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "User 'grace@example.com' added.\n",
        "the response's principalName (D33 is the --json difference)"
    );
    assert_eq!(
        body_of(&request(&server)),
        json!({
            "accessLevel": {"accountLicenseType": "stakeholder"},
            "user": {"principalName": "grace@example.com", "subjectKind": "user"},
        })
    );
}

#[test]
fn add_without_email_is_a_usage_error_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["users", "add", "--json"]);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a missing required option is loud here (D34)"
    );
    assert!(
        stderr_of(&output).contains("--email"),
        "stderr names the missing option: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty(), "nothing was sent");
}

#[test]
fn add_409_emits_the_conflict_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        USERS,
        MockResponse::json(
            409,
            json!({"message": "TF400506: The user 'ada@example.com' already exists."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["users", "add", "--email", "ada@example.com", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["code"],
        json!("conflict"),
        "the frozen taxonomy's 409 row"
    );
    assert_eq!(envelope(&output)["error"]["status"], json!(409));
}

#[test]
fn remove_sends_the_delete_without_a_prompt() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{USERS}/user-1"),
        MockResponse::json(204, json!({})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        "n\n",
        &["users", "remove", "user-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "User 'user-1' removed."}),
        "the message envelope where the oracle prints its human line under --json (D33)"
    );

    let request = request(&server);
    assert_eq!(request.method, "DELETE");
    assert_eq!(
        request.path,
        format!("{USERS}/user-1"),
        "the request proceeded: `users remove` never prompts (R1/R5)"
    );
    assert!(request.body.is_none(), "the DELETE carries no body");
    assert!(
        stderr_of(&output).is_empty(),
        "no prompt reached stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn remove_404_reports_the_user_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{USERS}/missing-id"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(&home, &server, &["users", "remove", "missing-id", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("User 'missing-id' not found")
    );
}
