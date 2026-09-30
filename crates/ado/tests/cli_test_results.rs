//! End-to-end tests for `ado test-results list|show|publish`: the
//! `_apis/test/runs` surface the frozen `lib/ado_cli/cli/test_results.ex`
//! builds, the list's filters (including the two Ruling 4(a) repairs), the run
//! detail, the three-request publish chain, and the file and error paths.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a
//! mock server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT`
//! credentials, so no credential resolution reaches the developer's keychain.
//! Every run scripts stdin explicitly, so no test can read a terminal.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

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

fn runs_path(project: &str) -> String {
    format!("/{ORG}/{project}/_apis/test/runs")
}

fn run_path(project: &str, run_id: u64) -> String {
    format!("{}/{run_id}", runs_path(project))
}

/// The captured `test-results list` body (`test_runs_list.json`): three rows —
/// the module's fixed pads read one, a name past its 39-character slice, and one
/// with no `runStatistics` at all.
fn runs() -> Value {
    json!({
        "count": 3,
        "value": [
            {
                "id": 101,
                "name": "Nightly Regression",
                "state": "Completed",
                "runStatistics": [
                    {"outcome": "TotalTests", "count": 42},
                    {"outcome": "Passed", "count": 40},
                    {"outcome": "Failed", "count": 2}
                ]
            },
            {
                "id": 102,
                "name": "A very long test run name that goes past the thirty-nine character slice",
                "runStatistics": [
                    {"state": "TotalTests", "count": 12},
                    {"state": "Passed", "count": 11},
                    {"outcome": "Failed", "count": null}
                ]
            },
            {"id": 103, "name": "Queued run", "state": "InProgress"}
        ]
    })
}

/// The captured `test-results show 42` body (`test_run_show.json`).
fn show_run() -> Value {
    json!({
        "id": 42,
        "name": "Nightly Regression",
        "state": "Completed",
        "startedDate": "2026-09-26T22:00:00.000Z",
        "completedDate": "2026-09-26T22:31:07.417Z",
        "runStatistics": [
            {"outcome": "TotalTests", "count": 42},
            {"outcome": "Passed", "count": 40},
            {"outcome": "Failed", "count": 2}
        ],
        "build": {"id": 501, "name": "20260926.1"}
    })
}

/// The captured create response: the module uploads against this id.
fn created_run() -> Value {
    json!({"id": 501, "name": "Nightly Regression", "state": "InProgress"})
}

/// The captured file the publish cases upload.
const RESULTS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites/>\n";

fn results_file(home: &TempHome) -> PathBuf {
    let path = home.path().join("results.xml");
    fs::write(&path, RESULTS).expect("write the results file");

    path
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

// ── list ────────────────────────────────────────────────────────────────

#[test]
fn list_emits_the_value_envelope_and_the_collection_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &runs_path("Alpha"), MockResponse::json(200, runs()));

    let output = run(&home, &server, &["test-results", "list", "Alpha", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": runs()["value"]}),
        "the value array under the value envelope"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, runs_path("Alpha"));
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "no filter given means no filter pair"
    );
}

/// The module's `$top` is `if t = Map.get(...)`: absent is no pair, `0` and a
/// negative are present options.
#[test]
fn list_sends_the_dollar_top_pair_only_when_the_option_is_given() {
    for (top, expected) in [("1", vec!["%24top=1"]), ("0", vec!["%24top=0"])] {
        let home = TempHome::new();
        let server = MockServer::start();
        server.expect("GET", &runs_path("Alpha"), MockResponse::json(200, runs()));

        let output = run(
            &home,
            &server,
            &["test-results", "list", "Alpha", "--top", top, "--json"],
        );

        assert_success(&output);
        let requests = requests(&server);
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].query_pairs(),
            vec![
                ("api-version".to_owned(), "7.1".to_owned()),
                ("%24top".to_owned(), top.to_owned())
            ],
            "--top {top}: the pair as sent, undecoded (the client escapes the dollar)"
        );
        assert!(
            requests[0].query.contains(expected[0]),
            "the pair is on the wire as sent: {}",
            requests[0].query
        );
    }
}

