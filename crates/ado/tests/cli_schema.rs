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
            "ado agent-pools",
            "ado areas",
            "ado banners",
            "ado branch-policies",
            "ado ci",
            "ado completion",
            "ado connections",
            "ado extensions",
            "ado imports",
            "ado iterations",
            "ado login",
            "ado logout",
            "ado packages",
            "ado pipelines",
            "ado pipelines-artifacts",
            "ado pipelines-builds",
            "ado pipelines-folders",
            "ado projects",
            "ado prs",
            "ado releases",
            "ado repos",
            "ado schema",
            "ado security",
            "ado teams",
            "ado test-coverage",
            "ado test-results",
            "ado users",
            "ado version",
            "ado whoami",
            "ado wikis",
            "ado workitems"
        ]
    );
}

#[test]
fn schema_ci_node_lists_every_shipped_subcommand() {
    let ci = find_node("ci").expect("the ci node");

    assert_eq!(
        ci["doc"],
        json!(
            "Watch Azure DevOps pipelines in real-time. Streams live build status (job/step progress) and per-line log output to your terminal, like `gh run watch` or `kubectl logs -f`. Exits when the build completes or on Ctrl+C."
        )
    );
    assert_eq!(ci["arguments"], json!([]));
    assert_eq!(
        subcommands(&ci)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        ["ado ci watch"],
        "the module's declaration order"
    );

    let watch = find_node("ci watch").expect("the watch node");
    assert_eq!(
        watch["doc"],
        json!(
            "Stream live status and per-line log output for an Azure DevOps build. The build status is polled every 2s (configurable via --poll-interval), and new log lines are printed as they appear. Exits with code 0 on success, 1 on build failure, 2 on cancellation."
        )
    );
    assert_eq!(
        option_names(&watch),
        [
            "branch",
            "definition",
            "json",
            "latest",
            "org",
            "pat",
            "poll-interval",
            "server",
            "verbose"
        ],
        "Annex A's four options plus the globals; the hyphen spelling argv accepts (Ruling 4(a))"
    );
    assert_eq!(
        option(&watch, "poll-interval")["doc"],
        json!(
            "How often to poll the build status, in milliseconds. Default 2000 (2s). Values below 250 are clamped to 2000. Lower values update faster but use more API quota."
        )
    );
    assert_eq!(
        option(&watch, "poll-interval")["type"],
        json!("string"),
        "D23: clap does not expose the value parser's type"
    );
    assert_eq!(
        option(&watch, "latest")["default"],
        json!("false"),
        "the frozen default: absent is false"
    );
    assert_eq!(
        option(&watch, "poll-interval")["default"],
        json!(""),
        "the module's own default is applied in code, not declared to clap (the banners precedent)"
    );
    assert_eq!(argument(&watch, "project")["required"], json!(true));
    assert_eq!(
        argument(&watch, "build_id")["required"],
        json!(false),
        "the frozen schema marks the second positional optional"
    );
    assert_eq!(argument(&watch, "build_id")["type"], json!("string"));
}

/// Wave 1 ported the two read paths; Task 9 adds the five lifecycle mutations,
/// Task 10 the diff, Task 11a the comments parent and Task 11b the reviewers
/// parent (their leaves are this node's grandchildren).
#[test]
fn schema_prs_node_lists_every_shipped_subcommand() {
    let prs = find_node("prs").expect("the prs node");

    let names = subcommands(&prs)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        [
            "ado prs list",
            "ado prs show",
            "ado prs create",
            "ado prs complete",
            "ado prs approve",
            "ado prs vote",
            "ado prs abandon",
            "ado prs diff",
            "ado prs comments",
            "ado prs reviewers"
        ]
    );
}

/// The reviewers subtree: the frozen schema marks `--reviewer` required but
/// CliMate never enforces it (the `Map.fetch!` crash, D34), so this tree's
/// `required: true` is the loud half the harness's two `(no --reviewer)` cases
/// pin; `--required` and `--search` keep the module's own names and the
/// `--json`/globals are the shared set.
#[test]
fn schema_prs_reviewers_node_marks_the_reviewer_required() {
    let reviewers = find_node("prs reviewers").expect("the reviewers node");

    assert_eq!(
        subcommands(&reviewers)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado prs reviewers list",
            "ado prs reviewers add",
            "ado prs reviewers remove"
        ]
    );

    let list = find_node("prs reviewers list").expect("the list node");
    assert!(
        option_names(&list).contains(&"search"),
        "the fuzzy filter stays a real flag: {:?}",
        option_names(&list)
    );

    for leaf in ["prs reviewers add", "prs reviewers remove"] {
        let node = find_node(leaf).expect("the leaf node");

        assert_eq!(
            option(&node, "reviewer")["required"],
            json!(true),
            "{leaf} marks --reviewer required (the oracle does too; it never enforces it, D34)"
        );
    }

    let add = find_node("prs reviewers add").expect("the add node");
    assert!(
        option_names(&add).contains(&"required"),
        "--required is the module's own name: {:?}",
        option_names(&add)
    );
    assert_eq!(argument(&add, "pr_id")["required"], json!(true));
}

