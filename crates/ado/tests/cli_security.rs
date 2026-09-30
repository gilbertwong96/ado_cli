//! End-to-end tests for `ado security grant|revoke`: the typed-flag guard, the
//! project-name → id lookup (and the UUID short-circuit), the caller-descriptor
//! fetch with the MSA and missing-descriptor refusals, and the
//! `_apis/accesscontrolentries` write with the module's own 400/401/403 message.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a
//! mock server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT` credentials,
//! so no credential resolution reaches the developer's keychain. Every run
//! scripts stdin explicitly, so no test can read a terminal (the guard is a flag,
//! not a prompt — nothing here reads stdin at all).

use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";

/// The Library security namespace GUID the frozen module posts to.
const NAMESPACE: &str = "b7e84409-6553-448a-bbb2-af228e07cbeb";

/// `projects_list.json`'s Alpha id.
const ALPHA_ID: &str = "6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c";

/// A project argument the UUID regex accepts: no lookup request at all.
const UUID: &str = "11111111-2222-3333-4444-555555555555";

/// The captured `aad.` descriptor.
const DESCRIPTOR: &str = "aad.QwErTyUiOpAsDfGhJkLzXcVbNm";

/// The verbose flag's spelling, long enough to type that it is never an accident.
const FLAG: &str = "--yes-this-mutates-secret-read";

/// The captured guard refusal (`sec-grant-guard-json`/`-human`).
const REFUSAL: &str = "Refusing to run without the safety flag. Re-run with --yes-this-mutates-secret-read to confirm.";

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
    command(home, server, args)
        .stdin(Stdio::null())
        .output()
        .expect("run ado")
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

fn projects_path() -> String {
    format!("/{ORG}/_apis/projects")
}

fn descriptor_path() -> String {
    format!("/{ORG}/_apis/connectionData")
}

fn acl_path() -> String {
    format!("/{ORG}/_apis/accesscontrolentries/{NAMESPACE}")
}

/// The captured caller: the `aad.` descriptor `fetch_caller_descriptor/0` accepts.
fn caller() -> Value {
    json!({
        "authenticatedUser": {
            "id": "c1d2e3f4-0002-0002-0002-000000000002",
            "displayName": "Ada Example",
            "uniqueName": "ada@example.test",
            "subjectDescriptor": DESCRIPTOR
        }
    })
}

/// The ACL response the frozen module reads as `{:ok, _}`; its body is discarded.
fn acl_response() -> MockResponse {
    MockResponse::json(
        200,
        json!({"count": 1, "value": [{"descriptor": DESCRIPTOR, "allow": 8, "deny": 0}]}),
    )
}

/// The three-request success chain a name project walks: the lookup, the
/// descriptor, the write.
fn expect_chain(server: &MockServer) {
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::from_fixture("projects_list"),
    );
    server.expect("GET", &descriptor_path(), MockResponse::json(200, caller()));
    server.expect("POST", &acl_path(), acl_response());
}

/// The two-request chain a UUID project walks (no lookup).
fn expect_uuid_chain(server: &MockServer) {
    server.expect("GET", &descriptor_path(), MockResponse::json(200, caller()));
    server.expect("POST", &acl_path(), acl_response());
}

fn body_of(request: &RecordedRequest) -> Value {
    serde_json::from_str(request.body.as_deref().expect("the request carries a body"))
        .expect("the body is JSON")
}

fn usage_error(output: &Output, names: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(output)
    );
    assert!(
        stderr_of(output).contains(names),
        "the usage error names {names}: {}",
        stderr_of(output)
    );
}

// ── grant ───────────────────────────────────────────────────────────────