#[test]
fn list_accepts_a_negative_top_like_the_oracle() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &runs_path("Alpha"), MockResponse::json(200, runs()));

    let output = run(
        &home,
        &server,
        &["test-results", "list", "Alpha", "--top", "-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("%24top".to_owned(), "-1".to_owned())
        ],
        "the oracle's OptionParser takes a negative integer (captured)"
    );
}

/// Ruling 4(a): the frozen option is hyphen-declared and unreachable
/// (`invalid option --build-id`); this build accepts it as its help advertises
/// and sends the module's `buildIds` pair.
#[test]
fn list_sends_the_repaired_build_id_pair() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &runs_path("Alpha"), MockResponse::json(200, runs()));

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "list",
            "Alpha",
            "--build-id",
            "42",
            "--json",
        ],
    );

    assert_success(&output);
    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("buildIds".to_owned(), "42".to_owned())
        ],
        "the module's `Map.put(params, \"buildIds\", build_id)`"
    );
}

/// Ruling 4(a): the second repaired filter, and a present empty value is sent
/// (`""` is truthy in Elixir).
#[test]
fn list_sends_the_repaired_min_last_updated_pair() {
    for (value, expected) in [("2026-01-01", "2026-01-01"), ("", "")] {
        let home = TempHome::new();
        let server = MockServer::start();
        server.expect("GET", &runs_path("Alpha"), MockResponse::json(200, runs()));

        let output = run(
            &home,
            &server,
            &[
                "test-results",
                "list",
                "Alpha",
                "--min-last-updated",
                value,
                "--json",
            ],
        );

        assert_success(&output);
        assert_eq!(
            requests(&server)[0].query_pairs(),
            vec![
                ("api-version".to_owned(), "7.1".to_owned()),
                ("minLastUpdatedDate".to_owned(), expected.to_owned())
            ],
            "--min-last-updated {value:?}"
        );
    }
}

#[test]
fn list_sends_all_three_filters_together() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &runs_path("Alpha"), MockResponse::json(200, runs()));

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "list",
            "Alpha",
            "--top",
            "5",
            "--build-id",
            "42",
            "--min-last-updated",
            "2026-01-01",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("%24top".to_owned(), "5".to_owned()),
            ("buildIds".to_owned(), "42".to_owned()),
            ("minLastUpdatedDate".to_owned(), "2026-01-01".to_owned())
        ]
    );
}

/// C12: the list carries a 404 and a 5xx test.
#[test]
fn list_404_is_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &runs_path("Alpha"),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(&home, &server, &["test-results", "list", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let document = envelope(&output);
    assert_eq!(document["ok"], json!(false));
    assert_eq!(document["error"]["code"], json!("not_found"));
    assert_eq!(
        document["error"]["message"],
        json!("Resource not found. Check the project/repo/build ID and your permissions.")
    );
    assert_eq!(document["error"]["status"], json!(404));
    assert_eq!(
        document["error"]["details"]["body"],
        json!("{\"message\":\"TF400813: The user is not authorized.\"}"),
        "the upstream body stays the bytes (D24)"
    );
}

#[test]
fn list_500_is_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &runs_path("Alpha"),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = run(&home, &server, &["test-results", "list", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let document = envelope(&output);
    assert_eq!(document["error"]["code"], json!("api_error"));
    assert_eq!(
        document["error"]["message"],
        json!("Azure DevOps server error. Retry later.")
    );
}

#[test]
fn list_human_prints_the_modules_stat_selection() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", &runs_path("Alpha"), MockResponse::json(200, runs()));

    let output = run(&home, &server, &["test-results", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines[0],
        "ID   Name                                                                      State       Total / Passed / Failed",
        "the module's four columns in its order: {stdout:?}"
    );
    assert!(
        lines[2].ends_with("Completed   42 / 40 / 2"),
        "the TotalTests/Passed/Failed stats by outcome: {stdout:?}"
    );
    assert!(
        lines[3]
            .contains("A very long test run name that goes past the thirty-nine character slice")
            && lines[3].ends_with("?           12 / 11 / 0"),
        "a missing state is `?`, a null count falls back to 0, and the name is kept whole (§8): {stdout:?}"
    );
    assert!(
        lines[4].ends_with("InProgress  ? / 0 / 0"),
        "no runStatistics at all is `? / 0 / 0`: {stdout:?}"
    );
}

#[test]
fn list_human_prints_the_header_alone_for_an_empty_project() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &runs_path("Empty"),
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["test-results", "list", "Empty"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("Total / Passed / Failed"),
        "the header still prints: {stdout:?}"
    );
    assert_eq!(stdout.lines().count(), 2, "header and rule: {stdout:?}");
}

#[test]
fn list_under_json_answers_the_empty_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &runs_path("Empty"),
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["test-results", "list", "Empty", "--json"]);

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": []}));
}

