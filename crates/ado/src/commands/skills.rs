//! `ado skills` — the whole of `lib/ado_cli/cli/skills.ex`: the embedded agent
//! surface. Five commands, five bespoke envelopes (the `json` option is local to
//! each leaf in the frozen schema), and no HTTP at all: the only side effect is
//! `install`'s filesystem write.
//!
//! The captures decide the renders:
//!
//!   * `list` without a path answers `{ok, count, skills}`; **with** a path it
//!     answers a sixth shape, `{ok, dir, entries}`, an `ls`-style one-level view;
//!   * `describe` answers `{ok, result}`; `read` answers
//!     `{ok, skill, path, content, metadata}` with the frontmatter stripped from
//!     the top-level content and kept (raw) for a reference file; `search`
//!     answers `{ok, query, count, results}`, whose hits carry the always-empty
//!     `context` field the frozen struct never sets;
//!   * `install` answers `{ok, result:{targets, installed, skipped, errors}}`,
//!     whose rows are `{target, skill, path}` — the error rows drop the message
//!     the human summary prints — and an unresolved target is the frozen's own
//!     `validation_error` document;
//!   * every unknown skill or missing file is a **loud** refusal (`not_found`),
//!     never a human-shaped success (the frozen prints `xx  …` on stdout and no
//!     envelope; D4's class).
//!
//! Two frozen behaviours are carried rather than repaired, and named in the
//! record: only `SKILL.md` is ever installed (the module doc's promise to copy
//! reference files is prose), and an unknown `--skill` is a per-skill `errors`
//! row with exit 0, not a command failure.

use std::collections::BTreeMap;

use ado_core::error::AdoError;
use ado_skills::install::{self, InstallStatus};
use ado_skills::{self, SkillError};
use serde_json::{Value, json};

use crate::context::Context;
use crate::output::Report;

/// The captured `install` options.
pub struct InstallOptions<'a> {
    pub target: &'a str,
    pub repo: Option<&'a str>,
    pub skill: Option<&'a str>,
    pub force: bool,
}

/// The frozen `--target` default: every per-user target, copilot excluded.
pub const DEFAULT_TARGET: &str = "all";

/// `ado skills list [PATH]`: every skill's summary, or one skill's one-level file
/// listing.
pub fn list(context: &Context, path: Option<&str>) -> Result<Report, AdoError> {
    match path {
        None => Ok(list_all(context)),
        Some(path) => {
            let listing = ado_skills::list_path(path).map_err(refusal)?;

            Ok(context.json_or_report(
                json!({
                    "ok": true,
                    "dir": listing.dir,
                    "entries": listing
                        .entries
                        .iter()
                        .map(|entry| json!({"path": entry.path, "is_dir": entry.is_dir}))
                        .collect::<Vec<_>>(),
                }),
                || Report::Text(listing_text(&listing)),
            ))
        }
    }
}

/// `ado skills describe NAME`: the frontmatter and the command index, no body.
pub fn describe(context: &Context, name: &str) -> Result<Report, AdoError> {
    let described = ado_skills::describe(name).map_err(refusal)?;

    Ok(context.json_or_report(
        json!({
            "ok": true,
            "result": {
                "name": described.name,
                "description": described.description,
                "version": described.version,
                "commands": described.commands,
            },
        }),
        || Report::Text(describe_text(&described)),
    ))
}

/// `ado skills read TARGET`: `SKILL.md` (frontmatter stripped) or a file under the
/// skill (verbatim).
pub fn read(context: &Context, target: &str) -> Result<Report, AdoError> {
    let (name, rest) = ado_skills::split_arg(target);

    if rest.is_empty() {
        let content = ado_skills::read_skill(name).map_err(refusal)?;

        Ok(read_report(
            context,
            name,
            "SKILL.md",
            ado_skills::frontmatter::strip_frontmatter(content),
        ))
    } else {
        let content = ado_skills::read_file(name, rest).map_err(refusal)?;

        Ok(read_report(context, name, rest, content.to_owned()))
    }
}

/// `ado skills search QUERY`: the ranked hits, name → command → description.
pub fn search(context: &Context, query: &str) -> Result<Report, AdoError> {
    let hits = ado_skills::search(query);

    Ok(context.json_or_report(
        json!({
            "ok": true,
            "query": query,
            "count": hits.len(),
            "results": hits
                .iter()
                .map(|hit| json!({
                    "skill": hit.skill,
                    "match_type": hit.match_type,
                    "matched": hit.matched,
                    "context": hit.context,
                }))
                .collect::<Vec<_>>(),
        }),
        || Report::Text(search_text(query, &hits)),
    ))
}

