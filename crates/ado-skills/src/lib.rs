//! The embedded `ado skills` assets, their frontmatter, the search index and the
//! installer — the whole of `lib/ado_cli/skills.ex` and `lib/ado_cli/frontmatter.ex`
//! that does not render output.
//!
//! The assets live in `assets/` (moved out of the Elixir tree's `priv/skills/`)
//! and are embedded at build time by `build.rs`, exactly as the frozen module's
//! compile-time `File.ls!` walk embedded them. The five commands' rendering stays
//! in `crates/ado`; this crate owns the data and the filesystem effects.

pub mod frontmatter;
pub mod install;

use std::collections::BTreeMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

/// One embedded skill: its frontmatter-derived index and every file under its
/// directory, keyed by the path relative to the skill (so `SKILL.md` and
/// `references/prs.md`). The map is ordered, which is what makes `list_path`'s
/// dedup pick the same survivor as the frozen `Map`'s term order.
#[derive(Debug)]
pub struct Skill {
    pub name: &'static str,
    pub description: String,
    pub version: String,
    pub commands: Vec<String>,
    files: BTreeMap<&'static str, &'static str>,
}

/// `AdoCli.Skills.describe/1`'s result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    pub name: String,
    pub description: String,
    pub version: String,
    pub commands: Vec<String>,
}

/// One `search/1` hit; `context` is the field the frozen struct carries and never
/// sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub skill: &'static str,
    pub match_type: &'static str,
    pub matched: String,
    pub context: String,
}

/// One `list_path/1` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub is_dir: bool,
}

/// `list_path/1`'s result: the directory it resolved and its one-level entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub dir: String,
    pub entries: Vec<Entry>,
}

/// The two refusals the skills commands share, with the frozen sentences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillError {
    UnknownSkill(String),
    FileNotFound(String),
}

impl SkillError {
    pub fn message(&self) -> String {
        match self {
            SkillError::UnknownSkill(name) => {
                format!("unknown skill {name:?}. Run 'ado skills list' to see available skills")
            }
            SkillError::FileNotFound(path) => format!("file not found: {path}"),
        }
    }
}

/// Every embedded skill, name-sorted (`list_skills/0`'s order).
pub fn skills() -> &'static [Skill] {
    static SKILLS: OnceLock<Vec<Skill>> = OnceLock::new();

    SKILLS.get_or_init(build_skills)
}

/// Every embedded skill's name, name-sorted.
pub fn names() -> Vec<&'static str> {
    skills().iter().map(|skill| skill.name).collect()
}

/// `AdoCli.Skills.describe/1`.
pub fn describe(name: &str) -> Result<Description, SkillError> {
    let skill = get(name).ok_or_else(|| SkillError::UnknownSkill(name.to_owned()))?;

    Ok(Description {
        name: skill.name.to_owned(),
        description: skill.description.clone(),
        version: skill.version.clone(),
        commands: skill.commands.clone(),
    })
}

/// `AdoCli.Skills.search/1`: case-insensitive substring matches over the name,
/// the description and every command, name-sorted by `{priority, skill}` with
/// name=0, command=1, description=2. The sort is stable, so a skill's command
/// hits keep the frozen accumulator's **reverse command order**.
pub fn search(query: &str) -> Vec<SearchHit> {
    let needle = query.to_lowercase();
    let mut hits = Vec::new();

    for skill in skills() {
        let mut matches: Vec<SearchHit> = skill
            .commands
            .iter()
            .rev()
            .filter(|command| command.to_lowercase().contains(&needle))
            .map(|command| SearchHit {
                skill: skill.name,
                match_type: "command",
                matched: command.clone(),
                context: String::new(),
            })
            .collect();

        if skill.description.to_lowercase().contains(&needle) {
            matches.push(SearchHit {
                skill: skill.name,
                match_type: "description",
                matched: skill.description.clone(),
                context: String::new(),
            });
        }

        if skill.name.to_lowercase().contains(&needle) {
            matches.push(SearchHit {
                skill: skill.name,
                match_type: "name",
                matched: skill.name.to_owned(),
                context: String::new(),
            });
        }

        hits.extend(matches);
    }

    hits.sort_by(|a, b| {
        match_priority(a.match_type)
            .cmp(&match_priority(b.match_type))
            .then_with(|| a.skill.cmp(b.skill))
    });

    hits
}

