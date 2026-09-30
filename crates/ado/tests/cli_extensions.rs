//! End-to-end tests for `ado extensions list|show|install|uninstall|enable|
//! disable`: the organization-scoped `_apis/extensionmanagement/installedextensions`
//! surface the frozen `lib/ado_cli/cli/extensions.ex` builds, the `--search`
//! client-side filter, the four writes' bodies, the human views and the error
//! paths.
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
const EXTENSIONS: &str = "/myorg/_apis/extensionmanagement/installedextensions";
const BUILD_QUALITY_CHECKS: &str =
    "/myorg/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks";

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

/// The installedextensions body of the capture (`extensions_list.json`): three
/// rows, one disabled, one version longer than the oracle's ten-character
/// display slice.
fn extensions() -> Value {
    json!({"count": 3, "value": [
        {"extensionId": "mspremier-bqc", "extensionName": "BuildQualityChecks",
         "publisherId": "mspremier", "version": "3.1.20240912.7",
         "installState": {"flags": "none"}},
        {"extensionName": "octopus-deploy", "publisherId": "octopus",
         "version": "1.2.3", "installState": {"flags": "disabled"}},
        {"extensionName": "SonarQube", "publisherId": "sonarsource",
         "version": "2.0.0", "installState": {"flags": "none"}}
    ]})
}

/// The captured `show` body (`extension_show.json`).
fn extension() -> Value {
    extensions()["value"][0].clone()
}

fn usage_error(output: &Output, option: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(output).is_empty(),
        "no envelope for a usage error: {}",
        stdout_of(output)
    );
    assert!(
        stderr_of(output).contains(option),
        "the usage error names {option}: {}",
        stderr_of(output)
    );
}

// ── list ────────────────────────────────────────────────────────────────

#[test]
fn list_emits_the_value_envelope_and_the_org_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", EXTENSIONS, MockResponse::json(200, extensions()));

    let output = run(&home, &server, &["extensions", "list", "--json"]);

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": extensions()["value"]}),
        "the value array under the value envelope"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, EXTENSIONS);
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "the list sends no filter pair"
    );
}

#[test]
fn list_renders_the_module_columns_and_the_full_version() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", EXTENSIONS, MockResponse::json(200, extensions()));

    let output = run(&home, &server, &["extensions", "list"]);

    assert_success(&output);
    let stdout = stdout_of(&output);
    let lines = stdout.lines().collect::<Vec<_>>();

    assert!(
        lines[0].starts_with("Publisher.Name") && lines[0].contains("Version"),
        "header line: {stdout}"
    );
    assert!(lines[0].contains("State"), "header line: {stdout}");
    assert!(
        lines[1].contains("---") && lines[1].chars().all(|cell| cell == '-' || cell == ' '),
        "the rule line: {stdout}"
    );
    assert!(
        lines[2].contains("mspremier.BuildQualityChecks")
            && lines[2].contains("3.1.20240912.7")
            && lines[2].contains("enabled"),
        "the first row keeps the id, the full version and the state: {stdout}"
    );
    assert!(
        lines[3].contains("octopus.octopus-deploy") && lines[3].contains("disabled"),
        "the second row: {stdout}"
    );
    assert!(
        !stdout.contains("extension(s)"),
        "this build's table style carries no count line (§8): {stdout}"
    );
}

#[test]
fn list_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/empty-org/_apis/extensionmanagement/installedextensions",
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = command(&home, &server, "empty-org", &["extensions", "list"])
        .output()
        .expect("run ado");

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No extensions found.\n");
}

#[test]
fn list_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/missing/_apis/extensionmanagement/installedextensions",
        MockResponse::json(
            404,
            json!({"message": "TF400813: The user is not authorized."}),
        ),
    );

    let output = command(&home, &server, "missing", &["extensions", "list", "--json"])
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
        "/broken/_apis/extensionmanagement/installedextensions",
        MockResponse::json(
            500,
            json!({"message": "TF400813: The server is unavailable."}),
        ),
    );

    let output = command(&home, &server, "broken", &["extensions", "list", "--json"])
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
fn list_404_human_writes_the_labelled_line_to_stderr() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/missing/_apis/extensionmanagement/installedextensions",
        MockResponse::json(404, json!({"message": "The organization does not exist."})),
    );

    let output = command(&home, &server, "missing", &["extensions", "list"])
        .output()
        .expect("run ado");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no document on stdout: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("[Not found]"),
        "the classified human line on stderr (D4's unification): {}",
        stderr_of(&output)
    );
}

