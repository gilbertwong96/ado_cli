//! End-to-end tests for `ado imports list|show|create`: the project-scoped
//! `_apis/git/repositories/{repo}/importRequests` surface the frozen
//! `lib/ado_cli/cli/imports.ex`
//! builds, the `$top` pair, the create body, the human views and the error
//! paths.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a
//! mock server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT`
//! credentials, so no credential resolution reaches the developer's keychain.
//! Every run scripts stdin explicitly, so no test can read a terminal.

use std::process::{Command, Output, Stdio};

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";
const REPOSITORY: &str = "NewRepo";
const COLLECTION: &str = "/myorg/Alpha/_apis/git/repositories/NewRepo/importRequests";
const IMPORT: &str = "/myorg/Alpha/_apis/git/repositories/NewRepo/importRequests/imp-1";

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

/// The captured `imports list` body (`imports_list.json`): three rows, one with
/// no `parameters` at all.
fn imports() -> Value {
    json!({"count": 3, "value": [
        {"id": "imp-1", "status": "completed",
         "url": "https://dev.azure.com/ado-harness/Alpha/_apis/git/repositories/NewRepo/importRequests/imp-1",
         "repository": {"id": "repo-1", "name": "NewRepo"},
         "parameters": {"gitSource": {"url": "https://github.com/owner/repo.git"},
                        "deleteServiceEndpointAfterImportIsDone": false},
         "detailedStatus": {"allStepsSucceeded": true, "errorMessage": null}},
        {"id": "imp-2", "status": "failed",
         "url": "https://dev.azure.com/ado-harness/Alpha/_apis/git/repositories/Other/importRequests/imp-2",
         "parameters": {"gitSource": {"url": "https://github.com/owner/other.git"},
                        "deleteServiceEndpointAfterImportIsDone": false},
         "detailedStatus": {"allStepsSucceeded": false,
                            "errorMessage": "TF401019: The import failed."}},
        {"id": "imp-3", "status": "queued",
         "url": "https://dev.azure.com/ado-harness/Alpha/_apis/git/repositories/Third/importRequests/imp-3"}
    ]})
}

/// The captured `imports show imp-1` body (`import_show.json`): a nil
/// `errorMessage` beside a `false` `allStepsSucceeded`.
fn import() -> Value {
    json!({
        "id": "imp-1", "status": "inProgress",
        "url": "https://dev.azure.com/ado-harness/Alpha/_apis/git/repositories/NewRepo/importRequests/imp-1",
        "parameters": {"gitSource": {"url": "https://github.com/owner/repo.git"},
                       "deleteServiceEndpointAfterImportIsDone": false},
        "detailedStatus": {"allStepsSucceeded": false, "errorMessage": null}
    })
}

/// The captured `imports create` response.
fn created() -> Value {
    json!({
        "id": "imp-new", "status": "queued",
        "url": "https://dev.azure.com/ado-harness/Alpha/_apis/git/repositories/NewRepo/importRequests/imp-new"
    })
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
    server.expect("GET", COLLECTION, MockResponse::json(200, imports()));

    let output = run(
        &home,
        &server,
        &["imports", "list", "Alpha", REPOSITORY, "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": imports()["value"]}),
        "the value array under the value envelope"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, COLLECTION);
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "no --top means no $top pair"
    );
}

#[test]
fn list_sends_the_dollar_top_pair_only_when_the_option_is_given() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", COLLECTION, MockResponse::json(200, imports()));

    let output = run(
        &home,
        &server,
        &[
            "imports", "list", "Alpha", REPOSITORY, "--top", "5", "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("%24top".to_owned(), "5".to_owned())
        ],
        "the module's $top pair, percent-encoded as the frozen client encodes it"
    );

    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", COLLECTION, MockResponse::json(200, imports()));

    let output = run(
        &home,
        &server,
        &[
            "imports", "list", "Alpha", REPOSITORY, "--top", "0", "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("%24top".to_owned(), "0".to_owned())
        ],
        "zero is truthy in the module's `if t = Map.get(...)`, so the pair is sent"
    );
}