#[test]
fn grant_walks_the_three_request_chain_and_emits_the_message_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_chain(&server);

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": format!(
            "Granted 'ViewSecrets' on Library namespace for project 'Alpha' ({ALPHA_ID}) to \
             the calling user. You can now download Secure Files without elevation. Revoke \
             later with 'ado security revoke --project Alpha --permission ViewSecrets \
             --yes-this-mutates-secret-read'."
        )}),
        "the module's sentence under the message envelope (D33)"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 3, "lookup, descriptor, write");
    assert_eq!(requests[0].path, projects_path());
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "the frozen lookup's version"
    );
    assert_eq!(requests[1].path, descriptor_path());
    assert_eq!(
        requests[1].query_pairs(),
        vec![("api-version".to_owned(), "7.1-preview.1".to_owned())],
        "the frozen descriptor fetch's preview version"
    );
    assert_eq!(requests[2].path, acl_path());
    assert_eq!(
        requests[2].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())]
    );
}

#[test]
fn grant_sends_the_modules_acl_body_with_the_resolved_id() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_chain(&server);

    run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_eq!(
        body_of(&requests(&server)[2]),
        json!({
            "token": ALPHA_ID,
            "merge": true,
            "accessControlEntries": [{
                "descriptor": DESCRIPTOR,
                "allow": 8,
                "deny": 0,
                "extendedInfo": {}
            }]
        }),
        "the captured grant body: the resolved id as the token, merge true, the ViewSecrets bit"
    );
}

#[test]
fn grant_uses_a_uuid_project_without_the_lookup() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_uuid_chain(&server);

    let output = run(&home, &server, &["security", "grant", UUID, FLAG, "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output)["message"],
        json!(format!(
            "Granted 'ViewSecrets' on Library namespace for project '{UUID}' ({UUID}) to \
             the calling user. You can now download Secure Files without elevation. Revoke \
             later with 'ado security revoke --project {UUID} --permission ViewSecrets \
             --yes-this-mutates-secret-read'."
        )),
        "the argument and the id are the same UUID"
    );

    let requests = requests(&server);
    assert_eq!(
        requests.len(),
        2,
        "a UUID skips the projects lookup: descriptor, write"
    );
    assert_eq!(requests[0].path, descriptor_path());
    assert_eq!(requests[1].path, acl_path());
    assert_eq!(body_of(&requests[1])["token"], json!(UUID));
}

/// A non-UUID argument is looked up: 36 hex characters without the 8-4-4-4-12
/// dashes does not satisfy the frozen regex.
#[test]
fn grant_looks_up_a_hex_argument_that_is_not_a_uuid() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::from_fixture("projects_list"),
    );

    let candidate = "6a1f8f6e2b8d4b9e9d2a1c3f5e7a9b0c".to_owned();
    let output = run(
        &home,
        &server,
        &["security", "grant", &candidate, FLAG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(requests(&server).len(), 1, "the lookup ran; nothing else");
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!(format!("Project '{candidate}' not found."))
    );
}

/// A project argument that starts with a dash is an option to the frozen
/// parser, which refuses the run with help on stdout; this build's clap refuses
/// it with its message on stderr alone (D5's class), so neither sends a request.
#[test]
fn a_dash_leading_project_is_a_usage_error_on_both_sides() {
    let home = TempHome::new();
    let server = MockServer::start();

    for project in ["-Alpha", "-----"] {
        let output = run(
            &home,
            &server,
            &["security", "grant", project, FLAG, "--json"],
        );

        assert_eq!(output.status.code(), Some(1), "{project}");
        assert!(stdout_of(&output).is_empty(), "{project}");
        assert!(
            stderr_of(&output).contains("unexpected argument"),
            "{project}: {}",
            stderr_of(&output)
        );
    }

    assert!(requests(&server).is_empty());
}

/// `-1` is a *positional* to the frozen OptionParser (its negative-number rule),
/// which looks the name up; clap reads it as an option and refuses (captured).
#[test]
fn a_negative_number_project_is_a_usage_error_here() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["security", "grant", "-1", FLAG, "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_of(&output).is_empty());
    assert!(
        stderr_of(&output).contains("-1"),
        "the argument is named: {}",
        stderr_of(&output)
    );
    assert!(requests(&server).is_empty());
}

/// `--` is the escape both parsers honour: the dash-leading project becomes the
/// positional, and the guard refuses for want of the flag on both sides.
#[test]
fn the_double_dash_escape_passes_a_dash_leading_project() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["security", "grant", "--", "-Alpha"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_of(&output).is_empty());
    assert!(stderr_of(&output).contains(REFUSAL));
    assert!(requests(&server).is_empty());
}

