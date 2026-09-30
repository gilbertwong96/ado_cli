//! End-to-end tests for `ado wikis list|show|pages list|show|create|update`: the
//! REST surface the frozen `lib/ado_cli/cli/wikis.ex` builds under
//! `_apis/wiki/wikis`, the `--json` envelopes, the human views, and the
//! guard/error paths — including the frozen `Map.fetch!` crash the two page
//! writes and `pages show` reach when a required option is missing (D34).
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
const WIKIS: &str = "/myorg/Alpha/_apis/wiki/wikis";

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

fn wikis() -> Value {
    json!([
        {
            "id": "wiki-1",
            "name": "Alpha Wiki",
            "type": "codeWiki",
            "remoteUrl": "https://dev.azure.com/myorg/Alpha/_git/Alpha.wiki",
            "url": "https://dev.azure.com/myorg/Alpha/_apis/wiki/wikis/wiki-1"
        },
        {"id": "wiki-2", "name": "Alpha Project Wiki", "type": "projectWiki"}
    ])
}

fn wiki_show() -> Value {
    json!({
        "id": "wiki-1",
        "name": "Alpha Wiki",
        "type": "codeWiki",
        "remoteUrl": "https://dev.azure.com/myorg/Alpha/_git/Alpha.wiki",
        "url": "https://dev.azure.com/myorg/Alpha/_apis/wiki/wikis/wiki-1"
    })
}

fn pages() -> Value {
    json!({
        "path": "/",
        "subPages": [
            {"path": "/Home"},
            {"path": "/Design"},
            {"path": "/Design/Architecture"},
            {}
        ]
    })
}

fn page_home() -> Value {
    json!({
        "id": 42,
        "path": "/Home",
        "content": "# Home\n\nWelcome to the wiki.",
        "eTag": "abc123",
        "url": "https://dev.azure.com/myorg/Alpha/_apis/wiki/wikis/wiki-1/pages/42"
    })
}

#[test]
fn list_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        WIKIS,
        MockResponse::json(200, json!({"count": 2, "value": wikis()})),
    );

    let output = run(&home, &server, &["wikis", "list", "Alpha", "--json"]);

    assert_success(&output);
    assert_eq!(envelope(&output), json!({"ok": true, "result": wikis()}));
    assert_eq!(requests(&server)[0].path, WIKIS);
}

#[test]
fn list_human_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        "/myorg/Empty/_apis/wiki/wikis",
        MockResponse::json(200, json!({"count": 0, "value": []})),
    );

    let output = run(&home, &server, &["wikis", "list", "Empty"]);

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No wikis found.\n");
}

#[test]
fn list_404_and_500_keep_the_classified_envelope() {
    for (project, status) in [("Missing", 404), ("Broken", 500)] {
        let home = TempHome::new();
        let server = MockServer::start();
        server.expect(
            "GET",
            &format!("/myorg/{project}/_apis/wiki/wikis"),
            MockResponse::json(status, json!({"message": "TF400813: nope."})),
        );

        let output = run(&home, &server, &["wikis", "list", project, "--json"]);

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
    }
}

#[test]
fn show_emits_the_value_envelope() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-1"),
        MockResponse::json(200, wiki_show()),
    );

    let output = run(
        &home,
        &server,
        &["wikis", "show", "Alpha", "wiki-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": wiki_show()})
    );
}

#[test]
fn show_human_prints_the_module_detail() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-1"),
        MockResponse::json(200, wiki_show()),
    );

    let output = run(&home, &server, &["wikis", "show", "Alpha", "wiki-1"]);

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        format!(
            "\nWiki Details\n\n{}\n  ID:   wiki-1\n  Name: Alpha Wiki\n  Type: codeWiki\n  URL:  https://dev.azure.com/myorg/Alpha/_git/Alpha.wiki\n\n",
            "─".repeat(60)
        ),
        "the module's detail: the box-drawing rule and `remoteUrl` before `url`"
    );
}

#[test]
fn show_encodes_the_wiki_id_as_one_path_segment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/a%2Bb"),
        MockResponse::json(200, wiki_show()),
    );

    let output = run(&home, &server, &["wikis", "show", "Alpha", "a+b", "--json"]);

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].path,
        format!("{WIKIS}/a%2Bb"),
        "the frozen URI.encode leaves + raw; this build escapes it (D22)"
    );
}

#[test]
fn show_404_keeps_the_module_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/nope"),
        MockResponse::json(404, json!({"message": "TF400813: nope."})),
    );

    let output = run(
        &home,
        &server,
        &["wikis", "show", "Alpha", "nope", "--json"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(envelope(&output)["error"]["code"], json!("not_found"));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Wiki 'nope' not found")
    );
}

#[test]
fn pages_list_emits_the_value_envelope_with_the_whole_body() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(200, pages()),
    );

    let output = run(
        &home,
        &server,
        &["wikis", "pages", "list", "Alpha", "wiki-1", "--json"],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": pages()}),
        "the module calls `json_or_format`, not the list variant: the body is not unwrapped"
    );
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("path".to_owned(), "%2F".to_owned()),
            ("recursionLevel".to_owned(), "OneLevel".to_owned())
        ],
        "the default path is `/` and the recursion is always OneLevel"
    );
}

