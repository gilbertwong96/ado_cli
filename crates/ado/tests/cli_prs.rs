//! End-to-end tests for `ado prs list|show`: the REST surface the frozen
//! `lib/ado_cli/cli/pull_requests.ex` builds — every method, path and query pair
//! verified against the frozen escript — the `--json` envelopes it emits, and the
//! human table/detail the module's formatters define.
//!
//! Both envelopes are pinned **byte-equal** to the captured oracle lines: every map
//! in the fixtures stays below Elixir's small-map threshold, so the term order the
//! oracle encodes is serde's sorted order, and each capture was confirmed
//! byte-identical to its `jq -S` form (W1-R12).
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