/// The UUID branch accepts all-lowercase and all-uppercase hex (the frozen regex
/// is case-insensitive) and passes the argument through verbatim.
#[test]
fn grant_takes_an_uppercase_uuid_verbatim() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_uuid_chain(&server);

    let uppercase = "11111111-2222-3333-4444-55555555555A";
    let output = run(
        &home,
        &server,
        &["security", "grant", uppercase, FLAG, "--json"],
    );

    assert_success(&output);
    assert_eq!(body_of(&requests(&server)[1])["token"], json!(uppercase));
}

#[test]
fn grant_prints_the_modules_sentence_in_human_mode() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_chain(&server);

    let output = run(&home, &server, &["security", "grant", "Alpha", FLAG]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "Granted 'ViewSecrets' on Library namespace for project 'Alpha' ({ALPHA_ID}) to \
             the calling user. You can now download Secure Files without elevation. Revoke \
             later with 'ado security revoke --project Alpha --permission ViewSecrets \
             --yes-this-mutates-secret-read'.\n"
        ),
        "the frozen prints the sentence in both modes (D33)"
    );
}

// ── revoke ──────────────────────────────────────────────────────────────

#[test]
fn revoke_sends_the_inverted_body_and_its_own_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_chain(&server);

    let output = run(
        &home,
        &server,
        &["security", "revoke", "Alpha", FLAG, "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": format!(
            "Revoked 'ViewSecrets' on Library namespace for project 'Alpha' ({ALPHA_ID}) \
             for the calling user."
        )})
    );
    assert_eq!(
        body_of(&requests(&server)[2]),
        json!({
            "token": ALPHA_ID,
            "merge": false,
            "accessControlEntries": [{
                "descriptor": DESCRIPTOR,
                "allow": 0,
                "deny": 0,
                "extendedInfo": {}
            }]
        }),
        "the captured revoke body: merge false and the bit off"
    );
}

// ── the guard (D32's shape) ─────────────────────────────────────────────

#[test]
fn grant_refuses_without_the_safety_flag_on_stderr_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["security", "grant", "Alpha"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "the refusal is not a document: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains(REFUSAL),
        "the module's refusal wording: {}",
        stderr_of(&output)
    );
    assert!(requests(&server).is_empty(), "nothing was sent");
}

#[test]
fn revoke_refuses_without_the_safety_flag_on_stderr_with_no_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["security", "revoke", "Alpha"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_of(&output).is_empty());
    assert!(stderr_of(&output).contains(REFUSAL));
    assert!(requests(&server).is_empty());
}

/// Under `--json` the refusal is still not a document (D32), where the oracle also
/// leaves stdout empty and exits 1.
#[test]
fn the_refusal_emits_no_envelope_under_json() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["security", "grant", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_of(&output).is_empty(), "{}", stdout_of(&output));
}

/// The guard is checked before the project and the permission, as
/// `validate_inputs/3`'s `cond` orders them.
#[test]
fn the_guard_beats_a_bad_project_and_a_bad_permission() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["security", "grant", "", "--permission", "Other"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains(REFUSAL));
    assert!(requests(&server).is_empty());
}

// ── --permission ────────────────────────────────────────────────────────

#[test]
fn an_unsupported_permission_is_refused_before_any_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "security",
            "grant",
            "Alpha",
            FLAG,
            "--permission",
            "ViewLibrary",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({"ok": false, "error": {
            "code": "validation_error",
            "message": "Unsupported permission 'ViewLibrary'. Currently only 'ViewSecrets' \
                is supported. Patches welcome at https://github.com/gilbertwong96/ado_cli."
        }}),
        "the frozen wording, under this build's error envelope (D4)"
    );
    assert!(requests(&server).is_empty());
}

#[test]
fn the_permission_defaults_to_view_secrets() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_chain(&server);

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_success(&output);
    assert!(
        envelope(&output)["message"]
            .as_str()
            .expect("a message")
            .starts_with("Granted 'ViewSecrets'"),
        "the absent option takes the module's default"
    );
}