#[test]
fn list_renders_the_modules_columns_and_the_source_url() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", COLLECTION, MockResponse::json(200, imports()));

    let output = run(&home, &server, &["imports", "list", "Alpha", REPOSITORY]);

    assert_success(&output);
    let stdout = stdout_of(&output);

    for column in ["ID", "Status", "Source"] {
        assert!(stdout.contains(column), "the {column} column: {stdout}");
    }
    assert!(stdout.contains("imp-1"), "the id: {stdout}");
    assert!(stdout.contains("completed"), "the status: {stdout}");
    assert!(
        stdout.contains("https://github.com/owner/repo.git"),
        "the nested source url: {stdout}"
    );
    assert!(
        !stdout.contains("imp-3 queued"),
        "the table separates its columns: {stdout}"
    );
}

#[test]
fn list_empty_prints_the_modules_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COLLECTION,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["imports", "list", "Alpha", REPOSITORY]);

    assert_success(&output);
    assert_eq!(stdout_of(&output).trim_end(), "No imports found.");

    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COLLECTION,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["imports", "list", "Alpha", REPOSITORY, "--json"],
    );

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": []}));
}

/// C12's list-error pair, the two classes: 404 and 5xx.
#[test]
fn list_404_and_500_are_the_classified_envelopes() {
    for (status, code, message) in [
        (
            404,
            "not_found",
            "Resource not found. Check the project/repo/build ID and your permissions.",
        ),
        (500, "api_error", "Azure DevOps server error. Retry later."),
    ] {
        let home = TempHome::new();
        let server = MockServer::start();
        server.expect(
            "GET",
            COLLECTION,
            MockResponse::json(status, json!({"message": "upstream refused"})),
        );

        let output = run(
            &home,
            &server,
            &["imports", "list", "Alpha", REPOSITORY, "--json"],
        );

        assert_eq!(output.status.code(), Some(1));
        let envelope = envelope(&output);

        assert_eq!(envelope["ok"], json!(false));
        assert_eq!(envelope["error"]["code"], json!(code));
        assert_eq!(envelope["error"]["status"], json!(status));
        assert_eq!(envelope["error"]["message"], json!(message));
        assert_eq!(
            envelope["error"]["details"]["body"],
            json!("{\"message\":\"upstream refused\"}"),
            "the upstream bytes, where the oracle re-renders the decoded map (D24)"
        );
    }
}

#[test]
fn list_404_human_writes_the_labelled_line_to_stderr() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        COLLECTION,
        MockResponse::json(404, json!({"message": "Not found"})),
    );

    let output = run(&home, &server, &["imports", "list", "Alpha", REPOSITORY]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no human line on stdout (D4)"
    );
    assert!(
        stderr_of(&output).contains("Resource not found."),
        "the classified wording on stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn list_requires_the_project_positional() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["imports", "list"]);

    usage_error(&output, "PROJECT");
    assert!(requests(&server).is_empty(), "a usage error sends nothing");
}

#[test]
fn list_requires_the_repository_positional() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["imports", "list", "Alpha"]);

    usage_error(&output, "REPOSITORY");
    assert!(requests(&server).is_empty(), "a usage error sends nothing");
}

#[test]
fn list_rejects_an_extra_positional_and_an_unknown_flag() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["imports", "list", "Alpha", REPOSITORY, "Extra"],
    );

    usage_error(&output, "Extra");

    let output = run(
        &home,
        &server,
        &["imports", "list", "Alpha", REPOSITORY, "--nope"],
    );

    usage_error(&output, "nope");
    assert!(requests(&server).is_empty(), "a usage error sends nothing");
}

#[test]
fn list_rejects_a_non_numeric_top() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["imports", "list", "Alpha", REPOSITORY, "--top", "abc"],
    );

    usage_error(&output, "top");
    assert!(requests(&server).is_empty(), "a usage error sends nothing");
}

// ── show ────────────────────────────────────────────────────────────────

#[test]
fn show_emits_the_value_envelope_and_the_id_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", IMPORT, MockResponse::json(200, import()));

    let output = run(
        &home,
        &server,
        &["imports", "show", "Alpha", REPOSITORY, "imp-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": import()}));

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, IMPORT);
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())]
    );
}

