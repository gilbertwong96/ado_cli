//! End-to-end tests for `ado branch-policies list|show|create|update|delete` —
//! the schema's `ado repos policies` group — against the REST surface the frozen
//! `lib/ado_cli/cli/branch_policies.ex` builds under `_apis/policy/configurations`.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a mock
//! server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT` credentials, so no
//! credential resolution reaches the developer's keychain.

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";
const CONFIGURATIONS: &str = "/myorg/Alpha/_apis/policy/configurations";

const BUILD_TYPE: &str = "fa4e907d-c16b-4a4c-9dfa-4906e5d171dd";
const REVIEWERS_TYPE: &str = "fd2167ab-9d2a-4d8b-b2c9-1cdfbb6d4c34";

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

/// Runs with `bytes` on stdin, so a command that prompted cannot proceed on the
/// developer's terminal (R8) and the proceed/refuse contract is observable.
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

fn received(server: &MockServer) -> Vec<RecordedRequest> {
    server.received()
}

fn request(server: &MockServer) -> RecordedRequest {
    let received = received(server);

    assert_eq!(received.len(), 1, "expected exactly one request");
    received[0].clone()
}

fn body_of(request: &RecordedRequest) -> Value {
    serde_json::from_str(request.body.as_deref().expect("the request carried a body"))
        .expect("the body is JSON")
}

/// The captured build-validation policy: type-specific settings, blocking off.
fn build_policy() -> Value {
    json!({
        "id": 42,
        "type": {"id": BUILD_TYPE, "displayName": "Build"},
        "isBlocking": false,
        "isEnabled": true,
        "createdDate": "2026-09-01T10:00:00.000Z",
        "settings": {
            "scope": [{"repositoryId": "Alpha.Core", "refName": "refs/heads/main", "matchKind": "Exact"}],
            "buildDefinitionId": 12,
            "queueId": 14,
            "validations": [{"displayName": "Build validation", "enabled": true}]
        }
    })
}

/// The captured required-reviewers policy: a second type with its own settings
/// shape, so a body that generalised from one type cannot pass both.
fn reviewers_policy() -> Value {
    json!({
        "id": 43,
        "type": {"id": REVIEWERS_TYPE, "displayName": "Required reviewers"},
        "isBlocking": true,
        "isEnabled": true,
        "createdDate": "2026-09-02T11:30:00.000Z",
        "settings": {
            "scope": [{"repositoryId": "Alpha.Core", "refName": "refs/heads/main", "matchKind": "Exact"}],
            "reviewerCount": 2,
            "creatorVoteCounts": false,
            "resetOnSourcePush": true
        }
    })
}

#[test]
fn list_sends_the_repository_id_pair_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        CONFIGURATIONS,
        MockResponse::json(
            200,
            json!({"count": 2, "value": [build_policy(), reviewers_policy()]}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["branch-policies", "list", "Alpha", "Alpha.Core", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": [build_policy(), reviewers_policy()]}),
        "the frozen `json_or_format` value envelope: the unwrapped value array"
    );

    let request = request(&server);
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, CONFIGURATIONS);
    assert_eq!(
        request.query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("repositoryId".to_owned(), "Alpha.Core".to_owned()),
        ],
        "the repositoryId is a query pair here (D25: the oracle glues it into the path)"
    );
}

#[test]
fn list_branch_adds_the_branch_pair() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        CONFIGURATIONS,
        MockResponse::json(
            200,
            json!({"count": 2, "value": [build_policy(), reviewers_policy()]}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "list",
            "Alpha",
            "Alpha.Core",
            "--branch",
            "main",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        request(&server).query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("repositoryId".to_owned(), "Alpha.Core".to_owned()),
            ("branch".to_owned(), "main".to_owned()),
        ],
        "the module's branch param is the option's value, verbatim"
    );
}

#[test]
fn list_renders_the_module_columns() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        CONFIGURATIONS,
        MockResponse::json(
            200,
            json!({"count": 2, "value": [build_policy(), reviewers_policy()]}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["branch-policies", "list", "Alpha", "Alpha.Core"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert!(
        lines[0].starts_with("ID") && lines[0].contains("Type") && lines[0].contains("Branch"),
        "header line: {stdout}"
    );
    assert!(lines[0].contains("Blocking") && lines[0].contains("Enabled"));
    assert!(
        lines[2].contains("42")
            && lines[2].contains("Build")
            && lines[2].contains("refs/heads/main"),
        "the first row carries the module's fields: {stdout}"
    );
    assert!(
        lines[2].contains("false") && lines[2].contains("true"),
        "the last two columns are the policy's booleans: {stdout}"
    );
}

#[test]
fn list_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        CONFIGURATIONS,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["branch-policies", "list", "Alpha", "Alpha.Core"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No policies found.\n");
}

#[test]
fn list_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        CONFIGURATIONS,
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["branch-policies", "list", "Alpha", "Alpha.Core", "--json"],
    );

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
}