#[test]
fn a_present_empty_permission_is_refused_like_any_other() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "security",
            "grant",
            "Alpha",
            FLAG,
            "--permission",
            "",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        envelope(&output)["error"]["message"]
            .as_str()
            .expect("a message")
            .starts_with("Unsupported permission ''."),
        "{}",
        stdout_of(&output)
    );
    assert!(requests(&server).is_empty());
}

// ── the project lookup's failures ───────────────────────────────────────

#[test]
fn a_project_the_list_does_not_name_is_not_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::from_fixture("projects_list"),
    );

    let output = run(
        &home,
        &server,
        &["security", "grant", "Nope", FLAG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({"ok": false, "error": {
            "code": "not_found",
            "message": "Project 'Nope' not found."
        }}),
        "the frozen wording; this build's envelope (D4)"
    );
    assert_eq!(requests(&server).len(), 1, "the lookup ran");
}

#[test]
fn an_empty_project_argument_is_refused_before_any_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["security", "grant", "", FLAG, "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Project argument is required."),
        "the frozen check runs before the lookup"
    );
    assert!(requests(&server).is_empty());
}

/// The module's two `#{inspect(err)}` sites keep their step name; the body's
/// `message` member replaces Elixir's tuple rendering (D24's class).
#[test]
fn a_failed_projects_lookup_keeps_the_modules_step_name() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Failed to look up project ID: TF400813: The server is unavailable.")
    );
    assert_eq!(envelope["error"]["details"]["status"], json!(500));
    assert_eq!(
        envelope["error"]["details"]["body"],
        json!("{\"message\":\"TF400813: The server is unavailable.\"}"),
        "D24: the raw upstream bytes, not the decoded map"
    );
}

#[test]
fn a_projects_body_without_a_value_array_is_a_loud_failure() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::json(200, json!({"count": 0})),
    );

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Failed to look up project ID: the response has no 'value' array.")
    );
    assert_eq!(requests(&server).len(), 1);
}

/// The frozen `%{"id" => id}` clause cannot match an entry without an `id`, and
/// the `CaseClauseError` is swallowed into a silent exit 0 (D34, captured
/// `sec-grant-proj-noid`). This build fails loudly, after the one lookup.
#[test]
fn a_matching_entry_without_an_id_is_a_loud_lookup_failure() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::json(200, json!({"count": 1, "value": [{"name": "Alpha"}]})),
    );

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "where the oracle exits 0 silently (D34)"
    );
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Failed to look up project ID: the entry for 'Alpha' has no id.")
    );
    assert_eq!(requests(&server).len(), 1, "no write was attempted");
}

/// A present `id` wins whatever it holds — the frozen `%{"id" => id}` binds the
/// key, not a string — and this build renders it the way `#{id}` interpolates.
#[test]
fn a_null_id_is_used_as_the_token() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::json(
            200,
            json!({"count": 1, "value": [{"name": "Alpha", "id": null}]}),
        ),
    );
    server.expect("GET", &descriptor_path(), MockResponse::json(200, caller()));
    server.expect("POST", &acl_path(), acl_response());

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_success(&output);
    assert_eq!(body_of(&requests(&server)[2])["token"], json!(null));
    assert!(
        envelope(&output)["message"]
            .as_str()
            .expect("a message")
            .starts_with("Granted 'ViewSecrets' on Library namespace for project 'Alpha' ()"),
        "`#{{nil}}` interpolates empty: {}",
        stdout_of(&output)
    );
}

// ── the caller descriptor ───────────────────────────────────────────────