#[test]
fn show_escapes_the_project_and_the_id_as_one_segment_each() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Alpha%20Beta/_apis/git/repositories/NewRepo/importRequests/imp%201",
        MockResponse::json(200, import()),
    );

    let output = run(
        &home,
        &server,
        &[
            "imports",
            "show",
            "Alpha Beta",
            REPOSITORY,
            "imp 1",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].path,
        "/myorg/Alpha%20Beta/_apis/git/repositories/NewRepo/importRequests/imp%201",
        "a space is %20 on both sides; a slash is %2F here and raw in the oracle (D22)"
    );
}

#[test]
fn show_renders_the_modules_detail_and_its_falsy_detail_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", IMPORT, MockResponse::json(200, import()));

    let output = run(
        &home,
        &server,
        &["imports", "show", "Alpha", REPOSITORY, "imp-1"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            concat!(
                "\nImport Status\n\n",
                "{}\n",
                "  ID:     imp-1\n",
                "  Status: inProgress\n",
                "  Source: https://github.com/owner/repo.git\n",
                "  Detail: false\n",
                "  URL:    https://dev.azure.com/ado-harness/Alpha/_apis/git/repositories/NewRepo/importRequests/imp-1\n",
                "\n"
            ),
            "-".repeat(60)
        ),
        "the module's layout, including the blank line the reset artefact leaves (captured)"
    );
}

#[test]
fn show_prints_the_none_placeholder_only_for_a_nil_url() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        IMPORT,
        MockResponse::json(
            200,
            json!({"id": "imp-2", "status": "failed",
                   "parameters": {"gitSource": {"url": "https://github.com/owner/other.git"}},
                   "detailedStatus": {"allStepsSucceeded": false}}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["imports", "show", "Alpha", REPOSITORY, "imp-1"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(stdout.contains("  URL:    (none)\n"), "{stdout}");
    assert!(
        !stdout.contains("  Source: (none)"),
        "an absent source is empty, not a placeholder: {stdout}"
    );
}

#[test]
fn show_omits_the_detail_line_when_detailed_status_is_absent() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        IMPORT,
        MockResponse::json(
            200,
            json!({"id": "imp-3", "status": "queued",
                   "url": "https://dev.azure.com/ado-harness/Alpha/_apis/git/repositories/Third/importRequests/imp-3"}),
        ),
    );

    let output = run(
        &home,
        &server,
        &["imports", "show", "Alpha", REPOSITORY, "imp-1"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);

    assert!(!stdout.contains("Detail:"), "no line at all: {stdout}");
    assert!(
        stdout.contains("  Source: \n"),
        "an absent source: {stdout}"
    );
}

#[test]
fn show_404_is_the_modules_wording_with_no_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Alpha/_apis/git/repositories/NewRepo/importRequests/missing",
        MockResponse::json(404, json!({"message": "Import request not found."})),
    );

    let output = run(
        &home,
        &server,
        &["imports", "show", "Alpha", REPOSITORY, "missing", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Import 'missing' not found"),
        "the module's own wording, in the error envelope this build always writes (D4)"
    );
    assert!(
        stderr_of(&output).is_empty(),
        "under --json the envelope is the whole output: {}",
        stderr_of(&output)
    );
}

#[test]
fn show_requires_both_positionals() {
    let home = TempHome::new();
    let server = MockServer::start();

    usage_error(
        &run(&home, &server, &["imports", "show", "Alpha"]),
        "REPOSITORY",
    );
    usage_error(&run(&home, &server, &["imports", "show"]), "PROJECT");
    assert!(requests(&server).is_empty(), "a usage error sends nothing");
}

// ── create ──────────────────────────────────────────────────────────────

#[test]
fn create_posts_the_module_body_and_reports_the_response() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", COLLECTION, MockResponse::json(200, created()));

    let output = run(
        &home,
        &server,
        &[
            "imports",
            "create",
            "Alpha",
            "NewRepo",
            "--url",
            "https://github.com/owner/repo.git",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": created()}),
        "the response under the value envelope (D33's value class)"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, COLLECTION);
    assert_eq!(
        sent_body(&requests[0]),
        json!({"parameters": {
            "gitSource": {"url": "https://github.com/owner/repo.git"},
            "deleteServiceEndpointAfterImportIsDone": false
        }}),
        "no credentials means no credential keys at all"
    );
}

