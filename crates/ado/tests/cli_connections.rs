//! End-to-end tests for `ado connections list|show|create|update|delete`: the
//! project-scoped `_apis/serviceendpoint/endpoints` surface the frozen
//! `lib/ado_cli/cli/connections.ex` builds, its three writes' bodies, the
//! `--json` envelopes, the two views, the error paths, and the wave's fourth
//! confirmation (`delete`, D30–D32).
//!
//! Every test owns its environment: a `TempHome` for the config directory, a
//! mock server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT`
//! credentials, so no credential resolution reaches the developer's keychain.
//! Every run scripts stdin explicitly, so no test can read a terminal.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";
const ENDPOINTS: &str = "/myorg/Alpha/_apis/serviceendpoint/endpoints";

fn command(home: &TempHome, server: &MockServer, org: &str, args: &[&str]) -> Command {
    let mut command = ado_cmd();
    home.apply(&mut command);
    command
        .env("ADO_ORG", org)
        .env("ADO_PAT", "test-pat")
        .env("ADO_SERVER", server.base_url())
        .args(args);
    command
}

fn run(home: &TempHome, server: &MockServer, args: &[&str]) -> Output {
    command(home, server, ORG, args)
        .stdin(Stdio::null())
        .output()
        .expect("run ado")
}

/// Runs the binary with `stdin` written to a pipe (never a terminal); an empty
/// slice is EOF, which is what the prompt's unanswered case means (D30).
fn run_with_stdin(home: &TempHome, server: &MockServer, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = command(home, server, ORG, args)
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

fn sent_body(request: &RecordedRequest) -> Value {
    serde_json::from_str(request.body.as_deref().expect("a request body"))
        .expect("the request body is JSON")
}

fn pair(key: &str, value: &str) -> (String, String) {
    (key.to_owned(), value.to_owned())
}

/// The list body of the frozen `connections_test.exs` (`{"value":[{id,name,type}],
/// "count":1}`), with the second member the human table's width probe reads.
fn connections() -> Value {
    json!({"count": 2, "value": [
        {"id": "c1", "name": "GitHub", "type": "github"},
        {"id": "c2", "name": "K8s", "type": "kubernetes"}
    ]})
}

/// The show body of the frozen test: `{"id":"c1","name":"GitHub","type":"github",
/// "url":"https://github.com","isReady":true}`.
fn connection() -> Value {
    json!({"id": "c1", "name": "GitHub", "type": "github", "url": "https://github.com", "isReady": true})
}

/// The create/update response body the frozen test serves.
fn created() -> Value {
    json!({"id": "new-id", "name": "GitHub", "type": "github", "url": "https://github.com", "isReady": true})
}

// ── list ─────────────────────────────────────────────────────────────────

#[test]
fn list_emits_the_value_envelope_and_the_project_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", ENDPOINTS, MockResponse::json(200, connections()));

    let output = run(&home, &server, &["connections", "list", "Alpha", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": connections()["value"]})
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, ENDPOINTS);
    assert_eq!(
        requests[0].query_pairs(),
        vec![pair("api-version", "7.1")],
        "no type, no extra pair"
    );
}

#[test]
fn list_passes_the_type_filter_as_a_query_pair() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", ENDPOINTS, MockResponse::json(200, connections()));

    let output = run(
        &home,
        &server,
        &["connections", "list", "Alpha", "--type", "github", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![pair("api-version", "7.1"), pair("type", "github")],
        "--type reaches the wire as a query pair (captured; the module does not filter client-side)"
    );
}

#[test]
fn list_renders_the_module_table_and_the_empty_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", ENDPOINTS, MockResponse::json(200, connections()));
    server.expect(
        "GET",
        "/myorg/Empty/_apis/serviceendpoint/endpoints",
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["connections", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert!(lines[0].starts_with("ID"), "header line: {stdout}");
    assert!(lines[0].contains("Name"), "header line: {stdout}");
    assert!(lines[0].contains("Type"), "header line: {stdout}");
    assert!(
        lines[2].contains("c1") && lines[2].contains("GitHub") && lines[2].contains("github"),
        "the first row carries the module's fields: {stdout}"
    );
    assert!(
        lines[3].contains("c2") && lines[3].contains("K8s"),
        "the second row: {stdout}"
    );

    let empty = run(&home, &server, &["connections", "list", "Empty"]);

    assert_success(&empty);
    assert_eq!(stdout_of(&empty), "No service connections found.\n");
}

#[test]
fn list_404_is_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Missing/_apis/serviceendpoint/endpoints",
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["connections", "list", "Missing", "--json"],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        stderr_of(&output)
    );
    let envelope = envelope(&output);

    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("Resource not found"),
        "the classified 404 wording: {envelope}"
    );
    assert_eq!(
        envelope["error"]["details"]["body"],
        json!("{\"message\":\"TF400813: The user is not authorized.\"}"),
        "D24: the raw upstream body, not the oracle's inspect/2 rendering"
    );
}

