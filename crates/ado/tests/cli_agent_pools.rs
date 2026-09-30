//! End-to-end tests for `ado agent-pools list|show|queues list`: the
//! organization-scoped `_apis/distributedtask/pools` surface the frozen
//! `lib/ado_cli/cli/agent_pools.ex` builds, its project-scoped `queues`
//! grandchild, the two-request `show` chain, the `--json` envelopes, the human
//! views and the error paths.
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
const POOLS: &str = "/myorg/_apis/distributedtask/pools";

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
    command(home, server, ORG, args).output().expect("run ado")
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

/// The list body of the frozen `agent_pools_test.exs`, with the two fields the
/// human table reads (`autoProvision`, `poolType`).
fn pools() -> Value {
    json!({"count": 2, "value": [
        {"id": 1, "name": "Default", "autoProvision": true, "poolType": "automation", "size": 1},
        {"id": 2, "name": "Hosted"}
    ]})
}

/// The show body of the frozen test: `{"id":1,"name":"Default","size":1}` plus
/// the fields the human detail reads.
fn pool() -> Value {
    json!({"id": 1, "name": "Default", "size": 1, "poolType": "automation", "autoProvision": true})
}

fn agents() -> Value {
    json!({"count": 2, "value": [
        {"id": 10, "name": "agent-1", "status": "online", "version": "3.230.0"},
        {"id": 11, "name": "agent-2", "status": "offline"}
    ]})
}

#[test]
fn list_emits_the_value_envelope_and_the_org_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", POOLS, MockResponse::json(200, pools()));

    let output = run(&home, &server, &["agent-pools", "list", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": pools()["value"]})
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, POOLS);
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "no filter, no extra pair"
    );
}

#[test]
fn list_renders_the_module_columns() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", POOLS, MockResponse::json(200, pools()));

    let output = run(&home, &server, &["agent-pools", "list"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert!(lines[0].starts_with("ID"), "header line: {stdout}");
    assert!(lines[0].contains("Auto-provision"), "header line: {stdout}");
    assert!(lines[0].contains("Type"), "header line: {stdout}");
    assert!(
        lines[2].contains('1') && lines[2].contains("Default") && lines[2].contains("automation"),
        "the first row carries the module's fields: {stdout}"
    );
    assert!(
        lines[3].contains('2') && lines[3].contains("Hosted"),
        "the second row: {stdout}"
    );
}

#[test]
fn list_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/empty-org/_apis/distributedtask/pools",
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = command(&home, &server, "empty-org", &["agent-pools", "list"])
        .output()
        .expect("run ado");

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No agent pools found.\n");
}

#[test]
fn list_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/missing/_apis/distributedtask/pools",
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = command(
        &home,
        &server,
        "missing",
        &["agent-pools", "list", "--json"],
    )
    .output()
    .expect("run ado");

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
fn list_500_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/broken/_apis/distributedtask/pools",
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = command(&home, &server, "broken", &["agent-pools", "list", "--json"])
        .output()
        .expect("run ado");

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
fn show_merges_the_pool_and_its_agents() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{POOLS}/1"),
        MockResponse::json(200, pool()),
    );
    server.expect(
        "GET",
        &format!("{POOLS}/1/agents"),
        MockResponse::json(200, agents()),
    );

    let output = run(&home, &server, &["agent-pools", "show", "1", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": {"pool": pool(), "agents": agents()}}),
        "the module's `%{{pool: pool, agents: agents}}` merge, agents as the raw body"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].path, format!("{POOLS}/1"));
    assert_eq!(requests[1].path, format!("{POOLS}/1/agents"));
    for request in requests {
        assert_eq!(
            request.query_pairs(),
            vec![("api-version".to_owned(), "7.1".to_owned())]
        );
    }
}

#[test]
fn show_falls_back_to_the_bare_pool_when_the_agents_fetch_fails() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{POOLS}/2"),
        MockResponse::json(200, json!({"id": 2, "name": "Hosted", "size": 0})),
    );
    server.expect(
        "GET",
        &format!("{POOLS}/2/agents"),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = run(&home, &server, &["agent-pools", "show", "2", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": {"id": 2, "name": "Hosted", "size": 0}}),
        "any agents failure falls back to the bare pool and still exits 0"
    );
    assert_eq!(requests(&server).len(), 2, "the agents fetch was attempted");
}

#[test]
fn show_renders_the_pool_fields_the_module_prints() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{POOLS}/1"),
        MockResponse::json(200, pool()),
    );
    server.expect(
        "GET",
        &format!("{POOLS}/1/agents"),
        MockResponse::json(200, agents()),
    );

    let output = run(&home, &server, &["agent-pools", "show", "1"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            concat!(
                "\nAgent Pool Details\n\n",
                "{}\n",
                "  ID:            1\n",
                "  Name:          Default\n",
                "  Type:          automation\n",
                "  Auto-provision: true\n",
                "\n",
            ),
            "─".repeat(60)
        ),
        "the module's detail layout; its agents block needs a list member and the \
         wrapped agents body is a map, so neither side prints agents (captured: the \
         oracle prints its four fields empty and no agents at all)"
    );
}

