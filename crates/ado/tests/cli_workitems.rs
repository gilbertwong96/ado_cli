//! End-to-end tests for `ado workitems list|show|query`: the WIQL surface the
//! frozen `lib/ado_cli/cli/work_items.ex` builds — every method, path, body and
//! query pair verified against the frozen escript — the `--json` envelopes it
//! emits, and the human tables/detail the module's formatters define.
//!
//! `list` and `query` envelopes are pinned **byte-equal** to the captured oracle
//! lines: the batch payload's maps stay below Elixir's small-map threshold, so
//! their term order matches serde's sorted order. `show` carries an `--expand`
//! payload that in real responses exceeds that threshold, so it is pinned as a
//! parsed `Value` (W1-R12 pins the shape; D1 owns ordering).
//!
//! Every test owns its environment: a `TempHome` for the config directory, a mock
//! server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT` credentials so no
//! credential resolution reaches the developer's keychain.

use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";

/// The frozen escript's literal `ado workitems list Alpha --json` line for the
/// `workitems_list` fixture (captured from the 0.5.0 binary, W1-R12).
const ORACLE_LIST_JSON: &str = r#"{"ok":true,"result":[{"_links":{"fields":{"href":"https://dev.azure.com/myorg/_apis/wit/fields"},"html":{"href":"https://dev.azure.com/myorg/Alpha/_workitems/edit/42"},"self":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/42"},"workItemRevisions":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/42/revisions"},"workItemType":{"href":"https://dev.azure.com/myorg/Alpha/_apis/wit/workItemTypes/Bug"},"workItemUpdates":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/42/updates"}},"fields":{"System.AssignedTo":{"descriptor":"aad.YzFkMmUzZjQtMDAwMS0wMDAxLTAwMDEtMDAwMDAwMDAwMDAx","displayName":"Alice Example","id":"c1d2e3f4-0001-0001-0001-000000000001","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2U=","uniqueName":"alice@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0001-0001-0001-000000000001"},"System.Id":42,"System.State":"Active","System.Title":"Checkout fails on expired cards","System.WorkItemType":"Bug"},"id":42,"rev":4,"url":"https://dev.azure.com/myorg/_apis/wit/workItems/42"},{"_links":{"fields":{"href":"https://dev.azure.com/myorg/_apis/wit/fields"},"html":{"href":"https://dev.azure.com/myorg/Alpha/_workitems/edit/43"},"self":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/43"},"workItemRevisions":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/43/revisions"},"workItemType":{"href":"https://dev.azure.com/myorg/Alpha/_apis/wit/workItemTypes/User%20Story"},"workItemUpdates":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/43/updates"}},"fields":{"System.AssignedTo":{"descriptor":"aad.YzFkMmUzZjQtMDAwMi0wMDAyLTAwMDItMDAwMDAwMDAwMDAy","displayName":"Bob Example","id":"c1d2e3f4-0002-0002-0002-000000000002","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2Uy","uniqueName":"bob@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0002-0002-0002-000000000002"},"System.Id":43,"System.State":"New","System.Title":"Payment retries for soft declines","System.WorkItemType":"User Story"},"id":43,"rev":2,"url":"https://dev.azure.com/myorg/_apis/wit/workItems/43"}]}"#;

/// The frozen escript's literal `ado workitems query Alpha --wiql … --json`
/// line for the `workitems_query` fixture.
const ORACLE_QUERY_JSON: &str = r#"{"ok":true,"result":[{"_links":{"fields":{"href":"https://dev.azure.com/myorg/_apis/wit/fields"},"html":{"href":"https://dev.azure.com/myorg/Alpha/_workitems/edit/7"},"self":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/7"},"workItemRevisions":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/7/revisions"},"workItemType":{"href":"https://dev.azure.com/myorg/Alpha/_apis/wit/workItemTypes/Task"},"workItemUpdates":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/7/updates"}},"fields":{"System.AssignedTo":{"descriptor":"aad.YzFkMmUzZjQtMDAwMy0wMDAzLTAwMDMtMDAwMDAwMDAwMDAz","displayName":"Carol Example","id":"c1d2e3f4-0003-0003-0003-000000000003","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2Uz","uniqueName":"carol@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0003-0003-0003-000000000003"},"System.Id":7,"System.State":"Active","System.Title":"Wire the reconciliation batch","System.WorkItemType":"Task"},"id":7,"rev":1,"url":"https://dev.azure.com/myorg/_apis/wit/workItems/7"},{"_links":{"fields":{"href":"https://dev.azure.com/myorg/_apis/wit/fields"},"html":{"href":"https://dev.azure.com/myorg/Alpha/_workitems/edit/8"},"self":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/8"},"workItemRevisions":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/8/revisions"},"workItemType":{"href":"https://dev.azure.com/myorg/Alpha/_apis/wit/workItemTypes/Bug"},"workItemUpdates":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/8/updates"}},"fields":{"System.AssignedTo":{"descriptor":"aad.YzFkMmUzZjQtMDAwMS0wMDAxLTAwMDEtMDAwMDAwMDAwMDAx","displayName":"Alice Example","id":"c1d2e3f4-0001-0001-0001-000000000001","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2U=","uniqueName":"alice@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0001-0001-0001-000000000001"},"System.Id":8,"System.State":"Resolved","System.Title":"Refund webhook retries forever","System.WorkItemType":"Bug"},"id":8,"rev":6,"url":"https://dev.azure.com/myorg/_apis/wit/workItems/8"},{"_links":{"fields":{"href":"https://dev.azure.com/myorg/_apis/wit/fields"},"html":{"href":"https://dev.azure.com/myorg/Alpha/_workitems/edit/9"},"self":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/9"},"workItemRevisions":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/9/revisions"},"workItemType":{"href":"https://dev.azure.com/myorg/Alpha/_apis/wit/workItemTypes/User%20Story"},"workItemUpdates":{"href":"https://dev.azure.com/myorg/_apis/wit/workItems/9/updates"}},"fields":{"System.Id":9,"System.State":"New","System.Title":"Export the ledger to CSV","System.WorkItemType":"User Story"},"id":9,"rev":1,"url":"https://dev.azure.com/myorg/_apis/wit/workItems/9"}]}"#;

/// The frozen escript's literal fallback line when the batch fetch fails: the
/// raw WIQL `workItems` (`{id,url}`) become the result, exit 0.
const ORACLE_FALLBACK_JSON: &str = r#"{"ok":true,"result":[{"id":42,"url":"https://dev.azure.com/myorg/_apis/wit/workItems/42"},{"id":43,"url":"https://dev.azure.com/myorg/_apis/wit/workItems/43"}]}"#;

const LIST_WIQL: &str = "SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], [System.AssignedTo] FROM WorkItems WHERE [System.TeamProject] = 'Alpha' ORDER BY [System.Id] DESC";

/// The repaired filtered query (D20): one valid clause per filter, in the
/// module's own order, where the frozen binary emits `WHERE AND … AND AND …`.
const LIST_WIQL_FILTERED: &str = "SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], [System.AssignedTo] FROM WorkItems WHERE [System.TeamProject] = 'Alpha' AND [System.WorkItemType] = 'Bug' AND [System.AssignedTo] = 'alice' AND [System.State] = 'Active' ORDER BY [System.Id] DESC";

const QUERY_WIQL: &str = "SELECT [System.Id] FROM WorkItems WHERE [System.State] = 'Active'";

const FIELDS_WIRE: &str =
    "System.Id%2CSystem.Title%2CSystem.State%2CSystem.WorkItemType%2CSystem.AssignedTo";

const WIQL: &str = "/myorg/Alpha/_apis/wit/wiql";
const WORK_ITEMS: &str = "/myorg/_apis/wit/workitems";
const WORK_ITEM_42: &str = "/myorg/_apis/wit/workitems/42";