/// D41's shape: a 200 whose body has no `value` key. The frozen `list_runs/1`
/// falls into `handle_api_result` with a no-op formatter and exits 0 with no
/// output; this build wraps the whole body as the single item.
#[test]
fn list_wraps_a_body_without_a_value_key_as_one_item() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &runs_path("NoValue"),
        MockResponse::json(200, json!({"count": 3})),
    );

    let output = run(
        &home,
        &server,
        &["test-results", "list", "NoValue", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": [{"count": 3}]}),
        "D41: the whole body is the one item where the oracle prints nothing"
    );
}

/// D22: the frozen module interpolates the project raw — a space makes Finch
/// refuse the request target — and this build escapes every segment strictly.
#[test]
fn list_encodes_the_project_as_one_path_segment() {
    let home = TempHome::new();
    let server = MockServer::start();
    let path = format!("/{ORG}/Alpha%20Beta/_apis/test/runs");
    server.expect("GET", &path, MockResponse::json(200, runs()));

    let output = run(
        &home,
        &server,
        &["test-results", "list", "Alpha Beta", "--json"],
    );

    assert_success(&output);
    assert_eq!(requests(&server)[0].path, path);
}

#[test]
fn list_refuses_a_missing_project_and_a_bad_top() {
    let home = TempHome::new();
    let server = MockServer::start();

    let missing = run(&home, &server, &["test-results", "list", "--json"]);
    usage_error(&missing, "<PROJECT>");

    let bad_top = run(
        &home,
        &server,
        &["test-results", "list", "Alpha", "--top", "abc", "--json"],
    );
    usage_error(&bad_top, "top");

    let missing_top = run(
        &home,
        &server,
        &["test-results", "list", "Alpha", "--top", "--json"],
    );
    usage_error(&missing_top, "top");

    let extra = run(&home, &server, &["test-results", "list", "Alpha", "Extra"]);
    usage_error(&extra, "Extra");

    assert!(requests(&server).is_empty(), "no request for a usage error");
}

// ── show ────────────────────────────────────────────────────────────────

#[test]
fn show_emits_the_run_envelope_and_the_module_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &run_path("Alpha", 42),
        MockResponse::json(200, show_run()),
    );

    let output = run(
        &home,
        &server,
        &["test-results", "show", "Alpha", "42", "--json"],
    );

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": show_run()}));

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, run_path("Alpha", 42));
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "the module's `Client.get(path)` with no params"
    );
}

#[test]
fn show_human_prints_the_modules_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &run_path("Alpha", 42),
        MockResponse::json(200, show_run()),
    );

    let output = run(&home, &server, &["test-results", "show", "Alpha", "42"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\n",
            "Test Run #42\n",
            "  Name:        Nightly Regression\n",
            "  State:       Completed\n",
            "  Started:     2026-09-26T22:00:00.000Z\n",
            "  Completed:   2026-09-26T22:31:07.417Z\n",
            "  Results:\n",
            "    TotalTests           42\n",
            "    Passed               40\n",
            "    Failed               2\n",
            "  Build:       501\n",
            "\n",
        )
    );
}

