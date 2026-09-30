//! End-to-end tests for `ado packages list|versions|show`: the REST surface the
//! frozen `lib/ado_cli/cli/packages.ex` builds under `_apis/packaging/feeds`,
//! its `--json` envelopes, the two human tables and the detail view, and the
//! guard/error paths.
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
const PACKAGES: &str = "/myorg/Alpha/_apis/packaging/feeds/feed-1/packages";

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
    command(home, server, args).output().expect("run ado")
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

fn packages() -> Value {
    json!([
        {
            "id": "pkg-uuid-1",
            "name": "myapp-builds",
            "protocolType": "UPack",
            "url": "https://dev.azure.com/myorg/_apis/packaging/feeds/feed-1/packages/pkg-uuid-1",
            "versions": [{"version": "2.0.0"}, {"version": "1.0.0"}, {"version": "0.9.0-beta"}]
        },
        {
            "id": "pkg-uuid-2",
            "name": "other-pkg",
            "protocolType": "UPack",
            "versions": []
        }
    ])
}

fn versions() -> Value {
    json!([
        {"version": "2.0.0", "isLatest": true, "isDeleted": false, "publishDate": "2026-09-01T10:00:00.000Z"},
        {"version": "1.0.0", "isLatest": false, "isDeleted": false, "publishDate": "2026-08-01T10:00:00.000Z"},
        {"version": "0.9.0-beta", "isLatest": false, "isDeleted": true, "publishDate": "2026-07-01T10:00:00.000Z"}
    ])
}

fn package_detail() -> Value {
    json!({
        "id": "pkg-uuid-1",
        "name": "myapp-builds",
        "version": "1.0.0",
        "protocolType": "UPack",
        "isLatest": false,
        "size": 2048,
        "publishDate": "2026-08-01T10:00:00.000Z"
    })
}

#[test]
fn list_emits_the_value_envelope_and_sends_the_upack_query() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        PACKAGES,
        MockResponse::json(200, json!({"count": 2, "value": packages()})),
    );

    let output = run(
        &home,
        &server,
        &["packages", "list", "Alpha", "feed-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": packages()}),
        "the frozen `json_or_format` value envelope: the unwrapped value array"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, PACKAGES);
    assert_eq!(
        requests[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("protocolType".to_owned(), "UPack".to_owned())
        ],
        "the list is the one packages command with a query"
    );
}

#[test]
fn list_human_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        PACKAGES,
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["packages", "list", "Alpha", "feed-1"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No packages found.\n");
}

#[test]
fn list_is_a_usage_error_without_a_feed_id() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["packages", "list", "Alpha"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("FEED_ID"),
        "the usage error names the missing positional: {}",
        stderr_of(&output)
    );
    assert!(stdout_of(&output).is_empty(), "no stdout on a usage error");
    assert!(requests(&server).is_empty(), "nothing was sent");
}

#[test]
fn list_404_and_500_keep_the_classified_envelope() {
    for (project, status) in [("Missing", 404), ("Broken", 500)] {
        let home = TempHome::new();
        let server = MockServer::start();
        let path = format!("/myorg/{project}/_apis/packaging/feeds/feed-1/packages");
        server.expect(
            "GET",
            &path,
            MockResponse::json(status, json!({"message": "TF400813: nope."})),
        );

        let output = run(
            &home,
            &server,
            &["packages", "list", project, "feed-1", "--json"],
        );

        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            envelope(&output)["error"]["code"],
            json!(if status == 404 {
                "not_found"
            } else {
                "api_error"
            }),
            "status {status}"
        );
        assert_eq!(envelope(&output)["error"]["status"], json!(status));
    }
}

#[test]
fn versions_emits_the_value_envelope_with_every_status_label() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{PACKAGES}/myapp-builds/versions"),
        MockResponse::json(200, json!({"count": 3, "value": versions()})),
    );

    let output = run(
        &home,
        &server,
        &[
            "packages",
            "versions",
            "Alpha",
            "feed-1",
            "myapp-builds",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": versions()}));

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].path,
        format!("{PACKAGES}/myapp-builds/versions")
    );
    assert_eq!(
        requests[0].query_pairs(),
        vec![("api-version".to_owned(), "7.1".to_owned())],
        "versions sends no protocol filter"
    );
}

