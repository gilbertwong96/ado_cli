//! End-to-end tests for `ado prs list|show|create|complete|abandon|approve|vote`:
//! the REST surface the frozen `lib/ado_cli/cli/pull_requests.ex` builds — every
//! method, path, query pair and body verified against the frozen escript — the
//! `--json` envelopes it emits, and the human table/detail/success the module's
//! formatters define.
//!
//! Both read envelopes are pinned **byte-equal** to the captured oracle lines:
//! every map in the fixtures stays below Elixir's small-map threshold, so the term
//! order the oracle encodes is serde's sorted order, and each capture was confirmed
//! byte-identical to its `jq -S` form (W1-R12). The write paths' envelopes are this
//! build's own (D33): the oracle prints its human success line even under `--json`.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a mock
//! server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT` credentials so no
//! credential resolution reaches the developer's keychain. No test reaches a real
//! organization; a stdin-driven run pipes bytes or closes the stream (R8).

use std::io::Write;
use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";

/// The frozen escript's literal `ado prs list Alpha Alpha.Core --json` line for the
/// `prs_list` fixture (captured from the 0.5.0 binary, W1-R12).
const ORACLE_LIST_JSON: &str = r#"{"ok":true,"result":[{"artifactId":"vstfs:///Git/PullRequestId/6f2e6a8c-0000-4000-8000-000000000000%2f9f1b7e0e-0001-4000-8000-000000000001%2f137","codeReviewId":137,"createdBy":{"descriptor":"aad.YzFkMmUzZjQtMDAwMS0wMDAxLTAwMDEtMDAwMDAwMDAwMDAx","displayName":"Alice Example","id":"c1d2e3f4-0001-0001-0001-000000000001","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2U=","uniqueName":"alice@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0001-0001-0001-000000000001"},"creationDate":"2026-09-14T09:31:07.83Z","description":"Retries soft declines up to three times before failing the order.","isDraft":false,"lastMergeCommit":{"commitId":"3333333333333333333333333333333333333333","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/3333333333333333333333333333333333333333"},"lastMergeSourceCommit":{"commitId":"1111111111111111111111111111111111111111","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/1111111111111111111111111111111111111111"},"lastMergeTargetCommit":{"commitId":"2222222222222222222222222222222222222222","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/2222222222222222222222222222222222222222"},"mergeId":"b7c1c3b0-0002-4000-8000-000000000002","mergeStatus":"succeeded","pullRequestId":137,"repository":{"id":"9f1b7e0e-0001-4000-8000-000000000001","name":"Alpha.Core","project":{"id":"6f2e6a8c-0000-4000-8000-000000000000","name":"Alpha","state":"wellFormed","url":"https://dev.azure.com/myorg/_apis/projects/6f2e6a8c-0000-4000-8000-000000000000","visibility":"private"},"url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001"},"reviewers":[{"displayName":"Bob Example","hasDeclined":false,"id":"c1d2e3f4-0002-0002-0002-000000000002","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2Uy","isFlagged":false,"reviewerUrl":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137/reviewers/c1d2e3f4-0002-0002-0002-000000000002","uniqueName":"bob@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0002-0002-0002-000000000002","vote":10},{"displayName":"Carol Example","hasDeclined":false,"id":"c1d2e3f4-0003-0003-0003-000000000003","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2Uz","isFlagged":false,"reviewerUrl":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137/reviewers/c1d2e3f4-0003-0003-0003-000000000003","uniqueName":"carol@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0003-0003-0003-000000000003","vote":0}],"sourceRefName":"refs/heads/feature/payments","status":"active","supportsIterations":true,"targetRefName":"refs/heads/main","title":"Add payment retries","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137"},{"artifactId":"vstfs:///Git/PullRequestId/6f2e6a8c-0000-4000-8000-000000000000%2f9f1b7e0e-0001-4000-8000-000000000001%2f138","codeReviewId":138,"createdBy":{"descriptor":"aad.YzFkMmUzZjQtMDAwNC0wMDA0LTAwMDQtMDAwMDAwMDAwMDA0","displayName":"Dana Example","id":"c1d2e3f4-0004-0004-0004-000000000004","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2U0","uniqueName":"dana@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0004-0004-0004-000000000004"},"creationDate":"2026-09-10T15:02:44.17Z","description":null,"isDraft":false,"lastMergeCommit":{"commitId":"6666666666666666666666666666666666666666","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/6666666666666666666666666666666666666666"},"lastMergeSourceCommit":{"commitId":"4444444444444444444444444444444444444444","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/4444444444444444444444444444444444444444"},"lastMergeTargetCommit":{"commitId":"5555555555555555555555555555555555555555","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/5555555555555555555555555555555555555555"},"mergeId":"b7c1c3b0-0005-4000-8000-000000000005","mergeStatus":"succeeded","pullRequestId":138,"repository":{"id":"9f1b7e0e-0001-4000-8000-000000000001","name":"Alpha.Core","project":{"id":"6f2e6a8c-0000-4000-8000-000000000000","name":"Alpha","state":"wellFormed","url":"https://dev.azure.com/myorg/_apis/projects/6f2e6a8c-0000-4000-8000-000000000000","visibility":"private"},"url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001"},"reviewers":[{"displayName":"Erin Example","hasDeclined":false,"id":"c1d2e3f4-0005-0005-0005-000000000005","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2U1","isFlagged":false,"reviewerUrl":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/138/reviewers/c1d2e3f4-0005-0005-0005-000000000005","uniqueName":"erin@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0005-0005-0005-000000000005","vote":10}],"sourceRefName":"refs/heads/feature/ledger-retirement","status":"completed","supportsIterations":true,"targetRefName":"refs/heads/main","title":"Retire the legacy ledger","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/138"}]}"#;

/// The frozen escript's literal `ado prs show Alpha Alpha.Core 137 --json` line for
/// the `prs_show` fixture.
const ORACLE_SHOW_JSON: &str = r#"{"ok":true,"result":{"_links":{"createdBy":{"href":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0001-0001-0001-000000000001"},"mergeCommit":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/3333333333333333333333333333333333333333"},"repository":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001"},"reviewers":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137/reviewers"},"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137"},"sourceBranch":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/refs/heads/feature/payments"},"sourceCommit":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/1111111111111111111111111111111111111111"},"statuses":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137/statuses"},"targetBranch":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/refs/heads/main"},"targetCommit":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/2222222222222222222222222222222222222222"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_git/Alpha.Core/pullrequest/137"},"workItems":{"href":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137/workitems"}},"artifactId":"vstfs:///Git/PullRequestId/6f2e6a8c-0000-4000-8000-000000000000%2f9f1b7e0e-0001-4000-8000-000000000001%2f137","codeReviewId":137,"completionOptions":{"deleteSourceBranch":true,"mergeStrategy":"squash","squashMerge":true,"transitionWorkItems":true},"createdBy":{"descriptor":"aad.YzFkMmUzZjQtMDAwMS0wMDAxLTAwMDEtMDAwMDAwMDAwMDAx","displayName":"Alice Example","id":"c1d2e3f4-0001-0001-0001-000000000001","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2U=","uniqueName":"alice@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0001-0001-0001-000000000001"},"creationDate":"2026-09-14T09:31:07.83Z","description":"Retries soft declines up to three times before failing the order.","isDraft":false,"labels":[{"active":true,"id":"7d3f4a5b-0006-4000-8000-000000000006","name":"payments"}],"lastMergeCommit":{"commitId":"3333333333333333333333333333333333333333","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/3333333333333333333333333333333333333333"},"lastMergeSourceCommit":{"commitId":"1111111111111111111111111111111111111111","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/1111111111111111111111111111111111111111"},"lastMergeTargetCommit":{"commitId":"2222222222222222222222222222222222222222","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/commits/2222222222222222222222222222222222222222"},"mergeId":"b7c1c3b0-0002-4000-8000-000000000002","mergeStatus":"succeeded","pullRequestId":137,"repository":{"id":"9f1b7e0e-0001-4000-8000-000000000001","name":"Alpha.Core","project":{"id":"6f2e6a8c-0000-4000-8000-000000000000","name":"Alpha","state":"wellFormed","url":"https://dev.azure.com/myorg/_apis/projects/6f2e6a8c-0000-4000-8000-000000000000","visibility":"private"},"url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001"},"reviewers":[{"displayName":"Bob Example","hasDeclined":false,"id":"c1d2e3f4-0002-0002-0002-000000000002","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2Uy","isFlagged":false,"reviewerUrl":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137/reviewers/c1d2e3f4-0002-0002-0002-000000000002","uniqueName":"bob@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0002-0002-0002-000000000002","vote":10},{"displayName":"Carol Example","hasDeclined":false,"id":"c1d2e3f4-0003-0003-0003-000000000003","imageUrl":"https://dev.azure.com/myorg/_apis/GraphProfile/MemberAvatars/YWxpY2Uz","isFlagged":false,"reviewerUrl":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137/reviewers/c1d2e3f4-0003-0003-0003-000000000003","uniqueName":"carol@example.test","url":"https://vssps.dev.azure.com/myorg/_apis/Identities/c1d2e3f4-0003-0003-0003-000000000003","vote":0}],"sourceRefName":"refs/heads/feature/payments","status":"active","supportsIterations":true,"targetRefName":"refs/heads/main","title":"Add payment retries","url":"https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137"}}"#;

/// The repository collection path both subcommands build on. The module sends
/// `pullrequests` in lower case; `show` appends the numeric id.
const LIST_PATH: &str = "/myorg/Alpha/_apis/git/repositories/Alpha.Core/pullrequests";
const SHOW_PATH: &str = "/myorg/Alpha/_apis/git/repositories/Alpha.Core/pullrequests/137";

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

/// The one request every list flow starts with, answering the fixture when the
/// request carries `query` as wire-form pairs.
fn expect_list(server: &MockServer, query: &[(&str, &str)]) {
    server.expect_query(
        "GET",
        LIST_PATH,
        query,
        MockResponse::from_fixture("prs_list"),
    );
}

fn expect_show(server: &MockServer) {
    server.expect("GET", SHOW_PATH, MockResponse::from_fixture("prs_show"));
}

fn assert_list_request(request: &RecordedRequest, query: Vec<(String, String)>) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, LIST_PATH);
    assert_eq!(
        request.query_pairs(),
        query,
        "the list GET's wire-form pairs, in the client's merge order (D12 compares parsed pairs)"
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
fn list_sends_the_status_default_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(
        &server,
        &[("api-version", "7.1"), ("searchCriteria.status", "active")],
    );

    let output = run(
        &home,
        &server,
        &["prs", "list", "Alpha", "Alpha.Core", "--json"],
    );

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
        json!({"ok": true, "result": fixture("prs_list")["value"].clone()}),
        "the value envelope carries the fixture's array under result"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one request per list");
    assert_list_request(
        &received[0],
        vec![api_version(), pair("searchCriteria.status", "active")],
    );
}

#[test]
fn list_404_is_the_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Missing/_apis/git/repositories/Alpha.Core/pullrequests",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["prs", "list", "Missing", "Alpha.Core", "--json"],
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
}

#[test]
fn list_500_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Broken/_apis/git/repositories/Alpha.Core/pullrequests",
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["prs", "list", "Broken", "Alpha.Core", "--json"],
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

/// The module's own parameter order (`status`, then `creator`, then `top`), with the
/// wire forms the client encodes: `$` is `%24`, `@` is `%40`, `.` survives.
#[test]
fn list_filters_reach_the_wire_in_the_modules_order() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(
        &server,
        &[
            ("api-version", "7.1"),
            ("%24top", "5"),
            ("searchCriteria.creatorId", "alice%40example.test"),
            ("searchCriteria.status", "completed"),
        ],
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "list",
            "Alpha",
            "Alpha.Core",
            "--status",
            "completed",
            "--creator",
            "alice@example.test",
            "--top",
            "5",
            "--json",
        ],
    );

    assert_success(&output);
    assert_list_request(
        &server.received()[0],
        vec![
            api_version(),
            pair("searchCriteria.status", "completed"),
            pair("searchCriteria.creatorId", "alice%40example.test"),
            pair("%24top", "5"),
        ],
    );
}