#[test]
fn an_msa_descriptor_is_refused_with_the_modules_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::from_fixture("projects_list"),
    );
    server.expect(
        "GET",
        &descriptor_path(),
        MockResponse::json(
            200,
            json!({"authenticatedUser": {"subjectDescriptor": "msa.MTIzNDU2Nzg5"}}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!(
            "Your identity is a personal Microsoft account (MSA). The Azure DevOps Security \
             API rejects 'msa.*' descriptors for permission grants, so this command cannot \
             elevate the caller. Use a work/school Entra ID (AAD) identity, or grant 'View \
             library item secrets' on the Library via the web UI."
        )
    );
    assert_eq!(requests(&server).len(), 2, "no write was attempted");
}

#[test]
fn a_missing_or_empty_descriptor_is_refused_with_the_modules_wording() {
    for body in [
        json!({"authenticatedUser": {"displayName": "Ada Example"}}),
        json!({"authenticatedUser": {"subjectDescriptor": ""}}),
        json!({"authenticatedUser": {"subjectDescriptor": 42}}),
        json!({}),
    ] {
        let home = TempHome::new();
        let server = MockServer::start();
        server.expect(
            "GET",
            &projects_path(),
            MockResponse::from_fixture("projects_list"),
        );
        server.expect(
            "GET",
            &descriptor_path(),
            MockResponse::json(200, body.clone()),
        );

        let output = run(
            &home,
            &server,
            &["security", "grant", "Alpha", FLAG, "--json"],
        );

        assert_eq!(output.status.code(), Some(1), "{body}");
        assert_eq!(
            envelope(&output)["error"]["message"],
            json!(
                "Could not determine caller identity descriptor. The /_apis/connectionData \
                 endpoint did not return a subjectDescriptor. Re-run 'ado whoami' to verify \
                 auth is healthy."
            ),
            "{body}"
        );
        assert_eq!(requests(&server).len(), 2, "{body}");
    }
}

#[test]
fn a_descriptor_body_that_is_not_an_object_takes_the_missing_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::from_fixture("projects_list"),
    );
    server.expect(
        "GET",
        &descriptor_path(),
        MockResponse::json(200, json!("nope")),
    );

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        envelope(&output)["error"]["message"]
            .as_str()
            .expect("a message")
            .starts_with("Could not determine caller identity descriptor.")
    );
}

#[test]
fn a_failed_descriptor_fetch_keeps_the_modules_step_name() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &projects_path(),
        MockResponse::from_fixture("projects_list"),
    );
    server.expect(
        "GET",
        &descriptor_path(),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Failed to fetch caller descriptor: TF400813: The server is unavailable.")
    );
    assert_eq!(requests(&server).len(), 2);
}

// ── the ACL branch ──────────────────────────────────────────────────────

/// The module's own message, with the raw response where the oracle renders the
/// decoded map with `inspect/2` (D24's class).
#[test]
fn a_403_rejection_carries_the_modules_three_causes() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &descriptor_path(), MockResponse::json(200, caller()));
    server.expect(
        "POST",
        &acl_path(),
        MockResponse::json(
            403,
            json!({"message": "TF400813: The user is not authorized.", "$id": "1"}),
        ),
    );

    let output = run(&home, &server, &["security", "grant", UUID, FLAG, "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);
    assert_eq!(
        envelope["error"]["code"],
        json!("forbidden"),
        "the status's class, as every error path keeps it"
    );
    assert_eq!(envelope["error"]["details"]["status"], json!(403));

    let message = envelope["error"]["message"].as_str().expect("a message");
    assert!(
        message.starts_with(
            "Azure DevOps rejected the grant (403). Common causes: (a) your token \
            lacks the 'vso.security_manage' scope"
        ),
        "{message}"
    );
    assert!(
        message.contains("(c) you are not a Project Collection Administrator."),
        "{message}"
    );
    assert!(
        message.ends_with(
            "Raw response: {\"$id\":\"1\",\"message\":\"TF400813: The user is not authorized.\"}"
        ),
        "the raw upstream bytes, sliced like the module's 200-character preview: {message}"
    );
}

#[test]
fn a_401_rejection_names_the_command_that_was_refused() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &descriptor_path(), MockResponse::json(200, caller()));
    server.expect(
        "POST",
        &acl_path(),
        MockResponse::json(401, json!({"message": "TF400813: The token is not valid."})),
    );

    let output = run(
        &home,
        &server,
        &["security", "revoke", UUID, FLAG, "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);
    assert_eq!(envelope["error"]["code"], json!("auth_required"));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .starts_with("Azure DevOps rejected the revoke (401)."),
        "{}",
        stdout_of(&output)
    );
}

