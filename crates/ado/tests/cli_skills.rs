//! End-to-end tests for `ado skills`: the five bespoke envelopes and the sixth
//! `list <path>` shape captured from the frozen oracle, the embedded assets'
//! frontmatter, the search ranking, the read path's frontmatter stripping, and the
//! install layout.
//!
//! Every test owns its environment: a `TempHome` (all per-user install targets
//! resolve under it) and a working directory inside the same tree, so an install
//! test can assert that nothing lands outside the temp tree. `skills` makes no HTTP
//! request, so no mock is started and `ADO_ORG`/`ADO_PAT`/`ADO_SERVER` are unset.
//! Stdin is null in every spawn.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use ado_testkit::{TempHome, ado_cmd, stderr_of, stdout_of};
use serde_json::{Value, json};

/// Runs `ado <args>` with the temp home and a fresh working directory under it.
fn run(home: &TempHome, args: &[&str]) -> Output {
    run_from(home, &home.path().join("run"), args)
}

fn run_from(home: &TempHome, cwd: &Path, args: &[&str]) -> Output {
    fs::create_dir_all(cwd).expect("the run directory");

    command(home, cwd, args)
        .stdin(Stdio::null())
        .output()
        .expect("run ado")
}

fn command(home: &TempHome, cwd: &Path, args: &[&str]) -> Command {
    let mut command = ado_cmd();
    home.apply(&mut command);
    command
        .env_remove("ADO_ORG")
        .env_remove("ADO_PAT")
        .env_remove("ADO_SERVER")
        .current_dir(cwd)
        .args(args);
    command
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout:\n{}\nstderr:\n{}",
        stdout_of(output),
        stderr_of(output)
    );
}

fn json(output: &Output) -> Value {
    serde_json::from_str(&stdout_of(output)).unwrap_or_else(|error| {
        panic!(
            "stdout is not one JSON document: {error}\nstdout:\n{}\nstderr:\n{}",
            stdout_of(output),
            stderr_of(output)
        )
    })
}

/// Every file under `root`, as paths relative to it and sorted.
fn files_under(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect(root, root, &mut files);
    files.sort();
    files
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| panic!("read {dir:?}: {error}"));

    for entry in entries {
        let entry = entry.expect("a directory entry");
        let path = entry.path();

        if path.is_dir() {
            collect(root, &path, files);
        } else {
            files.push(
                path.strip_prefix(root)
                    .expect("a path under the root")
                    .display()
                    .to_string(),
            );
        }
    }
}

fn skill_path(target: &Path, skill: &str) -> PathBuf {
    target.join(skill).join("SKILL.md")
}

// ── list ─────────────────────────────────────────────────────────────────

#[test]
fn list_json_is_the_captured_skills_document() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "list", "--json"]);

    assert_exit(&output, 0);
    let document = json(&output);
    assert_eq!(document["ok"], json!(true));
    assert_eq!(document["count"], json!(3));

    let skills = document["skills"].as_array().expect("the skills array");
    assert_eq!(skills.len(), 3);
    assert_eq!(skills[0]["name"], json!("ado-auth"));
    assert_eq!(skills[0]["version"], json!("0.6.0"));
    assert_eq!(skills[0]["command_count"], json!(8));
    assert_eq!(
        skills[0]["description"],
        json!(
            "Authenticate ado: PAT (CI-friendly), browser OAuth (AAD + MSA), device code (headless), env vars, self-hosted server"
        )
    );
    assert_eq!(skills[1]["name"], json!("ado-ci"));
    assert_eq!(skills[1]["command_count"], json!(15));
    assert_eq!(skills[2]["name"], json!("ado-cli"));
    assert_eq!(skills[2]["command_count"], json!(87));
    assert!(stderr_of(&output).is_empty());
}