/// "Present means sent": Elixir's `Map.get` default applies only when the option is
/// absent, and `if value` is truthy for `0` and `""` — so an explicit empty status or
/// creator and a zero top all reach the wire.
#[test]
fn list_present_empty_and_zero_options_reach_the_wire() {
    let home = TempHome::new();

    let status = MockServer::start();
    expect_list(
        &status,
        &[("api-version", "7.1"), ("searchCriteria.status", "")],
    );
    let output = run(
        &home,
        &status,
        &[
            "prs",
            "list",
            "Alpha",
            "Alpha.Core",
            "--status",
            "",
            "--json",
        ],
    );
    assert_success(&output);
    assert_list_request(
        &status.received()[0],
        vec![api_version(), pair("searchCriteria.status", "")],
    );

    let creator = MockServer::start();
    expect_list(
        &creator,
        &[
            ("api-version", "7.1"),
            ("searchCriteria.creatorId", ""),
            ("searchCriteria.status", "active"),
        ],
    );
    let output = run(
        &home,
        &creator,
        &[
            "prs",
            "list",
            "Alpha",
            "Alpha.Core",
            "--creator",
            "",
            "--json",
        ],
    );
    assert_success(&output);
    assert_list_request(
        &creator.received()[0],
        vec![
            api_version(),
            pair("searchCriteria.status", "active"),
            pair("searchCriteria.creatorId", ""),
        ],
    );

    let top = MockServer::start();
    expect_list(
        &top,
        &[
            ("api-version", "7.1"),
            ("%24top", "0"),
            ("searchCriteria.status", "active"),
        ],
    );
    let output = run(
        &home,
        &top,
        &["prs", "list", "Alpha", "Alpha.Core", "--top", "0", "--json"],
    );
    assert_success(&output);
    assert_list_request(
        &top.received()[0],
        vec![
            api_version(),
            pair("searchCriteria.status", "active"),
            pair("%24top", "0"),
        ],
    );
}

/// The Elixir `--top` is `:integer` and the oracle parses `--top -1`, sending it as
/// `$top=-1`; clap needs `allow_negative_numbers(true)` to accept the same argv.
#[test]
fn list_negative_top_reaches_the_wire() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(
        &server,
        &[
            ("api-version", "7.1"),
            ("%24top", "-1"),
            ("searchCriteria.status", "active"),
        ],
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "list",
            "Alpha",
            "Alpha.Core",
            "--top",
            "-1",
            "--json",
        ],
    );

    assert_success(&output);
    assert_list_request(
        &server.received()[0],
        vec![
            api_version(),
            pair("searchCriteria.status", "active"),
            pair("%24top", "-1"),
        ],
    );
}

#[test]
fn list_encodes_both_path_segments() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/My%20Project/_apis/git/repositories/Alpha%20Core/pullrequests",
        &[("api-version", "7.1"), ("searchCriteria.status", "active")],
        MockResponse::from_fixture("prs_list"),
    );

    let output = run(
        &home,
        &server,
        &["prs", "list", "My Project", "Alpha Core", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/git/repositories/Alpha%20Core/pullrequests",
        "a space is %20 in a path, not +"
    );
}

/// The stricter path encoder (D22) also escapes `/` and `?`, which Elixir leaves in
/// place (changing the URL's structure) while Azure names cannot contain them.
#[test]
fn list_encodes_path_separators_strictly() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        "/myorg/a%2Fb%3Fc/_apis/git/repositories/d%2Fe/pullrequests",
        &[("api-version", "7.1"), ("searchCriteria.status", "active")],
        MockResponse::from_fixture("prs_list"),
    );

    let output = run(&home, &server, &["prs", "list", "a/b?c", "d/e", "--json"]);

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/a%2Fb%3Fc/_apis/git/repositories/d%2Fe/pullrequests",
        "the stricter encoder escapes / and ? so a name cannot change the URL (D22)"
    );
}

#[test]
fn show_encodes_the_project_and_repository_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/My%20Project/_apis/git/repositories/Alpha%20Core/pullrequests/137",
        MockResponse::from_fixture("prs_show"),
    );

    let output = run(
        &home,
        &server,
        &["prs", "show", "My Project", "Alpha Core", "137", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/git/repositories/Alpha%20Core/pullrequests/137"
    );
}

#[test]
fn list_empty_answer_human_output_says_no_pull_requests_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        LIST_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["prs", "list", "Alpha", "Alpha.Core"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No pull requests found.\n");
    assert_eq!(server.received().len(), 1);
}

#[test]
fn list_empty_answer_json_is_the_empty_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        LIST_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["prs", "list", "Alpha", "Alpha.Core", "--json"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");
    assert_eq!(server.received().len(), 1);
}

#[test]
fn list_json_emits_no_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(
        &server,
        &[("api-version", "7.1"), ("searchCriteria.status", "active")],
    );

    let output = run(
        &home,
        &server,
        &["prs", "list", "Alpha", "Alpha.Core", "--json"],
    );

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

/// The module's `print_prs_table/1`: ID, Title, Source and Target (with
/// `refs/heads/` stripped) and Status — never the help text's Creator column.
#[test]
fn list_human_output_is_the_pull_requests_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(
        &server,
        &[("api-version", "7.1"), ("searchCriteria.status", "active")],
    );

    let output = run(&home, &server, &["prs", "list", "Alpha", "Alpha.Core"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        4,
        "a header row, a rule row and one row per pull request: {stdout}"
    );
    assert!(lines[0].starts_with("ID"), "header row: {stdout}");
    let title_at = lines[0].find("Title").expect("the Title header");
    let source_at = lines[0].find("Source").expect("the Source header");
    let target_at = lines[0].find("Target").expect("the Target header");
    let status_at = lines[0].find("Status").expect("the Status header");
    assert!(
        title_at > 0 && source_at > title_at && target_at > source_at && status_at > target_at,
        "the header order is ID, Title, Source, Target, Status (the module's formatter, not the help text): {stdout}"
    );
    assert!(
        !stdout.contains("Creator"),
        "the help text's Creator column is not in the formatter: {stdout}"
    );
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].starts_with("137")
            && lines[2].contains("Add payment retries")
            && lines[2].contains("feature/payments")
            && lines[2].contains("main")
            && lines[2].ends_with("active"),
        "the first row strips refs/heads/ and ends with the status: {stdout}"
    );
    assert!(
        lines[3].starts_with("138")
            && lines[3].contains("Retire the legacy ledger")
            && lines[3].contains("feature/ledger-retirement")
            && lines[3].ends_with("completed"),
        "the second row: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn list_without_a_project_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["prs", "list", "--json"]);

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
fn list_without_the_repository_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["prs", "list", "Alpha", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("REPO_ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn show_sends_the_pull_requests_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_show(&server);

    let output = run(
        &home,
        &server,
        &["prs", "show", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert_eq!(
        stdout,
        format!("{ORACLE_SHOW_JSON}\n"),
        "the oracle's exact bytes (W1-R12)"
    );

    let envelope: Value = serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({"ok": true, "result": fixture("prs_show")}),
        "the value envelope carries the pull request object"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one request per show");
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, SHOW_PATH);
    assert_eq!(
        received[0].query_pairs(),
        vec![api_version()],
        "show carries no criteria beyond the version"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn show_human_output_is_the_pull_request_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_show(&server);

    let output = run(
        &home,
        &server,
        &["prs", "show", "Alpha", "Alpha.Core", "137"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(stdout.contains("\nPull Request #137\n"), "stdout: {stdout}");
    assert!(
        stdout.contains(&format!("\n{}\n", "─".repeat(60))),
        "the module's 60-character rule: {stdout}"
    );
    assert!(
        stdout.contains("  Title:       Add payment retries\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(
            "  Description: Retries soft declines up to three times before failing the order.\n"
        ),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Status:      active\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Source:      refs/heads/feature/payments\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Target:      refs/heads/main\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Created By:  Alice Example\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Created:     2026-09-14T09:31:07.83Z\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Reviewers:   Bob Example, Carol Example\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(
            "  URL:         https://dev.azure.com/myorg/Alpha/_apis/git/repositories/9f1b7e0e-0001-4000-8000-000000000001/pullRequests/137\n"
        ),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "ANSI in piped output: {stdout:?}"
    );
}

#[test]
fn show_404_reports_the_pull_request_not_found_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Alpha/_apis/git/repositories/Alpha.Core/pullrequests/999",
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["prs", "show", "Alpha", "Alpha.Core", "999", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Pull request #999 not found in Alpha/Alpha.Core")
    );
    assert!(
        stderr_of(&output).is_empty(),
        "stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn show_without_the_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["prs", "show", "Alpha", "Alpha.Core", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("PR_ID"),
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

    let output = run(
        &home,
        &server,
        &["prs", "show", "Alpha", "Alpha.Core", "abc", "--json"],
    );

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
fn prs_without_a_subcommand_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["prs"]);

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
    assert!(
        server.received().is_empty(),
        "a missing subcommand sends no request"
    );
}

/// A closed stdout is a silent success: the table write returns EPIPE and the
/// binary exits 0 with no diagnostic (spec §6.3, R19).
#[test]
fn a_closed_stdout_is_a_silent_success() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_list(
        &server,
        &[("api-version", "7.1"), ("searchCriteria.status", "active")],
    );

    let mut child = command(&home, &server, &["prs", "list", "Alpha", "Alpha.Core"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ado");
    drop(child.stdout.take());

    let output = child.wait_with_output().expect("wait for ado");

    assert_eq!(server.received().len(), 1, "the table was rendered");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(output.stderr.is_empty(), "stderr: {}", stderr_of(&output));
}

// ── Wave 2: the `prs` lifecycle mutations (Task 9) ───────────────────────────
//
// The five commands were captured against the standalone mock before porting
// (`captures/task9/`): `create` one POST, `complete` a GET-then-PATCH pair,
// `abandon` one PATCH, `approve`/`vote` a connectionData GET (no `api-version`)
// then a reviewer PUT. None of the five prompts — every one was re-run with `n`
// on stdin and on EOF (R5), and the stdin tests below are that evidence. The
// oracle's `create` crashes on an absent `--description` and exits 0 silently
// (`opts.description` on a map without the key), so `--title`/`--source`/`--target`
// are clap-required here (D34) and an absent description is omitted from the body
// (D35). The invocations table's `--delete_source`/`--merge_strategy` are rejected
// by the frozen parser; the hyphenated spellings are the runnable ones (D17).

const CONNECTION_DATA_PATH: &str = "/myorg/_apis/connectionData";
const REVIEWER_ID: &str = "c1d2e3f4-0002-0002-0002-000000000002";

/// `/myorg/Alpha/_apis/git/repositories/Alpha.Core/pullrequests/{id}`.
fn pr_path(pr_id: i64) -> String {
    format!("{LIST_PATH}/{pr_id}")
}

/// `/myorg/Alpha/_apis/git/repositories/Alpha.Core/pullrequests/{id}/reviewers/{reviewer}`.
fn reviewer_path(pr_id: i64, reviewer: &str) -> String {
    format!("{}/reviewers/{reviewer}", pr_path(pr_id))
}

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

/// The request body as JSON.
fn sent_body(request: &RecordedRequest) -> Value {
    serde_json::from_str(request.body.as_deref().expect("a request body"))
        .expect("the request body is JSON")
}

/// The captured `POST …/pullrequests` answer: a created pull request with a web
/// link, which is what the human success line names.
fn created_pull_request() -> Value {
    json!({
        "pullRequestId": 145,
        "title": "Add checkout retries",
        "status": "active",
        "sourceRefName": "refs/heads/feature/payments",
        "targetRefName": "refs/heads/main",
        "creationDate": "2026-09-27T10:00:00.000Z",
        "_links": {"web": {"href": "https://dev.azure.com/myorg/Alpha/_git/Alpha.Core/pullrequest/145"}},
    })
}

/// The connectionData answer the vote flow reads: `authenticatedUser.id` is the
/// reviewer the PUT addresses.
fn connection_data() -> MockResponse {
    MockResponse::json(
        200,
        json!({"authenticatedUser": {"id": REVIEWER_ID, "displayName": "Ada Example"}}),
    )
}

fn expect_connection_data(server: &MockServer) {
    server.expect("GET", CONNECTION_DATA_PATH, connection_data());
}

/// The 137 fixture carries `lastMergeSourceCommit.commitId`, which `complete`
/// reads before PATCHing.
fn expect_complete_read(server: &MockServer) {
    server.expect("GET", &pr_path(137), MockResponse::from_fixture("prs_show"));
}

#[test]
fn create_sends_the_captured_body_and_returns_the_created_pull_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        LIST_PATH,
        MockResponse::json(200, created_pull_request()),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "create",
            "Alpha",
            "Alpha.Core",
            "--title",
            "Add checkout retries",
            "--description",
            "Retries soft declines.",
            "--source",
            "feature/payments",
            "--target",
            "main",
            "--draft",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "result": created_pull_request()})
        ),
        "the value envelope (D33: the oracle prints its human line here)"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one POST");
    assert_eq!(received[0].method, "POST");
    assert_eq!(received[0].path, LIST_PATH);
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
    assert_eq!(
        received[0].header("content-type"),
        Some("application/json"),
        "the frozen post/3 sends application/json"
    );
    assert_eq!(
        sent_body(&received[0]),
        json!({
            "title": "Add checkout retries",
            "description": "Retries soft declines.",
            "sourceRefName": "refs/heads/feature/payments",
            "targetRefName": "refs/heads/main",
            "isDraft": true,
        }),
        "the captured create body, with the short branch names prefixed"
    );
}

/// D35: the oracle's `create` raises on an absent `--description` and exits 0
/// having sent nothing; this build sends the four-key body instead. The body has
/// no `description` key at all.
#[test]
fn create_without_a_description_sends_the_body_without_the_key() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        LIST_PATH,
        MockResponse::json(200, created_pull_request()),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "create",
            "Alpha",
            "Alpha.Core",
            "--title",
            "Add checkout retries",
            "--source",
            "feature/payments",
            "--target",
            "main",
        ],
    );

    assert_success(&output);
    assert_eq!(
        server.received().len(),
        1,
        "the request the oracle never sent"
    );
    assert_eq!(
        sent_body(&server.received()[0]),
        json!({
            "title": "Add checkout retries",
            "sourceRefName": "refs/heads/feature/payments",
            "targetRefName": "refs/heads/main",
            "isDraft": false,
        }),
        "an absent description is omitted, not sent empty"
    );
    assert!(
        stdout_of(&output).contains("Pull request #145 created: Add checkout retries"),
        "the human line runs without --json: {}",
        stdout_of(&output)
    );
}