#[test]
fn a_400_rejection_is_the_classified_error_with_the_modules_prose() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &descriptor_path(), MockResponse::json(200, caller()));
    server.expect(
        "POST",
        &acl_path(),
        MockResponse::json(400, json!({"message": "The descriptor is not valid."})),
    );

    let output = run(&home, &server, &["security", "grant", UUID, FLAG, "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .starts_with("Azure DevOps rejected the grant (400)."),
        "{}",
        stdout_of(&output)
    );
}

/// Any other status keeps the module's `API error:` step name; the body's
/// `message` member replaces Elixir's tuple rendering.
#[test]
fn an_unlisted_acl_failure_keeps_the_modules_api_error_step() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &descriptor_path(), MockResponse::json(200, caller()));
    server.expect(
        "POST",
        &acl_path(),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = run(&home, &server, &["security", "grant", UUID, FLAG, "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!("API error: TF400813: The server is unavailable.")
    );
}

#[test]
fn a_rejection_without_a_message_member_falls_back_to_the_classified_text() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &descriptor_path(), MockResponse::json(200, caller()));
    server.expect(
        "POST",
        &acl_path(),
        MockResponse::json(403, json!({"$id": "1"})),
    );

    let output = run(&home, &server, &["security", "grant", UUID, FLAG, "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        envelope(&output)["error"]["message"]
            .as_str()
            .expect("a message")
            .ends_with("Raw response: {\"$id\":\"1\"}"),
        "{}",
        stdout_of(&output)
    );
}

// ── the D43 spellings and the usage errors ──────────────────────────────

/// D43: the frozen booleans also take `--flag=true|false` and `--no-flag`; this
/// build's `ArgAction::SetTrue` takes only the bare spelling (captured: the
/// oracle *proceeds* on `=true` and *refuses* on `=false`/`--no-…`).
#[test]
fn the_unported_boolean_spellings_are_usage_errors() {
    for spelling in [
        "--yes-this-mutates-secret-read=true",
        "--yes-this-mutates-secret-read=false",
        "--no-yes-this-mutates-secret-read",
        "--yes-this-mutates-secret-read=1",
        "--yes-this-mutates-secret-read=True",
    ] {
        let home = TempHome::new();
        let server = MockServer::start();

        let output = run(
            &home,
            &server,
            &["security", "grant", "Alpha", spelling, "--json"],
        );

        assert_eq!(output.status.code(), Some(1), "{spelling}");
        assert!(
            stdout_of(&output).is_empty(),
            "{spelling}: clap writes its usage error to stderr alone (D5)"
        );
        assert!(
            stderr_of(&output).contains("yes-this-mutates-secret-read"),
            "{spelling}: {}",
            stderr_of(&output)
        );
        assert!(
            requests(&server).is_empty(),
            "{spelling}: the flag never reached the wire"
        );
    }
}

/// D17's class: the option is declared with underscores and runnable only
/// hyphenated; the underscore spelling is a usage error on both sides.
#[test]
fn the_underscore_spelling_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "security",
            "grant",
            "Alpha",
            "--yes_this_mutates_secret_read",
        ],
    );

    usage_error(&output, "yes_this_mutates_secret_read");
    assert!(requests(&server).is_empty());
}

#[test]
fn the_missing_subcommand_and_positional_are_usage_errors() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["security"]);
    usage_error(&output, "missing sub-command");

    let output = run(&home, &server, &["security", "grant", FLAG]);
    usage_error(&output, "PROJECT_NAME_OR_ID");

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", "Extra", FLAG],
    );
    usage_error(&output, "Extra");

    assert!(requests(&server).is_empty());
}

/// A valueless `--permission` is the parser's usage error, as in the oracle.
#[test]
fn a_valueless_permission_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["security", "grant", "Alpha", FLAG, "--permission"],
    );

    usage_error(&output, "--permission");
    assert!(requests(&server).is_empty());
}