/// `ado skills install [--target TARGET --repo REPO --skill NAME --force]`.
pub fn install(context: &Context, options: InstallOptions<'_>) -> Result<Report, AdoError> {
    let home = dirs::home_dir().ok_or_else(|| {
        AdoError::validation("could not resolve the user home directory (is HOME set?)")
    })?;
    let cwd = std::env::current_dir().map_err(|error| {
        AdoError::validation(format!("could not resolve the working directory: {error}"))
    })?;
    let targets = install::targets(options.target, options.repo, &home, &cwd).map_err(|error| {
        AdoError::validation(format!(
            "could not resolve --target={}: {}",
            options.target,
            error.message()
        ))
    })?;
    let names = match options.skill {
        Some(skill) => vec![skill],
        None => ado_skills::names(),
    };
    let rows = install::install(&targets, &names, options.force);

    Ok(context.json_or_report(
        json!({
            "ok": true,
            "result": {
                "targets": targets
                    .iter()
                    .map(|target| json!({"name": target.name, "path": target.path.display().to_string()}))
                    .collect::<Vec<_>>(),
                "installed": rows_json(&rows, Status::Installed),
                "skipped": rows_json(&rows, Status::Skipped),
                "errors": rows_json(&rows, Status::Failed),
            },
        }),
        || Report::Text(install_text(&targets, &rows)),
    ))
}

// ── rendering ────────────────────────────────────────────────────────────

fn list_all(context: &Context) -> Report {
    let skills = ado_skills::skills();

    if context.json() {
        return Report::Json(json!({
            "ok": true,
            "count": skills.len(),
            "skills": skills
                .iter()
                .map(|skill| json!({
                    "name": skill.name,
                    "description": skill.description,
                    "version": skill.version,
                    "command_count": skill.commands.len(),
                }))
                .collect::<Vec<_>>(),
        }));
    }

    let mut out = String::from("\n");

    if skills.is_empty() {
        out.push_str("  No skills embedded in this build.\n");
    }

    for skill in skills {
        out.push_str(&format!("  {}\n", skill.name));
        out.push_str(&format!("    {}\n", skill.description));
        out.push_str(&format!(
            "    version: {}  ·  commands: {}\n",
            skill.version,
            skill.commands.len()
        ));

        if !skill.commands.is_empty() {
            out.push_str(&format!(
                "    run: ado skills describe {}     # see commands\n",
                skill.name
            ));
        }

        out.push('\n');
    }

    out.push('\n');
    Report::Text(out)
}

fn listing_text(listing: &ado_skills::Listing) -> String {
    let mut out = String::from("\n");
    out.push_str(&format!("  {}/\n", listing.dir));

    for entry in &listing.entries {
        out.push_str(&format!(
            "    {}{}\n",
            entry.path,
            if entry.is_dir { "/" } else { "" }
        ));
    }

    out.push('\n');
    out
}

fn describe_text(described: &ado_skills::Description) -> String {
    let mut out = String::from("\n");
    out.push_str(&format!("  {}\n", described.name));
    out.push_str(&format!("    {}\n", described.description));
    out.push_str(&format!("    version: {}\n", described.version));
    out.push_str(&format!("    commands: {}\n", described.commands.len()));

    if !described.commands.is_empty() {
        out.push('\n');
        out.push_str("    Commands covered by this skill:\n");

        for command in &described.commands {
            out.push_str(&format!("      • {command}\n"));
        }
    }

    out.push('\n');
    out.push_str(&format!(
        "    Run `ado skills read {}` to load the full body.\n",
        described.name
    ));
    out.push('\n');
    out
}

fn read_report(context: &Context, name: &str, path: &str, content: String) -> Report {
    if !context.json() {
        return Report::Text(content);
    }

    let described = ado_skills::describe(name).expect("the read resolved the skill");

    Report::Json(json!({
        "ok": true,
        "skill": name,
        "path": path,
        "content": content,
        "metadata": {
            "description": described.description,
            "version": described.version,
            "commands": described.commands,
        },
    }))
}