/// The two Task 12 trees: five leaves each, the required `--name` on the three
/// writes (D34's loud half), and the hyphenated date options the frozen parser
/// accepts where the schema spells them with underscores (D17).
#[test]
fn schema_areas_and_iterations_nodes_list_every_shipped_subcommand() {
    let areas = find_node("areas").expect("the areas node");

    assert_eq!(
        subcommands(&areas)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado areas list",
            "ado areas show",
            "ado areas create",
            "ado areas update",
            "ado areas delete"
        ]
    );

    let iterations = find_node("iterations").expect("the iterations node");

    assert_eq!(
        subcommands(&iterations)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado iterations list",
            "ado iterations show",
            "ado iterations create",
            "ado iterations update",
            "ado iterations delete"
        ]
    );

    for leaf in ["areas create", "areas update", "iterations create"] {
        let node = find_node(leaf).expect("the leaf node");

        assert_eq!(
            option(&node, "name")["required"],
            json!(true),
            "{leaf} marks --name required (the oracle does too; it never enforces it, D34)"
        );
    }

    let depth = find_node("areas list").expect("the areas list node");
    assert!(
        option_names(&depth).contains(&"depth"),
        "--depth is a real flag: {:?}",
        option_names(&depth)
    );
    assert_eq!(
        option(&depth, "depth")["type"],
        json!("string"),
        "this build's schema reports every value-taking option as a string (D23)"
    );

    let update = find_node("iterations update").expect("the iterations update node");
    for name in ["name", "start_date", "finish_date"] {
        assert!(
            option_names(&update).contains(&name),
            "{name} is an option of iterations update: {:?}",
            option_names(&update)
        );
        assert_eq!(
            option(&update, name)["required"],
            json!(false),
            "iterations update requires none of its options; the module's guard is D4"
        );
    }

    let list = find_node("iterations list").expect("the iterations list node");
    assert!(
        option_names(&list).contains(&"current"),
        "--current is a real flag: {:?}",
        option_names(&list)
    );
}

/// The two Task 13 trees: `teams` nests `members` one level deeper, its
/// `create`/`update` mark `--name` where the oracle's `Map.fetch!` makes a
/// missing one a silent exit 0 (D34's loud half), and `users` is the
/// organization-scoped area — no leaf takes a project positional, and `remove`
/// carries no `--force` (the captured parser rejects it; the module's doc
/// sentence promising confirmation is prose, not an invocation).
#[test]
fn schema_teams_and_users_nodes_list_every_shipped_subcommand() {
    let teams = find_node("teams").expect("the teams node");

    assert_eq!(
        subcommands(&teams)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado teams list",
            "ado teams show",
            "ado teams create",
            "ado teams update",
            "ado teams delete",
            "ado teams members"
        ]
    );

    let members = find_node("teams members").expect("the members node");
    assert_eq!(
        subcommands(&members)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        ["ado teams members list"]
    );

    assert_eq!(
        option(&find_node("teams list").expect("the list node"), "top")["type"],
        json!("string"),
        "this build's schema reports every value-taking option as a string (D23)"
    );

    for leaf in ["teams create", "users add"] {
        let node = find_node(leaf).expect("the leaf node");
        let option_name = if leaf == "users add" { "email" } else { "name" };

        assert_eq!(
            option(&node, option_name)["required"],
            json!(true),
            "{leaf} marks --{option_name} required (the oracle's own schema does; it never enforces it, D34)"
        );
    }

    let teams_update = find_node("teams update").expect("the update node");
    for name in ["name", "description"] {
        assert_eq!(
            option(&teams_update, name)["required"],
            json!(false),
            "teams update requires no option; the module's guard is D4 ({name})"
        );
    }

    let users = find_node("users").expect("the users node");

    assert_eq!(
        subcommands(&users)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado users list",
            "ado users show",
            "ado users add",
            "ado users remove"
        ]
    );

    for node in nodes(&users) {
        assert!(
            node["arguments"]
                .as_array()
                .expect("an argument array")
                .iter()
                .all(|argument| argument["name"] != json!("project")),
            "no users node takes a project positional: it is organization-scoped ({})",
            node["name"]
        );
    }

    assert_eq!(
        option_names(&find_node("users remove").expect("the remove node")),
        GLOBALS,
        "users remove has no --force: the captured parser rejects it (R5)"
    );
}