#[test]
fn list_human_is_the_captured_blocks() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "list"]);

    assert_exit(&output, 0);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\n",
            "  ado-auth\n",
            "    Authenticate ado: PAT (CI-friendly), browser OAuth (AAD + MSA), device code (headless), env vars, self-hosted server\n",
            "    version: 0.6.0  ·  commands: 8\n",
            "    run: ado skills describe ado-auth     # see commands\n",
            "\n",
            "  ado-ci\n",
            "    Use ado in CI/CD: auth setup, pipeline triggers, PR automation, package publishing, work item creation on failure\n",
            "    version: 0.6.0  ·  commands: 15\n",
            "    run: ado skills describe ado-ci     # see commands\n",
            "\n",
            "  ado-cli\n",
            "    Complete command reference for all 24 Azure DevOps service areas (projects, repos, workitems, pipelines, prs, releases, packages, and more)\n",
            "    version: 0.6.0  ·  commands: 87\n",
            "    run: ado skills describe ado-cli     # see commands\n",
            "\n",
            "\n",
        )
    );
    assert!(stderr_of(&output).is_empty());
}

#[test]
fn list_a_skill_shows_one_level_like_ls() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "list", "ado-cli"]);

    assert_exit(&output, 0);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\n",
            "  ado-cli/\n",
            "    ado-cli/SKILL.md\n",
            "    ado-cli/references/admin.md\n",
            "\n",
        ),
        "the dedup keeps one entry per first path component, in map order (admin.md)"
    );

    let output = run(&home, &["skills", "list", "ado-cli", "--json"]);

    assert_exit(&output, 0);
    assert_eq!(
        json(&output),
        json!({
            "ok": true,
            "dir": "ado-cli",
            "entries": [
                {"path": "ado-cli/SKILL.md", "is_dir": false},
                {"path": "ado-cli/references/admin.md", "is_dir": false},
            ],
        })
    );
}

#[test]
fn list_a_reference_directory_lists_every_file() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "list", "ado-cli/references", "--json"]);

    assert_exit(&output, 0);
    let document = json(&output);
    assert_eq!(document["dir"], json!("ado-cli/references"));
    assert_eq!(
        document["entries"],
        json!([
            {"path": "ado-cli/references/admin.md", "is_dir": false},
            {"path": "ado-cli/references/artifacts.md", "is_dir": false},
            {"path": "ado-cli/references/pipelines.md", "is_dir": false},
            {"path": "ado-cli/references/projects-teams-users.md", "is_dir": false},
            {"path": "ado-cli/references/prs.md", "is_dir": false},
            {"path": "ado-cli/references/repos.md", "is_dir": false},
            {"path": "ado-cli/references/workitems.md", "is_dir": false},
        ])
    );
}

#[test]
fn list_an_unmatched_path_is_a_heading_and_nothing_else() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "list", "ado-cli/references/admin.md"]);

    assert_exit(&output, 0);
    assert_eq!(
        stdout_of(&output),
        "\n  ado-cli/references/admin.md/\n\n",
        "the frozen treats it as a directory prefix, not a file"
    );
}

#[test]
fn list_an_unknown_skill_is_the_modules_sentence() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "list", "no-such"]);

    assert_exit(&output, 1);
    assert!(stdout_of(&output).is_empty());
    assert_eq!(
        stderr_of(&output),
        "[Not found] unknown skill \"no-such\". Run 'ado skills list' to see available skills\n"
    );

    let output = run(&home, &["skills", "list", "no-such", "--json"]);

    assert_exit(&output, 1);
    assert_eq!(
        json(&output),
        json!({
            "ok": false,
            "error": {
                "code": "not_found",
                "message": "unknown skill \"no-such\". Run 'ado skills list' to see available skills",
            },
        })
    );
}

// ── describe ─────────────────────────────────────────────────────────────

#[test]
fn describe_json_is_the_captured_result_document() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "describe", "ado-cli", "--json"]);

    assert_exit(&output, 0);
    let document = json(&output);
    assert_eq!(document["ok"], json!(true));

    let result = &document["result"];
    assert_eq!(result["name"], json!("ado-cli"));
    assert_eq!(result["version"], json!("0.6.0"));

    let commands = result["commands"].as_array().expect("the commands array");
    assert_eq!(commands.len(), 87, "the frontmatter's command list");
    assert_eq!(commands[0], json!("ado --version"));
    assert_eq!(
        commands[86],
        json!("ado test-coverage show PROJECT BUILD_ID")
    );
    assert!(
        !commands.iter().any(|command| command
            .as_str()
            .expect("a command string")
            .contains("ADO_SERVER")),
        "a command line containing ':' is claimed by the key branch and never joins the list"
    );
}