#[test]
fn show_human_prints_empty_fields_and_no_build_line_for_a_bare_run() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &run_path("Alpha", 43),
        MockResponse::json(200, json!({"id": 43})),
    );

    let output = run(&home, &server, &["test-results", "show", "Alpha", "43"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\n",
            "Test Run #43\n",
            "  Name:        \n",
            "  State:       \n",
            "  Started:     \n",
            "  Completed:   \n",
            "  Results:\n",
            "\n",
        ),
        "a nil field interpolates empty; `build` absent means no Build line"
    );
}

#[test]
fn show_human_reads_the_state_labels_and_a_missing_count() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &run_path("Alpha", 44),
        MockResponse::json(
            200,
            json!({
                "id": 44,
                "name": "State-labelled stats",
                "state": "InProgress",
                "runStatistics": [
                    {"state": "TotalTests", "count": 9},
                    {"state": "Passed", "count": null},
                    {"outcome": "Failed"}
                ]
            }),
        ),
    );

    let output = run(&home, &server, &["test-results", "show", "Alpha", "44"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\n",
            "Test Run #44\n",
            "  Name:        State-labelled stats\n",
            "  State:       InProgress\n",
            "  Started:     \n",
            "  Completed:   \n",
            "  Results:\n",
            "    TotalTests           9\n",
            "    Passed               \n",
            "    Failed               \n",
            "\n",
        ),
        "`outcome || state` labels and a nil count interpolates empty"
    );
}

#[test]
fn show_404_and_500_are_the_classified_envelopes() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &run_path("Alpha", 42),
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let not_found = run(
        &home,
        &server,
        &["test-results", "show", "Alpha", "42", "--json"],
    );
    assert_eq!(not_found.status.code(), Some(1));
    assert_eq!(envelope(&not_found)["error"]["code"], json!("not_found"));

    let server = MockServer::start();
    server.expect(
        "GET",
        &run_path("Alpha", 42),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let api_error = run(
        &home,
        &server,
        &["test-results", "show", "Alpha", "42", "--json"],
    );
    assert_eq!(api_error.status.code(), Some(1));
    assert_eq!(envelope(&api_error)["error"]["code"], json!("api_error"));
}

#[test]
fn show_refuses_a_missing_or_non_integer_run_id() {
    let home = TempHome::new();
    let server = MockServer::start();

    usage_error(
        &run(&home, &server, &["test-results", "show", "Alpha"]),
        "<RUN_ID>",
    );
    usage_error(
        &run(&home, &server, &["test-results", "show", "Alpha", "abc"]),
        "invalid value",
    );
    usage_error(
        &run(
            &home,
            &server,
            &["test-results", "show", "Alpha", "42", "Extra"],
        ),
        "Extra",
    );

    assert!(requests(&server).is_empty(), "no request for a usage error");
}

// ── publish ─────────────────────────────────────────────────────────────