/// The Task 14 tree under the runnable `branch-policies` spelling (R3/D18: the
/// oracle's schema calls the node `ado repos policies`, and argv accepts only the
/// hyphenated form). `create` marks `--type`/`--branch` required where the
/// oracle's `Map.fetch!` makes a missing one a silent exit 0 (D34's loud half),
/// `update` requires none, and each boolean is a `--flag`/`--no-flag` pair — the
/// clap tree's honest shape, where the oracle has one option whose OptionParser
/// `--no-` prefix becomes a second arg here (so the node lists `no_blocking` and
/// reports `blocking`'s default as false; the W0 schema case compares root nodes
/// and descendant names only).
#[test]
fn schema_branch_policies_node_lists_every_shipped_subcommand() {
    let policies = find_node("branch-policies").expect("the branch-policies node");

    assert_eq!(
        subcommands(&policies)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado branch-policies list",
            "ado branch-policies show",
            "ado branch-policies create",
            "ado branch-policies update",
            "ado branch-policies delete"
        ]
    );

    let create = find_node("branch-policies create").expect("the create node");

    assert_eq!(
        option_names(&create),
        [
            "blocking",
            "branch",
            "json",
            "no_blocking",
            "org",
            "pat",
            "server",
            "type",
            "verbose"
        ],
        "create carries the two required options and the flag pair"
    );

    for name in ["type", "branch"] {
        assert_eq!(
            option(&create, name)["required"],
            json!(true),
            "create marks --{name} required (the oracle's own schema does; it never enforces it, D34)"
        );
    }

    assert_eq!(
        option(&create, "blocking")["type"],
        json!("boolean"),
        "--blocking is a flag, not a value-taking option"
    );

    let update = find_node("branch-policies update").expect("the update node");

    assert_eq!(
        option_names(&update),
        [
            "blocking",
            "enabled",
            "json",
            "no_blocking",
            "no_enabled",
            "org",
            "pat",
            "server",
            "verbose"
        ],
        "update carries both flag pairs and requires neither"
    );

    for name in ["blocking", "enabled"] {
        assert_eq!(
            option(&update, name)["required"],
            json!(false),
            "update requires no option; an absent one keeps the existing value"
        );
    }

    let show = find_node("branch-policies show").expect("the show node");

    assert_eq!(
        argument(&show, "policy_id")["type"],
        json!("string"),
        "this build's schema reports a value-parser as a string (D23)"
    );
    assert_eq!(
        argument(&show, "policy_id")["required"],
        json!(true),
        "the positional really is required here (D23)"
    );
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