#[test]
fn describe_human_prints_the_command_index() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "describe", "ado-cli"]);

    assert_exit(&output, 0);
    let stdout = stdout_of(&output);
    assert!(stdout.starts_with(concat!(
        "\n",
        "  ado-cli\n",
        "    Complete command reference for all 24 Azure DevOps service areas (projects, repos, workitems, pipelines, prs, releases, packages, and more)\n",
        "    version: 0.6.0\n",
        "    commands: 87\n",
        "\n",
        "    Commands covered by this skill:\n",
        "      • ado --version\n",
    )));
    assert!(stdout.ends_with(concat!(
        "      • ado test-coverage show PROJECT BUILD_ID\n",
        "\n",
        "    Run `ado skills read ado-cli` to load the full body.\n",
        "\n",
    )));
    assert_eq!(stdout.matches("      • ").count(), 87);
}

#[test]
fn describe_an_unknown_skill_is_a_loud_error_on_both_paths() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "describe", "no-such"]);

    assert_exit(&output, 1);
    assert!(stdout_of(&output).is_empty());
    assert_eq!(
        stderr_of(&output),
        "[Not found] unknown skill \"no-such\". Run 'ado skills list' to see available skills\n"
    );

    let output = run(&home, &["skills", "describe", "no-such", "--json"]);

    assert_exit(&output, 1);
    assert_eq!(json(&output)["error"]["code"], json!("not_found"));
}

// ── read ─────────────────────────────────────────────────────────────────

#[test]
fn read_strips_the_frontmatter() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "read", "ado-cli"]);

    assert_exit(&output, 0);
    let stdout = stdout_of(&output);
    assert!(
        stdout.starts_with("# ado — Azure DevOps CLI\n"),
        "the body begins after the closing fence and the blank lines: {stdout:.80}"
    );
    assert!(!stdout.contains("name: ado-cli"));
    assert!(!stdout.trim_end().ends_with("---"));
}

#[test]
fn read_json_carries_the_metadata_the_human_path_strips() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "read", "ado-cli", "--json"]);

    assert_exit(&output, 0);
    let document = json(&output);
    assert_eq!(document["ok"], json!(true));
    assert_eq!(document["skill"], json!("ado-cli"));
    assert_eq!(document["path"], json!("SKILL.md"));
    assert!(
        document["content"]
            .as_str()
            .expect("the content")
            .starts_with("# ado — Azure DevOps CLI\n")
    );
    assert_eq!(document["metadata"]["version"], json!("0.6.0"));
    assert_eq!(
        document["metadata"]["commands"]
            .as_array()
            .expect("the commands")
            .len(),
        87
    );
    assert_eq!(
        document["metadata"]["description"],
        json!(
            "Complete command reference for all 24 Azure DevOps service areas (projects, repos, workitems, pipelines, prs, releases, packages, and more)"
        )
    );
}

#[test]
fn read_a_reference_file_is_verbatim_and_keeps_its_relative_path() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "read", "ado-cli/references/prs.md"]);

    assert_exit(&output, 0);
    assert!(stdout_of(&output).starts_with("# Pull Requests\n"));

    let output = run(
        &home,
        &["skills", "read", "ado-cli/references/prs.md", "--json"],
    );

    assert_exit(&output, 0);
    let document = json(&output);
    assert_eq!(document["skill"], json!("ado-cli"));
    assert_eq!(document["path"], json!("references/prs.md"));
    assert!(
        document["content"]
            .as_str()
            .expect("the content")
            .starts_with("# Pull Requests\n")
    );
}

#[test]
fn read_a_skill_md_as_a_reference_keeps_the_frontmatter() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "read", "ado-cli/SKILL.md"]);

    assert_exit(&output, 0);
    assert!(
        stdout_of(&output).starts_with("---\nname: ado-cli\n"),
        "only the top-level read strips the fence"
    );
}