fn command(home: &TempHome, server: &MockServer, args: &[&str]) -> Command {
    let mut command = ado_cmd();
    home.apply(&mut command);
    command
        .env_remove("ADO_ORG")
        .env_remove("ADO_PAT")
        .env_remove("ADO_SERVER")
        .env("ADO_ORG", ORG)
        .env("ADO_PAT", "test-pat")
        .env("ADO_SERVER", server.base_url())
        .args(args);
    command
}

fn run(home: &TempHome, server: &MockServer, args: &[&str]) -> Output {
    command(home, server, args).output().expect("run ado")
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

fn pair(key: &str, value: &str) -> (String, String) {
    (key.to_owned(), value.to_owned())
}

fn api_version() -> (String, String) {
    pair("api-version", "7.1")
}

/// The WIQL endpoint's answer: the ids the query resolves to plus the envelope a
/// real WIQL response carries.
fn wiql_response(ids: &[i64]) -> Value {
    json!({
        "queryType": "flat",
        "queryResultType": "workItem",
        "asOf": "2026-09-20T12:00:00.000Z",
        "columns": [
            {
                "referenceName": "System.Id",
                "name": "ID",
                "url": "https://dev.azure.com/myorg/_apis/wit/fields/System.Id",
            },
            {
                "referenceName": "System.Title",
                "name": "Title",
                "url": "https://dev.azure.com/myorg/_apis/wit/fields/System.Title",
            },
        ],
        "workItems": ids
            .iter()
            .map(|id| {
                json!({
                    "id": id,
                    "url": format!("https://dev.azure.com/myorg/_apis/wit/workItems/{id}"),
                })
            })
            .collect::<Vec<_>>(),
    })
}

/// The WIQL expectation every flow starts with.
fn expect_wiql(server: &MockServer, ids: &[i64]) {
    server.expect_query(
        "POST",
        WIQL,
        &[("api-version", "7.1")],
        MockResponse::json(200, wiql_response(ids)),
    );
}

/// The batch expectation: the fixture only answers the ids it was asked for.
fn expect_batch(server: &MockServer, fixture: &str, ids: &str) {
    server.expect_query(
        "GET",
        WORK_ITEMS,
        &[
            ("api-version", "7.1"),
            ("ids", ids),
            ("fields", FIELDS_WIRE),
        ],
        MockResponse::from_fixture(fixture),
    );
}

/// The two requests the list/query flow sends, in order: the WIQL POST and the
/// batch GET.
fn received_wiql_and_batch(server: &MockServer) -> (RecordedRequest, RecordedRequest) {
    let received = server.received();

    assert_eq!(
        received.len(),
        2,
        "expected the WIQL POST and the batch GET"
    );

    (received[0].clone(), received[1].clone())
}

/// The WIQL POST as sent: no query beyond the version, and the query string as
/// the exact JSON body.
fn assert_wiql_request(request: &RecordedRequest, path: &str, wiql: &str) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, path);
    assert_eq!(
        request.query_pairs(),
        vec![api_version()],
        "the WIQL POST carries only api-version"
    );
    assert_eq!(
        request.body.as_deref(),
        Some(format!(r#"{{"query":"{wiql}"}}"#).as_str()),
        "the exact WIQL body"
    );
}

fn assert_batch_request(request: &RecordedRequest, ids: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, WORK_ITEMS);
    assert_eq!(
        request.query_pairs(),
        vec![api_version(), pair("ids", ids), pair("fields", FIELDS_WIRE)],
        "the batch GET's wire-form pairs, in the client's order (D12 compares parsed pairs)"
    );
}

fn assert_no_table_bytes(stdout: &str) {
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
    for marker in ['\u{2500}', '\u{2501}', '\u{2502}', '\u{256d}'] {
        assert!(
            !stdout.contains(marker),
            "table or ANSI byte {marker:?} reached output: {stdout:?}"
        );
    }
}

#[test]
fn list_sends_the_unfiltered_wiql_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[42, 43]);
    expect_batch(&server, "workitems_list", "42%2C43");

    let output = run(&home, &server, &["workitems", "list", "Alpha", "--json"]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_LIST_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": fixture("workitems_list")["value"].clone()}),
        "the value envelope carries the fixture's array under result"
    );

    let (wiql, batch) = received_wiql_and_batch(&server);
    assert_wiql_request(&wiql, WIQL, LIST_WIQL);
    assert_batch_request(&batch, "42%2C43");
}

#[test]
fn list_404_is_the_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        WIQL,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(&home, &server, &["workitems", "list", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Resource not found. Check the project/repo/build ID and your permissions.")
    );
}

#[test]
fn list_500_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        WIQL,
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(&home, &server, &["workitems", "list", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
    assert_eq!(
        envelope["error"]["message"],
        json!("Azure DevOps server error. Retry later.")
    );
}

/// The repaired filtered query (D20): the frozen binary prepends `AND …` to each
/// clause and joins with ` AND `, sending `WHERE AND … AND AND …` — a query Azure
/// cannot parse. This pins the valid one.
#[test]
fn list_filters_build_one_valid_wiql_clause_each() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[42, 43]);
    expect_batch(&server, "workitems_list", "42%2C43");

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "list",
            "Alpha",
            "--type",
            "Bug",
            "--assigned-to",
            "alice",
            "--state",
            "Active",
            "--json",
        ],
    );

    assert_success(&output);

    let (wiql, _) = received_wiql_and_batch(&server);
    assert_wiql_request(&wiql, WIQL, LIST_WIQL_FILTERED);
}

#[test]
fn list_top_slices_the_ids_before_the_batch_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[42, 43]);
    expect_batch(&server, "workitems_list", "42");

    let output = run(
        &home,
        &server,
        &["workitems", "list", "Alpha", "--top", "1", "--json"],
    );

    assert_success(&output);

    let (wiql, batch) = received_wiql_and_batch(&server);
    assert_eq!(
        wiql.query_pairs(),
        vec![api_version()],
        "--top never reaches the wire as a param"
    );
    assert_batch_request(&batch, "42");
    assert!(
        !wiql.query.contains("top") && !batch.query.contains("top"),
        "--top is a client-side slice, exactly as Enum.take/2 is"
    );
}

/// `Enum.take(items, 0)` is `[]` — Elixir's `if top` is truthy for 0 — so the
/// empty branch answers without a batch request, and `--json` gets the envelope
/// (D21) where the frozen binary printed human text under `--json`.
#[test]
fn list_top_zero_answers_the_empty_envelope_without_a_batch_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[42, 43]);

    let output = run(
        &home,
        &server,
        &["workitems", "list", "Alpha", "--top", "0", "--json"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");
    assert_eq!(
        server.received().len(),
        1,
        "an empty slice sends no batch request"
    );
}

/// `Enum.take(list, -1)` takes the last item; the negative count is a slice of
/// the WIQL ids, like every other `--top`.
#[test]
fn list_negative_top_takes_from_the_end() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[42, 43]);
    expect_batch(&server, "workitems_list", "43");

    let output = run(
        &home,
        &server,
        &["workitems", "list", "Alpha", "--top", "-1", "--json"],
    );

    assert_success(&output);

    let (_, batch) = received_wiql_and_batch(&server);
    assert_batch_request(&batch, "43");
}

/// `URI.encode/1` encodes a space as `%20`; the stricter path encoder (D22) also
/// escapes `/` and `?`, which Elixir leaves in place (changing the URL's
/// structure) while Azure names cannot contain them.
#[test]
fn list_encodes_the_project_name_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "POST",
        "/myorg/My%20Project/_apis/wit/wiql",
        &[("api-version", "7.1")],
        MockResponse::json(200, wiql_response(&[])),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "list", "My Project", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/wit/wiql",
        "a space is %20 in a path, not +"
    );

    let strict = MockServer::start();
    strict.expect_query(
        "POST",
        "/myorg/a%2Fb%3Fc/_apis/wit/wiql",
        &[("api-version", "7.1")],
        MockResponse::json(200, wiql_response(&[])),
    );

    let output = run(&home, &strict, &["workitems", "list", "a/b?c", "--json"]);

    assert_success(&output);
    assert_eq!(
        strict.received()[0].path,
        "/myorg/a%2Fb%3Fc/_apis/wit/wiql",
        "the stricter encoder escapes / and ? so a name cannot change the URL (D22)"
    );
}

