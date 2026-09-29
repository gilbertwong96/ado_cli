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

fn argument<'a>(node: &'a Value, name: &str) -> &'a Value {
    node["arguments"]
        .as_array()
        .expect("an argument array")
        .iter()
        .find(|argument| argument["name"] == json!(name))
        .unwrap_or_else(|| panic!("the '{name}' argument is missing"))
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
            "ado login",
            "ado logout",
            "ado pipelines",
            "ado pipelines-artifacts",
            "ado pipelines-builds",
            "ado pipelines-folders",
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

/// `login` has one option of its own, `--method`; the Elixir's four local options
/// are this tree's globals (`--org`, `--pat`, `--server`, and with them `--json`
/// and `--verbose`, D2), and none of them is required — the command's own
/// validation answers for a missing org or PAT, with the codes and the `details`
/// payload the oracle emits.
#[test]
fn schema_login_node_has_the_method_option_and_the_globals() {
    let login = find_node("login").expect("the login node");

    assert_eq!(
        option_names(&login),
        ["json", "method", "org", "pat", "server", "verbose"],
        "the one local option plus the globals clap copies into every node"
    );
    assert!(
        subcommands(&login).is_empty(),
        "login is a leaf: it has no subcommands"
    );
    assert!(
        options(&login)
            .iter()
            .all(|option| option["required"] == json!(false))
    );

    let method = option(&login, "method");
    assert_eq!(method["type"], json!("string"));
    assert_eq!(method["short"], json!(""));
    assert_eq!(method["default"], json!(""));
    let doc = method["doc"].as_str().expect("the method doc");
    assert!(doc.contains("pat"), "doc: {doc}");
    assert!(doc.contains("device"), "doc: {doc}");
    assert!(
        doc.contains("Browser login is not available in this build"),
        "the help names what ships, not what Wave 3 adds: {doc}"
    );
}

/// Wave 2 completes the pipelines node: the two reads, the four definition
/// mutations, the `vars` group (Task 4) and the two Task 5 groups — `variables`
/// and `secure_files`.
///
/// R3: the schema's display name is `pipelines secure-files`, but the runnable
/// spelling is the underscore — `ado pipelines secure-files …` prints the parent's
/// help, so this build reports the name argv accepts (D18's rule).
#[test]
fn schema_pipelines_node_has_the_wave_two_subcommands() {
    let pipelines = find_node("pipelines").expect("the pipelines node");

    let names = subcommands(&pipelines)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        [
            "ado pipelines list",
            "ado pipelines show",
            "ado pipelines run",
            "ado pipelines create",
            "ado pipelines update",
            "ado pipelines delete",
            "ado pipelines vars",
            "ado pipelines variables",
            "ado pipelines secure_files",
        ]
    );

    let vars = pipelines["subcommands"]
        .as_array()
        .expect("the subcommand array")
        .iter()
        .find(|sub| sub["name"] == json!("ado pipelines vars"))
        .expect("the vars node");
    let vars_names = subcommands(vars)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(
        vars_names,
        [
            "ado pipelines vars list",
            "ado pipelines vars show",
            "ado pipelines vars create",
            "ado pipelines vars update",
            "ado pipelines vars delete",
        ]
    );

    let variables = pipelines["subcommands"]
        .as_array()
        .expect("the subcommand array")
        .iter()
        .find(|sub| sub["name"] == json!("ado pipelines variables"))
        .expect("the variables node");
    let variables_names = subcommands(variables)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(
        variables_names,
        [
            "ado pipelines variables list",
            "ado pipelines variables create",
            "ado pipelines variables delete",
        ]
    );

    let secure_files = pipelines["subcommands"]
        .as_array()
        .expect("the subcommand array")
        .iter()
        .find(|sub| sub["name"] == json!("ado pipelines secure_files"))
        .expect("the secure_files node");
    let secure_files_names = subcommands(secure_files)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(
        secure_files_names,
        [
            "ado pipelines secure_files list",
            "ado pipelines secure_files show",
            "ado pipelines secure_files upload",
            "ado pipelines secure_files delete",
        ]
    );

    for node in nodes(secure_files) {
        let name = node["name"].as_str().expect("a node name");
        assert!(
            name.starts_with("ado pipelines secure_files"),
            "the schema reports the parseable spelling argv accepts (R3): {name}"
        );
        assert!(
            !name.contains("secure-files"),
            "the Elixir's display name is not runnable: {name}"
        );
    }

    let upload = secure_files_names
        .iter()
        .position(|name| *name == "ado pipelines secure_files upload")
        .map(|index| &secure_files["subcommands"][index])
        .expect("the upload node");
    assert_eq!(
        option_names(upload),
        [
            "allow_exists",
            "file",
            "json",
            "org",
            "pat",
            "server",
            "verbose",
        ],
        "the oracle's two local options plus the globals"
    );
    assert_eq!(option(upload, "file")["required"], json!(true));
    assert_eq!(option(upload, "file")["type"], json!("string"));
    assert_eq!(option(upload, "allow_exists")["type"], json!("boolean"));
    assert_eq!(argument(upload, "name")["required"], json!(true));

    let delete = secure_files["subcommands"]
        .as_array()
        .expect("the subcommand array")
        .iter()
        .find(|sub| sub["name"] == json!("ado pipelines secure_files delete"))
        .expect("the secure_files delete node");
    assert_eq!(
        option_names(delete),
        ["force", "json", "org", "pat", "server", "verbose"],
        "--force is the one local option"
    );
}