/// Wave 2 Task 7 adds the three write paths in the oracle's own order, with the
/// runnable hyphenated `--assigned-to` spelling (D17: the schema's `assigned_to`
/// keyword is not an invocation) and the integer options reported as clap's
/// `string` (D23). `delete` has no `--force`. Task 8 adds the `comments` and
/// `attachments` group nodes and their leaves, in the oracle's order.
#[test]
fn schema_workitems_node_lists_every_shipped_subcommand() {
    let workitems = find_node("workitems").expect("the workitems node");

    let names = subcommands(&workitems)
        .iter()
        .map(|sub| sub["name"].as_str().expect("a subcommand name"))
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        [
            "ado workitems list",
            "ado workitems show",
            "ado workitems query",
            "ado workitems create",
            "ado workitems update",
            "ado workitems delete",
            "ado workitems comments",
            "ado workitems attachments"
        ]
    );

    let create = workitems["subcommands"]
        .as_array()
        .expect("the subcommand array")
        .iter()
        .find(|sub| sub["name"] == json!("ado workitems create"))
        .expect("the create node");
    assert_eq!(
        option_names(create),
        [
            "assigned-to",
            "description",
            "json",
            "org",
            "pat",
            "priority",
            "server",
            "state",
            "tags",
            "title",
            "type",
            "verbose"
        ]
    );
    assert_eq!(option(create, "type")["required"], json!(true));
    assert_eq!(option(create, "title")["required"], json!(true));
    assert_eq!(
        option(create, "priority")["type"],
        json!("string"),
        "the oracle types it integer; clap's value-parser reports string (D23)"
    );
    assert_eq!(argument(create, "project")["required"], json!(true));

    let update = workitems["subcommands"]
        .as_array()
        .expect("the subcommand array")
        .iter()
        .find(|sub| sub["name"] == json!("ado workitems update"))
        .expect("the update node");
    assert_eq!(
        option_names(update),
        [
            "assigned-to",
            "description",
            "json",
            "org",
            "pat",
            "priority",
            "server",
            "state",
            "tags",
            "title",
            "verbose"
        ]
    );
    assert_eq!(
        option_names(
            workitems["subcommands"]
                .as_array()
                .expect("the subcommand array")
                .iter()
                .find(|sub| sub["name"] == json!("ado workitems delete"))
                .expect("the delete node")
        ),
        GLOBALS,
        "the tree has no --force: the captured command refuses it (R5)"
    );

    let comments = find_node("workitems comments").expect("the comments node");
    assert_eq!(
        subcommands(&comments)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado workitems comments list",
            "ado workitems comments add",
            "ado workitems comments update"
        ]
    );
    let comments_add = find_node("workitems comments add").expect("the comments add node");
    assert_eq!(argument(&comments_add, "id")["required"], json!(true));
    assert_eq!(
        option(&comments_add, "text")["required"],
        json!(true),
        "the oracle's schema marks --text required; it never enforces it (D34)"
    );
    let comments_update = find_node("workitems comments update").expect("the comments update node");
    assert_eq!(
        argument(&comments_update, "comment_id")["required"],
        json!(true)
    );

    let attachments = find_node("workitems attachments").expect("the attachments node");
    assert_eq!(
        subcommands(&attachments)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado workitems attachments list",
            "ado workitems attachments download"
        ]
    );
    let download = find_node("workitems attachments download").expect("the download node");
    assert_eq!(
        argument(&download, "attachment_id")["type"],
        json!("string")
    );
    assert_eq!(
        option(&download, "output")["short"],
        json!(""),
        "the global -o is --org, exactly as the frozen node's duplicate short resolves"
    );
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

/// The Task 15 tree: `packages` (three leaves, the only area with a
/// three-positional command and a four-positional `show`), `releases` (the two
/// leaves and the three list filters, whose runnable spelling is
/// `--definition-id` where the schema says `definition_id`), and `wikis` with
/// its `pages` grandchild — one level deeper than the parents around it. The
/// three page leaves mark `--path`/`--content` required where the oracle's
/// `Map.fetch!` makes a missing one a silent exit 0 (D34's loud half).
#[test]
fn schema_packages_releases_and_wikis_nodes_are_complete() {
    let packages = find_node("packages").expect("the packages node");
    assert_eq!(
        subcommands(&packages)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado packages list",
            "ado packages versions",
            "ado packages show"
        ]
    );
    assert_eq!(
        subcommands(&packages)
            .iter()
            .map(|sub| sub["arguments"]
                .as_array()
                .expect("an argument array")
                .len())
            .collect::<Vec<_>>(),
        [2, 3, 4],
        "the three shapes: list takes two positionals, versions three, show four"
    );

    let releases = find_node("releases").expect("the releases node");
    assert_eq!(
        subcommands(&releases)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        ["ado releases list", "ado releases show"]
    );
    assert_eq!(
        option_names(&find_node("releases list").expect("the list node")),
        [
            "definition-id",
            "json",
            "org",
            "pat",
            "server",
            "status",
            "top",
            "verbose"
        ],
        "the runnable `--definition-id` spelling, not the schema's `definition_id`"
    );
    assert_eq!(
        argument(
            &find_node("releases show").expect("the show node"),
            "release_id"
        )["type"],
        json!("string"),
        "this build's schema reports a value-parser as a string (D23)"
    );

    let wikis = find_node("wikis").expect("the wikis node");
    assert_eq!(
        subcommands(&wikis)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        ["ado wikis list", "ado wikis show", "ado wikis pages"]
    );
    assert_eq!(
        subcommands(&find_node("wikis pages").expect("the pages node"))
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado wikis pages list",
            "ado wikis pages show",
            "ado wikis pages create",
            "ado wikis pages update"
        ],
        "the nested leaf the invocations table warns about"
    );

    let cases: [(&str, &[&str]); 4] = [
        ("list", &["json", "org", "pat", "path", "server", "verbose"]),
        ("show", &["json", "org", "pat", "path", "server", "verbose"]),
        (
            "create",
            &["content", "json", "org", "pat", "path", "server", "verbose"],
        ),
        (
            "update",
            &["content", "json", "org", "pat", "path", "server", "verbose"],
        ),
    ];

    for (leaf, expected) in cases {
        let node = find_node(&format!("wikis pages {leaf}")).expect("a page leaf");
        assert_eq!(
            option_names(&node),
            expected,
            "pages {leaf} carries its options and the five globals"
        );
    }

    for leaf in ["show", "create", "update"] {
        let node = find_node(&format!("wikis pages {leaf}")).expect("a page leaf");
        assert_eq!(
            option(&node, "path")["required"],
            json!(true),
            "pages {leaf} marks --path required where the oracle's Map.fetch! is silent (D34)"
        );
    }

    for leaf in ["create", "update"] {
        let node = find_node(&format!("wikis pages {leaf}")).expect("a page leaf");
        assert_eq!(
            option(&node, "content")["required"],
            json!(true),
            "pages {leaf} marks --content required too"
        );
    }
}