#[test]
fn list_escapes_single_quotes_in_the_wiql() {
    let home = TempHome::new();
    let server = MockServer::start();
    // The path carries the stricter encoding (D22): the frozen binary sends the
    // apostrophe literally, this encoder escapes it as %27.
    server.expect_query(
        "POST",
        "/myorg/Alpha%27s/_apis/wit/wiql",
        &[("api-version", "7.1")],
        MockResponse::json(200, wiql_response(&[])),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "list", "Alpha's", "--type", "Bug's", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received().len(),
        1,
        "an empty WIQL answer sends no batch request"
    );
    assert_wiql_request(
        &server.received()[0],
        "/myorg/Alpha%27s/_apis/wit/wiql",
        "SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], [System.AssignedTo] FROM WorkItems WHERE [System.TeamProject] = 'Alpha''s' AND [System.WorkItemType] = 'Bug''s' ORDER BY [System.Id] DESC",
    );
}

#[test]
fn list_empty_wiql_human_output_says_no_work_items_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[]);

    let output = run(&home, &server, &["workitems", "list", "Alpha"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No work items found.\n");
    assert_eq!(server.received().len(), 1);
}

/// The module's deliberate fallback branch: any batch failure degrades to the
/// raw WIQL `{id,url}` items as a success (W4 candidate for tightening, since it
/// catches transport errors too).
#[test]
fn list_falls_back_to_the_wiql_items_when_the_batch_fetch_fails() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[42, 43]);
    server.expect_query(
        "GET",
        WORK_ITEMS,
        &[("api-version", "7.1")],
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(&home, &server, &["workitems", "list", "Alpha", "--json"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{ORACLE_FALLBACK_JSON}\n"),
        "the oracle's exact fallback bytes"
    );

    let (_, batch) = received_wiql_and_batch(&server);
    assert_batch_request(&batch, "42%2C43");
}

#[test]
fn list_json_emits_no_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[42, 43]);
    expect_batch(&server, "workitems_list", "42%2C43");

    let output = run(&home, &server, &["workitems", "list", "Alpha", "--json"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let envelope: Value =
        serde_json::from_str(&stdout).expect("stdout is exactly one JSON document");

    assert_eq!(envelope["ok"], json!(true));
    assert_eq!(
        envelope["result"].as_array().map(Vec::len),
        Some(2),
        "the envelope is the value form"
    );
    assert!(
        envelope.get("count").is_none() && envelope.get("items").is_none(),
        "the count/items list form never ships (W1-R12): {stdout}"
    );
    assert!(
        !stdout.lines().any(|line| line.starts_with("ID")),
        "a table header reached --json output: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn list_human_output_is_the_work_items_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[42, 43]);
    expect_batch(&server, "workitems_list", "42%2C43");

    let output = run(&home, &server, &["workitems", "list", "Alpha"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        4,
        "a header row, a rule row and one row per work item: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header row: {stdout}");
    let type_at = lines[0].find("Type").expect("the Type header");
    let title_at = lines[0].find("Title").expect("the Title header");
    let state_at = lines[0].find("State").expect("the State header");
    assert!(
        type_at > 0 && title_at > type_at && state_at > title_at,
        "the header order is ID, Type, Title, State (the module's formatter, not the help text): {stdout}"
    );
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].starts_with("42")
            && lines[2].contains("Bug")
            && lines[2].contains("Checkout fails on expired cards")
            && lines[2].ends_with("Active"),
        "the first row: {stdout}"
    );
    assert!(
        lines[3].starts_with("43") && lines[3].contains("User Story") && lines[3].ends_with("New"),
        "the second row: {stdout}"
    );
    assert!(
        !stdout.contains("Alice Example") && !stdout.contains("Bob Example"),
        "the formatter has no Assigned To column, whatever the help text says: {stdout}"
    );
    assert!(
        !stdout.contains("https://dev.azure.com/myorg/Alpha/_workitems/edit/42"),
        "no link column reached the table: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn list_without_a_project_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems", "list", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("PROJECT"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn show_sends_the_expand_param_and_parses_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        WORK_ITEM_42,
        &[("api-version", "7.1"), ("%24expand", "all")],
        MockResponse::from_fixture("workitems_show"),
    );

    let output = run(&home, &server, &["workitems", "show", "42", "--json"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");

    assert_eq!(
        envelope,
        json!({"ok": true, "result": fixture("workitems_show")}),
        "the value envelope carries the work item object: the --expand payload is \
         pinned parsed (its real shape exceeds Elixir's small-map term order, and \
         W1-R12 pins the shape while D1 owns ordering)"
    );
    assert_eq!(
        server.received()[0].query_pairs(),
        vec![api_version(), pair("%24expand", "all")],
        "the default --expand reaches the wire as $expand=all"
    );
    assert_eq!(server.received()[0].path, WORK_ITEM_42);
    assert_no_table_bytes(&stdout);
}

#[test]
fn show_expand_replaces_the_default() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        WORK_ITEM_42,
        &[("api-version", "7.1"), ("%24expand", "relations")],
        MockResponse::from_fixture("workitems_show"),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "show", "42", "--expand", "relations", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].query_pairs(),
        vec![api_version(), pair("%24expand", "relations")],
        "--expand replaces the module's all default"
    );
}

#[test]
fn show_human_output_is_the_work_item_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        WORK_ITEM_42,
        MockResponse::from_fixture("workitems_show"),
    );

    let output = run(&home, &server, &["workitems", "show", "42"]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(stdout.contains("\nWork Item #42\n"), "stdout: {stdout}");
    assert!(stdout.contains("  Type:        Bug\n"), "stdout: {stdout}");
    assert!(
        stdout.contains("  Title:       Checkout fails on expired cards\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  State:       Active\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Assigned To: Alice Example\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Created By:  Bob Example\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Created:     2026-09-14T09:31:07.83Z\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(
            "  Description: Expired cards are declined but the checkout page still shows the order as placed, so customers retry and are double-charged....\n"
        ),
        "the module slices the description at 200 graphemes and appends an ellipsis: {stdout}"
    );
    assert!(
        stdout.contains("  URL:         https://dev.azure.com/myorg/_apis/wit/workItems/42\n"),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
}

#[test]
fn show_without_the_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems", "show", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn show_with_a_non_numeric_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems", "show", "abc", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("invalid value"),
        "the Elixir argument is :integer, so clap rejects a non-number: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn show_404_reports_the_work_item_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/wit/workitems/999",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(&home, &server, &["workitems", "show", "999", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Work item #999 not found")
    );
}

#[test]
fn query_sends_the_user_wiql_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[7, 8, 9]);
    expect_batch(&server, "workitems_query", "7%2C8%2C9");

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "query",
            "Alpha",
            "--wiql",
            QUERY_WIQL,
            "--json",
        ],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_QUERY_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let (wiql, batch) = received_wiql_and_batch(&server);
    assert_wiql_request(&wiql, WIQL, QUERY_WIQL);
    assert_batch_request(&batch, "7%2C8%2C9");
}

#[test]
fn query_top_slices_the_ids_before_the_batch_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[7, 8, 9]);
    expect_batch(&server, "workitems_query", "7");

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "query",
            "Alpha",
            "--wiql",
            QUERY_WIQL,
            "--top",
            "1",
            "--json",
        ],
    );

    assert_success(&output);

    let (_, batch) = received_wiql_and_batch(&server);
    assert_batch_request(&batch, "7");
}

#[test]
fn query_encodes_the_project_name_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "POST",
        "/myorg/My%20Project/_apis/wit/wiql",
        &[("api-version", "7.1")],
        MockResponse::json(200, wiql_response(&[])),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "query",
            "My Project",
            "--wiql",
            QUERY_WIQL,
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/wit/wiql",
        "a space is %20 in a path, not +"
    );
}