#[test]
fn create_prints_the_modules_block_in_human_mode() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", COLLECTION, MockResponse::json(200, created()));

    let output = run(
        &home,
        &server,
        &[
            "imports",
            "create",
            "Alpha",
            "NewRepo",
            "--url",
            "https://github.com/owner/repo.git",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\nImport request created.\n",
            "\n",
            "  ID:        imp-new\n",
            "  Status:    queued\n",
            "  URL:       https://dev.azure.com/ado-harness/Alpha/_apis/git/repositories/NewRepo/importRequests/imp-new\n",
            "\n",
            "Check status with:\n",
            "  ado imports show Alpha imp-new\n"
        ),
        "the module's block, blank lines included (captured)"
    );
}

#[test]
fn create_adds_the_credential_fields_only_when_given() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", COLLECTION, MockResponse::json(200, created()));

    let output = run(
        &home,
        &server,
        &[
            "imports",
            "create",
            "Alpha",
            "NewRepo",
            "--url",
            "https://github.com/owner/repo.git",
            "--user",
            "octocat",
            "--password",
            "ghp_secret",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0])["parameters"]["gitSource"],
        json!({
            "url": "https://github.com/owner/repo.git",
            "user": "octocat",
            "password": "ghp_secret"
        })
    );
}

#[test]
fn create_sends_an_empty_url_when_the_option_is_present_and_empty() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", COLLECTION, MockResponse::json(200, created()));

    let output = run(
        &home,
        &server,
        &[
            "imports", "create", "Alpha", "NewRepo", "--url", "", "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        sent_body(&requests(&server)[0])["parameters"]["gitSource"],
        json!({"url": ""}),
        "a present empty option is not an absent one"
    );
}

#[test]
fn create_without_the_url_option_is_a_loud_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["imports", "create", "Alpha", "NewRepo", "--json"],
    );

    usage_error(&output, "url");
    assert!(
        requests(&server).is_empty(),
        "no request, unlike D34's silence"
    );
}

#[test]
fn create_requires_both_positionals() {
    let home = TempHome::new();
    let server = MockServer::start();

    usage_error(
        &run(
            &home,
            &server,
            &["imports", "create", "--url", "https://github.com/o/r.git"],
        ),
        "PROJECT",
    );
    usage_error(
        &run(
            &home,
            &server,
            &[
                "imports",
                "create",
                "Alpha",
                "--url",
                "https://github.com/o/r.git",
            ],
        ),
        "REPO_NAME",
    );
    assert!(requests(&server).is_empty(), "a usage error sends nothing");
}

#[test]
fn create_404_is_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        COLLECTION,
        MockResponse::json(404, json!({"message": "The project does not exist."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "imports",
            "create",
            "Alpha",
            "NewRepo",
            "--url",
            "https://github.com/owner/repo.git",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
}

#[test]
fn create_400_is_the_api_error_envelope_with_the_upstream_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    let body = json!({"message": "The repository 'NewRepo' already exists."});
    server.expect("POST", COLLECTION, MockResponse::json(400, body.clone()));

    let output = run(
        &home,
        &server,
        &[
            "imports",
            "create",
            "Alpha",
            "NewRepo",
            "--url",
            "https://github.com/owner/repo.git",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(400));
    assert_eq!(
        envelope["error"]["message"],
        json!("API error 400: {\"message\":\"The repository 'NewRepo' already exists.\"}"),
        "the upstream bytes in the message, where the oracle re-renders inspect/2 (D24)"
    );
    assert_eq!(
        envelope["error"]["details"]["body"],
        json!(body.to_string()),
        "and in details.body (D24)"
    );
}

#[test]
fn create_escapes_the_repo_name_as_one_segment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        "/myorg/Alpha/_apis/git/repositories/a%2Fb/importRequests",
        MockResponse::json(200, created()),
    );

    let output = run(
        &home,
        &server,
        &[
            "imports",
            "create",
            "Alpha",
            "a/b",
            "--url",
            "https://github.com/owner/repo.git",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].path,
        "/myorg/Alpha/_apis/git/repositories/a%2Fb/importRequests",
        "the oracle interpolates `a/b` raw, splitting the path (D22)"
    );
}

#[test]
fn the_group_without_a_subcommand_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    usage_error(&run(&home, &server, &["imports"]), "sub-command");
}
