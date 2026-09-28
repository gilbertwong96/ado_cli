//! Tests for `ado schema`: the node shape an agent reads, and the binary's
//! end-to-end behaviour on the JSON, human and error paths.

use ado::VERSION;
use ado::commands::schema::{build_tree, find_node};
use ado_testkit::{ado, stderr_of, stdout_of};
use serde_json::{Map, Value, json};

const ROOT_DOC: &str = "Azure DevOps CLI - Manage Azure DevOps projects, repos, work items, and pipelines from the terminal.";
const VERSION_DOC: &str = "Print the ado version and exit.";

fn as_object(value: &Value) -> &Map<String, Value> {
    value.as_object().expect("a JSON object")
}

fn sorted_keys(value: &Value) -> Vec<&str> {
    let mut keys = as_object(value)
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    keys
}

fn subcommands(node: &Value) -> &[Value] {
    node["subcommands"].as_array().expect("a subcommand array")
}

fn options(node: &Value) -> &[Value] {
    node["options"].as_array().expect("an option array")
}

fn option_names(node: &Value) -> Vec<&str> {
    let mut names = options(node)
        .iter()
        .map(|option| option["name"].as_str().expect("an option name"))
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn option<'a>(node: &'a Value, name: &str) -> &'a Value {
    options(node)
        .iter()
        .find(|option| option["name"] == json!(name))
        .unwrap_or_else(|| panic!("the '{name}' option is missing"))
}

/// Every node reachable from `node`, including `node` itself.
fn nodes(node: &Value) -> Vec<&Value> {
    let mut all = vec![node];

    for sub in subcommands(node) {
        all.extend(nodes(sub));
    }

    all
}

const GLOBALS: [&str; 5] = ["json", "org", "pat", "server", "verbose"];

#[test]
fn schema_root_has_name_doc_and_subcommands() {
    let root = build_tree();

    assert_eq!(root["name"], json!("ado"));
    assert_eq!(root["doc"], json!(ROOT_DOC));
    assert_eq!(root["arguments"], json!([]));

    let names = subcommands(&root)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    for expected in ["ado version", "ado whoami", "ado schema"] {
        assert!(names.contains(&expected), "missing {expected} in {names:?}");
    }
}

#[test]
fn schema_root_lists_exactly_the_shipped_subcommands() {
    let root = build_tree();

    let mut names = subcommands(&root)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();
    names.sort_unstable();

    assert_eq!(
        names,
        [
            "ado completion",
            "ado projects",
            "ado prs",
            "ado repos",
            "ado schema",
            "ado version",
            "ado whoami",
            "ado workitems"
        ]
    );
}

/// Wave 1 ports the two read paths only: `create`, `complete`, `abandon`,
/// `approve`, `vote`, `diff`, `comments` and `reviewers` are Wave 2.
#[test]
fn schema_prs_node_has_only_the_wave_one_subcommands() {
    let prs = find_node("prs").expect("the prs node");

    let names = subcommands(&prs)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(names, ["ado prs list", "ado prs show"]);
}

#[test]
fn schema_node_has_the_contract_keys() {
    let tree = build_tree();

    for node in nodes(&tree) {
        assert_eq!(
            sorted_keys(node),
            ["arguments", "doc", "name", "options", "subcommands"],
            "node {}",
            node["name"]
        );
        assert!(node["name"].is_string(), "node {}", node["name"]);
        assert!(node["doc"].is_string(), "node {}", node["name"]);
        assert!(node["arguments"].is_array(), "node {}", node["name"]);
        assert!(node["options"].is_array(), "node {}", node["name"]);
        assert!(node["subcommands"].is_array(), "node {}", node["name"]);
    }

    let version = find_node("version").expect("the version node");

    assert_eq!(version["name"], json!("ado version"));
    assert_eq!(version["doc"], json!(VERSION_DOC));
    assert_eq!(version["arguments"], json!([]));
    assert_eq!(version["subcommands"], json!([]));
    assert_eq!(
        option_names(&version),
        GLOBALS,
        "every node carries the global options, and nothing else for a leaf"
    );
}

#[test]
fn option_objects_have_the_contract_keys() {
    for node in nodes(&build_tree()) {
        for option in options(node) {
            assert_eq!(
                sorted_keys(option),
                ["default", "doc", "name", "required", "short", "type"],
                "option {} of {}",
                option["name"],
                node["name"]
            );
            assert!(option["name"].is_string(), "option {}", option["name"]);
            assert!(option["short"].is_string(), "option {}", option["name"]);
            assert!(option["type"].is_string(), "option {}", option["name"]);
            assert!(option["default"].is_string(), "option {}", option["name"]);
            assert!(option["doc"].is_string(), "option {}", option["name"]);
            assert!(option["required"].is_boolean(), "option {}", option["name"]);
        }
    }
}