/// `ensure_ref_prefix/1`: a `refs/`-prefixed ref is kept verbatim (including a
/// tag ref), anything else gains `refs/heads/`.
#[test]
fn create_keeps_full_refs_and_prefixes_short_names() {
    let home = TempHome::new();

    for (source, target, expected_source, expected_target) in [
        (
            "refs/heads/feature/payments",
            "refs/heads/main",
            "refs/heads/feature/payments",
            "refs/heads/main",
        ),
        ("refs/tags/v1", "main", "refs/tags/v1", "refs/heads/main"),
    ] {
        let server = MockServer::start();
        server.expect(
            "POST",
            LIST_PATH,
            MockResponse::json(200, created_pull_request()),
        );

        let output = run(
            &home,
            &server,
            &[
                "prs",
                "create",
                "Alpha",
                "Alpha.Core",
                "--title",
                "Add checkout retries",
                "--source",
                source,
                "--target",
                target,
            ],
        );

        assert_success(&output);
        let body = sent_body(&server.received()[0]);
        assert_eq!(body["sourceRefName"], json!(expected_source));
        assert_eq!(body["targetRefName"], json!(expected_target));
    }
}

/// D34: a missing required option is a silent exit 0 in the oracle (its `opts.*`
/// access raises first); this build is a loud usage error naming the flag, with no
/// request.
#[test]
fn create_without_a_required_flag_is_a_loud_usage_error() {
    let home = TempHome::new();

    for (flag, argv) in [
        (
            "--title",
            vec![
                "prs",
                "create",
                "Alpha",
                "Alpha.Core",
                "--source",
                "s",
                "--target",
                "t",
            ],
        ),
        (
            "--source",
            vec![
                "prs",
                "create",
                "Alpha",
                "Alpha.Core",
                "--title",
                "T",
                "--target",
                "t",
            ],
        ),
        (
            "--target",
            vec![
                "prs",
                "create",
                "Alpha",
                "Alpha.Core",
                "--title",
                "T",
                "--source",
                "s",
            ],
        ),
    ] {
        let server = MockServer::start();

        let output = run(&home, &server, &argv);

        assert_eq!(output.status.code(), Some(1), "{flag} is required");
        assert!(
            stdout_of(&output).is_empty(),
            "no envelope for a usage error: {}",
            stdout_of(&output)
        );
        assert!(
            stderr_of(&output).contains(flag),
            "stderr names {flag}: {}",
            stderr_of(&output)
        );
        assert!(server.received().is_empty(), "{flag} missing: no request");
    }
}

#[test]
fn create_400_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        LIST_PATH,
        MockResponse::json(
            400,
            json!({"message": "TF401179: The source branch does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "create",
            "Alpha",
            "Alpha.Core",
            "--title",
            "Add checkout retries",
            "--source",
            "feature/payments",
            "--target",
            "main",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(400));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("API error 400"),
        "the captured 400 class (R7): {envelope}"
    );
    assert!(
        stderr_of(&output).is_empty(),
        "the envelope carries the failure: {}",
        stderr_of(&output)
    );
}

#[test]
fn create_human_output_names_the_created_pull_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        LIST_PATH,
        MockResponse::json(200, created_pull_request()),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "create",
            "Alpha",
            "Alpha.Core",
            "--title",
            "Add checkout retries",
            "--source",
            "feature/payments",
            "--target",
            "main",
        ],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(
        stdout.contains("Pull request #145 created: Add checkout retries\n"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("  Status:    active\n"), "stdout: {stdout}");
    assert!(
        stdout.contains("  Source:    refs/heads/feature/payments\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Target:    refs/heads/main\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("  Created:   2026-09-27T10:00:00.000Z\n"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains(
            "  URL:       https://dev.azure.com/myorg/Alpha/_git/Alpha.Core/pullrequest/145\n"
        ),
        "stdout: {stdout}"
    );
}

#[test]
fn complete_reads_the_pr_then_patches_the_captured_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_complete_read(&server);
    server.expect(
        "PATCH",
        &pr_path(137),
        MockResponse::json(200, json!({"pullRequestId": 137, "status": "completed"})),
    );

    let output = run(
        &home,
        &server,
        &["prs", "complete", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "{\"ok\":true,\"result\":{\"pullRequestId\":137,\"status\":\"completed\"}}\n",
        "the value envelope (D33)"
    );

    let received = server.received();
    assert_eq!(received.len(), 2, "the read and the patch");
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, pr_path(137));
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
    assert_eq!(received[1].method, "PATCH");
    assert_eq!(received[1].path, pr_path(137));
    assert_eq!(received[1].query_pairs(), vec![api_version()]);
    assert_eq!(received[1].header("content-type"), Some("application/json"));
    assert_eq!(
        sent_body(&received[1]),
        json!({
            "status": "completed",
            "lastMergeSourceCommit": {"commitId": "1111111111111111111111111111111111111111"},
            "deleteSourceBranch": false,
        }),
        "the captured completion body; no mergeStrategy key when the option is absent"
    );
}

/// The captured merge-strategy mapping: `squash` and `rebase` become the API's
/// camelCase values, `noFastForward` passes through, an unknown value passes
/// through, and an explicit empty value sends `""` (the key is present because the
/// option was present).
#[test]
fn complete_maps_the_merge_strategies_onto_the_captured_bodies() {
    let home = TempHome::new();

    for (option, expected) in [
        ("squash", "squashMerge"),
        ("rebase", "rebaseMerge"),
        ("noFastForward", "noFastForward"),
        ("merge", "merge"),
        ("", ""),
    ] {
        let server = MockServer::start();
        expect_complete_read(&server);
        server.expect(
            "PATCH",
            &pr_path(137),
            MockResponse::json(200, json!({"pullRequestId": 137, "status": "completed"})),
        );

        let output = run(
            &home,
            &server,
            &[
                "prs",
                "complete",
                "Alpha",
                "Alpha.Core",
                "137",
                "--delete-source",
                "--merge-strategy",
                option,
                "--json",
            ],
        );

        assert_success(&output);
        let body = sent_body(&server.received()[1]);
        assert_eq!(body["deleteSourceBranch"], json!(true));
        assert_eq!(
            body["mergeStrategy"],
            json!(expected),
            "{option:?} maps to {expected:?}"
        );
    }
}

#[test]
fn complete_without_a_last_merge_source_commit_is_loud() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &pr_path(137),
        MockResponse::json(200, json!({"pullRequestId": 137, "status": "active"})),
    );

    let output = run(
        &home,
        &server,
        &["prs", "complete", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Cannot complete PR #137: no lastMergeSourceCommit.commitId in the PR data.")
    );
    assert_eq!(
        server.received().len(),
        1,
        "no PATCH is attempted without a commit id"
    );
}

/// The captured GET 404 goes through the generic error handler, not the module's
/// message — the `not_found` class with the status's own wording.
#[test]
fn complete_get_404_keeps_the_generic_not_found_error() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &pr_path(999),
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["prs", "complete", "Alpha", "Alpha.Core", "999", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Resource not found. Check the project/repo/build ID and your permissions."),
        "the generic handler's message, not `Pull request #999 not found`"
    );
    assert_eq!(server.received().len(), 1, "the read 404 stops the flow");
}

#[test]
fn complete_patch_404_reports_the_module_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_complete_read(&server);
    server.expect(
        "PATCH",
        &pr_path(137),
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["prs", "complete", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Pull request #137 not found"),
        "the module's own halt_error wording"
    );
    assert_eq!(
        server.received().len(),
        2,
        "the read succeeded, then the patch 404ed"
    );
}

#[test]
fn complete_without_the_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["prs", "complete", "Alpha", "Alpha.Core"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("PR_ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

/// D17: the invocations table's `--delete_source` is the schema's option name; the
/// frozen parser and this build both reject it, only `--delete-source` runs.
#[test]
fn complete_rejects_the_underscore_spelling() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "complete",
            "Alpha",
            "Alpha.Core",
            "137",
            "--delete_source",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--delete_source"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "the rejected flag sends nothing"
    );
}

/// R5: `complete` changes a pull request's state and still asks nothing — with `n`
/// on stdin against a mock that serves the GET, the PATCH goes out.
#[test]
fn complete_with_n_on_stdin_still_patches() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_complete_read(&server);
    server.expect(
        "PATCH",
        &pr_path(137),
        MockResponse::json(200, json!({"pullRequestId": 137, "status": "completed"})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["prs", "complete", "Alpha", "Alpha.Core", "137"],
        b"n\n",
    );

    assert_success(&output);
    assert_eq!(
        server.received().len(),
        2,
        "the read and the patch both went out on `n`"
    );
    assert!(
        stdout_of(&output).contains("Pull request #137 completed (merged)."),
        "stdout: {}",
        stdout_of(&output)
    );
}