/// Task 2's two areas. Both group nodes are already the runnable hyphenated
/// spelling, so the node names are the oracle's; the integer positionals report
/// `type: "string"`/`required: true` where the oracle's schema says
/// `integer`/`false` (D23's two bullets), and the options are the ones the
/// frozen schema declares (`queues list --pool`; `test-coverage show`'s `--json`
/// is this build's global).
#[test]
fn schema_agent_pools_and_test_coverage_nodes_list_every_shipped_subcommand() {
    let agent_pools = find_node("agent-pools").expect("the agent-pools node");

    assert_eq!(
        subcommands(&agent_pools)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado agent-pools list",
            "ado agent-pools show",
            "ado agent-pools queues"
        ]
    );
    assert_eq!(
        subcommands(&find_node("agent-pools queues").expect("the queues node"))
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        ["ado agent-pools queues list"]
    );

    let show = find_node("agent-pools show").expect("the show node");
    assert_eq!(argument(&show, "pool_id")["type"], json!("string"));
    assert_eq!(argument(&show, "pool_id")["required"], json!(true));
    assert_eq!(
        option_names(&show),
        GLOBALS,
        "show declares no option of its own"
    );

    let queues_list = find_node("agent-pools queues list").expect("the queues list node");
    assert_eq!(
        option_names(&queues_list),
        ["json", "org", "pat", "pool", "server", "verbose"],
        "--pool is the module's one option"
    );
    assert_eq!(option(&queues_list, "pool")["type"], json!("string"));
    assert_eq!(
        option(&queues_list, "pool")["doc"],
        json!("Filter by numeric agent pool ID")
    );
    assert_eq!(argument(&queues_list, "project")["required"], json!(true));

    let coverage = find_node("test-coverage").expect("the test-coverage node");
    assert_eq!(
        subcommands(&coverage)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        ["ado test-coverage show"]
    );

    let coverage_show = find_node("test-coverage show").expect("the show node");
    assert_eq!(
        coverage_show["arguments"]
            .as_array()
            .expect("an argument array")
            .iter()
            .map(|argument| argument["name"].as_str().expect("an argument name"))
            .collect::<Vec<_>>(),
        ["project", "build_id"],
        "the module's two positionals, in its declaration order"
    );
    for name in ["project", "build_id"] {
        assert_eq!(
            argument(&coverage_show, name)["required"],
            json!(true),
            "{name}"
        );
    }
    assert_eq!(
        argument(&coverage_show, "build_id")["type"],
        json!("string")
    );
}