// ── the `--search` filter ───────────────────────────────────────────────

#[test]
fn search_filters_the_extension_name_case_insensitively_and_sends_no_query() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", EXTENSIONS, MockResponse::json(200, extensions()));
    server.expect("GET", EXTENSIONS, MockResponse::json(200, extensions()));

    let upper = run(
        &home,
        &server,
        &["extensions", "list", "--search", "Build", "--json"],
    );
    let lower = run(
        &home,
        &server,
        &["extensions", "list", "--search", "build", "--json"],
    );

    assert_success(&upper);
    assert_success(&lower);
    let one = json!([extensions()["value"][0].clone()]);

    assert_eq!(envelope(&upper), json!({"ok": true, "result": one}));
    assert_eq!(
        envelope(&lower),
        json!({"ok": true, "result": one}),
        "the match is case-insensitive"
    );

    for request in requests(&server) {
        assert_eq!(
            request.query_pairs(),
            vec![("api-version".to_owned(), "7.1".to_owned())],
            "the filter is client-side; nothing extra reaches the wire"
        );
    }
}

#[test]
fn search_reads_only_the_extension_name_and_an_empty_value_keeps_everything() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("GET", EXTENSIONS, MockResponse::json(200, extensions()));
    server.expect("GET", EXTENSIONS, MockResponse::json(200, extensions()));

    let publisher = run(
        &home,
        &server,
        &["extensions", "list", "--search", "mspremier", "--json"],
    );
    let empty = run(
        &home,
        &server,
        &["extensions", "list", "--search", "", "--json"],
    );

    assert_success(&publisher);
    assert_success(&empty);
    assert_eq!(
        envelope(&publisher),
        json!({"ok": true, "result": []}),
        "the publisher is not searched, only extensionName"
    );
    assert_eq!(
        envelope(&empty),
        json!({"ok": true, "result": extensions()["value"]}),
        "an empty --search is present and matches everything"
    );
}

// ── show ────────────────────────────────────────────────────────────────

#[test]
fn show_emits_the_value_envelope_and_the_extension_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BUILD_QUALITY_CHECKS,
        MockResponse::json(200, extension()),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "show",
            "mspremier.BuildQualityChecks",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": extension()})
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, BUILD_QUALITY_CHECKS);
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())]
    );
}

#[test]
fn show_renders_the_module_detail_layout() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        BUILD_QUALITY_CHECKS,
        MockResponse::json(200, extension()),
    );

    let output = run(
        &home,
        &server,
        &["extensions", "show", "mspremier.BuildQualityChecks"],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            concat!(
                "\nExtension Details\n\n",
                "{}\n",
                "  Publisher: mspremier\n",
                "  Name:      BuildQualityChecks\n",
                "  Version:   3.1.20240912.7\n",
                "  State:     enabled\n",
                "\n",
            ),
            "─".repeat(60)
        ),
        "the module's detail layout with a full version (its ten-character slice is \
         its own rendering, §8)"
    );
}

#[test]
fn show_encodes_the_id_as_one_path_segment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/extensionmanagement/installedextensions/pub%2Fname",
        MockResponse::json(200, json!({"extensionName": "name", "publisherId": "pub"})),
    );

    let output = run(
        &home,
        &server,
        &["extensions", "show", "pub/name", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].path,
        "/myorg/_apis/extensionmanagement/installedextensions/pub%2Fname",
        "the frozen URI.encode/1 leaves the slash raw; this build escapes the \
         segment strictly (D22)"
    );
}