#[test]
fn list_500_emits_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        CONFIGURATIONS,
        MockResponse::json(
            500,
            json!({"message": "TF400898: An Internal Error Occurred."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["branch-policies", "list", "Alpha", "Alpha.Core", "--json"],
    );

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
fn show_gets_the_policy_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/42"),
        MockResponse::json(200, build_policy()),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "show",
            "Alpha",
            "Alpha.Core",
            "42",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": build_policy()})
    );

    let request = request(&server);
    assert_eq!(request.path, format!("{CONFIGURATIONS}/42"));
    assert_eq!(
        request.query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())]
    );
}

#[test]
fn show_renders_the_module_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/42"),
        MockResponse::json(200, build_policy()),
    );

    let output = run(
        &home,
        &server,
        &["branch-policies", "show", "Alpha", "Alpha.Core", "42"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\nPolicy Details\n\n",
            "------------------------------------------------------------\n",
            "  ID:        42\n",
            "  Type:      Build\n",
            "  Branch:    refs/heads/main\n",
            "  Repository:Alpha.Core\n",
            "  Blocking:  false\n",
            "  Enabled:   true\n",
            "  Created:   2026-09-01T10:00:00.000Z\n",
        )
    );
}

#[test]
fn show_prints_the_none_fallbacks_and_the_empty_type() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/52"),
        MockResponse::json(
            200,
            json!({"id": 52, "isBlocking": false, "isEnabled": false, "createdDate": null}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["branch-policies", "show", "Alpha", "Alpha.Core", "52"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(
        stdout.contains("  Type:      \n"),
        "an absent type interpolates as the empty string, not (none): {stdout}"
    );
    assert!(
        stdout.contains("  Branch:    (none)\n"),
        "the module's `scope[\"refName\"] || \"(none)\"`: {stdout}"
    );
    assert!(
        stdout.contains("  Repository:(none)\n"),
        "the module's `scope[\"repositoryId\"] || \"(none)\"`: {stdout}"
    );
    assert!(
        stdout.contains("  Created:   \n"),
        "an absent createdDate interpolates as the empty string: {stdout}"
    );
}

#[test]
fn show_404_reports_the_module_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/999"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "show",
            "Alpha",
            "Alpha.Core",
            "999",
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
                "message": "Policy #999 not found",
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
fn show_rejects_a_non_integer_id_before_any_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "show",
            "Alpha",
            "Alpha.Core",
            "not-an-integer",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("POLICY_ID"),
        "clap's message names the positional: {}",
        stderr_of(&output)
    );
    assert!(received(&server).is_empty(), "a usage error sends nothing");
}

#[test]
fn create_posts_the_selected_type_and_scope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        CONFIGURATIONS,
        MockResponse::json(200, json!({"id": 50})),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "create",
            "Alpha",
            "Alpha.Core",
            "--type",
            BUILD_TYPE,
            "--branch",
            "refs/heads/main",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server)),
        json!({
            "type": {"id": BUILD_TYPE},
            "isBlocking": true,
            "isEnabled": true,
            "settings": {
                "scope": [{
                    "repositoryId": "Alpha.Core",
                    "refName": "refs/heads/main",
                    "matchKind": "Exact"
                }]
            }
        }),
        "the captured build-validation body: the default blocking is true"
    );
}

#[test]
fn create_no_blocking_sends_false() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        CONFIGURATIONS,
        MockResponse::json(200, json!({"id": 51})),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "create",
            "Alpha",
            "Alpha.Core",
            "--type",
            REVIEWERS_TYPE,
            "--branch",
            "refs/heads/release/2.0",
            "--no-blocking",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server)),
        json!({
            "type": {"id": REVIEWERS_TYPE},
            "isBlocking": false,
            "isEnabled": true,
            "settings": {
                "scope": [{
                    "repositoryId": "Alpha.Core",
                    "refName": "refs/heads/release/2.0",
                    "matchKind": "Exact"
                }]
            }
        }),
        "the captured second-type body: --no-blocking is the only false spelling"
    );
}