/// The three-request chain: create the run, mark it completed, upload the file
/// as raw bytes with the module's two query pairs. The frozen third request
/// carries `Path.basename/1` under the preview version and the file content as a
/// JSON **string** — the D25-family glue this build repairs to the request the
/// module documents (the bytes, that pair set).
#[test]
fn publish_runs_the_three_request_chain() {
    let home = TempHome::new();
    let server = MockServer::start();
    let runs = runs_path("Alpha");
    let attachments = format!("{}/501/attachments", runs);
    server.expect("POST", &runs, MockResponse::json(200, created_run()));
    server.expect(
        "PATCH",
        &run_path("Alpha", 501),
        MockResponse::json(200, json!({"id": 501, "state": "Completed"})),
    );
    server.expect(
        "POST",
        &attachments,
        MockResponse::json(200, json!({"id": "att-1"})),
    );
    let file = results_file(&home);

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            file.to_str().expect("a utf-8 path"),
            "--json",
        ],
    );

    assert_success(&output);

    let requests = requests(&server);
    assert_eq!(requests.len(), 3, "the module's three requests");

    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, runs);
    assert_eq!(
        serde_json::from_str::<Value>(requests[0].body.as_deref().expect("a body")).expect("json"),
        json!({"name": "Nightly Regression", "isAutomated": true, "state": "InProgress"}),
        "the module's create body, with no `build` key without --build-id"
    );

    assert_eq!(requests[1].method, "PATCH");
    assert_eq!(requests[1].path, run_path("Alpha", 501));
    assert_eq!(
        serde_json::from_str::<Value>(requests[1].body.as_deref().expect("a body")).expect("json"),
        json!({"state": "Completed"})
    );

    assert_eq!(requests[2].method, "POST");
    assert_eq!(requests[2].path, attachments);
    assert_eq!(
        requests[2].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1-preview.1".to_owned()),
            ("fileName".to_owned(), "results.xml".to_owned())
        ],
        "the module's intended pair set: the preview version and the basename"
    );
    assert_eq!(
        requests[2].body.as_deref(),
        Some(RESULTS),
        "the file's bytes, not a JSON encoding of them"
    );
    assert_eq!(
        requests[2].header("content-type"),
        Some("application/octet-stream")
    );
}

/// Ruling 4(a): the frozen `--build-id` is unreachable; this build accepts it
/// and links the run to the build the help documents.
#[test]
fn publish_links_the_build_with_the_repaired_flag() {
    let home = TempHome::new();
    let server = MockServer::start();
    let runs = runs_path("Alpha");
    server.expect("POST", &runs, MockResponse::json(200, created_run()));
    server.expect(
        "PATCH",
        &run_path("Alpha", 501),
        MockResponse::json(200, json!({"id": 501, "state": "Completed"})),
    );
    server.expect(
        "POST",
        &format!("{}/501/attachments", runs),
        MockResponse::json(200, json!({"id": "att-1"})),
    );
    let file = results_file(&home);

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            file.to_str().expect("a utf-8 path"),
            "--build-id",
            "42",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(requests(&server)[0].body.as_deref().expect("a body"))
            .expect("json"),
        json!({
            "name": "Nightly Regression",
            "isAutomated": true,
            "state": "InProgress",
            "build": {"id": 42}
        }),
        "the module's `Map.put(body, \"build\", %{{\"id\" => build_id}})`: the build link Ruling 4(a) repairs"
    );
}

#[test]
fn publish_under_json_emits_the_frozen_document() {
    let home = TempHome::new();
    let server = MockServer::start();
    let runs = runs_path("Alpha");
    server.expect("POST", &runs, MockResponse::json(200, created_run()));
    server.expect(
        "PATCH",
        &run_path("Alpha", 501),
        MockResponse::json(200, json!({"id": 501, "state": "Completed"})),
    );
    server.expect(
        "POST",
        &format!("{}/501/attachments", runs),
        MockResponse::json(200, json!({"id": "att-1"})),
    );
    let file = results_file(&home);

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            file.to_str().expect("a utf-8 path"),
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "run": {"id": 501, "name": "Nightly Regression"}}),
        "the module's own document, not the value/message envelope (D38's class)"
    );
}

#[test]
fn publish_human_prints_the_module_block_with_the_halt_marker() {
    let home = TempHome::new();
    let server = MockServer::start();
    let runs = runs_path("Alpha");
    server.expect("POST", &runs, MockResponse::json(200, created_run()));
    server.expect(
        "PATCH",
        &run_path("Alpha", 501),
        MockResponse::json(200, json!({"id": 501, "state": "Completed"})),
    );
    server.expect(
        "POST",
        &format!("{}/501/attachments", runs),
        MockResponse::json(200, json!({"id": "att-1"})),
    );
    let file = results_file(&home);

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            file.to_str().expect("a utf-8 path"),
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\n",
            "✓ Test run #501 created: Nightly Regression\n",
            "  File ",
        )
        .to_owned()
            + file.to_str().expect("a utf-8 path")
            + concat!(
                " attached.\n",
                "\n",
                "  View run: https://dev.azure.com/_test/runs?runId=501\n",
                "\n",
                "Done.\n",
            ),
        "the module's block, the path as given, and its halt_success marker"
    );
}