#[test]
fn abandon_sends_the_abandoned_status_and_returns_the_pull_request() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &pr_path(138),
        MockResponse::json(200, json!({"pullRequestId": 138, "status": "abandoned"})),
    );

    let output = run(
        &home,
        &server,
        &["prs", "abandon", "Alpha", "Alpha.Core", "138", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "{\"ok\":true,\"result\":{\"pullRequestId\":138,\"status\":\"abandoned\"}}\n",
        "the value envelope (D33)"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one PATCH and no read");
    assert_eq!(received[0].method, "PATCH");
    assert_eq!(received[0].path, pr_path(138));
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
    assert_eq!(sent_body(&received[0]), json!({"status": "abandoned"}));
}

#[test]
fn abandon_404_reports_the_module_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &pr_path(999),
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["prs", "abandon", "Alpha", "Alpha.Core", "999", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Pull request #999 not found")
    );
}

#[test]
fn abandon_without_the_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["prs", "abandon", "Alpha", "Alpha.Core"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("PR_ID"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

/// R5: `abandon` proceeds with `n` on stdin and with EOF — no prompt, no `--force`.
#[test]
fn abandon_does_not_prompt() {
    let home = TempHome::new();

    for stdin in [b"n\n".as_slice(), b"".as_slice()] {
        let server = MockServer::start();
        server.expect(
            "PATCH",
            &pr_path(138),
            MockResponse::json(200, json!({"pullRequestId": 138, "status": "abandoned"})),
        );

        let output = run_with_stdin(
            &home,
            &server,
            &["prs", "abandon", "Alpha", "Alpha.Core", "138"],
            stdin,
        );

        assert_success(&output);
        assert_eq!(
            server.received().len(),
            1,
            "the PATCH goes out with {stdin:?} on stdin"
        );
        assert_eq!(
            sent_body(&server.received()[0]),
            json!({"status": "abandoned"})
        );
    }
}

#[test]
fn approve_reads_connection_data_then_puts_the_vote() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_connection_data(&server);
    server.expect(
        "PUT",
        &reviewer_path(137, REVIEWER_ID),
        MockResponse::json(
            200,
            json!({"id": REVIEWER_ID, "vote": 10, "displayName": "Ada Example"}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["prs", "approve", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "result": {"id": REVIEWER_ID, "vote": 10, "displayName": "Ada Example"}})
        ),
        "the reviewer is the value envelope (D33)"
    );

    let received = server.received();
    assert_eq!(received.len(), 2, "the identity read and the vote");
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, CONNECTION_DATA_PATH);
    assert_eq!(
        received[0].query_pairs(),
        Vec::<(String, String)>::new(),
        "the captured connectionData request carries no api-version at all"
    );
    assert_eq!(received[1].method, "PUT");
    assert_eq!(received[1].path, reviewer_path(137, REVIEWER_ID));
    assert_eq!(received[1].query_pairs(), vec![api_version()]);
    assert_eq!(sent_body(&received[1]), json!({"vote": 10}));
}

#[test]
fn approve_without_an_authenticated_user_id_is_loud() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        CONNECTION_DATA_PATH,
        MockResponse::json(
            200,
            json!({"authenticatedUser": {"displayName": "Ada Example"}}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["prs", "approve", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("auth_required"));
    assert_eq!(
        envelope["error"]["message"],
        json!(
            "Cannot determine authenticated user identity for PR #137: Connection data did not include an authenticated user ID"
        )
    );
    assert_eq!(server.received().len(), 1, "no vote without an identity");
}

#[test]
fn approve_connection_data_failure_is_loud() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        CONNECTION_DATA_PATH,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &["prs", "approve", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .starts_with("Cannot determine authenticated user identity for PR #137:"),
        "the identity prefix is kept: {envelope}"
    );
    assert_eq!(
        server.received().len(),
        1,
        "no vote after a failed identity read"
    );
}

/// The five named values and the bare integer: the captured `vote_label/1` table,
/// each with its `{"vote": N}` body.
#[test]
fn vote_sends_each_value_and_labels_the_line() {
    let home = TempHome::new();

    for (vote, label) in [
        (10, "+10 (approved)"),
        (5, "+5 (approved with suggestions)"),
        (0, "0 (reset)"),
        (-5, "-5 (waiting for author)"),
        (-10, "-10 (rejected)"),
        (7, "7"),
    ] {
        let server = MockServer::start();
        expect_connection_data(&server);
        server.expect(
            "PUT",
            &reviewer_path(137, REVIEWER_ID),
            MockResponse::json(200, json!({"id": REVIEWER_ID, "vote": vote})),
        );

        let vote_arg = vote.to_string();
        let output = run(
            &home,
            &server,
            &[
                "prs",
                "vote",
                "Alpha",
                "Alpha.Core",
                "137",
                "--vote",
                &vote_arg,
            ],
        );

        assert_success(&output);
        assert_eq!(
            stdout_of(&output),
            format!("Voted {label} on PR #137.\n"),
            "the captured label for {vote}"
        );
        assert_eq!(
            sent_body(&server.received()[1]),
            json!({"vote": vote}),
            "the body carries the raw integer"
        );
    }
}

/// D34: the oracle's schema marks `--vote` required and CliMate never enforces it;
/// its `parsed.options.vote` access then exits 0 silently. This build is a loud
/// usage error.
#[test]
fn vote_without_the_option_is_a_loud_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["prs", "vote", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("--vote"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty(), "no request without --vote");
}

#[test]
fn vote_with_a_non_integer_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["prs", "vote", "Alpha", "Alpha.Core", "137", "--vote", "abc"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--vote"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn vote_404_reports_the_module_message() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_connection_data(&server);
    server.expect(
        "PUT",
        &reviewer_path(999, REVIEWER_ID),
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "vote",
            "Alpha",
            "Alpha.Core",
            "999",
            "--vote",
            "10",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Pull request #999 not found")
    );
    assert_eq!(server.received().len(), 2);
}

/// R5: `vote` proceeds with `n` on stdin — the mock serves the connectionData GET,
/// so a post-fetch prompt would have shown.
#[test]
fn vote_with_n_on_stdin_still_puts() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_connection_data(&server);
    server.expect(
        "PUT",
        &reviewer_path(137, REVIEWER_ID),
        MockResponse::json(200, json!({"id": REVIEWER_ID, "vote": 10})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &["prs", "vote", "Alpha", "Alpha.Core", "137", "--vote", "10"],
        b"n\n",
    );

    assert_success(&output);
    assert_eq!(
        server.received().len(),
        2,
        "the identity read and the vote both went out on `n`"
    );
    assert_eq!(stdout_of(&output), "Voted +10 (approved) on PR #137.\n");
}

/// `create` was probed the same way: with `n` on stdin the POST still goes out.
#[test]
fn create_with_n_on_stdin_still_posts() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        LIST_PATH,
        MockResponse::json(200, created_pull_request()),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "prs",
            "create",
            "Alpha",
            "Alpha.Core",
            "--title",
            "Add checkout retries",
            "--source",
            "feature/payments",
            "--target",
            "main",
        ],
        b"n\n",
    );

    assert_success(&output);
    assert_eq!(server.received().len(), 1, "the POST goes out on `n`");
}

/// R7: the frozen taxonomy classifies the captured statuses — a PATCH 409 is
/// `conflict`, and the oracle's `Cannot complete PR …` prose is stderr there.
#[test]
fn complete_patch_409_is_the_conflict_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_complete_read(&server);
    server.expect(
        "PATCH",
        &pr_path(137),
        MockResponse::json(
            409,
            json!({"message": "TF401181: The pull request cannot be completed because it has merge conflicts."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["prs", "complete", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("conflict"));
    assert_eq!(envelope["error"]["status"], json!(409));
    assert!(
        stderr_of(&output).is_empty(),
        "the envelope carries the failure: {}",
        stderr_of(&output)
    );
}

/// A reviewer PUT rejected with 400 classifies as `api_error` (the taxonomy's
/// "other status" row), not `validation_error`.
#[test]
fn vote_put_400_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_connection_data(&server);
    server.expect(
        "PUT",
        &reviewer_path(137, REVIEWER_ID),
        MockResponse::json(
            400,
            json!({"message": "You cannot record a vote for someone else."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "vote",
            "Alpha",
            "Alpha.Core",
            "137",
            "--vote",
            "10",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(400));
}

// ── diff (Task 10) ────────────────────────────────────────────────────────
//
// The captured request chain and bytes: `GET …/pullRequests/{id}/iterations`,
// `GET …/iterations/{n}/changes`, and — only for `--file`/`--unified` — the
// iteration list again, `GET …/items` per revision, and (for `--unified`)
// `GET …/diffs/commits`. The `--json` documents are the frozen `render_file_list`,
// `emit_diff_or_json` and `render_unified` shapes; the human diff bytes are the
// listener capture's.

const DIFF_REPO: &str = "/myorg/Alpha/_apis/git/repositories/Alpha.Core";

/// The listener capture's `--file` output: a replaced line, the trailing empty
/// line a file ending in a newline splits into, and the extra newline `IO.puts/1`
/// writes after the content.
const CAPTURED_FILE_DIFF: &str = "diff --git a/src/app.ex b/src/app.ex\n--- a/src/app.ex\n+++ b/src/app.ex\n@@ -1,4 +1,5 @@\n line one\n-line two\n+line two changed\n line three\n+line four\n \n";

fn diff_iterations_path(pr_id: &str) -> String {
    format!("{DIFF_REPO}/pullRequests/{pr_id}/iterations")
}

fn diff_changes_path(pr_id: &str, iteration_id: &str) -> String {
    format!("{DIFF_REPO}/pullRequests/{pr_id}/iterations/{iteration_id}/changes")
}

fn diff_items_path() -> String {
    format!("{DIFF_REPO}/items")
}

fn diff_commits_path() -> String {
    format!("{DIFF_REPO}/diffs/commits")
}

/// The captured PR 137 iteration list: ids 1 and 2, each with its commit pair.
fn diff_iterations() -> Value {
    json!({
        "count": 2,
        "value": [
            {"id": 1, "targetRefCommit": {"commitId": "aaaa1111"}, "sourceRefCommit": {"commitId": "bbbb2222"}},
            {"id": 2, "targetRefCommit": {"commitId": "aaaa1111"}, "sourceRefCommit": {"commitId": "cccc3333"}},
        ],
    })
}

/// The captured four-entry change list the default view renders.
fn diff_changes() -> Value {
    json!({
        "changeEntries": [
            {"changeTrackingId": 1, "changeId": 1, "changeType": 2, "item": {"path": "/src/app.ex", "additions": 3, "deletions": 1}},
            {"changeTrackingId": 2, "changeId": 2, "changeType": 1, "item": {"path": "/docs/readme.md", "additions": 5, "deletions": 0}},
            {"changeTrackingId": 3, "changeId": 3, "changeType": 4, "item": {"path": "/old/file.txt", "additions": 0, "deletions": 9}},
            {"changeTrackingId": 4, "changeId": 4, "changeType": "rename", "item": {"path": "/renamed/new.ex", "additions": 1, "deletions": 1}},
        ],
    })
}

fn expect_diff_iterations(server: &MockServer, pr_id: &str, response: Value) {
    server.expect(
        "GET",
        &diff_iterations_path(pr_id),
        MockResponse::json(200, response),
    );
}

/// The wire-encoded `path` pair value `expect_query` matches on.
fn encoded_path(path: &str) -> String {
    path.replace('/', "%2F")
}

/// One content revision: `GET …/items` with the three captured query pairs,
/// matched per `version` so a test can serve two different revisions of the same
/// path (which the committed harness scenario cannot express).
fn expect_diff_item(server: &MockServer, path: &str, version: &str, body: &str) {
    server.expect_query(
        "GET",
        &diff_items_path(),
        &[
            ("path", &encoded_path(path)),
            ("versionType", "commit"),
            ("version", version),
        ],
        MockResponse::bytes(200, body.as_bytes().to_vec()),
    );
}

/// The default view under `--json` is the frozen `render_file_list/3` document —
/// `ok`, the iteration, the totals and the per-file objects.
#[test]
fn diff_default_lists_the_changes_with_the_captured_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(&server, "137", diff_iterations());
    server.expect(
        "GET",
        &diff_changes_path("137", "2"),
        MockResponse::json(200, diff_changes()),
    );

    let output = run(
        &home,
        &server,
        &["prs", "diff", "Alpha", "Alpha.Core", "137", "--json"],
    );

    assert_success(&output);
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({
            "ok": true,
            "iteration": 2,
            "count": 4,
            "total_additions": 9,
            "total_deletions": 11,
            "changes": [
                {"path": "/src/app.ex", "change_type": "edit", "change_id": 1, "additions": 3, "deletions": 1},
                {"path": "/docs/readme.md", "change_type": "add", "change_id": 2, "additions": 5, "deletions": 0},
                {"path": "/old/file.txt", "change_type": "delete", "change_id": 3, "additions": 0, "deletions": 9},
                {"path": "/renamed/new.ex", "change_type": "rename", "change_id": 4, "additions": 1, "deletions": 1},
            ],
        })
    );

    let requests = server.received();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.path.as_str())
            .collect::<Vec<_>>(),
        [
            diff_iterations_path("137").as_str(),
            diff_changes_path("137", "2").as_str()
        ],
        "the default view fetches no content"
    );
}

/// The default view's human form: this build's table style (§8), one row per
/// change.
#[test]
fn diff_default_human_prints_the_change_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(&server, "137", diff_iterations());
    server.expect(
        "GET",
        &diff_changes_path("137", "2"),
        MockResponse::json(200, diff_changes()),
    );

    let output = run(
        &home,
        &server,
        &["prs", "diff", "Alpha", "Alpha.Core", "137"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("PATH") && stdout.contains("ADDITIONS") && stdout.contains("DELETIONS"),
        "the frozen columns: {stdout}"
    );
    for cell in [
        "/src/app.ex",
        "edit",
        "3",
        "1",
        "/docs/readme.md",
        "add",
        "/old/file.txt",
        "delete",
        "/renamed/new.ex",
        "rename",
    ] {
        assert!(stdout.contains(cell), "{cell} is missing: {stdout}");
    }
    assert_no_table_bytes(&stdout);
}

/// `--file` under `--json` is the frozen `emit_diff_or_json/5` document, and the
/// rendered bytes are the listener capture's.
#[test]
fn diff_file_renders_the_captured_diff_bytes() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(&server, "137", diff_iterations());
    server.expect(
        "GET",
        &diff_changes_path("137", "2"),
        MockResponse::json(200, diff_changes()),
    );
    expect_diff_iterations(&server, "137", diff_iterations());
    expect_diff_item(
        &server,
        "/src/app.ex",
        "aaaa1111",
        "line one\nline two\nline three\n",
    );
    expect_diff_item(
        &server,
        "/src/app.ex",
        "cccc3333",
        "line one\nline two changed\nline three\nline four\n",
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "137",
            "--file",
            "/src/app.ex",
            "--json",
        ],
    );

    assert_success(&output);
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({
            "ok": true,
            "iteration": 2,
            "path": "/src/app.ex",
            "change_type": "edit",
            "diff": CAPTURED_FILE_DIFF,
        })
    );

    let requests = server.received();
    assert_eq!(requests.len(), 5, "the captured chain: {requests:?}");
    assert_eq!(requests[2].path, diff_iterations_path("137"));
    assert_eq!(
        requests[3].query_pairs(),
        item_pairs("/src/app.ex", "aaaa1111")
    );
    assert_eq!(
        requests[4].query_pairs(),
        item_pairs("/src/app.ex", "cccc3333")
    );
}

