//! The frozen `AdoCli.CLI.Skills` installer: `resolve_target_dirs/3` and the
//! per-skill write. `home` and `cwd` are parameters, so a test resolves through
//! the real code without touching the developer's directories (the frozen
//! `System.user_home!()`/`File.cwd!/0` pair is the caller's job).
//!
//! The captured contract:
//!
//!   * `--target=all` is the four per-user targets in Erlang's flat-map key
//!     order — claude, codex, cursor, pi — not the module's declaration order;
//!   * `copilot` writes `<repo>/.github/ado-cli`; `--repo` must be an existing
//!     directory and defaults to the working directory with no repository check
//!     at all (the help's "cwd must be a git repo" is prose);
//!   * every other spelling is a custom path, expanded like `Path.expand/1`
//!     (`~`, `~/…`, or against the working directory);
//!   * a skill lands as `<target>/<skill>/SKILL.md` and **nothing else** — the
//!     reference files the module doc promises to copy never are;
//!   * an existing file is `skipped` unless `--force`, and the skill directory is
//!     created first (even for an unknown skill, whose failed read is then an
//!     `error` row rather than a command failure).

use std::fs;
use std::path::{Component, Path, PathBuf};

/// The per-user targets, in the frozen map's iteration order.
pub const PER_USER_TARGETS: [(&str, &str); 4] = [
    ("claude", ".claude/skills"),
    ("codex", ".codex/skills"),
    ("cursor", ".cursor/skills"),
    ("pi", ".pi/agent/skills"),
];

/// One resolved install destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub path: PathBuf,
}

/// Why a target spec could not resolve; the command wraps it in its own sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    RepoNotADirectory(String),
}

impl ResolveError {
    pub fn message(&self) -> String {
        match self {
            ResolveError::RepoNotADirectory(repo) => {
                format!("--repo={repo} does not exist or is not a directory")
            }
        }
    }
}

/// What happened to one skill in one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallStatus {
    Installed,
    Skipped,
    Failed(String),
}

/// One result row, in the frozen's `{target, skill, path, status}` shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRow {
    pub target: String,
    pub skill: String,
    pub path: PathBuf,
    pub status: InstallStatus,
}

/// `resolve_target_dirs/3`.
pub fn targets(
    spec: &str,
    repo: Option<&str>,
    home: &Path,
    cwd: &Path,
) -> Result<Vec<Target>, ResolveError> {
    if spec == "all" {
        return Ok(PER_USER_TARGETS
            .iter()
            .map(|(name, subdir)| Target {
                name: (*name).to_owned(),
                path: home.join(subdir),
            })
            .collect());
    }

    if let Some((name, subdir)) = PER_USER_TARGETS.iter().find(|(name, _)| *name == spec) {
        return Ok(vec![Target {
            name: (*name).to_owned(),
            path: home.join(subdir),
        }]);
    }

    if spec == "copilot" {
        let repo_path = match repo {
            Some(repo) => {
                let expanded = expand(repo, home, cwd);

                if !expanded.is_dir() {
                    return Err(ResolveError::RepoNotADirectory(repo.to_owned()));
                }

                expanded
            }
            None => cwd.to_path_buf(),
        };

        return Ok(vec![Target {
            name: "copilot".to_owned(),
            path: repo_path.join(".github").join("ado-cli"),
        }]);
    }

    Ok(vec![Target {
        name: "custom".to_owned(),
        path: expand(spec, home, cwd),
    }])
}

/// `install/4`: every target × every skill, in the frozen's order.
pub fn install(targets: &[Target], skills: &[&str], force: bool) -> Vec<InstallRow> {
    let mut rows = Vec::new();

    for target in targets {
        for skill in skills {
            rows.push(install_one(target, skill, force));
        }
    }

    rows
}

fn install_one(target: &Target, skill: &str, force: bool) -> InstallRow {
    let directory = target.path.join(skill);
    let file = directory.join("SKILL.md");

    let status = if file.exists() && !force {
        InstallStatus::Skipped
    } else if !directory.is_dir() {
        match fs::create_dir_all(&directory) {
            Ok(()) => write_skill(skill, &file),
            Err(error) => InstallStatus::Failed(error.to_string()),
        }
    } else {
        write_skill(skill, &file)
    };

    InstallRow {
        target: target.name.clone(),
        skill: skill.to_owned(),
        path: file,
        status,
    }
}

fn write_skill(skill: &str, file: &Path) -> InstallStatus {
    match crate::read_skill(skill) {
        Ok(content) => match fs::write(file, content) {
            Ok(()) => InstallStatus::Installed,
            Err(error) => InstallStatus::Failed(error.to_string()),
        },
        Err(error) => InstallStatus::Failed(format!("skill not embedded: {}", error.message())),
    }
}