#[test]
fn read_reports_the_modules_two_refusals() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "read", "no-such"]);
    assert_exit(&output, 1);
    assert!(stdout_of(&output).is_empty());
    assert_eq!(
        stderr_of(&output),
        "[Not found] unknown skill \"no-such\". Run 'ado skills list' to see available skills\n"
    );

    let output = run(&home, &["skills", "read", "ado-cli/nope.md"]);
    assert_exit(&output, 1);
    assert_eq!(
        stderr_of(&output),
        "[Not found] file not found: ado-cli/nope.md\n"
    );

    let output = run(&home, &["skills", "read", "ado-cli/references"]);
    assert_exit(&output, 1);
    assert_eq!(
        stderr_of(&output),
        "[Not found] file not found: ado-cli/references\n"
    );

    let output = run(&home, &["skills", "read", "no-such", "--json"]);
    assert_exit(&output, 1);
    assert_eq!(json(&output)["error"]["code"], json!("not_found"));
}

#[test]
fn read_a_space_separated_target_is_not_supported() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "read", "ado-cli references/prs.md"]);

    assert_exit(&output, 1);
    assert_eq!(
        stderr_of(&output),
        "[Not found] unknown skill \"ado-cli references\". Run 'ado skills list' to see available skills\n",
        "the help's \"skillname path\" prose names a form the splitter cannot produce"
    );
}

// ── search ───────────────────────────────────────────────────────────────

#[test]
fn search_ranks_name_then_reversed_commands_then_description() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "search", "ado", "--json"]);

    assert_exit(&output, 0);
    let document = json(&output);
    assert_eq!(document["ok"], json!(true));
    assert_eq!(document["query"], json!("ado"));
    assert_eq!(document["count"], json!(115));

    let results = document["results"].as_array().expect("the results");
    assert_eq!(results[0]["match_type"], json!("name"));
    assert_eq!(results[0]["skill"], json!("ado-auth"));
    assert!(
        results.iter().all(|result| result["context"] == json!("")),
        "the frozen struct carries `context` and nothing ever sets it"
    );

    // ado-auth's seven matching commands come back in reverse command order,
    // after the name hit and before the description hit.
    let ado_auth = results
        .iter()
        .filter(|result| result["skill"] == json!("ado-auth"))
        .map(|result| result["match_type"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        ado_auth,
        [
            json!("name"),
            json!("command"),
            json!("command"),
            json!("command"),
            json!("command"),
            json!("command"),
            json!("command"),
            json!("command"),
            json!("command"),
            json!("description"),
        ]
    );
    let matched = results
        .iter()
        .filter(|result| result["skill"] == json!("ado-auth"))
        .map(|result| result["matched"].clone())
        .collect::<Vec<_>>();
    assert_eq!(matched[1], json!("export ADO_ORG=org ADO_PAT=token"));
    assert_eq!(matched[8], json!("ado login"));
}

/// The group order is the frozen `Enum.group_by/2` map's key order (name-sorted),
/// **not** first-appearance order: `search ci`'s first hit is `ado-ci`'s name, but
/// its first group is `ado-auth` (a description-only match).
#[test]
fn search_human_groups_in_name_order_not_first_appearance() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "search", "ci"]);

    assert_exit(&output, 0);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\n",
            "  Matches for \"ci\" (7):\n",
            "\n",
            "  ado-auth\n",
            "    [command] ado login --method pat --org ORG --pat TOKEN   # explicit form (same result)\n",
            "    [description] Authenticate ado: PAT (CI-friendly), browser OAuth (AAD + MSA), device code (headless), env vars, self-hosted server\n",
            "\n",
            "  ado-ci\n",
            "    [name] ado-ci\n",
            "    [command] ado ci watch PROJECT BUILD_ID\n",
            "    [description] Use ado in CI/CD: auth setup, pipeline triggers, PR automation, package publishing, work item creation on failure\n",
            "\n",
            "  ado-cli\n",
            "    [command] ado ci watch PROJECT BUILD_ID\n",
            "    [command] ado branch-policies list PROJECT REPO\n",
            "\n",
            "\n",
        )
    );
}