#[test]
fn show_renders_the_bare_pool_without_the_agents_block() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{POOLS}/2"),
        MockResponse::json(200, json!({"id": 2, "name": "Hosted"})),
    );
    server.expect(
        "GET",
        &format!("{POOLS}/2/agents"),
        MockResponse::json(500, json!({"message": "nope"})),
    );

    let output = run(&home, &server, &["agent-pools", "show", "2"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            concat!(
                "\nAgent Pool Details\n\n",
                "{}\n",
                "  ID:            2\n",
                "  Name:          Hosted\n",
                "  Type:          \n",
                "  Auto-provision: \n",
                "\n",
            ),
            "─".repeat(60)
        ),
        "the bare-pool path is the oracle's own shape"
    );
}

#[test]
fn show_404_uses_the_modules_wording_and_skips_the_agents_fetch() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{POOLS}/999"),
        MockResponse::json(404, json!({"message": "The pool does not exist."})),
    );

    let output = run(&home, &server, &["agent-pools", "show", "999", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Agent pool #999 not found"),
        "the module's own wording where the frozen CLI halts on stderr with no envelope (D4)"
    );
    assert_eq!(
        requests(&server).len(),
        1,
        "a missing pool skips the agents fetch"
    );
}

#[test]
fn show_500_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{POOLS}/5"),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = run(&home, &server, &["agent-pools", "show", "5", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
}

#[test]
fn queues_list_emits_the_value_envelope_and_the_project_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    let queues = json!([
        {"id": 3, "name": "Alpha Pool Queue", "pool": {"id": 1, "name": "Default"}},
        {"id": 4, "name": "Hosted Queue", "pool": {"id": 2}}
    ]);
    server.expect(
        "GET",
        &format!("/{ORG}/Alpha/_apis/distributedtask/queues"),
        MockResponse::json(200, json!({"count": 2, "value": queues})),
    );

    let output = run(
        &home,
        &server,
        &["agent-pools", "queues", "list", "Alpha", "--json"],
    );

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": queues}));
    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].path,
        format!("/{ORG}/Alpha/_apis/distributedtask/queues")
    );
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())]
    );
}

#[test]
fn queues_list_pool_filter_sends_pool_id() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("/{ORG}/Alpha/_apis/distributedtask/queues"),
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &[
            "agent-pools",
            "queues",
            "list",
            "Alpha",
            "--pool",
            "1",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("poolId".to_owned(), "1".to_owned())
        ],
        "the module's own `poolId` pair, only when --pool is given"
    );
}

#[test]
fn queues_list_renders_the_module_columns() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("/{ORG}/Alpha/_apis/distributedtask/queues"),
        MockResponse::json(
            200,
            json!({"count": 2, "value": [
                {"id": 3, "name": "Alpha Pool Queue", "pool": {"id": 1, "name": "Default"}},
                {"id": 4, "name": "Hosted Queue", "pool": {"id": 2}}
            ]}),
        ),
    );

    let output = run(&home, &server, &["agent-pools", "queues", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert!(lines[0].starts_with("ID"), "header line: {stdout}");
    assert!(lines[0].contains("Pool"), "header line: {stdout}");
    assert!(
        lines[2].contains('3')
            && lines[2].contains("Alpha Pool Queue")
            && lines[2].contains("Default"),
        "the pool name comes from the nested object: {stdout}"
    );
    assert!(
        lines[3].contains("Hosted Queue") && lines[3].contains('2'),
        "a pool without a name falls back to its id: {stdout}"
    );
}

#[test]
fn queues_list_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("/{ORG}/Empty/_apis/distributedtask/queues"),
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["agent-pools", "queues", "list", "Empty"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No agent queues found.\n");
}

#[test]
fn queues_list_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("/{ORG}/Missing/_apis/distributedtask/queues"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["agent-pools", "queues", "list", "Missing", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
}

#[test]
fn queues_list_500_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("/{ORG}/Broken/_apis/distributedtask/queues"),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["agent-pools", "queues", "list", "Broken", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
}

#[test]
fn queues_list_encodes_the_project_as_one_segment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("/{ORG}/Alpha%2FBeta/_apis/distributedtask/queues"),
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["agent-pools", "queues", "list", "Alpha/Beta", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].path,
        format!("/{ORG}/Alpha%2FBeta/_apis/distributedtask/queues"),
        "the frozen URI.encode/1 leaves the slash raw; this build escapes it (D22)"
    );
}