/// `AdoCli.Skills.list_path/1`: `arg` split at its first `/`; unknown skills are
/// the shared refusal, an unmatched sub-path is a heading with no entries.
pub fn list_path(arg: &str) -> Result<Listing, SkillError> {
    let (name, sub) = split_arg(arg);
    let skill = get(name).ok_or_else(|| SkillError::UnknownSkill(name.to_owned()))?;
    let dir = if sub.is_empty() {
        name.to_owned()
    } else {
        format!("{name}/{sub}")
    };
    let prefix = format!("{dir}/");
    let mut seen: Vec<&str> = Vec::new();
    let mut entries = Vec::new();

    for path in skill.files.keys() {
        let Some(rest) = path.strip_prefix(&prefix) else {
            continue;
        };
        let component = rest.split('/').next().unwrap_or(rest);

        if seen.contains(&component) {
            continue;
        }

        seen.push(component);
        entries.push(Entry {
            path: (*path).to_owned(),
            is_dir: !path.contains('.'),
        });
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(Listing { dir, entries })
}

/// `AdoCli.Skills.read_skill/1`.
pub fn read_skill(name: &str) -> Result<&'static str, SkillError> {
    let skill = get(name).ok_or_else(|| SkillError::UnknownSkill(name.to_owned()))?;
    let full = format!("{name}/SKILL.md");

    skill
        .files
        .get(full.as_str())
        .copied()
        .ok_or(SkillError::FileNotFound(full))
}

/// `AdoCli.Skills.read_reference/2`.
pub fn read_file(name: &str, relative: &str) -> Result<&'static str, SkillError> {
    let skill = get(name).ok_or_else(|| SkillError::UnknownSkill(name.to_owned()))?;
    let full = format!("{name}/{relative}");

    skill
        .files
        .get(full.as_str())
        .copied()
        .ok_or(SkillError::FileNotFound(full))
}

fn get(name: &str) -> Option<&'static Skill> {
    skills().iter().find(|skill| skill.name == name)
}

/// `split_arg/1` and the read path's `split_target/1`: both split at the first
/// `/`, and a trailing slash leaves the sub-path empty.
pub fn split_arg(arg: &str) -> (&str, &str) {
    match arg.split_once('/') {
        Some((name, rest)) => (name, rest),
        None => (arg, ""),
    }
}

fn match_priority(match_type: &str) -> u8 {
    match match_type {
        "name" => 0,
        "command" => 1,
        "description" => 2,
        _ => 3,
    }
}