#[test]
fn show_404_uses_the_modules_wording_in_the_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/_apis/extensionmanagement/installedextensions/missing.thing",
        MockResponse::json(404, json!({"message": "Extension not found."})),
    );

    let output = run(
        &home,
        &server,
        &["extensions", "show", "missing.thing", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(envelope["error"]["status"], json!(404));
    assert_eq!(
        envelope["error"]["message"],
        json!("Extension 'missing.thing' not found"),
        "the module's own wording; the frozen halt_error writes it to stderr with \
         no envelope under --json, this build's one error path envelopes it (D4)"
    );
    assert!(
        stderr_of(&output).is_empty(),
        "under --json the envelope is the whole answer: {}",
        stderr_of(&output)
    );
}

// ── install ─────────────────────────────────────────────────────────────

#[test]
fn install_posts_the_modules_body_and_emits_the_message_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", EXTENSIONS, MockResponse::json(200, extension()));

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "install",
            "--publisher",
            "mspremier",
            "--name",
            "BuildQualityChecks",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Extension 'mspremier.BuildQualityChecks' installed."}),
        "the frozen write prints its human line under --json; this build's message envelope (D33)"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, EXTENSIONS);
    assert_eq!(
        requests[0].body.as_deref(),
        Some(r#"{"extensionName":"BuildQualityChecks","publisherId":"mspremier"}"#),
        "the module's two-field body"
    );
}

#[test]
fn install_human_output_is_the_success_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect("POST", EXTENSIONS, MockResponse::json(200, extension()));

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "install",
            "--publisher",
            "mspremier",
            "--name",
            "BuildQualityChecks",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Extension 'mspremier.BuildQualityChecks' installed.\n"
    );
}

#[test]
fn install_400_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "POST",
        EXTENSIONS,
        MockResponse::json(
            400,
            json!({"message": "Extension 'mspremier.BuildQualityChecks' is not available in the marketplace."}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "install",
            "--publisher",
            "mspremier",
            "--name",
            "BuildQualityChecks",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("api_error"));
    assert_eq!(envelope["error"]["status"], json!(400));
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .starts_with("API error 400:"),
        "the classified 400 wording: {envelope}"
    );
}

#[test]
fn install_without_a_publisher_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "install",
            "--name",
            "BuildQualityChecks",
            "--json",
        ],
    );

    usage_error(&output, "--publisher");
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn install_without_a_name_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "install",
            "--publisher",
            "mspremier",
            "--json",
        ],
    );

    usage_error(&output, "--name");
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

// ── uninstall ───────────────────────────────────────────────────────────

#[test]
fn uninstall_deletes_the_dotted_path_and_emits_the_message_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        BUILD_QUALITY_CHECKS,
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "uninstall",
            "--publisher",
            "mspremier",
            "--name",
            "BuildQualityChecks",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Extension 'mspremier.BuildQualityChecks' uninstalled."})
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].path, BUILD_QUALITY_CHECKS);
}

#[test]
fn uninstall_human_output_is_the_success_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        BUILD_QUALITY_CHECKS,
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "uninstall",
            "--publisher",
            "mspremier",
            "--name",
            "BuildQualityChecks",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Extension 'mspremier.BuildQualityChecks' uninstalled.\n"
    );
}

#[test]
fn uninstall_404_uses_the_modules_wording_in_the_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        "/myorg/_apis/extensionmanagement/installedextensions/missing.thing",
        MockResponse::json(404, json!({"message": "Extension not found."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "uninstall",
            "--publisher",
            "missing",
            "--name",
            "thing",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Extension 'missing.thing' not found"),
        "the module's own 404 wording (D4's halt_error class)"
    );
}

#[test]
fn uninstall_404_human_writes_the_modules_wording_to_stderr() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "DELETE",
        "/myorg/_apis/extensionmanagement/installedextensions/missing.thing",
        MockResponse::json(404, json!({"message": "Extension not found."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "uninstall",
            "--publisher",
            "missing",
            "--name",
            "thing",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no document on stdout: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("Extension 'missing.thing' not found"),
        "the module's own wording: {}",
        stderr_of(&output)
    );
}

#[test]
fn uninstall_without_a_publisher_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "uninstall",
            "--name",
            "BuildQualityChecks",
            "--json",
        ],
    );

    usage_error(&output, "--publisher");
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn uninstall_without_a_name_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "uninstall",
            "--publisher",
            "mspremier",
            "--json",
        ],
    );

    usage_error(&output, "--name");
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