#[test]
fn search_is_a_case_insensitive_substring_of_the_whole_query() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "search", "create PR", "--json"]);

    assert_exit(&output, 0);
    assert_eq!(json(&output)["count"], json!(9));

    let output = run(&home, &["skills", "search", "CREATE pr", "--json"]);

    assert_exit(&output, 0);
    assert_eq!(
        json(&output)["count"],
        json!(9),
        "the query is downcased as a whole, not split into words"
    );

    let output = run(&home, &["skills", "search", "zzz"]);
    assert_exit(&output, 0);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\n",
            "  No matches for \"zzz\".\n",
            "  Try `ado skills list` to see all available skills.\n",
            "\n",
        )
    );
}

#[test]
fn search_an_empty_query_matches_every_indexed_entry() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "search", "", "--json"]);

    assert_exit(&output, 0);
    assert_eq!(
        json(&output)["count"],
        json!(116),
        "3 names + 3 descriptions + 110 commands"
    );
}

#[test]
fn search_human_groups_by_skill() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "search", "ado-cli"]);

    assert_exit(&output, 0);
    assert_eq!(
        stdout_of(&output),
        concat!(
            "\n",
            "  Matches for \"ado-cli\" (3):\n",
            "\n",
            "  ado-cli\n",
            "    [name] ado-cli\n",
            "    [command] ado skills read ado-cli\n",
            "    [command] ado skills describe ado-cli\n",
            "\n",
            "\n",
        )
    );
}

// ── install ──────────────────────────────────────────────────────────────

#[test]
fn install_writes_only_skill_md_under_the_resolved_target() {
    let home = TempHome::new();
    let target = home.path().join(".pi").join("agent").join("skills");

    let output = run(&home, &["skills", "install", "--target", "pi", "--json"]);

    assert_exit(&output, 0);
    let document = json(&output);
    assert_eq!(document["ok"], json!(true));
    assert_eq!(
        document["result"]["targets"],
        json!([{"name": "pi", "path": target.display().to_string()}])
    );
    assert_eq!(
        document["result"]["installed"]
            .as_array()
            .expect("the installed rows")
            .len(),
        3
    );
    assert_eq!(document["result"]["skipped"], json!([]));
    assert_eq!(document["result"]["errors"], json!([]));

    assert_eq!(
        files_under(home.path()),
        [
            ".pi/agent/skills/ado-auth/SKILL.md",
            ".pi/agent/skills/ado-ci/SKILL.md",
            ".pi/agent/skills/ado-cli/SKILL.md",
        ],
        "SKILL.md only — the reference files the module doc promises are never copied"
    );
    let installed = fs::read_to_string(skill_path(&target, "ado-auth")).expect("the written skill");
    assert!(installed.starts_with("---\nname: ado-auth\n"));
    assert!(installed.contains("# Authentication"));
}

#[test]
fn install_skips_an_existing_file_and_force_overwrites() {
    let home = TempHome::new();
    let target = home.path().join(".pi").join("agent").join("skills");

    let first = run(&home, &["skills", "install", "--target", "pi", "--json"]);
    assert_exit(&first, 0);
    assert_eq!(
        json(&first)["result"]["installed"]
            .as_array()
            .expect("the installed rows")
            .len(),
        3
    );

    let second = run(&home, &["skills", "install", "--target", "pi", "--json"]);
    assert_exit(&second, 0);
    let document = json(&second);
    assert_eq!(document["result"]["installed"], json!([]));
    assert_eq!(
        document["result"]["skipped"]
            .as_array()
            .expect("the skipped rows")
            .len(),
        3
    );
    assert_eq!(
        document["result"]["skipped"][0]["path"],
        json!(skill_path(&target, "ado-auth").display().to_string())
    );

    fs::write(skill_path(&target, "ado-auth"), "stale").expect("clobber the installed skill");
    let third = run(
        &home,
        &["skills", "install", "--target", "pi", "--force", "--json"],
    );
    assert_exit(&third, 0);
    assert_eq!(
        json(&third)["result"]["installed"]
            .as_array()
            .expect("the installed rows")
            .len(),
        3
    );
    assert!(
        fs::read_to_string(skill_path(&target, "ado-auth"))
            .expect("the restored skill")
            .starts_with("---\nname: ado-auth\n")
    );
}