fn build_skills() -> Vec<Skill> {
    let mut by_skill: BTreeMap<&'static str, BTreeMap<&'static str, &'static str>> =
        BTreeMap::new();

    for (path, content) in EMBEDDED_FILES {
        let (skill, _) = path
            .split_once('/')
            .expect("every embedded asset lives under a skill directory");

        by_skill.entry(skill).or_default().insert(path, content);
    }

    by_skill
        .into_iter()
        .map(|(name, files)| {
            let skill_md = format!("{name}/SKILL.md");
            let (description, version, commands) = match files.get(skill_md.as_str()) {
                Some(content) => {
                    let frontmatter = frontmatter::parse(content);

                    (
                        frontmatter.get("description").cloned().unwrap_or_default(),
                        frontmatter.get("version").cloned().unwrap_or_default(),
                        frontmatter::parse_commands(content),
                    )
                }
                None => (String::new(), String::new(), Vec::new()),
            };

            Skill {
                name,
                description,
                version,
                commands,
                files,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_is_the_three_shipped_skills_name_sorted() {
        assert_eq!(names(), ["ado-auth", "ado-ci", "ado-cli"]);

        let counts = skills()
            .iter()
            .map(|skill| (skill.name, skill.version.as_str(), skill.commands.len()))
            .collect::<Vec<_>>();

        assert_eq!(
            counts,
            [
                ("ado-auth", "0.5.0", 8),
                ("ado-ci", "0.5.0", 15),
                ("ado-cli", "0.5.0", 87),
            ],
            "Ruling 5: the stale version literal is carried verbatim"
        );
    }

    #[test]
    fn every_skill_has_a_description_and_the_reference_files() {
        for skill in skills() {
            assert!(!skill.description.is_empty(), "{}", skill.name);
        }

        assert_eq!(
            skills()
                .iter()
                .find(|skill| skill.name == "ado-cli")
                .expect("ado-cli")
                .files
                .len(),
            8,
            "SKILL.md and the seven reference files"
        );
    }

    #[test]
    fn describe_carries_the_frontmatter_and_the_command_index() {
        let described = describe("ado-cli").expect("ado-cli");

        assert_eq!(described.name, "ado-cli");
        assert_eq!(described.version, "0.5.0");
        assert_eq!(described.commands.len(), 87);
        assert_eq!(described.commands[0], "ado --version");
        assert_eq!(
            described.commands[86],
            "ado test-coverage show PROJECT BUILD_ID"
        );
    }

    #[test]
    fn describe_of_an_unknown_skill_is_the_frozen_sentence() {
        assert_eq!(
            describe("nope").expect_err("unknown").message(),
            "unknown skill \"nope\". Run 'ado skills list' to see available skills"
        );
    }

    #[test]
    fn read_skill_and_read_file_keep_the_frozen_paths() {
        assert!(
            read_skill("ado-auth")
                .expect("ado-auth")
                .starts_with("---\nname: ado-auth\n")
        );
        assert!(
            read_file("ado-cli", "references/prs.md")
                .expect("prs")
                .starts_with("# Pull Requests\n")
        );
        assert_eq!(
            read_file("ado-cli", "references")
                .expect_err("a directory is not a file")
                .message(),
            "file not found: ado-cli/references"
        );
        assert_eq!(
            read_skill("ado-cli/SKILL.md")
                .expect_err("the splitter takes the name before the slash")
                .message(),
            "unknown skill \"ado-cli/SKILL.md\". Run 'ado skills list' to see available skills"
        );
    }

    #[test]
    fn list_path_dedups_by_the_first_component_after_the_prefix() {
        let listing = list_path("ado-cli").expect("ado-cli");

        assert_eq!(listing.dir, "ado-cli");
        assert_eq!(
            listing.entries,
            [
                Entry {
                    path: "ado-cli/SKILL.md".to_owned(),
                    is_dir: false
                },
                Entry {
                    path: "ado-cli/references/admin.md".to_owned(),
                    is_dir: false
                },
            ],
            "the reference files share one component and the sorted map keeps admin.md"
        );

        let references = list_path("ado-cli/references").expect("references");

        assert_eq!(references.entries.len(), 7);
        assert_eq!(references.entries[0].path, "ado-cli/references/admin.md");
    }

    #[test]
    fn list_path_of_an_unmatched_or_unknown_path() {
        let listing = list_path("ado-cli/nope").expect("an unmatched prefix is not an error");

        assert_eq!(listing.dir, "ado-cli/nope");
        assert!(listing.entries.is_empty());

        assert_eq!(
            list_path("nope/x").expect_err("unknown").message(),
            "unknown skill \"nope\". Run 'ado skills list' to see available skills"
        );
    }

    #[test]
    fn search_orders_name_then_reversed_commands_then_description() {
        let hits = search("ado-cli");

        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].match_type, "name");
        assert_eq!(hits[0].matched, "ado-cli");
        assert_eq!(hits[1].matched, "ado skills read ado-cli");
        assert_eq!(hits[2].matched, "ado skills describe ado-cli");
        assert!(hits.iter().all(|hit| hit.context.is_empty()));
    }

    #[test]
    fn search_sorts_by_priority_across_skills() {
        let hits = search("ci");

        assert_eq!(
            hits.iter()
                .map(|hit| (hit.match_type, hit.skill))
                .collect::<Vec<_>>(),
            [
                ("name", "ado-ci"),
                ("command", "ado-auth"),
                ("command", "ado-ci"),
                ("command", "ado-cli"),
                ("command", "ado-cli"),
                ("description", "ado-auth"),
                ("description", "ado-ci"),
            ]
        );
    }

    #[test]
    fn search_is_a_case_insensitive_substring_of_the_whole_query() {
        assert_eq!(search("create PR").len(), 9);
        assert_eq!(search("CREATE pr").len(), 9);
        assert_eq!(search("zzz").len(), 0);
        assert_eq!(
            search("").len(),
            116,
            "3 names + 3 descriptions + 110 commands"
        );
        assert_eq!(
            search("export ADO_SERVER").len(),
            0,
            "the colon line never joins the command index"
        );
    }
}