// ── enable ──────────────────────────────────────────────────────────────

#[test]
fn enable_patches_the_none_flags_body_and_emits_the_message_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        BUILD_QUALITY_CHECKS,
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "enable",
            "--publisher",
            "mspremier",
            "--name",
            "BuildQualityChecks",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Extension 'mspremier.BuildQualityChecks' enabled."})
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "PATCH");
    assert_eq!(requests[0].path, BUILD_QUALITY_CHECKS);
    assert_eq!(
        requests[0].body.as_deref(),
        Some(
            r#"{"extensionName":"BuildQualityChecks","installState":{"flags":"none"},"publisherId":"mspremier"}"#
        ),
        "the module's installState body"
    );
}

#[test]
fn enable_human_output_is_the_success_line() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        BUILD_QUALITY_CHECKS,
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "enable",
            "--publisher",
            "mspremier",
            "--name",
            "BuildQualityChecks",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Extension 'mspremier.BuildQualityChecks' enabled.\n"
    );
}

#[test]
fn enable_404_emits_the_classified_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        "/myorg/_apis/extensionmanagement/installedextensions/missing.thing",
        MockResponse::json(404, json!({"message": "Extension not found."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "enable",
            "--publisher",
            "missing",
            "--name",
            "thing",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    let envelope = envelope(&output);

    assert_eq!(envelope["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope["error"]["message"],
        json!("Resource not found. Check the project/repo/build ID and your permissions."),
        "enable has no module 404 wording; the classified envelope stands"
    );
}

#[test]
fn enable_without_a_publisher_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "enable",
            "--name",
            "BuildQualityChecks",
            "--json",
        ],
    );

    usage_error(&output, "--publisher");
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn enable_without_a_name_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["extensions", "enable", "--publisher", "mspremier", "--json"],
    );

    usage_error(&output, "--name");
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

// ── disable ─────────────────────────────────────────────────────────────

#[test]
fn disable_patches_the_disabled_flags_body_and_emits_the_message_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        BUILD_QUALITY_CHECKS,
        MockResponse::json(200, json!({})),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "disable",
            "--publisher",
            "mspremier",
            "--name",
            "BuildQualityChecks",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "message": "Extension 'mspremier.BuildQualityChecks' disabled."})
    );
    assert_eq!(
        requests(&server)[0].body.as_deref(),
        Some(
            r#"{"extensionName":"BuildQualityChecks","installState":{"flags":"disabled"},"publisherId":"mspremier"}"#
        )
    );
}

#[test]
fn disable_404_human_writes_the_labelled_line_to_stderr() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PATCH",
        "/myorg/_apis/extensionmanagement/installedextensions/missing.thing",
        MockResponse::json(404, json!({"message": "Extension not found."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "disable",
            "--publisher",
            "missing",
            "--name",
            "thing",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout_of(&output).is_empty(),
        "no document on stdout: {}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("[Not found]"),
        "the classified human line on stderr (D4's unification): {}",
        stderr_of(&output)
    );
}

#[test]
fn disable_without_a_publisher_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "disable",
            "--name",
            "BuildQualityChecks",
            "--json",
        ],
    );

    usage_error(&output, "--publisher");
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

#[test]
fn disable_without_a_name_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &[
            "extensions",
            "disable",
            "--publisher",
            "mspremier",
            "--json",
        ],
    );

    usage_error(&output, "--name");
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}

// ── the group and the positional ────────────────────────────────────────

#[test]
fn show_without_the_extension_id_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["extensions", "show", "--json"]);

    usage_error(&output, "EXTENSION_ID");
    assert!(
        server.received().is_empty(),
        "a usage error sends no request"
    );
}