#[test]
fn install_reports_an_unknown_skill_as_an_error_row_and_exits_zero() {
    let home = TempHome::new();
    let target = home.path().join(".pi").join("agent").join("skills");

    let output = run(
        &home,
        &[
            "skills", "install", "--target", "pi", "--skill", "no-such", "--json",
        ],
    );

    assert_exit(&output, 0);
    let document = json(&output);
    assert_eq!(document["result"]["installed"], json!([]));
    assert_eq!(document["result"]["skipped"], json!([]));
    assert_eq!(
        document["result"]["errors"],
        json!([{
            "target": "pi",
            "skill": "no-such",
            "path": skill_path(&target, "no-such").display().to_string(),
        }]),
        "the JSON error row drops the message the human form prints"
    );
    assert!(
        target.join("no-such").is_dir(),
        "the skill directory is created before the read fails"
    );
}

#[test]
fn install_all_resolves_every_per_user_target_in_map_order() {
    let home = TempHome::new();

    let output = run(&home, &["skills", "install", "--target", "all", "--json"]);

    assert_exit(&output, 0);
    let document = json(&output);
    let names = document["result"]["targets"]
        .as_array()
        .expect("the targets")
        .iter()
        .map(|target| target["name"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            json!("claude"),
            json!("codex"),
            json!("cursor"),
            json!("pi")
        ],
        "the frozen iterates its target map in term order, not declaration order"
    );
    assert_eq!(
        document["result"]["targets"][3]["path"],
        json!(
            home.path()
                .join(".pi")
                .join("agent")
                .join("skills")
                .display()
                .to_string()
        )
    );
    assert_eq!(
        files_under(home.path()).len(),
        12,
        "four targets, three skills each, SKILL.md only"
    );
}

#[test]
fn install_copilot_writes_the_repo_layout_and_requires_an_existing_repo() {
    let home = TempHome::new();
    let repo = home.path().join("repo");
    fs::create_dir_all(&repo).expect("the repo directory");

    let output = run(
        &home,
        &[
            "skills",
            "install",
            "--target",
            "copilot",
            "--repo",
            repo.to_str().expect("the repo path"),
            "--json",
        ],
    );

    assert_exit(&output, 0);
    assert_eq!(
        json(&output)["result"]["targets"][0]["path"],
        json!(repo.join(".github").join("ado-cli").display().to_string())
    );
    assert!(
        repo.join(".github")
            .join("ado-cli")
            .join("ado-auth")
            .is_dir()
    );

    let missing = home.path().join("nope");
    let output = run(
        &home,
        &[
            "skills",
            "install",
            "--target",
            "copilot",
            "--repo",
            missing.to_str().expect("the missing path"),
            "--json",
        ],
    );

    assert_exit(&output, 1);
    assert_eq!(
        json(&output),
        json!({
            "ok": false,
            "error": {
                "code": "validation_error",
                "message": format!(
                    "could not resolve --target=copilot: --repo={} does not exist or is not a directory",
                    missing.display()
                ),
            },
        })
    );

    let output = run(
        &home,
        &[
            "skills",
            "install",
            "--target",
            "copilot",
            "--repo",
            missing.to_str().expect("the missing path"),
        ],
    );

    assert_exit(&output, 1);
    assert!(stdout_of(&output).is_empty());
    assert_eq!(
        stderr_of(&output),
        format!(
            "[Validation error] could not resolve --target=copilot: --repo={} does not exist or is not a directory\n",
            missing.display()
        )
    );
}

#[test]
fn install_copilot_defaults_to_the_working_directory_without_checking_for_a_repo() {
    let home = TempHome::new();
    let cwd = home.path().join("not-a-repo");
    fs::create_dir_all(&cwd).expect("the working directory");

    let output = run_from(
        &home,
        &cwd,
        &["skills", "install", "--target", "copilot", "--json"],
    );

    assert_exit(&output, 0);
    assert_eq!(
        json(&output)["result"]["targets"][0]["path"],
        json!(
            cwd.canonicalize()
                .expect("the physical cwd")
                .join(".github")
                .join("ado-cli")
                .display()
                .to_string()
        ),
        "the help's 'cwd must be a git repo' is prose; the code never checks"
    );
}