#[test]
fn list_500_is_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Broken/_apis/serviceendpoint/endpoints",
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = run(&home, &server, &["connections", "list", "Broken", "--json"]);

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
fn list_404_human_writes_the_labelled_line_to_stderr() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Missing/_apis/serviceendpoint/endpoints",
        MockResponse::json(404, json!({"message": "Not found"})),
    );

    let output = run(&home, &server, &["connections", "list", "Missing"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "the human error is not on stdout here: {}",
        stdout_of(&output)
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("[Not found]") && stderr.contains("Resource not found"),
        "the labelled line names the class (D4): {stderr}"
    );
}

// ── show ─────────────────────────────────────────────────────────────────

#[test]
fn show_emits_the_value_envelope_and_the_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    for _ in 0..2 {
        server.expect(
            "GET",
            &format!("{ENDPOINTS}/c1"),
            MockResponse::json(200, connection()),
        );
    }

    let output = run(
        &home,
        &server,
        &["connections", "show", "Alpha", "c1", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": connection()})
    );
    assert_eq!(requests(&server)[0].path, format!("{ENDPOINTS}/c1"));

    let human = run(&home, &server, &["connections", "show", "Alpha", "c1"]);

    assert_success(&human);
    assert_eq!(
        stdout_of(&human),
        format!(
            concat!(
                "\nService Connection Details\n\n",
                "{}\n",
                "  ID:    c1\n",
                "  Name:  GitHub\n",
                "  Type:  github\n",
                "  URL:   https://github.com\n",
                "  Ready: true\n",
            ),
            "─".repeat(60)
        ),
        "the module's print_connection_detail/1 layout, colour dropped (§8)"
    );
}

#[test]
fn show_404_reports_the_modules_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    for _ in 0..2 {
        server.expect(
            "GET",
            &format!("{ENDPOINTS}/missing"),
            MockResponse::json(404, json!({"message": "Not found"})),
        );
    }

    let json = run(
        &home,
        &server,
        &["connections", "show", "Alpha", "missing", "--json"],
    );

    assert_eq!(json.status.code(), Some(1));
    let envelope = envelope(&json);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Service connection 'missing' not found"),
        "the module's own wording where the frozen CLI halts on stderr with no envelope (D4)"
    );

    let human = run(&home, &server, &["connections", "show", "Alpha", "missing"]);

    assert_eq!(human.status.code(), Some(1));
    assert!(
        stdout_of(&human).is_empty(),
        "the human error is not stdout"
    );
    assert_eq!(
        stderr_of(&human).trim(),
        "[Not found] Service connection 'missing' not found"
    );
}

// ── create ───────────────────────────────────────────────────────────────

#[test]
fn create_posts_the_captured_body_and_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", ENDPOINTS, MockResponse::json(200, created()));

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": created()}));

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, ENDPOINTS);
    assert_eq!(
        sent_body(&requests[0]),
        json!({
            "name": "GitHub",
            "type": "github",
            "url": "https://github.com",
            "authorization": {"scheme": "Token", "parameters": {}},
            "isReady": false,
            "serviceEndpointProjectReferences": [
                {"projectReference": {"name": "Alpha"}, "name": "GitHub"}
            ]
        }),
        "the captured body: positional name/type/url, the default Token scheme, the one project reference"
    );
}