/// Task 3's area: the five `connections` leaves, their positionals (the module's
/// `create` takes name/type/url **positionally** — the moduledoc's `--name`
/// spellings are not flags) and the D23 metadata this build reports.
#[test]
fn schema_connections_node_lists_every_shipped_subcommand() {
    let connections = find_node("connections").expect("the connections node");

    assert_eq!(
        subcommands(&connections)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado connections list",
            "ado connections show",
            "ado connections create",
            "ado connections update",
            "ado connections delete"
        ]
    );
    assert_eq!(
        connections["doc"],
        json!(
            "Manage service connections (a.k.a. service endpoints). A service connection stores credentials for external services (Azure subscriptions, GitHub repos, Docker registries, Kubernetes clusters) so pipelines can access them without re-entering secrets."
        ),
        "the oracle's group doc verbatim"
    );

    let list = find_node("connections list").expect("the list node");
    assert_eq!(
        option_names(&list),
        ["json", "org", "pat", "server", "type", "verbose"],
        "--type is the module's one option"
    );
    assert_eq!(option(&list, "type")["type"], json!("string"));
    assert_eq!(argument(&list, "project")["required"], json!(true));

    let create = find_node("connections create").expect("the create node");
    assert_eq!(
        create["arguments"]
            .as_array()
            .expect("an argument array")
            .iter()
            .map(|argument| argument["name"].as_str().expect("an argument name"))
            .collect::<Vec<_>>(),
        ["project", "name", "type", "url"],
        "the oracle's positional order; --name/--type/--url are not flags"
    );
    assert_eq!(
        option_names(&create),
        [
            "access-token",
            "data",
            "description",
            "json",
            "org",
            "pat",
            "ready",
            "scheme",
            "server",
            "verbose"
        ],
        "the five module options; D17's hyphenated spelling, not the keyword name"
    );
    assert_eq!(option(&create, "ready")["type"], json!("boolean"));
    assert_eq!(option(&create, "ready")["default"], json!("false"));

    let update = find_node("connections update").expect("the update node");
    assert_eq!(
        option_names(&update),
        [
            "access-token",
            "data",
            "description",
            "json",
            "name",
            "org",
            "pat",
            "server",
            "url",
            "verbose"
        ]
    );

    let delete = find_node("connections delete").expect("the delete node");
    assert_eq!(
        option_names(&delete),
        ["force", "json", "org", "pat", "server", "verbose"]
    );
    assert_eq!(option(&delete, "force")["type"], json!("boolean"));
}

/// The area's six nodes with the oracle's own texts: `list` carries the one
/// `--search` option, `show` the one positional, and the four writes the
/// `--publisher`/`--name` pair the module requires.
#[test]
fn schema_extensions_node_lists_every_shipped_subcommand() {
    let extensions = find_node("extensions").expect("the extensions node");

    assert_eq!(
        extensions["doc"],
        json!(
            "Manage Azure DevOps Marketplace extensions installed in the organization. Extensions add features like custom widgets, service hooks, and pipeline tasks."
        )
    );
    assert_eq!(
        subcommands(&extensions)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado extensions list",
            "ado extensions show",
            "ado extensions install",
            "ado extensions uninstall",
            "ado extensions enable",
            "ado extensions disable"
        ],
        "the module's declaration order"
    );

    let list = find_node("extensions list").expect("the list node");
    assert_eq!(
        option_names(&list),
        ["json", "org", "pat", "search", "server", "verbose"],
        "--search is the module's one option"
    );
    assert_eq!(option(&list, "search")["type"], json!("string"));
    assert_eq!(
        option(&list, "search")["doc"],
        json!("Filter to extensions whose name contains this string (case-insensitive)")
    );

    let show = find_node("extensions show").expect("the show node");
    assert_eq!(option_names(&show), GLOBALS, "show declares no option");
    assert_eq!(argument(&show, "extension_id")["type"], json!("string"));
    assert_eq!(argument(&show, "extension_id")["required"], json!(true));
    assert_eq!(
        argument(&show, "extension_id")["doc"],
        json!(
            "Extension ID in 'publisher.name' form (e.g. 'mspremier.BuildQualityChecks'). NOT a numeric ID."
        )
    );

    for command in ["install", "uninstall", "enable", "disable"] {
        let node = find_node(&format!("extensions {command}")).expect("a write node");

        assert_eq!(
            option_names(&node),
            [
                "json",
                "name",
                "org",
                "pat",
                "publisher",
                "server",
                "verbose"
            ],
            "{command}"
        );
        assert_eq!(option(&node, "publisher")["required"], json!(true));
        assert_eq!(option(&node, "name")["required"], json!(true));
    }

    assert_eq!(
        option(&find_node("extensions install").expect("install"), "name")["doc"],
        json!(
            "Extension name as listed on the marketplace (e.g. 'BuildQualityChecks', ' octopus-deploy')"
        )
    );
    assert_eq!(
        option(
            &find_node("extensions uninstall").expect("uninstall"),
            "name"
        )["doc"],
        json!("Extension name")
    );
}