/// Wave 1 ports the four read paths only: `queue`, `cancel` and `tags add` are
/// Wave 2. W1-1/D18: the node name is the hyphenated spelling every descendant
/// shares — `pipelines-builds` parses as argv, where the Elixir's schema spelling
/// (`ado pipelines builds …`) does not.
#[test]
fn schema_pipelines_builds_node_lists_every_shipped_subcommand() {
    let builds = find_node("pipelines-builds").expect("the pipelines-builds node");

    let names = subcommands(&builds)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        [
            "ado pipelines-builds list",
            "ado pipelines-builds show",
            "ado pipelines-builds queue",
            "ado pipelines-builds cancel",
            "ado pipelines-builds tags",
            "ado pipelines-builds definitions"
        ]
    );

    let tags = builds["subcommands"]
        .as_array()
        .expect("the subcommand array")
        .iter()
        .find(|sub| sub["name"] == json!("ado pipelines-builds tags"))
        .expect("the tags node");
    let tags_names = subcommands(tags)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();
    assert_eq!(
        tags_names,
        [
            "ado pipelines-builds tags list",
            "ado pipelines-builds tags add"
        ]
    );

    let definitions = builds["subcommands"]
        .as_array()
        .expect("the subcommand array")
        .iter()
        .find(|sub| sub["name"] == json!("ado pipelines-builds definitions"))
        .expect("the definitions node");
    let definitions_names = subcommands(definitions)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();
    assert_eq!(definitions_names, ["ado pipelines-builds definitions list"]);

    for node in nodes(&builds) {
        let name = node["name"].as_str().expect("a node name");
        assert!(
            name.starts_with("ado pipelines-builds"),
            "the schema reports the parseable hyphenated spelling, not the Elixir's space-separated name (W1-1/D18): {name}"
        );
        assert!(
            !name.contains("pipelines builds"),
            "the Elixir's unparseable name: {name}"
        );
    }
}

/// The folders node is new in Wave 2 and holds exactly the oracle's three
/// subcommands. W1-1/D18/R3: the schema spells the node `pipelines-folders`, the
/// parseable spelling argv accepts, where the oracle's display name is
/// `ado pipelines folders`.
#[test]
fn schema_pipelines_folders_node_lists_its_three_subcommands() {
    let folders = find_node("pipelines-folders").expect("the pipelines-folders node");

    let names = subcommands(&folders)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        [
            "ado pipelines-folders list",
            "ado pipelines-folders create",
            "ado pipelines-folders delete"
        ]
    );

    for node in nodes(&folders) {
        let name = node["name"].as_str().expect("a node name");
        assert!(
            name.starts_with("ado pipelines-folders"),
            "the schema reports the parseable hyphenated spelling, not the Elixir's space-separated display name (D18/R3): {name}"
        );
        assert!(
            !name.contains("pipelines folders"),
            "the Elixir's unparseable display name: {name}"
        );
    }
}

