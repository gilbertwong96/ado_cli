//! End-to-end tests for `ado pipelines-artifacts list|download`: the REST surface
//! the frozen `lib/ado_cli/cli/run_artifacts.ex` builds — every method, path and
//! query pair verified against the frozen escript — the `--json` envelope it emits,
//! the human table its formatter defines, and the byte download.
//!
//! The list envelope is pinned **byte-equal** to the captured oracle line: every map
//! in the fixture stays below Elixir's small-map threshold, so the term order the
//! oracle encodes is serde's sorted order, and the capture was confirmed
//! byte-identical to its `jq -S -c` form (W1-R12).
//!
//! `download` writes bytes, not a `Report`: it follows the artifact's
//! `resource.downloadUrl` and writes the body to `--output` (default
//! `./<artifact-name>.zip`), with the module's success line. The one deliberate
//! deviation is in the source URL — an absolute `downloadUrl` is requested verbatim,
//! because the frozen client prepends its base to it and produces a dead URL (D25).
//!
//! The hyphenated invocation (W1-1/D18) is this task's other distinguishing surface:
//! `pipelines-artifacts` is the parseable spelling and the schema reports it, while
//! the space spelling is an unknown subcommand that never reaches this command.
//!
//! Every test owns its environment: a `TempHome` for the config directory, a mock
//! server behind `ADO_SERVER`, and explicit `ADO_ORG`/`ADO_PAT` credentials so no
//! credential resolution reaches the developer's keychain.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;

use ado_testkit::{
    MockResponse, MockServer, RecordedRequest, TempHome, ado_cmd, stderr_of, stdout_of,
};
use serde_json::{Value, json};

const ORG: &str = "myorg";

/// The frozen escript's literal `ado pipelines-artifacts list Alpha 7 99 --json`
/// line for the `artifacts_list` fixture (captured from the 0.5.0 binary, W1-R12).
const ORACLE_LIST_JSON: &str = r##"{"ok":true,"result":[{"_links":{"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/7/runs/99/artifacts?artifactName=drop"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/results?buildId=99&view=artifacts"}},"id":1,"name":"drop","resource":{"data":"#/7/99/drop","downloadUrl":"https://dev.azure.com/myorg/Alpha/_apis/resources/Containers/1234567?itemPath=drop&%24format=zip","properties":{"artifactsize":"2048"},"type":"Container","url":"https://dev.azure.com/myorg/Alpha/_apis/resources/Containers/1234567?itemPath=drop"},"source":null},{"_links":{"self":{"href":"https://dev.azure.com/myorg/Alpha/_apis/pipelines/7/runs/99/artifacts?artifactName=TestResults"},"web":{"href":"https://dev.azure.com/myorg/Alpha/_build/results?buildId=99&view=artifacts"}},"id":2,"name":"TestResults","resource":{"data":"#/7/99/TestResults","downloadUrl":"https://dev.azure.com/myorg/Alpha/_apis/resources/Containers/1234568?itemPath=TestResults&%24format=zip","properties":{"artifactsize":"512"},"type":"Container","url":"https://dev.azure.com/myorg/Alpha/_apis/resources/Containers/1234568?itemPath=TestResults"},"source":null}]}"##;

const ARTIFACTS_PATH: &str = "/myorg/Alpha/_apis/pipelines/7/runs/99/artifacts";
/// The absolute `resource.downloadUrl` the mock serves the drop artifact from — the
/// path a verbatim request lands on.
const DOWNLOAD_PATH: &str = "/blob/drop.zip";
/// The same blob as a server-relative downloadUrl resolves it: org-injected, with
/// `api-version` merged in.
const RELATIVE_DOWNLOAD_PATH: &str = "/myorg/blob/drop.zip";

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