/// The two Task 5 areas: `imports` and `banners`, whose group docs and leaves
/// are the frozen modules' own `doc:` strings (`task5/imports-banners-schema.json`).
#[test]
fn schema_imports_node_lists_every_shipped_subcommand() {
    let imports = find_node("imports").expect("the imports node");

    assert_eq!(
        imports["doc"],
        json!(
            "Manage Git repository imports (e.g. GitHub → Azure DevOps migration). Creates a new Azure DevOps repo and populates it with the git history, branches, and tags from a source repository. The new repo is a one-time copy, not a mirror."
        )
    );
    assert_eq!(subcommands(&imports).len(), 3);
    assert_eq!(
        subcommands(&imports)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        ["ado imports list", "ado imports show", "ado imports create"],
        "the module's declaration order"
    );

    let list = find_node("imports list").expect("the list node");
    assert_eq!(
        option_names(&list),
        ["json", "org", "pat", "server", "top", "verbose"],
        "--top is the module's one option"
    );
    assert_eq!(
        option(&list, "top")["type"],
        json!("string"),
        "the oracle types it integer; clap's value-parser reports string (D23)"
    );
    assert_eq!(
        option(&list, "top")["doc"],
        json!("Maximum number of imports to return. Default 50.")
    );
    assert_eq!(
        argument(&list, "project")["doc"],
        json!("Project name or ID")
    );
    assert_eq!(
        argument(&list, "project")["required"],
        json!(true),
        "the oracle says false and enforces it anyway; ours describes the parse tree (D23)"
    );

    let show = find_node("imports show").expect("the show node");
    assert_eq!(option_names(&show), GLOBALS, "show declares no option");
    assert_eq!(argument(&show, "import_id")["type"], json!("string"));
    assert_eq!(
        argument(&show, "import_id")["doc"],
        json!("Import request ID (UUID, returned by `create`)")
    );

    let create = find_node("imports create").expect("the create node");
    assert_eq!(
        option_names(&create),
        [
            "json", "org", "password", "pat", "server", "url", "user", "verbose"
        ],
        "url/user/password, and no others"
    );
    assert_eq!(option(&create, "url")["required"], json!(true));
    assert_eq!(option(&create, "user")["required"], json!(false));
    assert_eq!(option(&create, "password")["required"], json!(false));
    assert_eq!(
        argument(&create, "repo_name")["doc"],
        json!("Name for the new repository (must not already exist in the project)")
    );
}

#[test]
fn schema_banners_node_lists_every_shipped_subcommand() {
    let banners = find_node("banners").expect("the banners node");

    assert_eq!(
        banners["doc"],
        json!(
            "Manage the organization-wide notification banner that appears at the top of the Azure DevOps web UI for every user. Useful for maintenance windows or org-wide announcements."
        )
    );
    assert_eq!(banners["arguments"], json!([]));
    assert_eq!(
        option_names(&banners),
        GLOBALS,
        "the group declares no option"
    );
    assert_eq!(
        subcommands(&banners)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        ["ado banners show", "ado banners set", "ado banners delete"],
        "the module's declaration order"
    );

    let show = find_node("banners show").expect("the show node");
    assert_eq!(option_names(&show), GLOBALS, "show declares no option");
    assert_eq!(show["arguments"], json!([]));

    let set = find_node("banners set").expect("the set node");
    assert_eq!(
        option_names(&set),
        [
            "json", "level", "message", "org", "pat", "server", "type", "verbose"
        ]
    );
    assert_eq!(option(&set, "message")["required"], json!(true));
    assert_eq!(option(&set, "type")["required"], json!(false));
    assert_eq!(option(&set, "level")["required"], json!(false));
    assert_eq!(
        option(&set, "message")["doc"],
        json!(
            "Banner text shown to users. Markdown is not supported; the text is rendered as plain text. Multi-word values do not need quoting (joined until next flag). Use @<file> or - to read from a file/stdin."
        )
    );

    let delete = find_node("banners delete").expect("the delete node");
    assert_eq!(option_names(&delete), GLOBALS, "delete declares no option");
    assert_eq!(delete["arguments"], json!([]));
}

