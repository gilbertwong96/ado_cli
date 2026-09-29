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