#[test]
fn create_full_body_carries_description_token_data_and_ready() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", ENDPOINTS, MockResponse::json(200, created()));

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
            "--description",
            "A GitHub PAT",
            "--scheme",
            "Token",
            "--access-token",
            "ghp_xxx",
            "--data",
            r#"{"subscriptionId":"s1"}"#,
            "--ready",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0]),
        json!({
            "name": "GitHub",
            "type": "github",
            "url": "https://github.com",
            "description": "A GitHub PAT",
            "authorization": {"scheme": "Token", "parameters": {"accessToken": "ghp_xxx"}},
            "isReady": true,
            "data": {"subscriptionId": "s1"},
            "serviceEndpointProjectReferences": [
                {"projectReference": {"name": "Alpha"}, "name": "GitHub"}
            ]
        }),
        "--data nests under `data`; the token sits at authorization.parameters.accessToken"
    );
}

#[test]
fn create_reads_the_token_from_stdin() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", ENDPOINTS, MockResponse::json(200, created()));

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
            "--access-token",
            "-",
            "--json",
        ],
        b"ghp_from_stdin\n",
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0])["authorization"]["parameters"]["accessToken"],
        json!("ghp_from_stdin"),
        "the stdin form is trimmed"
    );
}

#[test]
fn create_reads_the_token_from_a_file() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", ENDPOINTS, MockResponse::json(200, created()));
    let secret = home.path().join("token.txt");
    std::fs::write(&secret, "ghp_from_file\n").expect("write the secret file");

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
            "--access-token",
            &format!("@{}", secret.display()),
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0])["authorization"]["parameters"]["accessToken"],
        json!("ghp_from_file"),
        "the @file form is trimmed"
    );
}

#[test]
fn create_reports_a_missing_secret_file() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
            "--access-token",
            "@nope.txt",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .starts_with("Cannot read secret file \"nope.txt\": "),
        "the module's prefix, the io error's tail (§8): {envelope}"
    );
    assert!(requests(&server).is_empty(), "nothing was sent");
}

#[test]
fn create_rejects_data_that_is_not_a_json_object() {
    let home = TempHome::new();
    let server = MockServer::start();

    for (data, message) in [
        (
            "notjson",
            "--data is not valid JSON. Pass an object, e.g. '{\"subscriptionId\":\"...\"}'",
        ),
        (
            "[1]",
            "--data must be a JSON object, e.g. '{\"subscriptionId\":\"...\"}'",
        ),
    ] {
        let output = run(
            &home,
            &server,
            &[
                "connections",
                "create",
                "Alpha",
                "GitHub",
                "github",
                "https://github.com",
                "--data",
                data,
                "--json",
            ],
        );

        assert_eq!(output.status.code(), Some(1), "data={data}");
        assert_eq!(
            envelope(&output)["error"]["message"],
            json!(message),
            "the module's own wording (D4: an envelope here, stderr there)"
        );
        assert!(requests(&server).is_empty());
    }
}

#[test]
fn create_drops_empty_description_and_empty_token() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", ENDPOINTS, MockResponse::json(200, created()));

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
            "--description",
            "",
            "--access-token",
            "",
            "--json",
        ],
    );

    assert_success(&output);
    let body = sent_body(&requests(&server)[0]);

    assert!(
        body.get("description").is_none(),
        "put_if_present/3 treats `\"\"` as absent: {body}"
    );
    assert_eq!(body["authorization"]["parameters"], json!({}));
}

#[test]
fn create_sends_an_empty_scheme_verbatim() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", ENDPOINTS, MockResponse::json(200, created()));

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
            "--scheme",
            "",
            "--access-token",
            "x",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0])["authorization"]["scheme"],
        json!(""),
        "Map.get/3 finds the present-but-empty option, so the default Token is not applied"
    );
}