#[test]
fn publish_uses_the_basename_of_a_nested_file() {
    let home = TempHome::new();
    let server = MockServer::start();
    let nested = home.path().join("sub");
    fs::create_dir_all(&nested).expect("create the sub directory");
    fs::write(nested.join("results.xml"), RESULTS).expect("write the results file");
    let runs = runs_path("Alpha");
    server.expect("POST", &runs, MockResponse::json(200, created_run()));
    server.expect(
        "PATCH",
        &run_path("Alpha", 501),
        MockResponse::json(200, json!({"id": 501, "state": "Completed"})),
    );
    server.expect(
        "POST",
        &format!("{}/501/attachments", runs),
        MockResponse::json(200, json!({"id": "att-1"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            nested.join("results.xml").to_str().expect("a utf-8 path"),
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[2].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1-preview.1".to_owned()),
            ("fileName".to_owned(), "results.xml".to_owned())
        ],
        "Path.basename/1 puts only the file name in the query"
    );
}

#[test]
fn publish_uploads_a_binary_file_byte_for_byte() {
    let home = TempHome::new();
    let server = MockServer::start();
    let binary = home.path().join("binary.bin");
    let bytes: Vec<u8> = vec![0xff, 0xfe, 0x00, b'b', b'i', b'n', b'a', b'r', b'y', b'\n'];
    fs::write(&binary, &bytes).expect("write the binary file");
    let runs = runs_path("Alpha");
    server.expect("POST", &runs, MockResponse::json(200, created_run()));
    server.expect(
        "PATCH",
        &run_path("Alpha", 501),
        MockResponse::json(200, json!({"id": 501, "state": "Completed"})),
    );
    server.expect(
        "POST",
        &format!("{}/501/attachments", runs),
        MockResponse::json(200, json!({"id": "att-1"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            binary.to_str().expect("a utf-8 path"),
            "--json",
        ],
    );

    assert_success(&output);
    let requests = requests(&server);
    assert_eq!(
        requests.len(),
        3,
        "the oracle's JSON encode raises on a non-UTF-8 file and its rescue exits 0 after the PATCH; this build uploads"
    );
    assert_eq!(
        requests[2].body.as_deref(),
        Some(String::from_utf8_lossy(&bytes).as_ref()),
        "the bytes on the wire, recorded lossily"
    );
}

#[test]
fn publish_reports_a_missing_file_without_sending_anything() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            "nope.xml",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let document = envelope(&output);
    assert_eq!(document["error"]["code"], json!("validation_error"));
    assert_eq!(
        document["error"]["message"],
        json!("File not found: nope.xml")
    );
    assert!(
        requests(&server).is_empty(),
        "the read fails before any request"
    );
}

#[test]
fn publish_reports_an_unreadable_file_with_the_d44_family_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    let directory = home.path().join("adir");
    fs::create_dir_all(&directory).expect("create the directory");

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            directory.to_str().expect("a utf-8 path"),
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let document = envelope(&output);
    assert_eq!(document["error"]["code"], json!("validation_error"));
    let message = document["error"]["message"]
        .as_str()
        .expect("the message is a string");
    assert!(
        message.starts_with("Cannot read results file"),
        "the D44-family wording: {message}"
    );
    assert!(requests(&server).is_empty());
}

/// The module discards the PATCH result (`_ = Client.patch(...)`): a failed
/// completion does not fail the publish.
#[test]
fn publish_tolerates_a_failing_completion_patch() {
    let home = TempHome::new();
    let server = MockServer::start();
    let runs = runs_path("Alpha");
    server.expect("POST", &runs, MockResponse::json(200, created_run()));
    server.expect(
        "PATCH",
        &run_path("Alpha", 501),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );
    server.expect(
        "POST",
        &format!("{}/501/attachments", runs),
        MockResponse::json(200, json!({"id": "att-1"})),
    );
    let file = results_file(&home);

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            file.to_str().expect("a utf-8 path"),
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(requests(&server).len(), 3, "the upload still runs");
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "run": {"id": 501, "name": "Nightly Regression"}})
    );
}