fn item_pairs(path: &str, version: &str) -> Vec<(String, String)> {
    vec![
        api_version(),
        pair("path", &encoded_path(path)),
        pair("versionType", "commit"),
        pair("version", version),
    ]
}

/// The human form of `--file` is the bare diff plus `IO.puts/1`'s newline, and a
/// `--file` without the leading slash matches the same change.
#[test]
fn diff_file_human_prints_the_diff_and_accepts_the_bare_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(&server, "137", diff_iterations());
    server.expect(
        "GET",
        &diff_changes_path("137", "2"),
        MockResponse::json(200, diff_changes()),
    );
    expect_diff_iterations(&server, "137", diff_iterations());
    expect_diff_item(
        &server,
        "/src/app.ex",
        "aaaa1111",
        "line one\nline two\nline three\n",
    );
    expect_diff_item(
        &server,
        "/src/app.ex",
        "cccc3333",
        "line one\nline two changed\nline three\nline four\n",
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "137",
            "--file",
            "src/app.ex",
        ],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), format!("{CAPTURED_FILE_DIFF}\n"));
}

/// A `--file` that matches no change is `validation_error` and sends no content
/// request (captured: the refusal comes after the change list, before the items).
#[test]
fn diff_file_without_a_match_is_loud() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(&server, "137", diff_iterations());
    server.expect(
        "GET",
        &diff_changes_path("137", "2"),
        MockResponse::json(200, diff_changes()),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "137",
            "--file",
            "src/nope.ex",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!(
            "No change matches --file 'src/nope.ex'. Use 'ado prs diff' (no flags) to list files."
        )
    );
    assert_eq!(server.received().len(), 2, "no content request");
}

/// The two content modes together are refused before any request (captured: zero
/// requests on either side).
#[test]
fn diff_file_and_unified_together_are_refused() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "137",
            "--file",
            "/src/app.ex",
            "--unified",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Pass either --file or --unified, not both.")
    );
    assert!(
        server.received().is_empty(),
        "the refusal precedes every request"
    );
}

/// `--unified` reads `/diffs/commits` and joins the per-file diffs; an `add`
/// fetches only its new side and a `delete` only its old one, and `file_count` is
/// the change list's length (captured: the two counts differ).
#[test]
fn diff_unified_reads_the_repo_diff_and_joins_the_files() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(&server, "137", diff_iterations());
    server.expect(
        "GET",
        &diff_changes_path("137", "2"),
        MockResponse::json(200, diff_changes()),
    );
    expect_diff_iterations(&server, "137", diff_iterations());
    server.expect_query(
        "GET",
        &diff_commits_path(),
        &[
            ("baseVersionType", "commit"),
            ("baseVersion", "aaaa1111"),
            ("targetVersionType", "commit"),
            ("targetVersion", "cccc3333"),
        ],
        MockResponse::json(
            200,
            json!({
                "changes": [
                    {"changeType": 2, "item": {"path": "/src/app.ex"}},
                    {"changeType": 1, "item": {"path": "/docs/readme.md"}},
                    {"changeType": 4, "item": {"path": "/old/file.txt"}},
                ],
            }),
        ),
    );
    expect_diff_item(
        &server,
        "/src/app.ex",
        "aaaa1111",
        "line one\nline two\nline three\n",
    );
    expect_diff_item(
        &server,
        "/src/app.ex",
        "cccc3333",
        "line one\nline two changed\nline three\nline four\n",
    );
    expect_diff_item(
        &server,
        "/docs/readme.md",
        "cccc3333",
        "line one\nline two\n",
    );
    expect_diff_item(&server, "/old/file.txt", "aaaa1111", "line one\nline two\n");

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "137",
            "--unified",
            "--json",
        ],
    );

    assert_success(&output);
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");

    let expected_diff = [
        CAPTURED_FILE_DIFF,
        "diff --git a/docs/readme.md b/docs/readme.md\n--- a/docs/readme.md\n+++ b/docs/readme.md\n@@ -0,0 +1,3 @@\n+line one\n+line two\n+\n",
        "diff --git a/old/file.txt b/old/file.txt\n--- a/old/file.txt\n+++ b/old/file.txt\n@@ -1,3 +0,0 @@\n-line one\n-line two\n-\n",
    ]
    .join("\n");

    assert_eq!(
        envelope,
        json!({
            "ok": true,
            "iteration": 2,
            "mode": "unified",
            "file_count": 4,
            "diff": expected_diff,
        })
    );

    let requests = server.received();
    assert_eq!(requests.len(), 8, "the captured chain: {requests:?}");
    assert_eq!(requests[2].path, diff_iterations_path("137"));
    assert_eq!(requests[3].path, diff_commits_path());
}

/// `--iteration N` skips the first iteration-list GET; `--file` with it still
/// re-reads the list (captured: changes first, then the list).
#[test]
fn diff_iteration_uses_the_number_and_re_reads_the_list_only_for_content() {
    let home = TempHome::new();

    // The default view with --iteration 1 is one request.
    let server = MockServer::start();
    server.expect(
        "GET",
        &diff_changes_path("137", "1"),
        MockResponse::json(
            200,
            json!({"changeEntries": [
                {"changeTrackingId": 1, "changeId": 11, "changeType": "add", "item": {"path": "/src/first.ex", "additions": 7, "deletions": 0}},
            ]}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "137",
            "--iteration",
            "1",
            "--json",
        ],
    );

    assert_success(&output);
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["iteration"], json!(1));
    assert_eq!(envelope["changes"][0]["path"], json!("/src/first.ex"));
    assert_eq!(
        server
            .received()
            .iter()
            .map(|request| request.path.clone())
            .collect::<Vec<_>>(),
        [diff_changes_path("137", "1")],
        "no iteration list when the number is given"
    );

    // --file with --iteration 1 re-reads the list after the change list.
    let server = MockServer::start();
    server.expect(
        "GET",
        &diff_changes_path("137", "1"),
        MockResponse::json(
            200,
            json!({"changeEntries": [
                {"changeTrackingId": 1, "changeId": 11, "changeType": "add", "item": {"path": "/src/first.ex", "additions": 7, "deletions": 0}},
            ]}),
        ),
    );
    expect_diff_iterations(&server, "137", diff_iterations());
    expect_diff_item(&server, "/src/first.ex", "aaaa1111", "");
    expect_diff_item(&server, "/src/first.ex", "bbbb2222", "line one\n");

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "137",
            "--iteration",
            "1",
            "--file",
            "/src/first.ex",
            "--json",
        ],
    );

    assert_success(&output);
    let requests = server.received();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.path.as_str())
            .collect::<Vec<_>>(),
        [
            diff_changes_path("137", "1").as_str(),
            diff_iterations_path("137").as_str(),
            diff_items_path().as_str(),
            diff_items_path().as_str(),
        ]
    );
    assert_eq!(
        requests[2].query_pairs(),
        item_pairs("/src/first.ex", "aaaa1111")
    );
    assert_eq!(
        requests[3].query_pairs(),
        item_pairs("/src/first.ex", "bbbb2222")
    );
}

/// D34: an iteration number below 1 has no `resolve_iteration/2` clause and the
/// oracle exits 0 silently; this build refuses the value — `0` against the range
/// and `-1` as an unexpected argument — and sends nothing.
#[test]
fn diff_iteration_below_one_is_a_usage_error() {
    let home = TempHome::new();

    for (value, expected) in [("0", "--iteration"), ("-1", "-1")] {
        let server = MockServer::start();

        let output = run(
            &home,
            &server,
            &[
                "prs",
                "diff",
                "Alpha",
                "Alpha.Core",
                "137",
                "--iteration",
                value,
                "--json",
            ],
        );

        assert_eq!(output.status.code(), Some(1), "--iteration {value}");
        assert!(
            stderr_of(&output).contains(expected),
            "stderr names the offending token for --iteration {value}: {}",
            stderr_of(&output)
        );
        assert!(stdout_of(&output).is_empty());
        assert!(server.received().is_empty());
    }
}

/// D5: a non-integer iteration value is loud on both sides; the oracle prints the
/// command help on stdout first.
#[test]
fn diff_iteration_that_is_not_a_number_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "137",
            "--iteration",
            "abc",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--iteration"),
        "stderr names the flag: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