/// The same run with its working directory pinned, for the default `./<name>.zip`
/// destination.
fn run_in(home: &TempHome, server: &MockServer, cwd: &Path, args: &[&str]) -> Output {
    command(home, server, args)
        .current_dir(cwd)
        .output()
        .expect("run ado")
}

fn fixture(name: &str) -> Value {
    let response = MockResponse::from_fixture(name);

    serde_json::from_slice(&response.body).expect("the fixture is JSON")
}

/// The `artifacts_list` fixture with the `drop` artifact's `downloadUrl` replaced:
/// Azure sends an absolute URL there, and the relative form exercises the
/// path-based resolution the frozen client applies to every URL.
fn artifacts_with_download(download_url: &str) -> Value {
    let mut artifacts = fixture("artifacts_list");

    for artifact in artifacts["value"].as_array_mut().expect("a value array") {
        if artifact["name"] == json!("drop") {
            artifact["resource"]["downloadUrl"] = json!(download_url);
        }
    }

    artifacts
}

fn zip_bytes() -> Vec<u8> {
    MockResponse::from_bytes_fixture("artifacts_download.zip").body
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

/// The reader-level failure the truncating mock provokes: ureq's own
/// platform-independent constant (`UnexpectedEof`, "Peer disconnected"). The text
/// alone does not pin the path — ureq shares that constant with a pre-header close.
/// The mock does: `spawn_truncating_blob` writes the whole header block before it
/// drops the stream, so once `get_raw` returns, a reader failure is the only thing
/// that can fail.
const MID_BODY_FAILURE: &str = "[Network error] Request failed: io: Peer disconnected";

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

fn expect_artifacts(server: &MockServer, body: &Value) {
    server.expect("GET", ARTIFACTS_PATH, MockResponse::json(200, body.clone()));
}

fn expect_blob(server: &MockServer, path: &str) {
    server.expect(
        "GET",
        path,
        MockResponse::from_bytes_fixture("artifacts_download.zip"),
    );
}

fn assert_query(request: &RecordedRequest, path: &str, query: Vec<(String, String)>) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, path);
    assert_eq!(
        request.query_pairs(),
        query,
        "the GET's wire-form pairs, in the client's merge order (D12 compares parsed pairs)"
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
fn list_sends_the_artifacts_path_and_emits_the_oracle_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    let artifacts = fixture("artifacts_list");
    expect_artifacts(&server, &artifacts);

    let output = run(
        &home,
        &server,
        &["pipelines-artifacts", "list", "Alpha", "7", "99", "--json"],
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
        json!({"ok": true, "result": artifacts["value"].clone()}),
        "the value envelope carries the fixture's array under result"
    );
    assert!(
        envelope.get("count").is_none() && envelope.get("items").is_none(),
        "the count/items list form never ships (W1-R12): {stdout}"
    );

    let received = server.received();
    assert_eq!(received.len(), 1, "one request per list");
    assert_query(&received[0], ARTIFACTS_PATH, vec![api_version()]);
    assert_no_table_bytes(&stdout);
}

#[test]
fn list_encodes_the_project_in_the_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/My%20Project/_apis/pipelines/7/runs/99/artifacts",
        MockResponse::from_fixture("artifacts_list"),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "list",
            "My Project",
            "7",
            "99",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/My%20Project/_apis/pipelines/7/runs/99/artifacts",
        "a space is %20 in a path, not +"
    );
}

/// The stricter path encoder (D22) also escapes `/` and `?`, which Elixir leaves in
/// place (the `?` splits the URL into a query and the oracle's request lands on the
/// wrong path) while project names cannot contain them.
#[test]
fn list_encodes_path_separators_strictly() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/a%2Fb%3Fc/_apis/pipelines/7/runs/99/artifacts",
        MockResponse::from_fixture("artifacts_list"),
    );

    let output = run(
        &home,
        &server,
        &["pipelines-artifacts", "list", "a/b?c", "7", "99", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        server.received()[0].path,
        "/myorg/a%2Fb%3Fc/_apis/pipelines/7/runs/99/artifacts",
        "the stricter encoder escapes / and ? so a name cannot change the URL (D22)"
    );
}