fn search_text(query: &str, hits: &[ado_skills::SearchHit]) -> String {
    let mut out = String::from("\n");

    if hits.is_empty() {
        out.push_str(&format!("  No matches for {query:?}.\n"));
        out.push_str("  Try `ado skills list` to see all available skills.\n");
        out.push('\n');

        return out;
    }

    out.push_str(&format!("  Matches for {query:?} ({}):\n", hits.len()));
    out.push('\n');

    // The frozen groups with `Enum.group_by/2` and iterates the resulting map, so
    // the groups come out in map key (name) order — not first-appearance order,
    // which the priority sort can order differently (`search ci` is the captured
    // case: the name hit is `ado-ci` while the first group is `ado-auth`).
    let mut groups: BTreeMap<&str, Vec<&ado_skills::SearchHit>> = BTreeMap::new();

    for hit in hits {
        groups.entry(hit.skill).or_default().push(hit);
    }

    for (skill, skill_hits) in groups {
        out.push_str(&format!("  {skill}\n"));

        for hit in skill_hits {
            out.push_str(&format!("    [{}] {}\n", hit.match_type, hit.matched));
        }

        out.push('\n');
    }

    out.push('\n');
    out
}

fn install_text(targets: &[install::Target], rows: &[install::InstallRow]) -> String {
    let mut out = String::from("\n");
    out.push_str(&format!(
        "  Installing {} skills to {} target(s):\n",
        ado_skills::names().len(),
        targets.len()
    ));

    for target in targets {
        out.push_str(&format!("    - {}: {}\n", target.name, target.path.display()));
    }

    if targets.iter().all(|target| target.name != "copilot") {
        out.push('\n');
        out.push_str("  Note: copilot installs per-repo (to <repo>/.github/ado-cli/).\n");
        out.push_str("        Run from inside your repo: ado skills install --target copilot\n");
    }

    out.push('\n');

    let installed = count(rows, Status::Installed);
    let skipped = count(rows, Status::Skipped);
    let errors = count(rows, Status::Failed);

    out.push_str(&format!("  Installed: {installed}\n"));
    out.push_str(&format!(
        "  Skipped:   {skipped}{}\n",
        if skipped > 0 {
            " (use --force to overwrite)"
        } else {
            ""
        }
    ));
    out.push_str(&format!("  Errors:    {errors}\n"));

    for row in rows {
        if let InstallStatus::Failed(message) = &row.status {
            out.push('\n');
            out.push_str(&format!(
                "    xx  {}/{}: {message}\n",
                row.target, row.skill
            ));
            out.push_str(&format!("        {}\n", row.path.display()));
        }
    }

    out.push('\n');
    out
}

// ── helpers ──────────────────────────────────────────────────────────────

/// The frozen `{error, reason}` arms all end in the same `halt_error` sentence
/// with a `not_found` shape in this build (D4's class).
fn refusal(error: SkillError) -> AdoError {
    AdoError::not_found(error.message())
}

#[derive(Clone, Copy)]
enum Status {
    Installed,
    Skipped,
    Failed,
}

fn count(rows: &[install::InstallRow], status: Status) -> usize {
    rows.iter().filter(|row| same_status(&row.status, status)).count()
}

fn same_status(row: &InstallStatus, status: Status) -> bool {
    matches!(
        (row, status),
        (InstallStatus::Installed, Status::Installed)
            | (InstallStatus::Skipped, Status::Skipped)
            | (InstallStatus::Failed(_), Status::Failed)
    )
}