#[test]
fn query_human_output_is_the_work_items_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[7, 8, 9]);
    expect_batch(&server, "workitems_query", "7%2C8%2C9");

    let output = run(
        &home,
        &server,
        &["workitems", "query", "Alpha", "--wiql", QUERY_WIQL],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        5,
        "a header row, a rule row and one row per work item: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header row: {stdout}");
    assert!(
        lines[2].starts_with("7") && lines[2].ends_with("Active"),
        "the first row: {stdout}"
    );
    assert!(
        lines[3].starts_with("8") && lines[3].ends_with("Resolved"),
        "the second row: {stdout}"
    );
    assert!(
        lines[4].starts_with("9") && lines[4].ends_with("New"),
        "the third row: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

/// `--wiql` is optional in the frozen tree (`required: false`); its absence is
/// the module's own validation message, not a clap parse error. Under `--json`
/// that is the error envelope, like every other command error (D4 class).
#[test]
fn query_without_wiql_is_a_validation_error_without_a_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems", "query", "Alpha"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope on the human path: {}",
        stdout_of(&output)
    );
    assert_eq!(
        stderr_of(&output),
        "[Validation error] --wiql is required for the query command\n"
    );
    assert!(
        server.received().is_empty(),
        "a validation error sends no request"
    );
}

#[test]
fn query_without_wiql_under_json_is_the_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems", "query", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({
            "ok": false,
            "error": {
                "code": "validation_error",
                "message": "--wiql is required for the query command",
            },
        })
    );
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn workitems_without_a_subcommand_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr_of(&output),
        "[Validation error] missing sub-command\n"
    );
    assert!(
        stdout_of(&output).is_empty(),
        "stdout: {}",
        stdout_of(&output)
    );
}

/// A closed stdout is a silent success: the table write returns EPIPE and the
/// binary exits 0 with no diagnostic (spec §6.3, R19).
#[test]
fn a_closed_stdout_is_a_silent_success() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_wiql(&server, &[42, 43]);
    expect_batch(&server, "workitems_list", "42%2C43");

    let mut child = command(&home, &server, &["workitems", "list", "Alpha"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ado");
    drop(child.stdout.take());

    let output = child.wait_with_output().expect("wait for ado");

    assert_eq!(server.received().len(), 2, "the table was rendered");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(output.stderr.is_empty(), "stderr: {}", stderr_of(&output));
}

// ── write paths: create, update, delete ──────────────────────────────────
//
// Every method, path and body below is the request the frozen escript sent,
// captured against the standalone mock; every response the tests serve is that
// capture's answer. The two work item writes carry `application/json-patch+json`
// and a **JSON-patch array**, not an object: `create` builds its six `add`
// operations in the module's order (title, description, assigned-to, state,
// priority, tags), and `update` builds five in *its* order (title, description,
// state, assigned-to, priority) and prepends a `replace` for tags when it is
// given. The `--json` success output is this build's value/message envelope where
// the frozen CLI prints its human success line (D33). Stdin is always scripted:
// `/dev/null` is EOF, and the captures show no prompt for the delete (R5) — it
// proceeds on EOF and on `n`, and the tree has no `--force`.

use std::io::Write;

const CREATE_BUG_PATH: &str = "/myorg/Alpha/_apis/wit/workitems/$Bug";
const CREATE_USER_STORY_PATH: &str = "/myorg/Alpha/_apis/wit/workitems/$User%20Story";
const UPDATE_42_PATH: &str = "/myorg/_apis/wit/workitems/42";
const UPDATE_43_PATH: &str = "/myorg/_apis/wit/workitems/43";
const DELETE_42_PATH: &str = "/myorg/_apis/wit/workitems/42";
const JSON_PATCH: &str = "application/json-patch+json";

/// Runs the binary with `stdin` written to a pipe (never a terminal); an empty
/// slice is EOF.
fn run_with_stdin(home: &TempHome, server: &MockServer, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = command(home, server, args)
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

/// The request body as JSON — an array for the JSON-patch writes.
fn sent_body(request: &RecordedRequest) -> Value {
    serde_json::from_str(request.body.as_deref().expect("a request body"))
        .expect("the request body is JSON")
}

/// The captured `POST .../workitems/$Bug` answer.
fn created_bug() -> Value {
    json!({
        "id": 42,
        "rev": 1,
        "fields": {
            "System.Id": 42,
            "System.Title": "Checkout fails on expired cards",
            "System.WorkItemType": "Bug",
            "System.State": "New",
        },
        "_links": {
            "html": {"href": "https://dev.azure.com/myorg/Alpha/_workitems/edit/42"},
        },
        "url": "https://dev.azure.com/myorg/_apis/wit/workItems/42",
    })
}

/// The captured `PATCH .../workitems/42` answer.
fn updated_work_item() -> Value {
    json!({
        "id": 42,
        "rev": 5,
        "fields": {
            "System.Id": 42,
            "System.Title": "Checkout fails on expired cards (renamed)",
            "System.State": "Active",
        },
    })
}

#[test]
fn create_posts_the_captured_json_patch_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        CREATE_BUG_PATH,
        MockResponse::json(200, created_bug()),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "create",
            "Alpha",
            "--type",
            "Bug",
            "--title",
            "Checkout fails on expired cards",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "result": created_bug()}),
        "the created work item is the value envelope"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "POST");
    assert_eq!(received[0].path, CREATE_BUG_PATH);
    assert_eq!(
        received[0].query_pairs(),
        vec![api_version()],
        "the write carries only api-version"
    );
    assert_eq!(
        received[0].header("content-type"),
        Some(JSON_PATCH),
        "captured: the write API requires a JSON-patch content type"
    );
    assert_eq!(
        sent_body(&received[0]),
        json!([{"op": "add", "path": "/fields/System.Title", "value": "Checkout fails on expired cards"}]),
        "an array of patch operations, not a plain object — the field set the capture names"
    );
}

#[test]
fn create_full_patch_has_the_captured_field_order_and_integer_priority() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        CREATE_USER_STORY_PATH,
        MockResponse::json(200, created_bug()),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "create",
            "Alpha",
            "--type",
            "User Story",
            "--title",
            "Payment retries for soft declines",
            "--description",
            "Description body",
            "--assigned-to",
            "alice",
            "--state",
            "Active",
            "--priority",
            "2",
            "--tags",
            "frontend,ui",
            "--json",
        ],
    );

    assert_success(&output);

    let received = server.received();
    assert_eq!(
        received[0].path, CREATE_USER_STORY_PATH,
        "the type is a path segment: a space becomes %20"
    );
    assert_eq!(
        sent_body(&received[0]),
        json!([
            {"op": "add", "path": "/fields/System.Title", "value": "Payment retries for soft declines"},
            {"op": "add", "path": "/fields/System.Description", "value": "Description body"},
            {"op": "add", "path": "/fields/System.AssignedTo", "value": "alice"},
            {"op": "add", "path": "/fields/System.State", "value": "Active"},
            {"op": "add", "path": "/fields/Microsoft.VSTS.Common.Priority", "value": 2},
            {"op": "add", "path": "/fields/System.Tags", "value": "frontend,ui"},
        ]),
        "captured: create's order is title, description, assigned-to, state, priority, tags"
    );
    assert_eq!(
        received[0].header("content-type"),
        Some(JSON_PATCH),
        "captured: the JSON-patch content type on the full patch too"
    );
}

#[test]
fn create_human_output_is_the_module_lines() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        CREATE_BUG_PATH,
        MockResponse::json(200, created_bug()),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "create",
            "Alpha",
            "--type",
            "Bug",
            "--title",
            "Checkout fails on expired cards",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Work item #42 created: Checkout fails on expired cards\n  Type:  Bug\n  State: New\n  URL:   https://dev.azure.com/myorg/Alpha/_workitems/edit/42\n",
        "the module's success line and three detail lines"
    );
}

#[test]
fn create_without_type_is_a_usage_error_that_names_the_option() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems", "create", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--type"),
        "the required option is named: {}",
        stderr_of(&output)
    );
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a missing required option sends no request"
    );
}