#[test]
fn list_empty_answer_human_output_says_no_artifacts_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ARTIFACTS_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["pipelines-artifacts", "list", "Alpha", "7", "99"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No artifacts found.\n");
    assert_eq!(server.received().len(), 1);
}

#[test]
fn list_empty_answer_json_is_the_empty_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ARTIFACTS_PATH,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["pipelines-artifacts", "list", "Alpha", "7", "99", "--json"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "{\"ok\":true,\"result\":[]}\n");
    assert_eq!(server.received().len(), 1);
}

#[test]
fn list_json_emits_no_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_artifacts(&server, &fixture("artifacts_list"));

    let output = run(
        &home,
        &server,
        &["pipelines-artifacts", "list", "Alpha", "7", "99", "--json"],
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
        !stdout.lines().any(|line| line.starts_with("Name")),
        "a table header reached --json output: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

/// The module's `print_artifacts_table/1`: Name and Size, where Size is
/// `resource.size || "?"` — and real Azure artifact resources carry no `size` field,
/// so the cell is `?` for both artifacts. The renderer's own shape is regenerated
/// surface (spec D9), so the header and the cells are asserted, not the module's
/// padding or its count line.
#[test]
fn list_human_output_is_the_artifacts_table() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_artifacts(&server, &fixture("artifacts_list"));

    let output = run(
        &home,
        &server,
        &["pipelines-artifacts", "list", "Alpha", "7", "99"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert_eq!(
        lines.len(),
        4,
        "a header row, a rule row and one row per artifact: {stdout}"
    );
    assert!(lines[0].starts_with("Name"), "header row: {stdout}");
    let size_at = lines[0].find("Size").expect("the Size header");
    assert!(size_at > 0, "the header order is Name, Size: {stdout}");
    assert!(
        lines[1].chars().all(|c| c == '-' || c == ' '),
        "the rule row is plain ASCII: {stdout}"
    );
    assert!(
        lines[2].starts_with("drop") && lines[2].ends_with('?'),
        "the first row: {stdout}"
    );
    assert!(
        lines[3].starts_with("TestResults") && lines[3].ends_with('?'),
        "the second row: {stdout}"
    );
    assert_no_table_bytes(&stdout);
}

#[test]
fn list_without_a_run_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["pipelines-artifacts", "list", "Alpha", "7", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("RUN_ID"),
        "the usage error names the missing positional: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

/// The Elixir's `pipeline_id` and `run_id` are `:integer` positionals; a non-number
/// is a usage error on both sides (CliMate's `invalid argument … expected type
/// integer` vs clap's), and it never reaches the wire.
#[test]
fn list_with_a_non_numeric_pipeline_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "list",
            "Alpha",
            "abc",
            "99",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("PIPELINE_ID"),
        "the usage error names the invalid positional: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn download_writes_the_bytes_to_the_output_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("out.zip");
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("{}{DOWNLOAD_PATH}", server.base_url())),
    );
    expect_blob(&server, DOWNLOAD_PATH);

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            target.to_str().expect("a utf-8 path"),
        ],
    );

    assert_success(&output);
    let zip = zip_bytes();
    assert_eq!(
        fs::read(&target).expect("the artifact file"),
        zip,
        "the downloaded file is the artifact's bytes, byte for byte"
    );
    assert_eq!(
        stdout_of(&output),
        format!("Downloaded {} bytes to {}\n", zip.len(), target.display()),
        "the module's success line, on the human path"
    );

    let received = server.received();
    assert_eq!(received.len(), 2, "the list, then the blob");
    assert_query(&received[0], ARTIFACTS_PATH, vec![api_version()]);
}