/// The wave's Task 6 area: the module's three leaves and its two repaired
/// filters, whose help text is the only spelling they can be passed under
/// (Ruling 4(a) — the frozen `--build-id`/`--min-last-updated` cannot match).
#[test]
fn schema_test_results_node_lists_every_shipped_subcommand() {
    let test_results = find_node("test-results").expect("the test-results node");

    assert_eq!(
        test_results["doc"],
        json!(
            "Manage Azure DevOps test results. Lists recent test runs, shows individual run details, and publishes results from standard format files (Cobertura XML, JUnit, etc.)."
        )
    );
    assert_eq!(test_results["arguments"], json!([]));
    assert_eq!(
        subcommands(&test_results)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        [
            "ado test-results list",
            "ado test-results show",
            "ado test-results publish"
        ],
        "the module's declaration order"
    );

    let list = find_node("test-results list").expect("the list node");
    assert_eq!(
        option_names(&list),
        [
            "build-id",
            "json",
            "min-last-updated",
            "org",
            "pat",
            "server",
            "top",
            "verbose"
        ],
        "the module's two filters and --top"
    );
    assert_eq!(
        argument(&list, "project")["required"],
        json!(true),
        "CliMate enforces the positional the schema marks false (D23)"
    );
    assert_eq!(
        option(&list, "top")["doc"],
        json!("Max runs to return (default: 50)")
    );
    assert_eq!(
        option(&list, "build-id")["doc"],
        json!("Filter by build ID")
    );
    assert_eq!(
        option(&list, "min-last-updated")["doc"],
        json!("ISO date filter for last updated")
    );

    let show = find_node("test-results show").expect("the show node");
    assert_eq!(option_names(&show), GLOBALS, "show declares no option");
    assert_eq!(
        show["arguments"]
            .as_array()
            .expect("an argument array")
            .iter()
            .map(|argument| argument["name"].as_str().expect("an argument name"))
            .collect::<Vec<_>>(),
        ["project", "run_id"],
        "the module's two positionals, in its declaration order"
    );
    assert_eq!(argument(&show, "run_id")["type"], json!("string"));
    assert_eq!(argument(&show, "run_id")["required"], json!(true));

    let publish = find_node("test-results publish").expect("the publish node");
    assert_eq!(
        option_names(&publish),
        [
            "build-id", "file", "json", "name", "org", "pat", "server", "verbose"
        ]
    );
    assert_eq!(option(&publish, "name")["required"], json!(true));
    assert_eq!(option(&publish, "file")["required"], json!(true));
    assert_eq!(
        option(&publish, "build-id")["doc"],
        json!(
            "Numeric build ID to attach results to. If omitted, results are published as a standalone run (not linked to any build)."
        )
    );
    assert_eq!(argument(&publish, "project")["required"], json!(true));
}

/// The wave's Task 7 area: one root node with two leaves, where the frozen tree
/// lists the same node twice (D14 — the duplicate collapses here). The safety
/// flag's schema name is the hyphenated flag argv accepts (D17's class, the
/// `--yes_this_mutates_secret_read` spelling is `invalid option` on the oracle).
#[test]
fn schema_security_node_lists_every_shipped_subcommand() {
    let security = find_node("security").expect("the security node");

    assert_eq!(
        security["doc"],
        json!(
            "Manage Azure DevOps security permissions on the caller identity. Currently supports toggling the Library 'ViewSecrets' bit for the calling user only. Use this as a workaround when the auto-elevation in 'ado pipelines secure_files download' is unavailable."
        )
    );
    assert_eq!(security["arguments"], json!([]));
    assert_eq!(
        subcommands(&security)
            .iter()
            .map(|sub| sub["name"].as_str().expect("a subcommand name"))
            .collect::<Vec<_>>(),
        ["ado security grant", "ado security revoke"],
        "the module's declaration order"
    );

    let grant = find_node("security grant").expect("the grant node");
    assert_eq!(
        option_names(&grant),
        [
            "json",
            "org",
            "pat",
            "permission",
            "server",
            "verbose",
            "yes-this-mutates-secret-read"
        ],
        "the guard and the permission, under the spelling argv accepts"
    );
    assert_eq!(option(&grant, "permission")["type"], json!("string"));
    assert_eq!(
        option(&grant, "permission")["doc"],
        json!("Permission name (currently only ViewSecrets is supported)")
    );
    assert_eq!(
        option(&grant, "yes-this-mutates-secret-read")["type"],
        json!("boolean"),
        "the guard is a flag, not a value"
    );
    assert_eq!(
        option(&grant, "yes-this-mutates-secret-read")["default"],
        json!("false"),
        "the frozen default: absent is a refusal (the schema stringifies it)"
    );
    assert_eq!(
        option(&grant, "permission")["default"],
        json!(""),
        "the module's own default is applied in code, not declared to clap (the banners precedent)"
    );
    assert_eq!(
        argument(&grant, "project_name_or_id")["required"],
        json!(true),
        "CliMate enforces the positional the schema marks false (D23)"
    );

    let revoke = find_node("security revoke").expect("the revoke node");
    assert_eq!(option_names(&revoke), option_names(&grant));
    assert_eq!(
        argument(&revoke, "project_name_or_id")["required"],
        json!(true)
    );
}