#[test]
fn create_without_title_is_a_usage_error_that_names_the_option() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["workitems", "create", "Alpha", "--type", "Bug", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--title"),
        "the required option is named: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn create_400_is_the_error_envelope_with_the_upstream_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        "/myorg/Broken/_apis/wit/workitems/$Bug",
        MockResponse::json(
            400,
            json!({"message": "TF400898: An Internal Error Occurred."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "create",
            "Broken",
            "--type",
            "Bug",
            "--title",
            "T",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(
        envelope["error"]["code"],
        json!("api_error"),
        "the frozen taxonomy: 400 is api_error, not validation_error (R7)"
    );
    assert_eq!(envelope["error"]["status"], json!(400));
    assert_eq!(
        envelope["error"]["details"]["body"],
        json!(r#"{"message":"TF400898: An Internal Error Occurred."}"#),
        "this build keeps the upstream bytes (D24)"
    );
}

#[test]
fn update_sends_a_json_patch_array_not_a_plain_object() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        UPDATE_42_PATH,
        MockResponse::json(200, updated_work_item()),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "update",
            "42",
            "--title",
            "Checkout fails on expired cards (renamed)",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "result": updated_work_item()}),
        "the updated work item is the value envelope"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "PATCH");
    assert_eq!(received[0].path, UPDATE_42_PATH);
    assert_eq!(received[0].header("content-type"), Some(JSON_PATCH));
    assert_eq!(
        sent_body(&received[0]),
        json!([{"op": "add", "path": "/fields/System.Title", "value": "Checkout fails on expired cards (renamed)"}]),
        "the body is a patch array: a plain object here is the defect this task exists to avoid"
    );
}

#[test]
fn update_all_fields_prepends_tags_with_the_replace_op() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        UPDATE_43_PATH,
        MockResponse::json(200, updated_work_item()),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "update",
            "43",
            "--title",
            "Payment retries for soft declines (renamed)",
            "--description",
            "New body",
            "--state",
            "Closed",
            "--assigned-to",
            "bob",
            "--priority",
            "1",
            "--tags",
            "a,b",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[0]),
        json!([
            {"op": "replace", "path": "/fields/System.Tags", "value": "a,b"},
            {"op": "add", "path": "/fields/System.Title", "value": "Payment retries for soft declines (renamed)"},
            {"op": "add", "path": "/fields/System.Description", "value": "New body"},
            {"op": "add", "path": "/fields/System.State", "value": "Closed"},
            {"op": "add", "path": "/fields/System.AssignedTo", "value": "bob"},
            {"op": "add", "path": "/fields/Microsoft.VSTS.Common.Priority", "value": 1},
        ]),
        "captured: update's order is tags (replace, first), then title, description, state, assigned-to, priority"
    );
}

#[test]
fn update_human_output_is_the_module_lines() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        UPDATE_42_PATH,
        MockResponse::json(200, updated_work_item()),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "update",
            "42",
            "--title",
            "Checkout fails on expired cards (renamed)",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Work item #42 updated.\n  Title: Checkout fails on expired cards (renamed)\n  State: Active\n"
    );
}

#[test]
fn update_without_options_is_a_validation_error_without_a_request() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems", "update", "42", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({
            "ok": false,
            "error": {
                "code": "validation_error",
                "message": "At least one field to update is required (--title, --state, --assigned-to, etc.)",
            },
        }),
        "the module's guard, as this build's envelope"
    );
    assert!(server.received().is_empty());
}

#[test]
fn update_404_reports_the_work_item_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        "/myorg/_apis/wit/workitems/999",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "update", "999", "--title", "T", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Work item #999 not found")
    );
}

#[test]
fn delete_sends_the_delete_and_emits_the_message_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        DELETE_42_PATH,
        MockResponse::json(200, json!({"id": 42, "rev": 6})),
    );

    let output = run(&home, &server, &["workitems", "delete", "42", "--json"]);

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "message": "Work item #42 deleted."}),
        "a delete reports a message, not an API value"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "DELETE");
    assert_eq!(received[0].path, DELETE_42_PATH);
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

/// R5's evidence: with `n` on stdin the DELETE still goes out, so a prompt added
/// here would fail this test.
#[test]
fn delete_with_stdin_n_sends_the_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        DELETE_42_PATH,
        MockResponse::json(200, json!({"id": 42, "rev": 6})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["workitems", "delete", "42", "--json"],
        b"n\n",
    );

    assert_success(&output);
    assert_eq!(server.received().len(), 1, "the delete is not prompted");
}

/// R5's other blind spot: EOF (the default stdin) must not hide a prompt either.
#[test]
fn delete_with_eof_sends_the_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        DELETE_42_PATH,
        MockResponse::json(200, json!({"id": 42, "rev": 6})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["workitems", "delete", "42", "--json"],
        b"",
    );

    assert_success(&output);
    assert_eq!(server.received().len(), 1, "the delete is not prompted");
}