/// A pull request with no iterations is the frozen module's message, exit 1.
#[test]
fn diff_without_iterations_is_loud() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(&server, "138", json!({"count": 0, "value": []}));

    let output = run(
        &home,
        &server,
        &["prs", "diff", "Alpha", "Alpha.Core", "138", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!("PR #138 has no iterations (nothing to diff).")
    );
    assert_eq!(server.received().len(), 1);
}

/// An empty change list is still one document, `count: 0`.
#[test]
fn diff_empty_change_list_is_the_empty_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(&server, "141", diff_iterations());
    server.expect(
        "GET",
        &diff_changes_path("141", "2"),
        MockResponse::json(200, json!({"changeEntries": []})),
    );

    let output = run(
        &home,
        &server,
        &["prs", "diff", "Alpha", "Alpha.Core", "141", "--json"],
    );

    assert_success(&output);
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(
        envelope,
        json!({
            "ok": true,
            "iteration": 2,
            "count": 0,
            "total_additions": 0,
            "total_deletions": 0,
            "changes": [],
        })
    );
}

/// A 404 on the iteration list is the generic `not_found` envelope with the body's
/// status (D24's body rendering lives in the harness rule, not here).
#[test]
fn diff_iterations_404_is_the_generic_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect_query(
        "GET",
        &diff_iterations_path("140"),
        &[],
        MockResponse::json(
            404,
            json!({"message": "TF401180: The pull request 140 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["prs", "diff", "Alpha", "Alpha.Core", "140", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
}

/// D36: an iteration without a commit pair is this build's `api_error` where the
/// frozen `Helpers.bail/2` catch-all calls it a network failure.
#[test]
fn diff_iteration_without_a_commit_pair_is_an_api_error() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(
        &server,
        "139",
        json!({"count": 1, "value": [{"id": 1, "targetRefCommit": {"commitId": "aaaa1111"}}]}),
    );
    server.expect(
        "GET",
        &diff_changes_path("139", "1"),
        MockResponse::json(
            200,
            json!({"changeEntries": [
                {"changeTrackingId": 1, "changeId": 1, "changeType": 2, "item": {"path": "/src/app.ex", "additions": 1, "deletions": 1}},
            ]}),
        ),
    );
    expect_diff_iterations(
        &server,
        "139",
        json!({"count": 1, "value": [{"id": 1, "targetRefCommit": {"commitId": "aaaa1111"}}]}),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "139",
            "--file",
            "/src/app.ex",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Iteration is missing sourceRefCommit or targetRefCommit")
    );
    assert_eq!(
        server.received().len(),
        3,
        "no content request for a rejected iteration"
    );
}

/// D36: a content fetch that 404s on a file which must exist is `not_found` with
/// the module's own message.
#[test]
fn diff_file_item_404_is_not_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_diff_iterations(&server, "137", diff_iterations());
    server.expect(
        "GET",
        &diff_changes_path("137", "2"),
        MockResponse::json(200, diff_changes()),
    );
    expect_diff_iterations(&server, "137", diff_iterations());
    server.expect_query(
        "GET",
        &diff_items_path(),
        &[
            ("path", &encoded_path("/src/app.ex")),
            ("version", "aaaa1111"),
        ],
        MockResponse::json(404, json!({"message": "TF401180: not found"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "diff",
            "Alpha",
            "Alpha.Core",
            "137",
            "--file",
            "/src/app.ex",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_str(&stdout_of(&output)).expect("stdout is one JSON document");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!("File not found in commit aaaa1111")
    );
}

/// The missing positional is a usage error (D5's class: the oracle prints its
/// help on stdout first). The assertion names the positional, so an
/// unrecognised-subcommand error cannot satisfy it for the wrong reason.
#[test]
fn diff_without_a_positional_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["prs", "diff", "Alpha", "Alpha.Core", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_of(&output).is_empty());
    assert!(
        stderr_of(&output).contains("PR_ID"),
        "stderr names the missing positional: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

// ── Wave 2: the `prs comments` commands (Task 11a) ──────────────────────────
//
// Captured against the standalone mock before porting (`captures/task11a/`).
// `list` and `update` spell the path `pullRequests` (capital R); `add`, `delete`
// and `resolve` spell it `pullrequests` — captured, both kept. The write paths'
// `--json` documents are the frozen ones and are mirrored (D38), not rewritten
// into the `{ok,result}` envelope: `update --dry-run` prints its actions document
// even without `--json`, `resolve --status bogus` passes the value through
// unvalidated (captured), and a missing `--content` on `add` is a silent empty
// body in the oracle where this build's clap is loud (D34's class). The delete
// prompt is the wave's third: `Close thread N? [y/N] ` (or `Close comment N in
// thread N? [y/N] `), the oracle refuses with `Cancelled.` on stdout and exit 0,
// and this build refuses on stderr with exit 1 (R2/D31/D32). The invocations
// table's underscore spellings (`--file_path`, `--thread_id`, `--comment_id`,
// `--end_line`, `--resolved_by_me`, `--dry_run`) are rejected by the frozen
// parser; the hyphenated ones are runnable (D17).

/// `…/pullRequests/{pr_id}` — `list` and `update` spell it with a capital R.
fn comments_route(pr_id: i64) -> String {
    format!("/{ORG}/Alpha/_apis/git/repositories/Alpha.Core/pullRequests/{pr_id}")
}

/// `…/pullrequests/{pr_id}` — `add`, `delete` and `resolve` spell it lower case.
fn comments_route_lower(pr_id: i64) -> String {
    format!("/{ORG}/Alpha/_apis/git/repositories/Alpha.Core/pullrequests/{pr_id}")
}

fn threads_route(pr_id: i64) -> String {
    format!("{}/threads", comments_route(pr_id))
}

fn thread_route(pr_id: i64, thread_id: i64) -> String {
    format!("{}/threads/{thread_id}", comments_route(pr_id))
}

fn thread_comment_route(pr_id: i64, thread_id: i64, comment_id: i64) -> String {
    format!("{}/comments/{comment_id}", thread_route(pr_id, thread_id))
}

/// `…/pullRequests/{pr_id}` without the org — the shape the `--dry-run` document
/// prints, because the oracle names the path its client will prefix.
fn comments_local_route(pr_id: i64) -> String {
    format!("/Alpha/_apis/git/repositories/Alpha.Core/pullRequests/{pr_id}")
}

fn lower_thread_route(pr_id: i64, thread_id: i64) -> String {
    format!("{}/threads/{thread_id}", comments_route_lower(pr_id))
}

fn lower_thread_comment_route(pr_id: i64, thread_id: i64, comment_id: i64) -> String {
    format!(
        "{}/comments/{comment_id}",
        lower_thread_route(pr_id, thread_id)
    )
}

fn lower_threads_route(pr_id: i64) -> String {
    format!("{}/threads", comments_route_lower(pr_id))
}

/// The captured `GET …/threads` answer: an inline thread with a reply and a
/// resolved thread.
fn review_threads() -> Value {
    json!([
        {
            "id": 7,
            "status": "active",
            "threadContext": {"filePath": "/src/app.ex"},
            "comments": [
                {"id": 21, "author": {"displayName": "Alice"}, "content": "Please add a retry here.", "parentCommentId": 0},
                {"id": 22, "author": {"displayName": "Bob"}, "content": "Good catch, fixing.", "parentCommentId": 21}
            ]
        },
        {
            "id": 8,
            "status": "fixed",
            "comments": [
                {"id": 23, "author": {"displayName": "Carol"}, "content": "Nit: rename this.", "parentCommentId": 0}
            ]
        }
    ])
}

fn expect_threads(server: &MockServer, pr_id: i64, body: Value) {
    server.expect("GET", &threads_route(pr_id), MockResponse::json(200, body));
}

#[test]
fn comments_list_emits_the_oracle_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_threads(&server, 137, review_threads());

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "list",
            "Alpha",
            "Alpha.Core",
            "137",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", json!({"ok": true, "result": review_threads()})),
        "the value envelope (W1-R12); the oracle's captured line is the same document"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, threads_route(137));
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

#[test]
fn comments_list_human_is_the_modules_thread_listing() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_threads(&server, 137, review_threads());

    let output = run(
        &home,
        &server,
        &["prs", "comments", "list", "Alpha", "Alpha.Core", "137"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "\n  Thread 7 [active]\n    [21] Alice: Please add a retry here.\n    [22] Bob: Good catch, fixing.\n\n  Thread 8 [fixed]\n    [23] Carol: Nit: rename this.\n\n",
        "the module's compact listing, byte for byte"
    );
}

#[test]
fn comments_list_all_human_expands_comments_and_reply_markers() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_threads(&server, 137, review_threads());

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "list",
            "Alpha",
            "Alpha.Core",
            "137",
            "--all",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "\n  Thread 7 [active] on /src/app.ex\n    [21] Alice:\n      Please add a retry here.\n    [22] (reply to 21) Bob:\n      Good catch, fixing.\n\n  Thread 8 [fixed]\n    [23] Carol:\n      Nit: rename this.\n\n",
        "the expanded listing, byte for byte"
    );
}

#[test]
fn comments_list_empty_human_is_a_lone_blank_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_threads(&server, 8, json!([]));

    let output = run(
        &home,
        &server,
        &["prs", "comments", "list", "Alpha", "Alpha.Core", "8"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "\n");
}

#[test]
fn comments_list_404_is_the_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &threads_route(999),
        MockResponse::json(
            404,
            json!({"message": "The pull request 999 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "list",
            "Alpha",
            "Alpha.Core",
            "999",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
}

#[test]
fn comments_list_500_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &threads_route(500),
        MockResponse::json(
            500,
            json!({"message": "TF400898: An Internal Error Occurred."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "list",
            "Alpha",
            "Alpha.Core",
            "500",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
}

#[test]
fn comments_add_posts_the_general_thread_body_and_the_json_document() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        &lower_threads_route(137),
        MockResponse::json(
            200,
            json!({"id": 11, "comments": [{"id": 21, "content": "Looks good"}]}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--content",
            "Looks good",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "thread_id": 11, "comment_id": 21, "message": "Comment added."})
        )
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one POST");
    assert_eq!(received[0].method, "POST");
    assert_eq!(received[0].path, lower_threads_route(137));
    assert_eq!(
        sent_body(&received[0]),
        json!({
            "comments": [{"commentType": "text", "content": "Looks good", "parentCommentId": 0}],
            "status": "active"
        })
    );
}

#[test]
fn comments_add_inline_posts_the_canonical_path_and_range() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        &lower_threads_route(140),
        MockResponse::json(200, json!({"id": 14, "comments": [{"id": 24}]})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "add",
            "Alpha",
            "Alpha.Core",
            "140",
            "--content",
            "Range note",
            "--file-path",
            "src/foo.ex",
            "--line",
            "3",
            "--end-line",
            "5",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "thread_id": 14, "comment_id": 24, "message": "Comment added to /src/foo.ex:3-5."})
        )
    );

    let received = server.received();
    assert_eq!(
        sent_body(&received[0]),
        json!({
            "comments": [{"commentType": "text", "content": "Range note", "parentCommentId": 0}],
            "status": "active",
            "threadContext": {
                "filePath": "/src/foo.ex",
                "rightFileStart": {"line": 3, "offset": 1},
                "rightFileEnd": {"line": 5, "offset": 1}
            }
        })
    );
}

#[test]
fn comments_add_reply_posts_to_the_thread_comments_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        &format!("{}/comments", lower_thread_route(137, 8)),
        MockResponse::json(200, json!({"id": 27, "content": "Reply to 3"})),
    );

    // No --comment-id: the captured default is 0. The response has no `comments`
    // array, so the oracle's document carries `comment_id: null`.
    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--content",
            "Reply to 3",
            "--thread-id",
            "8",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "thread_id": 27, "comment_id": null, "message": "Reply added to thread 8."})
        )
    );

    let received = server.received();
    assert_eq!(received[0].method, "POST");
    assert_eq!(
        received[0].path,
        format!("{}/comments", lower_thread_route(137, 8))
    );
    assert_eq!(
        sent_body(&received[0]),
        json!({"commentType": "text", "content": "Reply to 3", "parentCommentId": 0})
    );
}