#[test]
fn create_404_reports_the_project_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        "/myorg/Missing/_apis/serviceendpoint/endpoints",
        MockResponse::json(404, json!({"message": "Not found"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Missing",
            "GitHub",
            "github",
            "https://github.com",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Project 'Missing' not found"),
        "the module's own wording (D4)"
    );
}

#[test]
fn create_prints_the_human_success_line_even_under_json() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", ENDPOINTS, MockResponse::json(200, created()));

    let human = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
        ],
    );

    assert_success(&human);
    assert_eq!(
        stdout_of(&human),
        concat!(
            "Service connection 'GitHub' created.\n",
            "\n",
            "  ID:    new-id\n",
            "  Type:  github\n",
            "  URL:   https://github.com\n",
            "  Ready: true\n",
        )
    );
}

#[test]
fn create_refuses_the_underscore_token_spelling() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
            "--access_token",
            "x",
        ],
    );

    assert_eq!(output.status.code(), Some(1), "D17: the flag is hyphenated");
    assert!(stdout_of(&output).is_empty());
    assert!(
        stderr_of(&output).contains("--access_token"),
        "the refusal names the spelling: {}",
        stderr_of(&output)
    );
    assert!(requests(&server).is_empty());
}

#[test]
fn create_refuses_the_ready_value_spelling_the_oracle_accepts() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "create",
            "Alpha",
            "GitHub",
            "github",
            "https://github.com",
            "--ready=false",
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "the unported boolean spelling: the oracle parses --ready=false, this build's clap refuses it"
    );
    assert!(
        stderr_of(&output).contains("--ready"),
        "the refusal names the flag: {}",
        stderr_of(&output)
    );
    assert!(requests(&server).is_empty());
}

// ── update ───────────────────────────────────────────────────────────────

#[test]
fn update_puts_only_the_options_given() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &format!("{ENDPOINTS}/c1"),
        MockResponse::json(200, connection()),
    );

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "update",
            "Alpha",
            "c1",
            "--name",
            "Renamed",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": connection()})
    );

    let requests = requests(&server);
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(requests[0].path, format!("{ENDPOINTS}/c1"));
    assert_eq!(
        sent_body(&requests[0]),
        json!({"name": "Renamed"}),
        "one option, one body member"
    );
}

#[test]
fn update_rotates_the_token_under_the_default_scheme() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &format!("{ENDPOINTS}/c1"),
        MockResponse::json(200, connection()),
    );

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "update",
            "Alpha",
            "c1",
            "--access-token",
            "ghp_new",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0]),
        json!({"authorization": {"scheme": "Token", "parameters": {"accessToken": "ghp_new"}}}),
        "a token-only update re-declares the Token scheme"
    );
}

#[test]
fn update_nests_the_data_object_and_keeps_the_other_options() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &format!("{ENDPOINTS}/c1"),
        MockResponse::json(200, connection()),
    );

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "update",
            "Alpha",
            "c1",
            "--name",
            "Renamed",
            "--description",
            "Renamed",
            "--url",
            "https://github.com/new",
            "--data",
            r#"{"subscriptionId":"s2"}"#,
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0]),
        json!({
            "name": "Renamed",
            "description": "Renamed",
            "url": "https://github.com/new",
            "data": {"subscriptionId": "s2"}
        })
    );
}

#[test]
fn update_guard_refuses_an_empty_body() {
    let home = TempHome::new();
    let server = MockServer::start();

    for args in [
        vec!["connections", "update", "Alpha", "c1", "--json"],
        vec![
            "connections",
            "update",
            "Alpha",
            "c1",
            "--description",
            "",
            "--json",
        ],
    ] {
        let output = run(&home, &server, &args);

        assert_eq!(output.status.code(), Some(1), "args={args:?}");
        assert_eq!(
            envelope(&output)["error"]["message"],
            json!(
                "At least one of --name, --description, --url, --access-token, or --data is required."
            ),
            "the module's guard, in this build's envelope (D4)"
        );
        assert!(requests(&server).is_empty());
    }
}