#[test]
fn delete_human_output_is_the_module_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        DELETE_42_PATH,
        MockResponse::json(200, json!({"id": 42, "rev": 6})),
    );

    let output = run(&home, &server, &["workitems", "delete", "42"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Work item #42 deleted.\n");
}

#[test]
fn delete_404_reports_the_work_item_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        "/myorg/_apis/wit/workitems/999",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(&home, &server, &["workitems", "delete", "999", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Work item #999 not found")
    );
}

#[test]
fn delete_without_an_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems", "delete", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("id"),
        "the usage error names the positional: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

/// The tree has no `--force` on this delete (captured: `invalid option --force`),
/// so it must not grow one for symmetry.
#[test]
fn delete_has_no_force_flag() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["workitems", "delete", "42", "--force", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--force"),
        "the unknown flag is named: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

// ── Task 8: work item comments and attachments ─────────────────────────────
//
// The module's path spellings are not uniform and the captures keep them: the
// comments read uses `workItems` (capital I) where every other path uses
// `workitems`. Both writes are the module's one-operation `System.History`
// JSON patch under `application/json-patch+json` — the same content type as
// Task 7's writes. The download is a metadata GET and then a raw-content GET
// **on the same path** (the raw one carrying the resolved filename), so its
// byte tests register two expectations for one path: the mock answers
// newest-first. D28's streamed write is the shared helper the artifact download
// already uses, so the failed-stream invariants read the same way here.

use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::thread;

const COMMENTS_42_PATH: &str = "/myorg/_apis/wit/workItems/42/comments";
const COMMENTS_8_PATH: &str = "/myorg/_apis/wit/workItems/8/comments";
const COMMENTS_NO_KEY_PATH: &str = "/myorg/_apis/wit/workItems/7/comments";
const COMMENTS_404_PATH: &str = "/myorg/_apis/wit/workItems/999/comments";
const COMMENT_ADD_PATH: &str = "/myorg/_apis/wit/workitems/500";
const COMMENT_UPDATE_PATH: &str = "/myorg/_apis/wit/workitems/501";
const COMMENT_ADD_404_PATH: &str = "/myorg/_apis/wit/workitems/896";
const COMMENT_UPDATE_404_PATH: &str = "/myorg/_apis/wit/workitems/897";
const ATTACHMENTS_42_PATH: &str = "/myorg/_apis/wit/workitems/42/attachments";
const ATTACHMENTS_8_PATH: &str = "/myorg/_apis/wit/workitems/8/attachments";
const ATTACHMENTS_NO_KEY_PATH: &str = "/myorg/_apis/wit/workitems/7/attachments";
const ATTACHMENTS_404_PATH: &str = "/myorg/_apis/wit/workitems/999/attachments";
const ATTACHMENT_1_PATH: &str = "/myorg/_apis/wit/attachments/att-1";
const ATTACHMENT_2_PATH: &str = "/myorg/_apis/wit/attachments/att-2";
const ATTACHMENT_3_PATH: &str = "/myorg/_apis/wit/attachments/att-3";
const ATTACHMENT_404_PATH: &str = "/myorg/_apis/wit/attachments/att-404";

/// The reader-level failure a short body provokes: ureq's own constant. The
/// scripted listener writes the whole header block before it drops the stream,
/// so once `get_raw` returns, a reader failure is the only thing that can fail.
const MID_BODY_FAILURE: &str = "[Network error] Request failed: io: Peer disconnected";

fn run_in(home: &TempHome, server: &MockServer, cwd: &Path, args: &[&str]) -> Output {
    command(home, server, args)
        .current_dir(cwd)
        .output()
        .expect("run ado")
}

/// The captured `GET .../workItems/42/comments` answer, with the fields the
/// module's formatter reads.
fn comments_body() -> Value {
    json!({
        "totalCount": 2,
        "count": 2,
        "comments": [
            {
                "id": 7,
                "text": "Looks good",
                "createdBy": {"displayName": "Alice Example", "id": "u1"},
                "createdDate": "2026-09-20T12:00:00.000Z",
                "url": "https://dev.azure.com/myorg/_apis/wit/workItems/42/comments/7",
            },
            {
                "id": 8,
                "text": "Second note",
                "createdBy": {"displayName": "Bob Example"},
                "createdDate": "2026-09-21T09:30:00.000Z",
            },
        ],
    })
}

/// The captured `GET .../workitems/42/attachments` answer.
fn attachments_body() -> Value {
    json!({
        "count": 2,
        "attachments": [
            {
                "id": "att-1",
                "url": "https://dev.azure.com/myorg/_apis/wit/attachments/att-1",
                "attributes": {"name": "report.pdf"},
            },
            {
                "id": "att-2",
                "url": "https://dev.azure.com/myorg/_apis/wit/attachments/att-2",
                "attributes": {"name": "notes.txt"},
            },
        ],
    })
}

/// The metadata answer the download reads its filename from; the raw GET is
/// answered separately, from the same path.
fn attachment_metadata(id: &str, name: Option<&str>) -> Value {
    match name {
        Some(name) => json!({
            "id": id,
            "url": format!("https://dev.azure.com/myorg/_apis/wit/attachments/{id}"),
            "attributes": {"name": name},
        }),
        None => json!({"id": id, "attributes": {}}),
    }
}

/// Registers the download's two answers for one path: the raw bytes first, so
/// the metadata expectation (newest-first) answers the metadata GET and leaves
/// the bytes for the raw GET.
fn expect_download(server: &MockServer, path: &str, metadata: Value, bytes: Vec<u8>) {
    server.expect("GET", path, MockResponse::bytes(200, bytes));
    server.expect("GET", path, MockResponse::json(200, metadata));
}

/// The `.tmp` files left in `home`, which must be none after any download: a
/// successful rename removes the temp, a failed stream removes it too.
fn temp_siblings(home: &TempHome) -> Vec<String> {
    fs::read_dir(home.path())
        .expect("the temp home")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect()
}

/// A scripted listener for the download's two requests on one path: the first
/// connection gets the metadata JSON (with `connection: close`, so the client
/// opens a fresh connection for the raw GET), the second gets headers declaring
/// `declared_len` bytes and then a short prefix before the stream drops
/// mid-body.
fn spawn_truncating_attachment(metadata: String, prefix: Vec<u8>, declared_len: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a truncating attachment server");
    let port = listener.local_addr().expect("the bound address").port();

    thread::spawn(move || {
        let mut answered_metadata = false;

        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };

            loop {
                let mut head = [0u8; 4096];
                let Ok(read) = stream.read(&mut head) else {
                    break;
                };

                if read == 0 {
                    break;
                }

                if !answered_metadata {
                    answered_metadata = true;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
                        metadata.len(),
                        metadata
                    );
                    let _ = stream.flush();
                    continue;
                }

                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ncontent-length: {declared_len}\r\n\r\n"
                );
                let _ = stream.write_all(&prefix);
                let _ = stream.flush();

                return;
            }
        }
    });

    port
}

#[test]
fn comments_list_sends_the_module_path_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COMMENTS_42_PATH,
        MockResponse::json(200, comments_body()),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "comments", "list", "42", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "result": comments_body()["comments"]}),
        "the comments array is the value envelope"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "GET");
    assert_eq!(
        received[0].path, COMMENTS_42_PATH,
        "the comments read keeps the module's capital-I path"
    );
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

#[test]
fn comments_list_human_output_is_the_module_layout() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COMMENTS_42_PATH,
        MockResponse::json(200, comments_body()),
    );

    let output = run(&home, &server, &["workitems", "comments", "list", "42"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "\n  [7] Alice Example (2026-09-20T12:00:00.000Z)\n  Looks good\n\n  [8] Bob Example (2026-09-21T09:30:00.000Z)\n  Second note\n\n"
    );
}

#[test]
fn comments_list_with_an_empty_array_is_the_empty_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COMMENTS_8_PATH,
        MockResponse::json(200, json!({"count": 0, "comments": []})),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "comments", "list", "8", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "result": []}),
        "a present-but-empty comments array is the empty value envelope on both sides"
    );
}

/// Captured: against a body without a `comments` key the frozen CLI prints
/// `No comments found.` even under `--json`; this build emits the empty value
/// envelope (D21).
#[test]
fn comments_list_without_the_comments_key_is_an_empty_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COMMENTS_NO_KEY_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "comments", "list", "7", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "result": []}),
        "the missing key is the module's own not-found branch, not an error"
    );
}

#[test]
fn comments_list_without_the_comments_key_human_says_no_comments_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COMMENTS_NO_KEY_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["workitems", "comments", "list", "7"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No comments found.\n");
}

#[test]
fn comments_list_404_is_the_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COMMENTS_404_PATH,
        MockResponse::json(
            404,
            json!({"message": "TF401232: Work item 999 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "comments", "list", "999", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Resource not found. Check the project/repo/build ID and your permissions.")
    );
    assert_eq!(
        envelope["error"]["details"]["body"],
        json!(r#"{"message":"TF401232: Work item 999 does not exist."}"#),
        "this build keeps the upstream bytes (D24)"
    );
}

#[test]
fn comments_list_500_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COMMENTS_NO_KEY_PATH,
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "comments", "list", "7", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
    assert_eq!(
        envelope["error"]["message"],
        json!("Azure DevOps server error. Retry later.")
    );
}

#[test]
fn comments_list_without_an_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["workitems", "comments", "list", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("id"),
        "the usage error names the positional: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn comments_add_posts_the_system_history_patch_and_emits_the_message_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        COMMENT_ADD_PATH,
        MockResponse::json(200, json!({"id": 500, "rev": 2})),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "add",
            "500",
            "--text",
            "Looks good",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "message": "Comment added to work item #500."}),
        "the frozen success line under --json is D33's class; this build emits the message envelope"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "PATCH");
    assert_eq!(received[0].path, COMMENT_ADD_PATH);
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
    assert_eq!(
        received[0].header("content-type"),
        Some(JSON_PATCH),
        "captured: the history write is a JSON patch"
    );
    assert_eq!(
        sent_body(&received[0]),
        json!([{"op": "add", "path": "/fields/System.History", "value": "Looks good"}]),
        "the captured one-operation patch, not a plain object"
    );
}

#[test]
fn comments_add_human_output_is_the_module_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        COMMENT_ADD_PATH,
        MockResponse::json(200, json!({"id": 500, "rev": 2})),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "add",
            "500",
            "--text",
            "Looks good",
        ],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Comment added to work item #500.\n");
}

/// No prompt: with `n` on stdin the PATCH still goes out (the capture sends it
/// on `n` and on EOF too).
#[test]
fn comments_add_with_stdin_n_sends_the_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        COMMENT_ADD_PATH,
        MockResponse::json(200, json!({"id": 500, "rev": 2})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "add",
            "500",
            "--text",
            "Looks good",
            "--json",
        ],
        b"n\n",
    );

    assert_success(&output);
    assert_eq!(
        server.received().len(),
        1,
        "the comment write is not prompted"
    );
}