#[test]
fn install_an_unknown_target_is_a_custom_path_expanded_from_home_or_cwd() {
    let home = TempHome::new();

    let output = run(
        &home,
        &["skills", "install", "--target", "~/custom", "--json"],
    );

    assert_exit(&output, 0);
    assert_eq!(
        json(&output)["result"]["targets"][0]["path"],
        json!(home.path().join("custom").display().to_string())
    );

    let relative = run_from(
        &home,
        &home.path().join("run"),
        &["skills", "install", "--target", "custom", "--json"],
    );

    assert_exit(&relative, 0);
    assert_eq!(
        json(&relative)["result"]["targets"][0]["path"],
        json!(
            home.path()
                .join("run")
                .canonicalize()
                .expect("the physical cwd")
                .join("custom")
                .display()
                .to_string()
        )
    );
}

#[test]
fn install_human_names_the_targets_and_omits_the_note_for_copilot() {
    let home = TempHome::new();
    let pi = home.path().join(".pi").join("agent").join("skills");

    let output = run(&home, &["skills", "install", "--target", "pi"]);

    assert_exit(&output, 0);
    assert_eq!(
        stdout_of(&output),
        format!(
            concat!(
                "\n",
                "  Installing 3 skills to 1 target(s):\n",
                "    - pi: {}\n",
                "\n",
                "  Note: copilot installs per-repo (to <repo>/.github/ado-cli/).\n",
                "        Run from inside your repo: ado skills install --target copilot\n",
                "\n",
                "  Installed: 3\n",
                "  Skipped:   0\n",
                "  Errors:    0\n",
                "\n",
            ),
            pi.display()
        )
    );

    let repo = home.path().join("repo");
    fs::create_dir_all(&repo).expect("the repo directory");
    let copilot = run_from(&home, &repo, &["skills", "install", "--target", "copilot"]);

    assert_exit(&copilot, 0);
    let stdout = stdout_of(&copilot);
    assert!(!stdout.contains("Note: copilot"), "{stdout}");
    assert!(stdout.contains("    - copilot: "), "{stdout}");
}

#[test]
fn install_reports_a_failed_skill_in_the_human_summary() {
    let home = TempHome::new();
    let pi = home.path().join(".pi").join("agent").join("skills");

    let output = run(
        &home,
        &["skills", "install", "--target", "pi", "--skill", "no-such"],
    );

    assert_exit(&output, 0);
    assert_eq!(
        stdout_of(&output),
        format!(
            concat!(
                "\n",
                "  Installing 3 skills to 1 target(s):\n",
                "    - pi: {}\n",
                "\n",
                "  Note: copilot installs per-repo (to <repo>/.github/ado-cli/).\n",
                "        Run from inside your repo: ado skills install --target copilot\n",
                "\n",
                "  Installed: 0\n",
                "  Skipped:   0\n",
                "  Errors:    1\n",
                "\n",
                "    xx  pi/no-such: skill not embedded: unknown skill \"no-such\". Run 'ado skills list' to see available skills\n",
                "        {}/no-such/SKILL.md\n",
                "\n",
            ),
            pi.display(),
            pi.display()
        )
    );
}

// ── usage errors ─────────────────────────────────────────────────────────

#[test]
fn every_leaf_refuses_a_missing_argument() {
    let home = TempHome::new();

    for (args, expected) in [
        (vec!["skills"], "missing sub-command"),
        (
            vec!["skills", "describe"],
            "required arguments were not provided",
        ),
        (
            vec!["skills", "read"],
            "required arguments were not provided",
        ),
        (
            vec!["skills", "search"],
            "required arguments were not provided",
        ),
        (
            vec!["skills", "list", "ado-cli", "extra"],
            "unexpected argument 'extra'",
        ),
        (
            vec!["skills", "read", "ado-cli", "references/prs.md"],
            "unexpected argument 'references/prs.md'",
        ),
        (
            vec!["skills", "install", "--target"],
            "a value is required for '--target <TARGET>'",
        ),
    ] {
        let output = run(&home, &args);

        assert_exit(&output, 1);
        assert!(
            stderr_of(&output).contains(expected),
            "{args:?}: stderr:\n{}",
            stderr_of(&output)
        );
        assert!(stdout_of(&output).is_empty(), "{args:?} writes no envelope");
    }
}