#[test]
fn update_404_reports_the_connection_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &format!("{ENDPOINTS}/missing"),
        MockResponse::json(404, json!({"message": "Not found"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "update",
            "Alpha",
            "missing",
            "--name",
            "X",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Service connection 'missing' not found")
    );
}

// ── delete and the prompt ────────────────────────────────────────────────

#[test]
fn delete_force_sends_the_delete_and_the_message_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{ENDPOINTS}/c1"),
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &["connections", "delete", "Alpha", "c1", "--force", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Service connection 'c1' deleted from 'Alpha'."})
    );
    assert!(
        stderr_of(&output).is_empty(),
        "--force skips the prompt entirely: {}",
        stderr_of(&output)
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].path, format!("{ENDPOINTS}/c1"));
    assert_eq!(requests[0].query_pairs(), vec![pair("api-version", "7.1")]);
    assert!(requests[0].body.is_none(), "the delete carries no body");
}

#[test]
fn delete_force_human_prints_the_module_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{ENDPOINTS}/c1"),
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &["connections", "delete", "Alpha", "c1", "--force"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Service connection 'c1' deleted from 'Alpha'.\n"
    );
}

#[test]
fn delete_answered_yes_prompts_on_stderr_and_deletes() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{ENDPOINTS}/c1"),
        MockResponse::json(200, json!({})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["connections", "delete", "Alpha", "c1", "--json"],
        b"y\n",
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Service connection 'c1' deleted from 'Alpha'."})
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("Delete service connection 'Alpha/c1'? This cannot be undone. [y/N] "),
        "the question is on stderr (D31): {stderr}"
    );
    assert!(
        !stdout_of(&output).contains("[y/N]"),
        "no prompt may reach stdout: {}",
        stdout_of(&output)
    );
    assert_eq!(requests(&server).len(), 1, "the confirmed delete was sent");
}

#[test]
fn delete_answered_no_refuses_and_sends_nothing() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["connections", "delete", "Alpha", "c1"],
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
        stderr.contains("Delete service connection 'Alpha/c1'? This cannot be undone. [y/N] "),
        "the question is on stderr: {stderr}"
    );
    assert!(
        stderr.contains("Aborted."),
        "this build's refusal is on stderr: {stderr}"
    );
    assert!(requests(&server).is_empty(), "a refusal sends nothing");
}

#[test]
fn delete_only_the_trimmed_downcased_y_confirms() {
    let home = TempHome::new();
    let server = MockServer::start();

    let yes = run_with_stdin(
        &home,
        &server,
        &["connections", "delete", "Alpha", "c1"],
        b"yes\n",
    );

    assert_eq!(yes.status.code(), Some(1));
    assert!(stderr_of(&yes).contains("Aborted."));
    assert!(requests(&server).is_empty(), "`yes` is not the helper's y");

    server.expect(
        "DELETE",
        &format!("{ENDPOINTS}/c1"),
        MockResponse::json(200, json!({})),
    );

    let confirm = run_with_stdin(
        &home,
        &server,
        &["connections", "delete", "Alpha", "c1"],
        b" Y \n",
    );

    assert_success(&confirm);
    assert_eq!(requests(&server).len(), 1, "Y trims and downcases to y");
}

#[test]
fn delete_at_eof_refuses_and_sends_nothing() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["connections", "delete", "Alpha", "c1"],
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
        stderr.contains("Delete service connection 'Alpha/c1'? This cannot be undone. [y/N] ")
            && stderr.contains("Aborted."),
        "EOF behaves exactly like an answered no: {stderr}"
    );
    assert!(requests(&server).is_empty(), "a refusal sends nothing");
}

#[test]
fn delete_refusal_under_json_emits_no_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &["connections", "delete", "Alpha", "c1", "--json"],
        b"n\n",
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "a refusal is not an API failure and has no envelope: {}",
        stdout_of(&output)
    );
    assert!(stderr_of(&output).contains("Aborted."));
    assert!(requests(&server).is_empty());
}

#[test]
fn delete_404_reports_the_connection_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &format!("{ENDPOINTS}/missing"),
        MockResponse::json(404, json!({"message": "Not found"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "connections",
            "delete",
            "Alpha",
            "missing",
            "--force",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Service connection 'missing' not found")
    );
}