#[test]
fn create_takes_the_last_of_the_two_blocking_spellings() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        CONFIGURATIONS,
        MockResponse::json(200, json!({"id": 55})),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "create",
            "Alpha",
            "Alpha.Core",
            "--type",
            BUILD_TYPE,
            "--branch",
            "refs/heads/main",
            "--blocking",
            "--no-blocking",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server))["isBlocking"],
        json!(false),
        "the oracle's Map.new over OptionParser's list: the last spelling wins"
    );

    let server = MockServer::start();
    server.expect(
        "POST",
        CONFIGURATIONS,
        MockResponse::json(200, json!({"id": 56})),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "create",
            "Alpha",
            "Alpha.Core",
            "--type",
            BUILD_TYPE,
            "--branch",
            "refs/heads/main",
            "--no-blocking",
            "--blocking",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server))["isBlocking"],
        json!(true),
        "the reversed order wins the other way"
    );
}

#[test]
fn create_sends_the_branch_verbatim() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        CONFIGURATIONS,
        MockResponse::json(200, json!({"id": 53})),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "create",
            "Alpha",
            "Alpha.Core",
            "--type",
            BUILD_TYPE,
            "--branch",
            "refs/heads/feature/*",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&request(&server))["settings"]["scope"][0]["refName"],
        json!("refs/heads/feature/*"),
        "the wildcard branch is not rewritten and the refs/heads prefix is not added"
    );
}

#[test]
fn create_prints_the_created_id_in_both_modes() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        CONFIGURATIONS,
        MockResponse::json(200, json!({"id": 50})),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "create",
            "Alpha",
            "Alpha.Core",
            "--type",
            BUILD_TYPE,
            "--branch",
            "refs/heads/main",
        ],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Policy #50 created.\n");
}

#[test]
fn create_without_type_is_a_usage_error_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "create",
            "Alpha",
            "Alpha.Core",
            "--branch",
            "refs/heads/main",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--type"),
        "clap names the missing required option (D34's loud half): {}",
        stderr_of(&output)
    );
    assert!(
        received(&server).is_empty(),
        "the oracle's silent exit 0 sends nothing; ours must send nothing too"
    );
}

#[test]
fn create_without_branch_is_a_usage_error_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "create",
            "Alpha",
            "Alpha.Core",
            "--type",
            BUILD_TYPE,
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--branch"),
        "clap names the missing required option (D34's loud half): {}",
        stderr_of(&output)
    );
    assert!(received(&server).is_empty(), "a usage error sends nothing");
}

#[test]
fn create_409_emits_the_conflict_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        CONFIGURATIONS,
        MockResponse::json(
            409,
            json!({"message": "TF402582: The policy configuration already exists."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "create",
            "Alpha",
            "Alpha.Core",
            "--type",
            BUILD_TYPE,
            "--branch",
            "refs/heads/main",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("conflict"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Conflict — the resource already exists or is in an invalid state.")
    );
}

#[test]
fn update_gets_then_puts_the_preserved_type_and_settings() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/44"),
        MockResponse::json(200, reviewers_policy()),
    );
    server.expect(
        "PUT",
        &format!("{CONFIGURATIONS}/44"),
        MockResponse::json(200, reviewers_policy()),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "update",
            "Alpha",
            "Alpha.Core",
            "44",
            "--blocking",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output)["result"],
        reviewers_policy(),
        "the answered policy is the value envelope (D33)"
    );

    let received = received(&server);
    assert_eq!(received.len(), 2, "update reads first, then writes");
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[1].method, "PUT");
    assert_eq!(
        body_of(&received[1]),
        json!({
            "type": {"id": REVIEWERS_TYPE, "displayName": "Required reviewers"},
            "isBlocking": true,
            "isEnabled": true,
            "settings": {
                "scope": [{"repositoryId": "Alpha.Core", "refName": "refs/heads/main", "matchKind": "Exact"}],
                "reviewerCount": 2,
                "creatorVoteCounts": false,
                "resetOnSourcePush": true
            }
        }),
        "the whole type object and the type-specific settings are preserved verbatim"
    );
}

#[test]
fn update_no_options_preserves_both_flags() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/47"),
        MockResponse::json(200, build_policy()),
    );
    server.expect(
        "PUT",
        &format!("{CONFIGURATIONS}/47"),
        MockResponse::json(200, build_policy()),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "update",
            "Alpha",
            "Alpha.Core",
            "47",
            "--json",
        ],
    );

    assert_success(&output);
    let body = body_of(&received(&server)[1]);

    assert_eq!(body["isBlocking"], json!(false));
    assert_eq!(body["isEnabled"], json!(true));
}

#[test]
fn update_no_enabled_sends_false_and_keeps_blocking() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/45"),
        MockResponse::json(200, reviewers_policy()),
    );
    server.expect(
        "PUT",
        &format!("{CONFIGURATIONS}/45"),
        MockResponse::json(200, reviewers_policy()),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "update",
            "Alpha",
            "Alpha.Core",
            "45",
            "--no-enabled",
            "--json",
        ],
    );

    assert_success(&output);
    let body = body_of(&received(&server)[1]);

    assert_eq!(body["isEnabled"], json!(false));
    assert_eq!(
        body["isBlocking"],
        json!(true),
        "an absent --blocking keeps the existing value"
    );
}