/// W1-1/D18: `ado schema --json` names the node the way the binary parses it, so an
/// agent can copy the name straight into argv.
#[test]
fn schema_json_reports_the_hyphenated_builds_spelling() {
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

    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is a JSON document");
    let names = nodes(&value["schema"])
        .into_iter()
        .map(|node| node["name"].as_str().expect("a node name").to_owned())
        .collect::<Vec<_>>();

    assert!(
        names.iter().any(|name| name == "ado pipelines-builds list"),
        "the hyphenated list node is in the tree: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name.contains("pipelines builds")),
        "no node carries the Elixir's unparseable name: {names:?}"
    );
}

/// Wave 1 ports the read paths only: the artifacts node holds exactly `list` and
/// `download`. W1-1/D18: the node name is the hyphenated spelling every descendant
/// shares — `pipelines-artifacts` parses as argv, where the Elixir's schema spelling
/// (`ado pipelines artifacts …`) does not.
#[test]
fn schema_pipelines_artifacts_node_has_only_the_wave_one_subcommands() {
    let artifacts = find_node("pipelines-artifacts").expect("the pipelines-artifacts node");

    let names = subcommands(&artifacts)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        [
            "ado pipelines-artifacts list",
            "ado pipelines-artifacts download"
        ]
    );

    for node in nodes(&artifacts) {
        let name = node["name"].as_str().expect("a node name");
        assert!(
            name.starts_with("ado pipelines-artifacts"),
            "the schema reports the parseable hyphenated spelling, not the Elixir's space-separated name (W1-1/D18): {name}"
        );
        assert!(
            !name.contains("pipelines artifacts"),
            "the Elixir's unparseable name: {name}"
        );
    }
}

/// W1-1/D18: `ado schema --json` names the artifacts node the way the binary
/// parses it, so an agent can copy the name straight into argv.
#[test]
fn schema_json_reports_the_hyphenated_artifacts_spelling() {
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

    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is a JSON document");
    let names = nodes(&value["schema"])
        .into_iter()
        .map(|node| node["name"].as_str().expect("a node name").to_owned())
        .collect::<Vec<_>>();

    assert!(
        names
            .iter()
            .any(|name| name == "ado pipelines-artifacts download"),
        "the hyphenated download node is in the tree: {names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|name| name.contains("pipelines artifacts")),
        "no node carries the Elixir's unparseable name: {names:?}"
    );
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

/// D23: the three metadata deviations the row records, pinned here because §10's
/// schema comparison checks node names only. An argument or option the oracle types
/// `integer` reports `"string"` (clap's value-parser type), a positional reports
/// `"required":true` where the oracle says `false` (the parse tree really requires
/// it), and the oracle's `assigned_to` keyword key reports as the hyphenated flag an
/// agent can pass.
#[test]
fn schema_wave_one_nodes_pin_the_documented_metadata() {
    let show = find_node("workitems show").expect("the workitems show node");
    assert_eq!(argument(&show, "id")["type"], json!("string"));
    assert_eq!(argument(&show, "id")["required"], json!(true));

    let list = find_node("workitems list").expect("the workitems list node");
    assert_eq!(option(&list, "top")["type"], json!("string"));
    assert_eq!(argument(&list, "project")["required"], json!(true));
    assert!(
        option_names(&list).contains(&"assigned-to"),
        "the flag an agent can pass: {:?}",
        option_names(&list)
    );
    assert!(
        !option_names(&list).contains(&"assigned_to"),
        "the Elixir keyword key leaking into JSON: {:?}",
        option_names(&list)
    );

    let repos_show = find_node("repos show").expect("the repos show node");
    for name in ["project", "repo_id"] {
        assert_eq!(
            argument(&repos_show, name)["required"],
            json!(true),
            "{name}"
        );
    }

    let download = find_node("pipelines-artifacts download").expect("the download node");
    for name in ["project", "pipeline_id", "run_id", "artifact_name"] {
        assert_eq!(argument(&download, name)["required"], json!(true), "{name}");
    }
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