#[test]
fn pages_list_honours_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(
            200,
            json!({"path": "/Design", "subPages": [{"path": "/Design/Architecture"}]}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis", "pages", "list", "Alpha", "wiki-1", "--path", "/Design", "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("path".to_owned(), "%2FDesign".to_owned()),
            ("recursionLevel".to_owned(), "OneLevel".to_owned())
        ]
    );
}

#[test]
fn pages_list_human_empty_prints_the_frozen_sentence() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-empty/pages"),
        MockResponse::json(200, json!({"path": "/", "subPages": []})),
    );

    let output = run(
        &home,
        &server,
        &["wikis", "pages", "list", "Alpha", "wiki-empty"],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "No pages found.\n");
}

#[test]
fn pages_list_human_prints_every_path_and_the_root_fallback() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(200, pages()),
    );

    let output = run(
        &home,
        &server,
        &["wikis", "pages", "list", "Alpha", "wiki-1"],
    );

    assert_success(&output);
    let stdout = stdout_of(&output);
    for path in ["/Home", "/Design", "/Design/Architecture"] {
        assert!(stdout.contains(path), "missing {path} in {stdout}");
    }
    assert!(
        stdout.contains("/"),
        "a page without a path prints the `/` fallback: {stdout}"
    );
}

#[test]
fn pages_show_emits_one_envelope_and_does_not_repeat_the_content() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(200, page_home()),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis", "pages", "show", "Alpha", "wiki-1", "--path", "/Home", "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": page_home()}),
        "one document: the frozen module writes the content before its envelope (D40)"
    );
    assert_eq!(
        requests(&server)[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("path".to_owned(), "%2FHome".to_owned()),
            ("includeContent".to_owned(), "true".to_owned())
        ]
    );
}

#[test]
fn pages_show_human_prints_the_content_once() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(200, page_home()),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis", "pages", "show", "Alpha", "wiki-1", "--path", "/Home",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "# Home\n\nWelcome to the wiki.\n",
        "the frozen path prints the content twice (D40); this build prints it once"
    );
}

#[test]
fn pages_show_404_keeps_the_module_wording() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(404, json!({"message": "TF400813: nope."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis", "pages", "show", "Alpha", "wiki-1", "--path", "/Missing", "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        envelope(&output)["error"]["message"],
        json!("Page '/Missing' not found")
    );
}

#[test]
fn pages_show_without_a_path_is_a_loud_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(
        &home,
        &server,
        &["wikis", "pages", "show", "Alpha", "wiki-1"],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "the oracle's Map.fetch! crash is a silent exit 0; this build is loud (D34)"
    );
    assert!(
        stderr_of(&output).contains("--path"),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(requests(&server).is_empty(), "nothing was sent");
}

#[test]
fn pages_create_puts_the_content_with_the_module_comment() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(
            200,
            json!({"id": 43, "path": "/Created-Slug", "content": "hello world", "eTag": "new456"}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis",
            "pages",
            "create",
            "Alpha",
            "wiki-1",
            "--path",
            "/New-Page",
            "--content",
            "hello world",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": {"id": 43, "path": "/Created-Slug", "content": "hello world", "eTag": "new456"}}),
        "a write path takes the value envelope under --json (D33)"
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(
        serde_json::from_str::<Value>(requests[0].body.as_deref().expect("a body"))
            .expect("the body is JSON"),
        json!({"content": "hello world"}),
        "the module's one-key body"
    );
    assert_eq!(
        requests[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("path".to_owned(), "%2FNew-Page".to_owned()),
            ("comment".to_owned(), "Created+via+ado+CLI".to_owned())
        ],
        "the module's comment pair and the argument path"
    );
    assert_eq!(
        requests[0].header("if-match"),
        None,
        "create sends no If-Match"
    );
}

#[test]
fn pages_create_human_names_the_response_path() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(200, json!({"id": 43, "path": "/Created-Slug"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis",
            "pages",
            "create",
            "Alpha",
            "wiki-1",
            "--path",
            "/New-Page",
            "--content",
            "hello",
        ],
    );

    assert_success(&output);
    assert_eq!(
        stdout_of(&output),
        "Page '/Created-Slug' created.\n",
        "the line names the response's path, not the argument's (the capture's discriminator)"
    );
}

#[test]
fn pages_create_joins_unquoted_multiword_content() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(200, json!({"id": 43, "path": "/New-Page"})),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis",
            "pages",
            "create",
            "Alpha",
            "wiki-1",
            "--path",
            "/New-Page",
            "--content",
            "hello",
            "world",
        ],
    );

    assert_success(&output);
    assert_eq!(
        serde_json::from_str::<Value>(requests(&server)[0].body.as_deref().expect("a body"))
            .expect("the body is JSON"),
        json!({"content": "hello world"}),
        "the frozen CLI joins a multivalue option's words; this build normalises argv the same way"
    );
}