#[test]
fn update_preserves_a_nil_type_and_nil_flags() {
    let home = TempHome::new();
    let server = MockServer::start();
    let bare = json!({"id": 54, "settings": {"scope": [], "reviewerCount": 1}});
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/54"),
        MockResponse::json(200, bare.clone()),
    );
    server.expect(
        "PUT",
        &format!("{CONFIGURATIONS}/54"),
        MockResponse::json(200, bare),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "update",
            "Alpha",
            "Alpha.Core",
            "54",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        body_of(&received(&server)[1]),
        json!({
            "type": null,
            "isBlocking": null,
            "isEnabled": null,
            "settings": {"scope": [], "reviewerCount": 1}
        }),
        "the captured bare-policy body: absent fields are JSON null, never invented"
    );
}

#[test]
fn update_get_404_reports_the_module_wording_and_puts_nothing() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/998"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "update",
            "Alpha",
            "Alpha.Core",
            "998",
            "--blocking",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Policy #998 not found"),
        "the module's own wording (D4)"
    );
    assert_eq!(
        received(&server).len(),
        1,
        "a failed read never reaches the PUT"
    );
}

#[test]
fn update_put_404_emits_the_classified_envelope_with_the_raw_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/48"),
        MockResponse::json(200, build_policy()),
    );
    server.expect(
        "PUT",
        &format!("{CONFIGURATIONS}/48"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "update",
            "Alpha",
            "Alpha.Core",
            "48",
            "--no-enabled",
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
                "message": "Resource not found. Check the project/repo/build ID and your permissions.",
                "details": {
                    "status": 404,
                    "body": "{\"message\":\"TF400813: The user is not authorized.\"}",
                },
            },
        }),
        "the PUT error keeps the classified envelope with the raw body (D24)"
    );
}

#[test]
fn update_put_409_emits_the_conflict_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/49"),
        MockResponse::json(200, build_policy()),
    );
    server.expect(
        "PUT",
        &format!("{CONFIGURATIONS}/49"),
        MockResponse::json(
            409,
            json!({"message": "TF402582: The policy configuration already exists."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "update",
            "Alpha",
            "Alpha.Core",
            "49",
            "--no-blocking",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(envelope(&output)["error"]["code"], json!("conflict"));
}

#[test]
fn update_prints_the_policy_line_in_both_modes() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{CONFIGURATIONS}/44"),
        MockResponse::json(200, build_policy()),
    );
    server.expect(
        "PUT",
        &format!("{CONFIGURATIONS}/44"),
        MockResponse::json(200, build_policy()),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "update",
            "Alpha",
            "Alpha.Core",
            "44",
            "--blocking",
        ],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Policy #44 updated.\n");
}

#[test]
fn delete_sends_the_delete_without_a_prompt() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{CONFIGURATIONS}/42"),
        MockResponse::json(200, json!({})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        "n\n",
        &[
            "branch-policies",
            "delete",
            "Alpha",
            "Alpha.Core",
            "42",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Policy #42 deleted."}),
        "the message envelope (D33)"
    );
    assert_eq!(
        request(&server).method,
        "DELETE",
        "the frozen delete never prompts (R1/R5): the request goes out even on n"
    );
    assert!(
        !stdout_of(&output).contains("[y/N]") && !stderr_of(&output).contains("[y/N]"),
        "no prompt on either path"
    );
}

#[test]
fn delete_prints_the_policy_line_in_both_modes() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{CONFIGURATIONS}/42"),
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &["branch-policies", "delete", "Alpha", "Alpha.Core", "42"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Policy #42 deleted.\n");
}

#[test]
fn delete_404_emits_the_raw_body_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{CONFIGURATIONS}/999"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "delete",
            "Alpha",
            "Alpha.Core",
            "999",
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
                "message": "Resource not found. Check the project/repo/build ID and your permissions.",
                "details": {
                    "status": 404,
                    "body": "{\"message\":\"TF400813: The user is not authorized.\"}",
                },
            },
        }),
        "the frozen delete passes the undecoded body on, so this envelope matches the oracle's"
    );
}

#[test]
fn delete_rejects_a_non_integer_id_before_any_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "branch-policies",
            "delete",
            "Alpha",
            "Alpha.Core",
            "not-an-integer",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("POLICY_ID"),
        "clap's message names the positional: {}",
        stderr_of(&output)
    );
    assert!(received(&server).is_empty(), "a usage error sends nothing");
}