fn rows_json(rows: &[install::InstallRow], status: Status) -> Vec<Value> {
    rows.iter()
        .filter(|row| same_status(&row.status, status))
        .map(|row| {
            json!({
                "target": row.target,
                "skill": row.skill,
                "path": row.path.display().to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ado_core::env::MapEnv;
    use ado_core::credentials::InMemoryStore;
    use ado_testkit::TempHome;

    use super::*;

    fn context(json: bool) -> Context {
        let home = TempHome::new();
        let opts = crate::args::GlobalOpts {
            org: None,
            pat: None,
            server: None,
            verbose: false,
            json,
        };

        Context::for_test(opts, MapEnv::new(), InMemoryStore::new(), &home)
    }

    #[test]
    fn the_list_document_is_the_captured_shape() {
        let report = list(&context(true), None).expect("list");

        let Report::Json(document) = report else {
            panic!("--json must answer the envelope");
        };
        assert_eq!(document["ok"], json!(true));
        assert_eq!(document["count"], json!(3));
        assert_eq!(document["skills"][0]["name"], json!("ado-auth"));
        assert_eq!(document["skills"][2]["command_count"], json!(87));
    }

    #[test]
    fn the_read_document_strips_the_frontmatter_and_keeps_the_metadata() {
        let report = read(&context(true), "ado-cli").expect("read");

        let Report::Json(document) = report else {
            panic!("--json must answer the envelope");
        };
        assert_eq!(document["path"], json!("SKILL.md"));
        assert!(
            document["content"]
                .as_str()
                .expect("the content")
                .starts_with("# ado — Azure DevOps CLI\n")
        );
        assert_eq!(document["metadata"]["commands"].as_array().unwrap().len(), 87);
        assert!(document["metadata"].get("name").is_none());
    }

    #[test]
    fn the_read_human_form_prints_the_stripped_body() {
        let report = read(&context(false), "ado-auth").expect("read");

        let Report::Text(content) = report else {
            panic!("human mode must print the body");
        };
        assert!(content.starts_with("# Authentication\n"));
    }

    #[test]
    fn an_unknown_skill_is_a_not_found_refusal() {
        let error = describe(&context(true), "nope").expect_err("unknown");

        assert_eq!(error.code, ado_core::error::ErrorCode::NotFound);
        assert_eq!(
            error.message,
            "unknown skill \"nope\". Run 'ado skills list' to see available skills"
        );
    }

    #[test]
    fn the_search_human_groups_in_name_order_not_first_appearance() {
        let report = search(&context(false), "ci").expect("search");

        let Report::Text(text) = report else {
            panic!("human mode must print the groups");
        };
        let groups = text
            .lines()
            .filter(|line| line.starts_with("  ado-"))
            .collect::<Vec<_>>();

        assert_eq!(groups, ["  ado-auth", "  ado-ci", "  ado-cli"]);

        let auth_group = text.find("  ado-auth\n").expect("the ado-auth group");
        let ci_name_hit = text.find("[name] ado-ci").expect("the name hit");

        assert!(
            auth_group < ci_name_hit,
            "the name hit sorts first in the list but prints inside the last group: {text}"
        );
    }

    #[test]
    fn the_search_no_match_human_form_is_the_frozen_pair() {
        let report = search(&context(false), "zzz").expect("search");

        let Report::Text(text) = report else {
            panic!("human mode must print the sentence");
        };
        assert_eq!(
            text,
            "\n  No matches for \"zzz\".\n  Try `ado skills list` to see all available skills.\n\n"
        );
    }

    #[test]
    fn the_install_human_form_pads_the_three_counts() {
        let report = install(
            &context(false),
            InstallOptions {
                target: "pi",
                repo: None,
                skill: Some("no-such"),
                force: false,
            },
        )
        .expect("install");

        let Report::Text(text) = report else {
            panic!("human mode must print the summary");
        };
        assert!(text.contains("  Installed: 0\n  Skipped:   0\n  Errors:    1\n"));
        assert!(text.contains("    xx  pi/no-such: skill not embedded: unknown skill"));
        assert!(text.contains("  Note: copilot installs per-repo"), "{text}");
    }

    #[test]
    fn the_install_json_rows_drop_the_error_message() {
        let report = install(
            &context(true),
            InstallOptions {
                target: "pi",
                repo: None,
                skill: Some("no-such"),
                force: false,
            },
        )
        .expect("install");

        let Report::Json(document) = report else {
            panic!("--json must answer the envelope");
        };
        assert_eq!(document["ok"], json!(true));
        assert_eq!(document["result"]["installed"], json!([]));
        assert_eq!(document["result"]["errors"][0]["skill"], json!("no-such"));
        assert_eq!(
            document["result"]["errors"][0]
                .as_object()
                .expect("an error row")
                .len(),
            3,
            "target, skill and path only"
        );
    }

    #[test]
    fn an_unresolved_target_is_the_frozen_validation_document() {
        let error = install(
            &context(true),
            InstallOptions {
                target: "copilot",
                repo: Some("/definitely/not/here"),
                skill: None,
                force: false,
            },
        )
        .expect_err("the repo does not exist");

        assert_eq!(error.code, ado_core::error::ErrorCode::ValidationError);
        assert_eq!(
            error.message,
            "could not resolve --target=copilot: --repo=/definitely/not/here does not exist or is not a directory"
        );
    }
}