#[test]
fn comments_add_with_eof_sends_the_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        COMMENT_ADD_PATH,
        MockResponse::json(200, json!({"id": 500, "rev": 2})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "add",
            "500",
            "--text",
            "Looks good",
            "--json",
        ],
        b"",
    );

    assert_success(&output);
    assert_eq!(
        server.received().len(),
        1,
        "EOF does not hide a prompt either"
    );
}

#[test]
fn comments_add_without_text_is_a_usage_error_that_names_the_option() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["workitems", "comments", "add", "500", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--text"),
        "the usage error names the required option: {}",
        stderr_of(&output)
    );
    assert!(
        stdout_of(&output).is_empty(),
        "no help on stdout: {}",
        stdout_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "the oracle's silent exit 0 sends nothing; this build refuses before the wire"
    );
}

#[test]
fn comments_add_404_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        COMMENT_ADD_404_PATH,
        MockResponse::json(
            404,
            json!({"message": "TF401232: Work item 896 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "add",
            "896",
            "--text",
            "Looks good",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Resource not found. Check the project/repo/build ID and your permissions."),
        "the module has no 404 clause here: the shared classification carries it"
    );
}

#[test]
fn comments_update_posts_the_edited_history_patch() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        COMMENT_UPDATE_PATH,
        MockResponse::json(200, json!({"id": 501, "rev": 3})),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "update",
            "501",
            "7",
            "--text",
            "Edited text",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "message": "Comment updated on work item #501."})
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].path, COMMENT_UPDATE_PATH);
    assert_eq!(received[0].header("content-type"), Some(JSON_PATCH));
    assert_eq!(
        sent_body(&received[0]),
        json!([{"op": "add", "path": "/fields/System.History", "value": "[Edited] Edited text"}]),
        "captured: the edit is a new history entry prefixed with [Edited]"
    );
}

#[test]
fn comments_update_human_output_is_the_module_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        COMMENT_UPDATE_PATH,
        MockResponse::json(200, json!({"id": 501, "rev": 3})),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "update",
            "501",
            "7",
            "--text",
            "Edited text",
        ],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Comment updated on work item #501.\n");
}

#[test]
fn comments_update_with_stdin_n_sends_the_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        COMMENT_UPDATE_PATH,
        MockResponse::json(200, json!({"id": 501, "rev": 3})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "update",
            "501",
            "7",
            "--text",
            "Edited text",
            "--json",
        ],
        b"n\n",
    );

    assert_success(&output);
    assert_eq!(
        server.received().len(),
        1,
        "the comment edit is not prompted"
    );
}

#[test]
fn comments_update_without_text_is_a_usage_error_that_names_the_option() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["workitems", "comments", "update", "501", "7", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--text"),
        "the usage error names the required option: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn comments_update_without_a_comment_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "update",
            "501",
            "--text",
            "Edited text",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("COMMENT_ID"),
        "the usage error names the missing positional: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn comments_update_404_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        COMMENT_UPDATE_404_PATH,
        MockResponse::json(
            404,
            json!({"message": "TF401232: Work item 897 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "comments",
            "update",
            "897",
            "7",
            "--text",
            "Edited text",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
}

#[test]
fn attachments_list_sends_the_module_path_and_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENTS_42_PATH,
        MockResponse::json(200, attachments_body()),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "attachments", "list", "42", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "result": attachments_body()["attachments"]})
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].path, ATTACHMENTS_42_PATH,
        "the attachments read keeps the module's lowercase path"
    );
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

#[test]
fn attachments_list_human_output_is_the_module_layout() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENTS_42_PATH,
        MockResponse::json(200, attachments_body()),
    );

    let output = run(&home, &server, &["workitems", "attachments", "list", "42"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "\n  att-1  report.pdf\n     https://dev.azure.com/myorg/_apis/wit/attachments/att-1\n\n  att-2  notes.txt\n     https://dev.azure.com/myorg/_apis/wit/attachments/att-2\n\n"
    );
}

#[test]
fn attachments_list_with_an_empty_array_is_the_empty_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENTS_8_PATH,
        MockResponse::json(200, json!({"count": 0, "attachments": []})),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "attachments", "list", "8", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "result": []})
    );
}

/// Captured: against a body without an `attachments` key the frozen CLI prints
/// `No attachments found.` even under `--json`; this build emits the empty value
/// envelope (D21).
#[test]
fn attachments_list_without_the_attachments_key_is_an_empty_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENTS_NO_KEY_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "attachments", "list", "7", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_of(&output)).expect("stdout is one JSON document"),
        json!({"ok": true, "result": []})
    );
}

#[test]
fn attachments_list_without_the_attachments_key_human_says_no_attachments_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENTS_NO_KEY_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["workitems", "attachments", "list", "7"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No attachments found.\n");
}

#[test]
fn attachments_list_404_is_the_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENTS_404_PATH,
        MockResponse::json(
            404,
            json!({"message": "TF401232: Work item 999 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "attachments", "list", "999", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
}

#[test]
fn attachments_list_500_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENTS_NO_KEY_PATH,
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["workitems", "attachments", "list", "7", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
    assert_eq!(
        envelope["error"]["message"],
        json!("Azure DevOps server error. Retry later.")
    );
}

#[test]
fn attachments_list_without_an_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["workitems", "attachments", "list", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("id"),
        "the usage error names the positional: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn attachments_download_sends_the_metadata_then_the_named_raw_get() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_download(
        &server,
        ATTACHMENT_1_PATH,
        attachment_metadata("att-1", Some("out.bin")),
        b"attachment bytes".to_vec(),
    );

    let output = run_in(
        &home,
        &server,
        home.path(),
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "--output",
            "out.bin",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Downloaded 16 bytes to out.bin\n",
        "the download prints the module's line even under --json"
    );
    assert_eq!(
        fs::read(home.path().join("out.bin")).expect("the downloaded file"),
        b"attachment bytes"
    );

    let received = server.received();
    assert_eq!(received.len(), 2, "the metadata GET, then the raw GET");
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, ATTACHMENT_1_PATH);
    assert_eq!(
        received[0].query_pairs(),
        vec![api_version()],
        "the metadata GET carries only api-version"
    );
    assert_eq!(received[1].method, "GET");
    assert_eq!(received[1].path, ATTACHMENT_1_PATH);
    assert_eq!(
        received[1].query_pairs(),
        vec![api_version(), pair("fileName", "out.bin")],
        "the raw GET names the resolved file (the frozen client glues the version onto it; D25's class)"
    );
    assert!(
        temp_siblings(&home).is_empty(),
        "no temp sibling is left behind: {:?}",
        temp_siblings(&home)
    );
}

#[test]
fn attachments_download_defaults_to_the_attachment_name() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_download(
        &server,
        ATTACHMENT_2_PATH,
        attachment_metadata("att-2", Some("notes.txt")),
        b"noted".to_vec(),
    );

    let output = run_in(
        &home,
        &server,
        home.path(),
        &["workitems", "attachments", "download", "42", "att-2"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Downloaded 5 bytes to notes.txt\n");
    assert_eq!(
        fs::read(home.path().join("notes.txt")).expect("the file"),
        b"noted"
    );
}

#[test]
fn attachments_download_defaults_to_the_id_name_without_an_attribute_name() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_download(
        &server,
        ATTACHMENT_3_PATH,
        attachment_metadata("att-3", None),
        b"bytes".to_vec(),
    );

    let output = run_in(
        &home,
        &server,
        home.path(),
        &["workitems", "attachments", "download", "42", "att-3"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Downloaded 5 bytes to attachment_att-3\n",
        "a metadata answer without attributes.name falls back to attachment_<id>"
    );
    assert_eq!(
        fs::read(home.path().join("attachment_att-3")).expect("the file"),
        b"bytes"
    );
}

/// The work item `id` positional is declared but never read by the frozen flow:
/// the requests name only the attachment id, and two work item ids produce the
/// same chain.
#[test]
fn attachments_download_ignores_the_work_item_id_positional() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_download(
        &server,
        ATTACHMENT_1_PATH,
        attachment_metadata("att-1", Some("out.bin")),
        b"bytes".to_vec(),
    );
    expect_download(
        &server,
        ATTACHMENT_1_PATH,
        attachment_metadata("att-1", Some("out.bin")),
        b"bytes".to_vec(),
    );

    let first = run_in(
        &home,
        &server,
        home.path(),
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "--output",
            "one.bin",
        ],
    );
    let second = run_in(
        &home,
        &server,
        home.path(),
        &[
            "workitems",
            "attachments",
            "download",
            "43",
            "att-1",
            "--output",
            "two.bin",
        ],
    );

    assert_success(&first);
    assert_success(&second);

    let received = server.received();
    assert_eq!(received.len(), 4);
    assert_eq!(received[0].path, ATTACHMENT_1_PATH);
    assert_eq!(received[2].path, ATTACHMENT_1_PATH);
    assert!(
        received
            .iter()
            .all(|request| request.path == ATTACHMENT_1_PATH),
        "the work item id never reaches the wire"
    );
}