#[test]
fn download_defaults_to_the_artifact_name_zip_in_the_current_directory() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("{}{DOWNLOAD_PATH}", server.base_url())),
    );
    expect_blob(&server, DOWNLOAD_PATH);

    let output = run_in(
        &home,
        &server,
        home.path(),
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
        ],
    );

    assert_success(&output);
    let zip = zip_bytes();
    assert_eq!(
        fs::read(home.path().join("drop.zip")).expect("the default file"),
        zip,
        "the default destination is ./<artifact-name>.zip"
    );
    assert_eq!(
        stdout_of(&output),
        format!("Downloaded {} bytes to drop.zip\n", zip.len())
    );
}

/// D25: Azure's `resource.downloadUrl` is absolute, and Rust requests it verbatim
/// with no added `api-version` — the URL carries its own signed query. The frozen
/// client prepends its base instead (`/myorg/https://…` on the wire), a dead URL on
/// a real organization.
#[test]
fn download_requests_an_absolute_download_url_verbatim() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("{}{DOWNLOAD_PATH}", server.base_url())),
    );
    expect_blob(&server, DOWNLOAD_PATH);

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            home.path()
                .join("verbatim.zip")
                .to_str()
                .expect("a utf-8 path"),
        ],
    );

    assert_success(&output);
    let received = server.received();
    assert_query(&received[1], DOWNLOAD_PATH, Vec::new());
    assert!(
        received[1].query_pairs().is_empty(),
        "an absolute downloadUrl is requested as given, with its own query and no api-version (D25): {}",
        received[1].query
    );
    assert_eq!(
        fs::read(home.path().join("verbatim.zip")).expect("the file"),
        zip_bytes()
    );
}

/// The other half of D25: a relative `downloadUrl` is resolved against the client's
/// base with the org injected and the version merged, exactly as the frozen client
/// does — captured `GET /myorg/blob/drop.zip?api-version=7.1`.
#[test]
fn download_resolves_a_relative_download_url_against_the_client_base() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_artifacts(&server, &artifacts_with_download(DOWNLOAD_PATH));
    expect_blob(&server, RELATIVE_DOWNLOAD_PATH);

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            home.path()
                .join("relative.zip")
                .to_str()
                .expect("a utf-8 path"),
        ],
    );

    assert_success(&output);
    assert_query(
        &server.received()[1],
        RELATIVE_DOWNLOAD_PATH,
        vec![api_version()],
    );
    assert_eq!(
        fs::read(home.path().join("relative.zip")).expect("the file"),
        zip_bytes()
    );
}

#[test]
fn download_without_the_artifact_reports_not_found() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_artifacts(&server, &fixture("artifacts_list"));

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "missing",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "the not-found message is not an envelope: {}",
        stdout_of(&output)
    );
    assert_eq!(
        stderr_of(&output),
        "[Not found] Artifact 'missing' not found in run #99\n"
    );
    assert_eq!(
        server.received().len(),
        1,
        "the download never starts for an artifact that is not in the list"
    );
}

#[test]
fn download_without_the_artifact_under_json_is_the_error_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_artifacts(&server, &fixture("artifacts_list"));

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "missing",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).expect("stdout is a JSON document"),
        json!({
            "ok": false,
            "error": {
                "code": "not_found",
                "message": "Artifact 'missing' not found in run #99",
            },
        })
    );
    assert!(
        server.received().len() == 1,
        "no blob request for a missing artifact"
    );
}

/// The help text promises case-sensitive names, mirroring `Enum.find(…, &(&1["name"]
/// == name))`: a different case is a different artifact.
#[test]
fn download_is_case_sensitive_on_the_artifact_name() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_artifacts(&server, &fixture("artifacts_list"));

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "DROP",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr_of(&output),
        "[Not found] Artifact 'DROP' not found in run #99\n"
    );
    assert_eq!(server.received().len(), 1, "no blob request");
}