#[test]
fn pages_create_without_a_path_or_content_is_a_loud_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    for (args, missing) in [
        (
            vec![
                "wikis",
                "pages",
                "create",
                "Alpha",
                "wiki-1",
                "--content",
                "hello",
            ],
            "--path",
        ),
        (
            vec![
                "wikis",
                "pages",
                "create",
                "Alpha",
                "wiki-1",
                "--path",
                "/New-Page",
            ],
            "--content",
        ),
    ] {
        let output = run(&home, &server, &args);

        assert_eq!(
            output.status.code(),
            Some(1),
            "the oracle's Map.fetch! is a silent exit 0 (D34); args: {args:?}"
        );
        assert!(
            stderr_of(&output).contains(missing),
            "the usage error names {missing}: {}",
            stderr_of(&output)
        );
    }

    assert!(requests(&server).is_empty());
}

#[test]
fn pages_update_reads_the_page_then_puts_with_if_match() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(
            200,
            json!({"id": 44, "path": "/Updated-Slug", "content": "new text", "eTag": "new789"}),
        ),
    );
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-1/pages"),
        MockResponse::json(200, page_home()),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis",
            "pages",
            "update",
            "Alpha",
            "wiki-1",
            "--path",
            "/Home",
            "--content",
            "new text",
            "--json",
        ],
    );

    assert_success(&output);
    assert_eq!(
        envelope(&output),
        json!({"ok": true, "result": {"id": 44, "path": "/Updated-Slug", "content": "new text", "eTag": "new789"}})
    );

    let requests = requests(&server);
    assert_eq!(requests.len(), 2, "the read-then-write pair");
    assert_eq!(requests[0].method, "GET");
    assert_eq!(
        requests[0].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("path".to_owned(), "%2FHome".to_owned()),
            ("includeContent".to_owned(), "true".to_owned())
        ],
        "the read asks for the content so the eTag comes back"
    );
    assert_eq!(requests[1].method, "PUT");
    assert_eq!(
        requests[1].query_pairs(),
        vec![
            ("api-version".to_owned(), "7.1".to_owned()),
            ("path".to_owned(), "%2FHome".to_owned()),
            ("comment".to_owned(), "Updated+via+ado+CLI".to_owned())
        ],
        "the write carries the module's own comment"
    );
    assert_eq!(
        requests[1].header("if-match"),
        Some("abc123"),
        "the read's eTag becomes the write's optimistic-concurrency guard"
    );
}

#[test]
fn pages_update_without_an_etag_sends_no_if_match() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "PUT",
        &format!("{WIKIS}/wiki-empty/pages"),
        MockResponse::json(200, json!({"id": 50, "path": "/No-ETag-Slug"})),
    );
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-empty/pages"),
        MockResponse::json(
            200,
            json!({"id": 50, "path": "/Home", "content": "no etag here"}),
        ),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis",
            "pages",
            "update",
            "Alpha",
            "wiki-empty",
            "--path",
            "/Home",
            "--content",
            "new text",
        ],
    );

    assert_success(&output);
    assert_eq!(stdout_of(&output), "Page '/No-ETag-Slug' updated.\n");
    assert_eq!(
        requests(&server)[1].header("if-match"),
        None,
        "an absent eTag adds no header"
    );
}

#[test]
fn pages_update_get_404_is_the_classified_envelope_and_sends_no_put() {
    let home = TempHome::new();
    let server = MockServer::start();
    server.expect(
        "GET",
        &format!("{WIKIS}/wiki-gone/pages"),
        MockResponse::json(404, json!({"message": "TF400813: nope."})),
    );

    let output = run(
        &home,
        &server,
        &[
            "wikis",
            "pages",
            "update",
            "Alpha",
            "wiki-gone",
            "--path",
            "/Home",
            "--content",
            "new text",
            "--json",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(envelope(&output)["error"]["code"], json!("not_found"));
    assert_eq!(
        requests(&server).len(),
        1,
        "the failed read short-circuits the write"
    );
}

#[test]
fn pages_update_without_a_path_or_content_is_a_loud_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    for (args, missing) in [
        (
            vec![
                "wikis",
                "pages",
                "update",
                "Alpha",
                "wiki-1",
                "--content",
                "new",
            ],
            "--path",
        ),
        (
            vec![
                "wikis", "pages", "update", "Alpha", "wiki-1", "--path", "/Home",
            ],
            "--content",
        ),
    ] {
        let output = run(&home, &server, &args);

        assert_eq!(output.status.code(), Some(1), "args: {args:?}");
        assert!(
            stderr_of(&output).contains(missing),
            "the usage error names {missing}: {}",
            stderr_of(&output)
        );
        assert!(requests(&server).is_empty());
    }
}

#[test]
fn the_pages_parent_without_a_subcommand_is_a_usage_error() {
    let home = TempHome::new();
    let server = MockServer::start();

    let output = run(&home, &server, &["wikis", "pages"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr_of(&output).contains("missing sub-command"),
        "the shared parent path, not an unknown command: {}",
        stderr_of(&output)
    );
    assert!(requests(&server).is_empty());
}