#[test]
fn versions_encodes_a_plus_in_the_package_name_as_one_path_segment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{PACKAGES}/name%2Bplus/versions"),
        MockResponse::json(200, json!({"count": 3, "value": versions()})),
    );

    let output = run(
        &home,
        &server,
        &[
            "packages",
            "versions",
            "Alpha",
            "feed-1",
            "name+plus",
            "--json",
        ],
    );

    assert_eq!(
        requests(&server)[0].path,
        format!("{PACKAGES}/name%2Bplus/versions"),
        "the frozen URI.encode leaves + raw; this build escapes it (D22)"
    );
    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": versions()}));
}

#[test]
fn versions_human_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{PACKAGES}/empty-pkg/versions"),
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(
        &home,
        &server,
        &["packages", "versions", "Alpha", "feed-1", "empty-pkg"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No versions found.\n");
}

#[test]
fn versions_is_a_usage_error_without_a_package_name() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["packages", "versions", "Alpha", "feed-1"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("PACKAGE_NAME"));
    assert!(requests(&server).is_empty());
}

#[test]
fn show_emits_the_value_envelope_for_the_exact_version() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{PACKAGES}/myapp-builds/versions/1.0.0"),
        MockResponse::json(200, package_detail()),
    );

    let output = run(
        &home,
        &server,
        &[
            "packages",
            "show",
            "Alpha",
            "feed-1",
            "myapp-builds",
            "1.0.0",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": package_detail()})
    );
    assert_eq!(
        requests(&server)[0].path,
        format!("{PACKAGES}/myapp-builds/versions/1.0.0")
    );
}

#[test]
fn show_encodes_the_version_as_one_path_segment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{PACKAGES}/myapp-builds/versions/1.0.0%2Bbuild.5"),
        MockResponse::json(200, package_detail()),
    );

    let output = run(
        &home,
        &server,
        &[
            "packages",
            "show",
            "Alpha",
            "feed-1",
            "myapp-builds",
            "1.0.0+build.5",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].path,
        format!("{PACKAGES}/myapp-builds/versions/1.0.0%2Bbuild.5"),
        "the frozen URI.encode leaves + raw; this build escapes it (D22)"
    );
}

#[test]
fn show_human_prints_the_module_detail_once() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{PACKAGES}/myapp-builds/versions/1.0.0"),
        MockResponse::json(200, package_detail()),
    );

    let output = run(
        &home,
        &server,
        &[
            "packages",
            "show",
            "Alpha",
            "feed-1",
            "myapp-builds",
            "1.0.0",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "\nPackage Details\n\n{}\n  Name:     myapp-builds\n  Version:  1.0.0\n  Protocol: UPack\n  Status:   normal\n  Size:     2048\n  Published:2026-08-01T10:00:00.000Z\n\n",
            "-".repeat(60)
        ),
        "the module's detail, with its `Published:` spelling and its missing size fallback"
    );
}

#[test]
fn show_404_keeps_the_module_wording_in_the_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{PACKAGES}/myapp-builds/versions/9.9.9"),
        MockResponse::json(404, json!({"message": "TF400813: nope."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "packages",
            "show",
            "Alpha",
            "feed-1",
            "myapp-builds",
            "9.9.9",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output),
        json!({
            "ok": false,
            "error": {
                "code": "not_found",
                "status": 404,
                "message": "Package 'myapp-builds@9.9.9' not found",
                "details": {
                    "status": 404,
                    "body": "{\"message\":\"TF400813: nope.\"}"
                }
            }
        }),
        "the module's `halt_error` wording rides the classified envelope (D4)"
    );
}

#[test]
fn show_human_404_keeps_the_module_wording_on_stderr() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{PACKAGES}/myapp-builds/versions/9.9.9"),
        MockResponse::json(404, json!({"message": "TF400813: nope."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "packages",
            "show",
            "Alpha",
            "feed-1",
            "myapp-builds",
            "9.9.9",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_of(&output).is_empty());
    assert!(
        stderr_of(&output).contains("Package 'myapp-builds@9.9.9' not found"),
        "stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn show_is_a_usage_error_without_a_version() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["packages", "show", "Alpha", "feed-1", "myapp-builds"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr_of(&output).contains("PACKAGE_VERSION"));
    assert!(requests(&server).is_empty());
}