/// The frozen `download_and_save/2` prints the same `Downloaded …` line under
/// `--json`; the artifact bytes are not an envelope and `--json` does not change
/// the human line.
#[test]
fn download_json_success_is_the_same_human_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("json.zip");
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("{}{DOWNLOAD_PATH}", server.base_url())),
    );
    expect_blob(&server, DOWNLOAD_PATH);

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            target.to_str().expect("a utf-8 path"),
            "--json",
        ],
    );

    assert_success(&output);
    let zip = zip_bytes();
    assert_eq!(
        stdout_of(&output),
        format!("Downloaded {} bytes to {}\n", zip.len(), target.display()),
        "no envelope wraps the success line"
    );
    assert_eq!(fs::read(&target).expect("the file"), zip);
}

/// D25's second sentence: a failed download classifies by its HTTP status (404 →
/// `not_found`), where the oracle's `get_raw/2` returns `%{status: 404}` with no
/// body, falls through its api-error clause and reports `[Network error] Request
/// failed: %{status: 404}`.
#[test]
fn download_a_failed_blob_classifies_by_status() {
    let home = TempHome::new();
    let server = MockServer::start();
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("{}{DOWNLOAD_PATH}", server.base_url())),
    );
    server.expect(
        "GET",
        DOWNLOAD_PATH,
        MockResponse::from_fixture("error_404").with_status(404),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_slice(&output.stdout).expect("stdout is a JSON document");
    assert_eq!(
        envelope["error"]["code"],
        json!("not_found"),
        "the status is the classification, not a network error: {envelope}"
    );
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Resource not found. Check the project/repo/build ID and your permissions.")
    );
    assert_eq!(server.received().len(), 2, "the list, then the failed blob");
}

#[test]
fn download_a_failed_artifacts_list_reports_the_api_error() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        ARTIFACTS_PATH,
        MockResponse::json(
            500,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope: Value =
        serde_json::from_slice(&output.stdout).expect("stdout is a JSON document");
    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(500));
    assert_eq!(
        envelope["error"]["message"],
        json!("Azure DevOps server error. Retry later.")
    );
    assert_eq!(server.received().len(), 1, "the blob never starts");
}

/// The oracle's `File.write!` raises and the frozen binary exits 1 with no output;
/// Rust answers with the usual command-level error presentation (D4) — plain stderr
/// without `--json` — and the artifact bytes are never written.
#[test]
fn download_to_an_unwritable_path_reports_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("no-such-dir").join("out.zip");
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("{}{DOWNLOAD_PATH}", server.base_url())),
    );
    expect_blob(&server, DOWNLOAD_PATH);

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
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
        stderr_of(&output).contains("Could not write the artifact to"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(!target.exists(), "no file at the unwritable path");
}