#[test]
fn publish_create_and_attach_failures_are_the_classified_envelopes() {
    let home = TempHome::new();
    let file = results_file(&home);

    let server = MockServer::start();
    server.expect(
        "POST",
        &runs_path("Alpha"),
        MockResponse::json(404, json!({"message": "The project does not exist."})),
    );
    let create = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            file.to_str().expect("a utf-8 path"),
            "--json",
        ],
    );
    assert_eq!(create.status.code(), Some(1));
    assert_eq!(envelope(&create)["error"]["code"], json!("not_found"));
    assert_eq!(
        requests(&server).len(),
        1,
        "the chain stops at the first failure"
    );

    let server = MockServer::start();
    let runs = runs_path("Alpha");
    server.expect("POST", &runs, MockResponse::json(200, created_run()));
    server.expect(
        "PATCH",
        &run_path("Alpha", 501),
        MockResponse::json(200, json!({"id": 501, "state": "Completed"})),
    );
    server.expect(
        "POST",
        &format!("{}/501/attachments", runs),
        MockResponse::json(500, json!({"message": "TF400813: The upload failed."})),
    );
    let attach = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha",
            "--name",
            "Nightly Regression",
            "--file",
            file.to_str().expect("a utf-8 path"),
            "--json",
        ],
    );
    assert_eq!(attach.status.code(), Some(1));
    assert_eq!(envelope(&attach)["error"]["code"], json!("api_error"));
    assert_eq!(requests(&server).len(), 3);
}

#[test]
fn publish_refuses_a_missing_name_or_file_as_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    usage_error(
        &run(
            &home,
            &server,
            &[
                "test-results",
                "publish",
                "Alpha",
                "--file",
                "results.xml",
                "--json",
            ],
        ),
        "name",
    );
    usage_error(
        &run(
            &home,
            &server,
            &["test-results", "publish", "Alpha", "--name", "n", "--json"],
        ),
        "file",
    );
    usage_error(
        &run(
            &home,
            &server,
            &[
                "test-results",
                "publish",
                "--name",
                "n",
                "--file",
                "results.xml",
                "--json",
            ],
        ),
        "<PROJECT>",
    );

    assert!(requests(&server).is_empty(), "no request for a usage error");
}

#[test]
fn publish_encodes_the_project_as_one_path_segment() {
    let home = TempHome::new();
    let server = MockServer::start();
    let path = format!("/{ORG}/Alpha%20Beta/_apis/test/runs");
    server.expect("POST", &path, MockResponse::json(200, created_run()));
    server.expect(
        "PATCH",
        &format!("{path}/501"),
        MockResponse::json(200, json!({"id": 501, "state": "Completed"})),
    );
    server.expect(
        "POST",
        &format!("{path}/501/attachments"),
        MockResponse::json(200, json!({"id": "att-1"})),
    );
    let file = results_file(&home);

    let output = run(
        &home,
        &server,
        &[
            "test-results",
            "publish",
            "Alpha Beta",
            "--name",
            "Nightly Regression",
            "--file",
            file.to_str().expect("a utf-8 path"),
            "--json",
        ],
    );

    assert_success(&output);
    let requests = requests(&server);
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert!(
            request.path.starts_with(&path),
            "every request carries the escaped project: {}",
            request.path
        );
    }
}