#[test]
fn schema_argument_objects_have_the_contract_keys() {
    let schema = find_node("schema").expect("the schema node");
    let arguments = schema["arguments"].as_array().expect("an argument array");

    assert_eq!(arguments.len(), 1);
    assert_eq!(
        sorted_keys(&arguments[0]),
        ["doc", "name", "required", "type"]
    );
    assert_eq!(arguments[0]["name"], json!("name"));
    assert_eq!(arguments[0]["type"], json!("string"));
    assert_eq!(arguments[0]["required"], json!(false));
    assert_eq!(
        arguments[0]["doc"],
        json!("Optional: dump only this command + descendants")
    );
}

#[test]
fn schema_option_defaults_are_stringified() {
    let version = find_node("version").expect("the version node");

    let json_flag = option(&version, "json");
    assert_eq!(json_flag["type"], json!("boolean"));
    assert_eq!(json_flag["default"], json!("false"));
    assert_eq!(json_flag["short"], json!(""));
    assert_eq!(json_flag["required"], json!(false));
    assert_eq!(json_flag["doc"], json!("Output raw JSON"));

    let org = option(&version, "org");
    assert_eq!(org["type"], json!("string"));
    assert_eq!(org["default"], json!(""));
    assert_eq!(org["short"], json!("o"));
    assert_eq!(org["required"], json!(false));
    assert_eq!(
        org["doc"],
        json!("Azure DevOps organization name (or set ADO_ORG env var)")
    );
}

#[test]
fn schema_server_short_is_s() {
    let version = find_node("version").expect("the version node");

    assert_eq!(option(&version, "server")["short"], json!("s"));
    assert_eq!(option(&version, "verbose")["short"], json!("v"));
    assert_eq!(option(&version, "json")["short"], json!(""));
}

#[test]
fn schema_version_target_shape_matches_oracle() {
    let output = ado()
        .args(["schema", "version", "--json"])
        .output()
        .expect("run `ado schema version --json`");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(&output)
    );

    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is a JSON document");

    insta::assert_json_snapshot!(value);
}

#[test]
fn schema_unknown_target_is_validation_error() {
    let output = ado()
        .args(["schema", "nope", "--json"])
        .output()
        .expect("run `ado schema nope --json`");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).expect("stdout is a JSON document"),
        json!({
            "ok": false,
            "error": {
                "code": "validation_error",
                "message": "no command named \"nope\". Run `ado schema` to see all commands.",
            },
        })
    );
    assert!(output.stderr.is_empty(), "stderr: {}", stderr_of(&output));
}

#[test]
fn schema_unknown_target_plain_message() {
    let output = ado()
        .args(["schema", "nope"])
        .output()
        .expect("run `ado schema nope`");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr_of(&output),
        "[Validation error] no command named \"nope\". Run `ado schema` to see all commands.\n"
    );
    assert!(output.stdout.is_empty(), "stdout: {}", stdout_of(&output));
}

#[test]
fn schema_json_is_parseable_and_ansi_free() {
    let output = ado()
        .args(["schema", "--json"])
        .output()
        .expect("run `ado schema --json`");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(&output)
    );
    assert!(
        !output.stdout.contains(&0x1B),
        "stdout contains an ESC byte: {:?}",
        stdout_of(&output)
    );

    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is a JSON document");

    assert_eq!(value["ok"], json!(true));
    assert_eq!(value["schema"]["name"], json!("ado"));
    assert_eq!(value["schema"]["version"], json!(VERSION));
}

#[test]
fn schema_plain_matches_the_documented_shape() {
    let output = ado().arg("schema").output().expect("run `ado schema`");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_of(&output)
    );

    let text = stdout_of(&output);

    assert!(
        text.starts_with(&format!("ado v{VERSION} — command tree\n")),
        "human output: {text}"
    );
    assert!(
        text.contains(&format!("\n{}\n", "─".repeat(60))),
        "human output: {text}"
    );
    assert!(text.contains("\n    ado version\n"), "human output: {text}");
    assert!(
        text.trim_end()
            .ends_with("Run with --json for a structured tree."),
        "human output: {text}"
    );
    assert!(
        !output.stdout.contains(&0x1B),
        "stdout contains an ESC byte: {text:?}"
    );
}