#[test]
fn download_without_the_artifact_name_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("ARTIFACT_NAME"),
        "the usage error names the missing positional: {}",
        stderr_of(&output)
    );
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn pipelines_artifacts_without_a_subcommand_is_a_validation_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["pipelines-artifacts"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no help output: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("missing sub-command"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(server.received().is_empty(), "no request");
}

/// W1-1/D18: the parseable spelling is the hyphenated one — it reaches the wire —
/// and the Elixir schema's space spelling does not run this command at all. The
/// oracle prints the `ado pipelines` group help on stdout plus `unknown sub-command
/// artifacts` on stderr, exit 1; the Rust binary answers with clap's usage error on
/// stderr, exit 1 (spec §8, usage-error presentation: D5). Both shapes mean the same
/// contract: the space spelling is not the artifacts command.
#[test]
fn the_hyphenated_spelling_runs_and_the_space_spelling_does_not() {
    let home = TempHome::new();

    let hyphenated = MockServer::start();
    expect_artifacts(&hyphenated, &fixture("artifacts_list"));
    let output = run(
        &home,
        &hyphenated,
        &["pipelines-artifacts", "list", "Alpha", "7", "99", "--json"],
    );
    assert_success(&output);
    assert_eq!(stdout_of(&output), format!("{ORACLE_LIST_JSON}\n"));
    assert_eq!(
        hyphenated.received().len(),
        1,
        "the hyphenated spelling is the command that reaches the wire"
    );

    let spaced = MockServer::start();
    let output = run(
        &home,
        &spaced,
        &[
            "pipelines",
            "artifacts",
            "list",
            "Alpha",
            "7",
            "99",
            "--json",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "the space spelling is not the artifacts command"
    );
    assert!(
        stdout_of(&output).is_empty(),
        "no envelope for the unknown subcommand: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("'artifacts'"),
        "the usage error names the subcommand the space spelling tried to run: {}",
        stderr_of(&output)
    );
    assert!(
        spaced.received().is_empty(),
        "the space spelling sends no request"
    );
}

/// A closed stdout is a silent success: the success line's write returns EPIPE and
/// the binary exits 0 with no diagnostic (spec §6.4, R19). The artifact file is
/// written before the line, so it is on disk either way.
#[test]
fn a_closed_stdout_is_a_silent_success() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("epipe.zip");
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("{}{DOWNLOAD_PATH}", server.base_url())),
    );
    expect_blob(&server, DOWNLOAD_PATH);

    let mut child = command(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            target.to_str().expect("a utf-8 path"),
        ],
    )
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("spawn ado");
    drop(child.stdout.take());

    let output = child.wait_with_output().expect("wait for ado");

    assert_eq!(server.received().len(), 2, "the list and the blob went out");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(output.stderr.is_empty(), "stderr: {}", stderr_of(&output));
    assert_eq!(
        fs::read(&target).expect("the artifact file"),
        zip_bytes(),
        "the file is written even though the success line could not be"
    );
}

/// Eleven MiB of deterministic pseudo-random bytes built here, never a committed
/// fixture: bigger than the 10 MiB buffering cap `read_to_vec` would impose, so
/// the download only succeeds if it streams the body without a limit.
fn pseudo_random_bytes(len: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;

    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 33) as u8
        })
        .collect()
}

fn sha256(bytes: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);

    hasher.finalize().to_vec()
}

/// A one-shot server that declares a larger `Content-Length` than it sends and
/// then closes: the connection drops after the headers, mid-body.
fn spawn_truncating_blob(prefix: Vec<u8>, declared_len: usize) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a truncating server");
    let port = listener.local_addr().expect("the bound address").port();

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut head = [0u8; 1024];
            let _ = stream.read(&mut head);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ncontent-length: {declared_len}\r\n\r\n"
            );
            let _ = stream.write_all(&prefix);
            let _ = stream.flush();
        }
    });

    port
}

/// A download is not buffered, so an artifact larger than 10 MiB lands whole: the
/// written file's length and sha256 are the served body's. The frozen CLI reads the
/// body unbounded too, so this is parity, not a deviation.
#[test]
fn download_streams_a_body_larger_than_the_buffering_cap() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("large.zip");
    let large = pseudo_random_bytes(11 * 1024 * 1024 + 7);
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("{}{DOWNLOAD_PATH}", server.base_url())),
    );
    server.expect(
        "GET",
        DOWNLOAD_PATH,
        MockResponse::bytes(200, large.clone()),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            target.to_str().expect("a utf-8 path"),
        ],
    );

    assert_success(&output);
    let written = fs::read(&target).expect("the artifact file");
    assert_eq!(
        written.len(),
        large.len(),
        "the whole body, not the first 10 MiB"
    );
    assert_eq!(
        sha256(&written),
        sha256(&large),
        "the bytes on disk are the bytes the server sent"
    );
    assert_eq!(
        stdout_of(&output),
        format!("Downloaded {} bytes to {}\n", large.len(), target.display())
    );
}