/// `Path.expand/1`: `~` is the home directory, an absolute path is normalised,
/// and a relative path is resolved against the working directory. The
/// normalisation is lexical (`.` dropped, `..` popped), never a symlink walk.
fn expand(path: &str, home: &Path, cwd: &Path) -> PathBuf {
    if path == "~" {
        return home.to_path_buf();
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return normalize(&home.join(rest));
    }

    if path.starts_with('/') {
        return normalize(Path::new(path));
    }

    normalize(&cwd.join(path))
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component),
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_the_four_per_user_targets_in_map_order() {
        let home = Path::new("/home/u");

        let resolved = targets("all", None, home, Path::new("/work")).expect("all resolves");

        assert_eq!(
            resolved,
            [
                ("claude", "/home/u/.claude/skills"),
                ("codex", "/home/u/.codex/skills"),
                ("cursor", "/home/u/.cursor/skills"),
                ("pi", "/home/u/.pi/agent/skills"),
            ]
            .map(|(name, path)| Target {
                name: name.to_owned(),
                path: PathBuf::from(path),
            })
        );
    }

    #[test]
    fn a_named_per_user_target_resolves_alone() {
        let resolved = targets("pi", None, Path::new("/home/u"), Path::new("/work"))
            .expect("pi resolves");

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "pi");
        assert_eq!(resolved[0].path, PathBuf::from("/home/u/.pi/agent/skills"));
    }

    #[test]
    fn copilot_needs_an_existing_directory_and_defaults_to_the_working_directory() {
        let home = Path::new("/home/u");
        let cwd = Path::new("/work/here");

        let default = targets("copilot", None, home, cwd).expect("the cwd always resolves");
        assert_eq!(default[0].name, "copilot");
        assert_eq!(default[0].path, PathBuf::from("/work/here/.github/ado-cli"));

        let missing = targets("copilot", Some("/nope"), home, cwd);
        assert_eq!(
            missing,
            Err(ResolveError::RepoNotADirectory("/nope".to_owned()))
        );
        assert_eq!(
            missing.expect_err("missing").message(),
            "--repo=/nope does not exist or is not a directory"
        );
    }

    #[test]
    fn copilot_expands_a_tilde_repo() {
        let home = ado_testkit_home();

        let resolved = targets("copilot", Some("~"), &home, Path::new("/work")).expect("~ resolves");

        assert_eq!(resolved[0].path, home.join(".github").join("ado-cli"));

        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn any_other_spelling_is_a_custom_path() {
        let home = Path::new("/home/u");
        let cwd = Path::new("/work/here");

        assert_eq!(
            targets("~/custom", None, home, cwd).expect("a tilde path")[0].path,
            PathBuf::from("/home/u/custom")
        );
        assert_eq!(
            targets("relative", None, home, cwd).expect("a relative path")[0].path,
            PathBuf::from("/work/here/relative")
        );
        assert_eq!(
            targets("/abs/./x/../y", None, home, cwd).expect("an absolute path")[0].path,
            PathBuf::from("/abs/y"),
            "Path.expand/1 normalises lexically"
        );
        assert_eq!(
            targets("", None, home, cwd).expect("an empty path")[0].path,
            PathBuf::from("/work/here"),
            "Path.expand(\"\") is the working directory"
        );
    }

    #[test]
    fn a_custom_target_is_named_custom() {
        let resolved = targets("anything", None, Path::new("/home/u"), Path::new("/work"))
            .expect("a custom target");

        assert_eq!(resolved[0].name, "custom");
    }

    #[test]
    fn install_writes_skill_md_and_reports_each_row() {
        let home = ado_testkit_home();

        let rows = install(
            &targets("pi", None, &home, &home).expect("pi resolves"),
            &["ado-auth"],
            false,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].skill, "ado-auth");
        assert_eq!(rows[0].status, InstallStatus::Installed);
        assert!(rows[0].path.ends_with("ado-auth/SKILL.md"));
        assert!(rows[0].path.is_file());

        let again = install(
            &targets("pi", None, &home, &home).expect("pi resolves"),
            &["ado-auth"],
            false,
        );
        assert_eq!(again[0].status, InstallStatus::Skipped);

        fs::write(&rows[0].path, "clobbered").expect("clobber");
        let forced = install(
            &targets("pi", None, &home, &home).expect("pi resolves"),
            &["ado-auth"],
            true,
        );
        assert_eq!(forced[0].status, InstallStatus::Installed);
        assert!(fs::read_to_string(&rows[0].path).expect("restored").starts_with("---"));

        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn an_unknown_skill_is_a_failed_row_after_its_directory_is_created() {
        let home = ado_testkit_home();
        let target = targets("pi", None, &home, &home).expect("pi resolves");

        let rows = install(&target, &["no-such"], false);

        assert_eq!(rows[0].status, InstallStatus::Failed("skill not embedded: unknown skill \"no-such\". Run 'ado skills list' to see available skills".to_owned()));
        assert!(target[0].path.join("no-such").is_dir());

        fs::remove_dir_all(&home).ok();
    }

    /// A fresh directory per test, without depending on the testkit from this
    /// crate.
    fn ado_testkit_home() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ado-skills-install-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("the temp home");

        path
    }
}