#[test]
fn comments_add_reads_stdin_content() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        &lower_threads_route(137),
        MockResponse::json(200, json!({"id": 11, "comments": [{"id": 21}]})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--content",
            "-",
            "--json",
        ],
        b"Stdin add\n\n",
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[0])["comments"][0]["content"],
        json!("Stdin add"),
        "the trailing newlines the module strips"
    );
}

#[test]
fn comments_add_missing_content_is_a_loud_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--content"),
        "the missing required option is named: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty(), "nothing is sent");
}

#[test]
fn comments_add_invalid_status_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--content",
            "X",
            "--status",
            "bogus",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!(
            "Invalid --status 'bogus'. Must be one of: active, fixed, wontFix, closed, byDesign."
        )
    );
    assert!(server.received().is_empty(), "nothing is sent");
}

#[test]
fn comments_add_missing_file_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--content",
            "@/nonexistent/file.md",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("/nonexistent/file.md"),
        "the message names the file: {}",
        envelope["error"]["message"]
    );
    assert!(server.received().is_empty(), "nothing is sent");
}

#[test]
fn comments_update_content_patches_the_comment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &thread_comment_route(137, 7, 3),
        MockResponse::json(200, json!({"id": 3, "content": "Edited text"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "update",
            "Alpha",
            "Alpha.Core",
            "137",
            "7",
            "3",
            "--content",
            "Edited text",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "thread_id": 7, "comment_id": 3, "status": null, "message": "Comment updated."})
        ),
        "the captured document: the response's id and the positional thread id"
    );
    assert_eq!(
        sent_body(&server.received()[0]),
        json!({"content": "Edited text"})
    );
}

#[test]
fn comments_update_status_patches_the_thread_and_keeps_the_arg_ids() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &thread_route(137, 8),
        MockResponse::json(200, json!({"id": 8, "status": "fixed"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "update",
            "Alpha",
            "Alpha.Core",
            "137",
            "8",
            "3",
            "--status",
            "fixed",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "thread_id": 8, "comment_id": 3, "status": null, "message": "Thread status updated."})
        ),
        "captured: the status-only path reports the argument ids and a null status"
    );
    assert_eq!(sent_body(&server.received()[0]), json!({"status": "fixed"}));
}

#[test]
fn comments_update_both_patches_the_thread_then_the_comment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &thread_route(137, 9),
        MockResponse::json(200, json!({"id": 9, "status": "fixed"})),
    );
    server.expect(
        "PATCH",
        &thread_comment_route(137, 9, 4),
        MockResponse::json(200, json!({"id": 4, "content": "Both edited"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "update",
            "Alpha",
            "Alpha.Core",
            "137",
            "9",
            "4",
            "--content",
            "Both edited",
            "--status",
            "fixed",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "thread_id": 9, "comment_id": 4, "status": "fixed", "message": "Comment and thread status updated."})
        )
    );

    let received = server.received();
    assert_eq!(
        received.len(),
        2,
        "the thread PATCH, then the comment PATCH"
    );
    assert_eq!(received[0].path, thread_route(137, 9));
    assert_eq!(sent_body(&received[0]), json!({"status": "fixed"}));
    assert_eq!(received[1].path, thread_comment_route(137, 9, 4));
    assert_eq!(sent_body(&received[1]), json!({"content": "Both edited"}));
}

#[test]
fn comments_update_resolved_by_me_uses_connection_data() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_connection_data(&server);
    server.expect(
        "PATCH",
        &thread_route(137, 10),
        MockResponse::json(200, json!({"id": 10, "status": "fixed"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "update",
            "Alpha",
            "Alpha.Core",
            "137",
            "10",
            "3",
            "--status",
            "fixed",
            "--resolved-by-me",
            "--json",
        ],
    );

    assert_success(&output);
    let received = server.received();
    assert_eq!(received.len(), 2, "connectionData, then the thread PATCH");
    assert_eq!(received[0].path, CONNECTION_DATA_PATH);
    assert_eq!(
        sent_body(&received[1]),
        json!({"status": "fixed", "resolvedBy": {"id": REVIEWER_ID}})
    );
}

#[test]
fn comments_update_dry_run_prints_the_actions_document_and_sends_nothing() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "update",
            "Alpha",
            "Alpha.Core",
            "137",
            "9",
            "4",
            "--content",
            "Both edited",
            "--status",
            "fixed",
            "--dry-run",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({
                "ok": true,
                "dry_run": true,
                "actions": [
                    {"method": "PATCH", "path": format!("{}/threads/9", comments_local_route(137)), "body": {"status": "fixed"}},
                    {"method": "PATCH", "path": format!("{}/threads/9/comments/4", comments_local_route(137)), "body": {"content": "Both edited"}}
                ]
            })
        ),
        "the captured actions document, compact and even without --json"
    );
    assert!(server.received().is_empty(), "a dry run sends nothing");
}

#[test]
fn comments_update_without_content_or_status_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "update",
            "Alpha",
            "Alpha.Core",
            "137",
            "7",
            "3",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["error"]["code"], json!("validation_error"));
    assert_eq!(
        envelope["error"]["message"],
        json!(
            "Must pass --content and/or --status. Pass --content to edit a comment, --status to change a thread's resolution state, or both."
        )
    );
    assert!(server.received().is_empty());
}

#[test]
fn comments_update_rejects_the_underscore_spelling() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "update",
            "Alpha",
            "Alpha.Core",
            "137",
            "7",
            "3",
            "--content",
            "X",
            "--resolved_by_me",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--resolved_by_me"),
        "the rejected spelling is named: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn comments_delete_with_force_closes_the_thread() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &lower_thread_route(137, 21),
        MockResponse::json(200, json!({"id": 21, "status": "closed"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "delete",
            "Alpha",
            "Alpha.Core",
            "137",
            "21",
            "--force",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", json!({"ok": true, "closed": "thread 21"}))
    );
    assert_eq!(
        sent_body(&server.received()[0]),
        json!({"status": "closed"})
    );
}

#[test]
fn comments_delete_with_comment_id_deletes_the_comment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &lower_thread_comment_route(137, 25, 4),
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "delete",
            "Alpha",
            "Alpha.Core",
            "137",
            "25",
            "--comment-id",
            "4",
            "--force",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "closed": "comment 4 in thread 25"})
        )
    );
    let received = server.received();
    assert_eq!(received[0].method, "DELETE");
    assert_eq!(received[0].path, lower_thread_comment_route(137, 25, 4));
    assert!(received[0].body.is_none(), "a DELETE carries no body");
}

#[test]
fn comments_delete_asks_and_proceeds_on_yes() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &lower_thread_route(137, 20),
        MockResponse::json(200, json!({"id": 20, "status": "closed"})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "delete",
            "Alpha",
            "Alpha.Core",
            "137",
            "20",
            "--json",
        ],
        b"y\n",
    );

    assert_success(&output);
    assert!(
        stderr_of(&output).contains("Close thread 20? [y/N] "),
        "the question is on stderr (D31): {}",
        stderr_of(&output)
    );
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", json!({"ok": true, "closed": "thread 20"})),
        "stdout carries exactly one document"
    );
    assert_eq!(server.received().len(), 1, "the confirmed request went out");
}

#[test]
fn comments_delete_refuses_on_no_and_exits_1() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "delete",
            "Alpha",
            "Alpha.Core",
            "137",
            "26",
            "--json",
        ],
        b"n\n",
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "a refusal is never a success (R2)"
    );
    assert!(stdout_of(&output).is_empty(), "no document on stdout");
    assert!(
        stderr_of(&output).contains("Cancelled."),
        "the refusal is on stderr: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty(), "nothing was done");
}

#[test]
fn comments_delete_treats_eof_as_a_refusal() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "delete",
            "Alpha",
            "Alpha.Core",
            "137",
            "26",
        ],
        b"",
    );

    assert_eq!(output.status.code(), Some(1), "EOF is an answered no (D30)");
    assert!(stdout_of(&output).is_empty());
    assert!(stderr_of(&output).contains("Cancelled."));
    assert!(server.received().is_empty());
}

#[test]
fn comments_delete_force_skips_the_prompt() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &lower_thread_route(137, 21),
        MockResponse::json(200, json!({"id": 21, "status": "closed"})),
    );

    let output = run_with_stdin(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "delete",
            "Alpha",
            "Alpha.Core",
            "137",
            "21",
            "--force",
        ],
        b"",
    );

    assert_success(&output);
    assert!(
        stderr_of(&output).is_empty(),
        "--force asks nothing: {}",
        stderr_of(&output)
    );
    assert_eq!(server.received().len(), 1);
}

#[test]
fn comments_resolve_patches_the_thread_and_mirrors_the_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &lower_thread_route(137, 5),
        MockResponse::json(200, json!({"id": 5, "status": "fixed"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "resolve",
            "Alpha",
            "Alpha.Core",
            "137",
            "5",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", json!({"ok": true, "thread": 5, "status": "fixed"})),
        "the captured document (note the `thread` key)"
    );
    assert_eq!(sent_body(&server.received()[0]), json!({"status": "fixed"}));
}

#[test]
fn comments_resolve_passes_an_unknown_status_through() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        &lower_thread_route(137, 5),
        MockResponse::json(200, json!({"id": 5, "status": "bogus"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "resolve",
            "Alpha",
            "Alpha.Core",
            "137",
            "5",
            "--status",
            "bogus",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[0]),
        json!({"status": "bogus"}),
        "captured: resolve does not validate the status"
    );
}

#[test]
fn comments_resolve_resolved_by_me_sends_resolved_by() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_connection_data(&server);
    server.expect(
        "PATCH",
        &lower_thread_route(137, 4),
        MockResponse::json(200, json!({"id": 4, "status": "fixed"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "resolve",
            "Alpha",
            "Alpha.Core",
            "137",
            "4",
            "--resolved-by-me",
            "--json",
        ],
    );

    assert_success(&output);
    let received = server.received();
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].path, CONNECTION_DATA_PATH);
    assert_eq!(
        sent_body(&received[1]),
        json!({"status": "fixed", "resolvedBy": {"id": REVIEWER_ID}})
    );
}

#[test]
fn comments_resolve_without_a_thread_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "comments",
            "resolve",
            "Alpha",
            "Alpha.Core",
            "137",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("THREAD_ID"),
        "stderr names the missing positional: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

// ── Wave 2: the `prs reviewers` commands (Task 11b) ─────────────────────────
//
// Captured against the standalone mock before porting (`captures/task11b/`).
// All three leaves spell the path `pullrequests` (lower case, like `vote`), the
// `--reviewer` value is interpolated into the path *and* sent as the body's
// `id` (`{"id": X}` optional, `{"id": X, "isRequired": true}` required), and
// `--search` is the module's client-side fuzzy filter over `displayName` and
// `uniqueName` (substring or subsequence, case-insensitive; an absent or empty
// query is no filter at all). Under `--json` the frozen CLI prints its human
// success prose on the write paths (D33) and a `{"ok":true,"count":N,"items":…}`
// document on `list`, which this build mirrors. Without `--reviewer` the frozen
// `Map.fetch!(parsed.options, :reviewer)` raises inside `AdoCli.CLI.run/1`'s
// swallowed rescue, so both writes exit 0 with both streams empty — D34's class,
// loud here. No underscore spelling is rejected by the frozen parser this time
// (`--reviewer`, `--required` and `--search` are the schema's own names).

/// The captured `GET …/reviewers` answer: a required voter, an optional voter
/// with a negative vote, and a reviewer with neither `vote` nor `isRequired`.
fn pr_reviewers() -> Value {
    json!([
        {
            "id": "aaaaaaaa-0001-0001-0001-000000000001",
            "displayName": "Ada Example",
            "uniqueName": "ada@example.com",
            "vote": 10,
            "isRequired": true,
        },
        {
            "id": "bbbbbbbb-0002-0002-0002-000000000002",
            "displayName": "Bob Jones",
            "uniqueName": "bob@example.com",
            "vote": -5,
            "isRequired": false,
        },
        {
            "id": "cccccccc-0003-0003-0003-000000000003",
            "displayName": "Carol Ng",
            "uniqueName": "carol@example.com",
        }
    ])
}