/// A connection dropped after the headers is a classified transport failure, and
/// nothing appears at the target — the body streams to a sibling temp file that is
/// removed, so there is never a partial artifact to mistake for a real one.
#[test]
fn download_leaves_no_file_when_the_stream_breaks() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("partial.zip");
    let port = spawn_truncating_blob(b"PK\x03\x04partial".to_vec(), 4096);
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("http://127.0.0.1:{port}/blob/drop.zip")),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            target.to_str().expect("a utf-8 path"),
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
/// to a sibling temp file, so the target keeps its old bytes byte for byte, exactly
/// as the module's read-then-`File.write!/2` order does.
#[test]
fn download_keeps_a_pre_existing_file_when_the_stream_breaks() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("existing.zip");
    fs::write(&target, b"old artifact bytes").expect("seed the target");
    let port = spawn_truncating_blob(b"PK\x03\x04partial".to_vec(), 4096);
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("http://127.0.0.1:{port}/blob/drop.zip")),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            target.to_str().expect("a utf-8 path"),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "the download failed");
    assert!(
        stderr_of(&output).contains(MID_BODY_FAILURE),
        "the mid-body reader error is classified: {}",
        stderr_of(&output)
    );
    assert_eq!(
        fs::read(&target).expect("the pre-existing file"),
        b"old artifact bytes",
        "an existing target survives a failed download byte for byte"
    );
    assert!(
        temp_siblings(&home).is_empty(),
        "no temp sibling is left behind: {:?}",
        temp_siblings(&home)
    );
}

/// The temp file's name carries a unique infix, so a pre-existing `{target}.tmp` —
/// a name nobody ever asked us to touch — survives a failed download byte for byte,
/// and the only `.tmp` left in the directory is that bystander.
#[test]
fn download_keeps_a_pre_existing_tmp_file_when_the_stream_breaks() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("existing.zip");
    let bystander = home.path().join("existing.zip.tmp");
    fs::write(&bystander, b"someone else's temp").expect("seed the bystander");
    let port = spawn_truncating_blob(b"PK\x03\x04partial".to_vec(), 4096);
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("http://127.0.0.1:{port}/blob/drop.zip")),
    );

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            target.to_str().expect("a utf-8 path"),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "the download failed");
    assert!(
        stderr_of(&output).contains(MID_BODY_FAILURE),
        "the mid-body reader error is classified: {}",
        stderr_of(&output)
    );
    assert_eq!(
        fs::read(&bystander).expect("the pre-existing temp"),
        b"someone else's temp",
        "a pre-existing `<target>.tmp` is not our temp and must survive"
    );
    assert_eq!(
        temp_siblings(&home),
        vec!["existing.zip.tmp".to_owned()],
        "our own temp was removed and only the bystander remains"
    );
    assert!(!target.exists(), "the target never appears");
}

/// A successful stream replaces a pre-existing target with the new bytes, through
/// the temp file's rename, and leaves no temp sibling.
#[test]
fn download_replaces_an_existing_file_on_success() {
    let home = TempHome::new();
    let server = MockServer::start();
    let target = home.path().join("existing.zip");
    fs::write(&target, b"stale artifact bytes").expect("seed the target");
    expect_artifacts(
        &server,
        &artifacts_with_download(&format!("{}{DOWNLOAD_PATH}", server.base_url())),
    );
    expect_blob(&server, DOWNLOAD_PATH);

    let output = run(
        &home,
        &server,
        &[
            "pipelines-artifacts",
            "download",
            "Alpha",
            "7",
            "99",
            "drop",
            "--output",
            target.to_str().expect("a utf-8 path"),
        ],
    );

    assert_success(&output);
    assert_eq!(
        fs::read(&target).expect("the replaced file"),
        zip_bytes(),
        "the download replaces the old bytes"
    );
    assert!(
        temp_siblings(&home).is_empty(),
        "no temp sibling is left behind: {:?}",
        temp_siblings(&home)
    );
}