#[test]
fn attachments_download_writes_non_utf8_bytes_verbatim() {
    let home = TempHome::new();
    let server = MockServer::start();
    let body = MockResponse::from_bytes_fixture("artifacts_download.zip").body;
    expect_download(
        &server,
        ATTACHMENT_1_PATH,
        attachment_metadata("att-1", Some("raw.bin")),
        body.clone(),
    );

    let output = run_in(
        &home,
        &server,
        home.path(),
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "--output",
            "raw.bin",
        ],
    );

    assert_success(&output);
    assert_eq!(
        fs::read(home.path().join("raw.bin")).expect("the file"),
        body,
        "the raw GET's bytes land byte-for-byte, binary included"
    );
    assert_eq!(
        stdout_of(&output),
        format!("Downloaded {} bytes to raw.bin\n", body.len())
    );
}

/// A connection dropped after the headers is a classified transport failure, and
/// nothing appears at the target — the body streams to a sibling temp file that
/// is removed, so there is never a partial attachment to mistake for a real one
/// (D28).
#[test]
fn attachments_download_leaves_no_file_when_the_stream_breaks() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("partial.bin");
    let metadata = attachment_metadata("att-1", None).to_string();
    let port = spawn_truncating_attachment(metadata.clone(), b"PK\x03\x04partial".to_vec(), 4096);

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "--output",
            target.to_str().expect("a utf-8 path"),
            "--server",
            &format!("http://127.0.0.1:{port}"),
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "a short body is not a success"
    );
    assert!(
        stdout_of(&output).is_empty(),
        "no success line: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains(MID_BODY_FAILURE),
        "the mid-body reader error is classified: {}",
        stderr_of(&output)
    );
    assert!(!target.exists(), "no partial file appears at the target");
    assert!(
        temp_siblings(&home).is_empty(),
        "no temp sibling is left behind: {:?}",
        temp_siblings(&home)
    );
}

/// A failed stream must not destroy a pre-existing `--output` file: the body goes
/// to a sibling temp file, so the target keeps its old bytes byte for byte (D28).
#[test]
fn attachments_download_keeps_a_pre_existing_file_when_the_stream_breaks() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("existing.bin");
    fs::write(&target, b"old attachment bytes").expect("seed the target");
    let metadata = attachment_metadata("att-1", None).to_string();
    let port = spawn_truncating_attachment(metadata.clone(), b"PK\x03\x04partial".to_vec(), 4096);

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "--output",
            target.to_str().expect("a utf-8 path"),
            "--server",
            &format!("http://127.0.0.1:{port}"),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "the download failed");
    assert_eq!(
        fs::read(&target).expect("the pre-existing file"),
        b"old attachment bytes",
        "an existing target survives a failed download byte for byte"
    );
    assert!(
        temp_siblings(&home).is_empty(),
        "no temp sibling is left behind: {:?}",
        temp_siblings(&home)
    );
}

/// The temp name carries a unique infix, so a pre-existing `{target}.tmp` — a
/// name nobody asked us to touch — survives a failed download byte for byte.
#[test]
fn attachments_download_keeps_a_pre_existing_tmp_file_when_the_stream_breaks() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("existing.bin");
    let bystander = home.path().join("existing.bin.tmp");
    fs::write(&bystander, b"someone else's temp").expect("seed the bystander");
    let metadata = attachment_metadata("att-1", None).to_string();
    let port = spawn_truncating_attachment(metadata.clone(), b"PK\x03\x04partial".to_vec(), 4096);

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "--output",
            target.to_str().expect("a utf-8 path"),
            "--server",
            &format!("http://127.0.0.1:{port}"),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "the download failed");
    assert_eq!(
        fs::read(&bystander).expect("the pre-existing temp"),
        b"someone else's temp",
        "a pre-existing `<target>.tmp` is not our temp and must survive"
    );
    assert_eq!(
        temp_siblings(&home),
        vec!["existing.bin.tmp".to_owned()],
        "our own temp was removed and only the bystander remains"
    );
    assert!(!target.exists(), "the target never appears");
}

#[test]
fn attachments_download_to_an_unwritable_path_reports_the_attachment_noun() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("no-such-dir").join("out.bin");
    expect_download(
        &server,
        ATTACHMENT_1_PATH,
        attachment_metadata("att-1", Some("out.bin")),
        b"bytes".to_vec(),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "--output",
            target.to_str().expect("a utf-8 path"),
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no success line on a failed write: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("Could not write the attachment to"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(!target.exists(), "no file at the unwritable path");
}

#[test]
fn attachments_download_404_on_the_metadata_is_the_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENT_404_PATH,
        MockResponse::json(
            404,
            json!({"message": "TF401232: Attachment att-404 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-404",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(server.received().len(), 1, "the raw GET never starts");
}

/// The raw-content GET's status is classified (D25's class), where the frozen
/// `get_raw/2` answers `%{status: 404}` with no body and reports a network error.
#[test]
fn attachments_download_404_on_the_raw_get_classifies_by_status() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENT_1_PATH,
        MockResponse::json(404, json!({"message": "gone"})),
    );
    server.expect(
        "GET",
        ATTACHMENT_1_PATH,
        MockResponse::json(200, attachment_metadata("att-1", Some("out.bin"))),
    );

    let output = run_in(
        &home,
        &server,
        home.path(),
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "--output",
            "out.bin",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(
        envelope["error"]["code"],
        json!("not_found"),
        "the status is the classification, not a network error: {envelope}"
    );
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        server.received().len(),
        2,
        "the metadata, then the failed raw GET"
    );
    assert!(!home.path().join("out.bin").exists(), "nothing is written");
}

#[test]
fn attachments_download_redirect_is_the_redirect_error() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ATTACHMENT_1_PATH,
        MockResponse::json(302, json!({"message": "sign in"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("auth_required"));
    assert_eq!(envelope["error"]["status"], json!(302));
    assert_eq!(
        envelope["error"]["message"],
        json!("API redirected without a Location header. Run 'ado login' to authenticate."),
        "the redirect is refused with its true status (D8); the raw GET never starts"
    );
    assert_eq!(server.received().len(), 1);
}

#[test]
fn attachments_download_without_an_attachment_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["workitems", "attachments", "download", "42", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("ATTACHMENT_ID"),
        "the usage error names the missing positional: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

/// Captured: the frozen node declares `-o` for both `--org` and `--output`, and
/// the global wins — `-o short.bin` sets the organization, so the request goes to
/// `/short.bin/...`. This build cannot declare a local `-o` at all (clap panics on
/// a duplicate short), and `--output` is the only spelling for the file.
#[test]
fn attachments_download_short_o_is_the_org_flag_not_output() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/otherorg/_apis/wit/attachments/att-1",
        MockResponse::json(404, json!({"message": "no route"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "workitems",
            "attachments",
            "download",
            "42",
            "att-1",
            "-o",
            "otherorg",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].path, "/otherorg/_apis/wit/attachments/att-1",
        "-o is the global organization flag, exactly as the frozen binary reads it"
    );
}