const ADA: &str = "aaaaaaaa-0001-0001-0001-000000000001";
const BOB: &str = "bbbbbbbb-0002-0002-0002-000000000002";

fn reviewers_route(pr_id: i64) -> String {
    format!("{}/reviewers", pr_path(pr_id))
}

fn expect_reviewers(server: &MockServer, pr_id: i64, value: Value) {
    let count = value.as_array().map_or(0, Vec::len);

    server.expect(
        "GET",
        &reviewers_route(pr_id),
        MockResponse::json(200, json!({"count": count, "value": value})),
    );
}

fn expect_reviewer_item(server: &MockServer, pr_id: i64, reviewer: &str, body: Value) {
    server.expect(
        "PUT",
        &reviewer_path(pr_id, reviewer),
        MockResponse::json(200, body),
    );
}

#[test]
fn reviewers_list_emits_the_oracle_list_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewers(&server, 137, pr_reviewers());

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "list",
            "Alpha",
            "Alpha.Core",
            "137",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "count": 3, "items": pr_reviewers()})
        ),
        "the list envelope, the oracle's captured document"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "GET");
    assert_eq!(received[0].path, reviewers_route(137));
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
}

#[test]
fn reviewers_list_human_is_the_builds_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewers(&server, 137, pr_reviewers());

    let output = run(
        &home,
        &server,
        &["prs", "reviewers", "list", "Alpha", "Alpha.Core", "137"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        5,
        "a header row, a rule row and one row per reviewer: {stdout}"
    );
    assert!(
        lines[0].contains("Display Name")
            && lines[0].contains("Email")
            && lines[0].contains("Vote")
            && lines[0].contains("Required"),
        "the module's four columns: {stdout}"
    );
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].starts_with("Ada Example")
            && lines[2].contains("ada@example.com")
            && lines[2].contains("10")
            && lines[2].ends_with("yes"),
        "the required voter's row: {stdout}"
    );
    assert!(
        lines[3].starts_with("Bob Jones")
            && lines[3].contains("bob@example.com")
            && lines[3].contains("-5")
            && lines[3].ends_with("no"),
        "the optional voter's row: {stdout}"
    );
    assert!(
        lines[4].starts_with("Carol Ng") && lines[4].ends_with("no"),
        "a reviewer without isRequired is not required: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn reviewers_list_with_no_reviewers_says_so() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewers(&server, 8, json!([]));

    let output = run(
        &home,
        &server,
        &["prs", "reviewers", "list", "Alpha", "Alpha.Core", "8"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No reviewers.\n");
}

#[test]
fn reviewers_list_search_filters_by_substring_case_insensitively() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewers(&server, 137, pr_reviewers());

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "list",
            "Alpha",
            "Alpha.Core",
            "137",
            "--search",
            "ADA",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "count": 1, "items": [pr_reviewers()[0].clone()]})
        ),
        "the query is matched case-insensitively against displayName and uniqueName"
    );
}

#[test]
fn reviewers_list_search_matches_a_subsequence_over_both_fields() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewers(&server, 137, pr_reviewers());

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "list",
            "Alpha",
            "Alpha.Core",
            "137",
            "--search",
            "aae",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({
                "ok": true,
                "count": 2,
                "items": [pr_reviewers()[0].clone(), pr_reviewers()[2].clone()],
            })
        ),
        "captured: aae is a subsequence of Ada Example, and of carol@example.com (whose \
         uniqueName carries the second a) — the fuzzy filter reads uniqueName too"
    );
}

#[test]
fn reviewers_list_search_with_an_empty_query_is_no_filter() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewers(&server, 137, pr_reviewers());

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "list",
            "Alpha",
            "Alpha.Core",
            "137",
            "--search",
            "",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "count": 3, "items": pr_reviewers()})
        ),
        "captured: an empty query returns every reviewer"
    );
}

#[test]
fn reviewers_list_search_with_no_match_is_an_empty_list() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewers(&server, 137, pr_reviewers());

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "list",
            "Alpha",
            "Alpha.Core",
            "137",
            "--search",
            "zzz",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!("{}\n", json!({"ok": true, "count": 0, "items": []}))
    );
}

/// D41's shape: a 200 whose body has no `value` key. The frozen `list_reviewers/1`
/// matches `{:ok, %{"value" => …}}` with no fallback, so its `CaseClauseError` is
/// swallowed into exit 0 with both streams empty; this build's `Client::list`
/// passes the body through and `items/1` wraps it as one element, so the document
/// is a one-item list rather than nothing.
#[test]
fn reviewers_list_without_a_value_key_wraps_the_body_as_one_item() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &reviewers_route(138),
        MockResponse::json(200, json!({"count": 0, "message": "no value here"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "list",
            "Alpha",
            "Alpha.Core",
            "138",
            "--json",
        ],
    );

    assert_success(&output);
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the list envelope");
    assert_eq!(
        envelope,
        json!({"ok": true, "count": 1, "items": [{"count": 0, "message": "no value here"}]}),
        "D41: the whole body is wrapped as one item (where the oracle prints nothing)"
    );
}

#[test]
fn reviewers_list_404_is_the_not_found_envelope_with_the_upstream_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &reviewers_route(999),
        MockResponse::json(
            404,
            json!({"message": "The pull request 999 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "list",
            "Alpha",
            "Alpha.Core",
            "999",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["details"]["body"],
        json!(r#"{"message":"The pull request 999 does not exist."}"#),
        "the body stays the upstream bytes (D24)"
    );
}

#[test]
fn reviewers_list_500_is_an_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &reviewers_route(500),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized to access this resource."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "list",
            "Alpha",
            "Alpha.Core",
            "500",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
}

#[test]
fn reviewers_list_without_a_pr_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["prs", "reviewers", "list", "Alpha", "Alpha.Core", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("PR_ID"),
        "stderr names the missing positional: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty());
}

#[test]
fn reviewers_add_puts_the_reviewer_id() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewer_item(
        &server,
        137,
        ADA,
        json!({"id": ADA, "displayName": "Ada Example", "vote": 0, "isRequired": true}),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--reviewer",
            ADA,
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "result": {"id": ADA, "displayName": "Ada Example", "vote": 0, "isRequired": true}})
        ),
        "the answered reviewer is the value envelope (D33)"
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "PUT");
    assert_eq!(received[0].path, reviewer_path(137, ADA));
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
    assert_eq!(sent_body(&received[0]), json!({"id": ADA}));
}

#[test]
fn reviewers_add_required_sends_is_required_true() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewer_item(
        &server,
        139,
        BOB,
        json!({"id": BOB, "displayName": "Bob Jones", "isRequired": true}),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "add",
            "Alpha",
            "Alpha.Core",
            "139",
            "--reviewer",
            BOB,
            "--required",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&server.received()[0]),
        json!({"id": BOB, "isRequired": true})
    );
    assert_eq!(
        server.received()[0].path,
        reviewer_path(139, BOB),
        "the reviewer id addresses the item route"
    );
}

#[test]
fn reviewers_add_human_names_the_requiredness() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_reviewer_item(&server, 137, ADA, json!({"id": ADA}));
    expect_reviewer_item(&server, 139, BOB, json!({"id": BOB}));

    let optional = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--reviewer",
            ADA,
        ],
    );
    assert_success(&optional);
    assert_eq!(
        stdout_of(&optional),
        format!("Reviewer {ADA} added (optional).\n")
    );

    let required = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "add",
            "Alpha",
            "Alpha.Core",
            "139",
            "--reviewer",
            BOB,
            "--required",
        ],
    );
    assert_success(&required);
    assert_eq!(
        stdout_of(&required),
        format!("Reviewer {BOB} added (required).\n")
    );
}

#[test]
fn reviewers_add_encodes_an_email_path_segment_but_not_the_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &reviewer_path(137, "ada%40example.com"),
        MockResponse::json(200, json!({"id": ADA, "uniqueName": "ada@example.com"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--reviewer",
            "ada@example.com",
            "--json",
        ],
    );

    assert_success(&output);
    let received = server.received();
    assert_eq!(
        received[0].path,
        reviewer_path(137, "ada%40example.com"),
        "the path segment is percent-encoded (D22); the frozen URI.encode/1 left the @ alone"
    );
    assert_eq!(
        sent_body(&received[0]),
        json!({"id": "ada@example.com"}),
        "the body carries the reviewer exactly as given"
    );
}

#[test]
fn reviewers_add_404_is_the_not_found_envelope_naming_the_reviewer() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &reviewer_path(999, ADA),
        MockResponse::json(
            404,
            json!({"message": "The pull request 999 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "add",
            "Alpha",
            "Alpha.Core",
            "999",
            "--reviewer",
            ADA,
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!(format!(
            "Reviewer not found: {ADA}. Use the user's GUID from Azure DevOps."
        ))
    );
}

#[test]
fn reviewers_add_400_is_the_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &reviewer_path(137, ADA),
        MockResponse::json(
            400,
            json!({"message": "VS403352: The identity is invalid."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--reviewer",
            ADA,
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(400));
}

#[test]
fn reviewers_add_without_a_reviewer_is_a_loud_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "add",
            "Alpha",
            "Alpha.Core",
            "137",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("--reviewer"),
        "stderr names the missing option: {}",
        stderr_of(&output)
    );
    assert!(
        stdout_of(&output).is_empty(),
        "no document for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "the oracle exits 0 silently here; this build refuses before the request (D34)"
    );
}

#[test]
fn reviewers_remove_deletes_the_reviewer() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &reviewer_path(137, ADA),
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "remove",
            "Alpha",
            "Alpha.Core",
            "137",
            "--reviewer",
            ADA,
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "{}\n",
            json!({"ok": true, "message": format!("Reviewer {ADA} removed.")})
        )
    );

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, "DELETE");
    assert_eq!(received[0].path, reviewer_path(137, ADA));
    assert_eq!(received[0].query_pairs(), vec![api_version()]);
    assert!(received[0].body.is_none(), "a DELETE carries no body");
}

#[test]
fn reviewers_remove_human_prints_the_success_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &reviewer_path(137, ADA),
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "remove",
            "Alpha",
            "Alpha.Core",
            "137",
            "--reviewer",
            ADA,
        ],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), format!("Reviewer {ADA} removed.\n"));
}

#[test]
fn reviewers_remove_404_is_the_not_found_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &reviewer_path(999, ADA),
        MockResponse::json(
            404,
            json!({"message": "The pull request 999 does not exist."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "remove",
            "Alpha",
            "Alpha.Core",
            "999",
            "--reviewer",
            ADA,
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!(format!("Reviewer not found: {ADA}"))
    );
}

#[test]
fn reviewers_remove_500_is_an_api_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        &reviewer_path(137, BOB),
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized to access this resource."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "remove",
            "Alpha",
            "Alpha.Core",
            "137",
            "--reviewer",
            BOB,
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).starts_with('{'),
        "the frozen CLI prints `xx  Remove failed: …` prose here; this build keeps stdout a \
         document: {}",
        stdout_of(&output)
    );
    let envelope: Value = serde_json::from_str(&stdout_of(&output)).expect("the error envelope");
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
}

#[test]
fn reviewers_remove_without_a_reviewer_is_a_loud_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "prs",
            "reviewers",
            "remove",
            "Alpha",
            "Alpha.Core",
            "137",
            "--json",
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "the captured oracle exits 0 with both streams empty (D34)"
    );
    assert!(
        stderr_of(&output).contains("--reviewer"),
        "stderr names the missing option: {}",
        stderr_of(&output)
    );
    assert!(stdout_of(&output).is_empty());
    assert!(server.received().is_empty());
}
