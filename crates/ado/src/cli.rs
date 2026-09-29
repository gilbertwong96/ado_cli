use clap::{Arg, ArgAction, Command};

pub fn command() -> Command {
    Command::new("ado")
        .about("Azure DevOps CLI - Manage Azure DevOps projects, repos, work items, and pipelines from the terminal.")
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .arg(
            Arg::new("org")
                .short('o')
                .long("org")
                .value_name("ORG")
                .global(true)
                .help("Azure DevOps organization name (or set ADO_ORG env var)"),
        )
        .arg(
            Arg::new("pat")
                .short('t')
                .long("pat")
                .value_name("TOKEN")
                .global(true)
                .help("Personal Access Token (or set ADO_PAT env var)"),
        )
        .arg(
            Arg::new("server")
                .short('s')
                .long("server")
                .value_name("URL")
                .global(true)
                .help("Azure DevOps Server URL for self-hosted (or set ADO_SERVER env var)"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue)
                .global(true)
                .help("Enable verbose output"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .global(true)
                .help("Output raw JSON"),
        )
        .subcommand(Command::new("version").about("Print the ado version and exit."))
        .subcommand(Command::new("whoami").about("Show current authentication status."))
        .subcommand(
            Command::new("login")
                .about(
                    "Authenticate with Azure DevOps. Use --method pat with a Personal Access Token (required for CI and headless environments), or --method device to print a code+URL for signing in on any device. With --method omitted, a Personal Access Token on --pat or ADO_PAT selects pat. After login the token is stored in the OS credential store and the organization and method are recorded in the config file — the token is never written to that file.",
                )
                .arg(
                    Arg::new("method")
                        .long("method")
                        .value_name("METHOD")
                        .help(
                            "Auth method. Valid: pat (Personal Access Token; required for CI), device (device code flow; visit URL on any device). Browser login is not available in this build.",
                        ),
                ),
        )
        .subcommand(Command::new("logout").about("Remove stored credentials."))
        .subcommand(
            Command::new("areas")
                .about(
                    "Manage Azure DevOps area paths (classification nodes). Areas organize work items into a hierarchy (e.g. 'Project\\Team\\Feature') for filtering and reporting.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List area paths in a project as a tree (default: only top-level; use --depth for children). Output is a hierarchical tree by default; pass --json for the raw root node with nested children.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("depth")
                                .long("depth")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help(
                                    "Depth of children to retrieve (1 = top-level only, 2 = includes sub-areas)",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show details of a single area path (ID, name, full path, structure type). Returns 404 if the path does not exist.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("area_path")
                                .value_name("AREA_PATH")
                                .required(true)
                                .help(
                                    "Area path using backslashes (e.g. MyProject\\Area\\SubArea). Escape the backslash in shells or wrap in single quotes.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("create")
                        .about(
                            "Create a new area path. Omit --parent to create at the project root, or pass --parent to nest under an existing area.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("name")
                                .long("name")
                                .value_name("NAME")
                                .required(true)
                                .help("Name for the new area path (no backslashes)"),
                        )
                        .arg(
                            Arg::new("parent")
                                .long("parent")
                                .value_name("PATH")
                                .help(
                                    "Parent area path to nest under (e.g. MyProject\\Team). Omit to create at the root.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("update")
                        .about(
                            "Rename an existing area path. Only the leaf name is changed; the path prefix is preserved.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("area_path")
                                .value_name("AREA_PATH")
                                .required(true)
                                .help("Current area path (e.g. MyProject\\OldName)"),
                        )
                        .arg(
                            Arg::new("name")
                                .long("name")
                                .value_name("NAME")
                                .required(true)
                                .help("New name (no backslashes)"),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .about(
                            "Delete an area path. Fails if the area has child areas or work items still assigned to it; reassign or remove those first.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("area_path")
                                .value_name("AREA_PATH")
                                .required(true)
                                .help("Area path to delete (e.g. MyProject\\OldArea)"),
                        ),
                ),
        )
        .subcommand(
            Command::new("iterations")
                .about(
                    "Manage Azure DevOps iterations (sprints). Iterations are time-boxed containers for work items used in Scrum-like workflows. They belong to a specific team (a project can have multiple teams with different sprint cadences).",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List all iterations (sprints) for a team. Output is a table (ID, Name, Start, Finish). Use --current to show only the active sprint.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("team")
                                .value_name("TEAM")
                                .required(true)
                                .help(
                                    "Team name or ID. Iterations are team-scoped — each team can have different sprint cadences.",
                                ),
                        )
                        .arg(
                            Arg::new("current")
                                .long("current")
                                .action(ArgAction::SetTrue)
                                .help(
                                    "If true, only return the iteration that is currently in-progress (matches today's date).",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show details of a single iteration: ID, name, full path, start date, finish date.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("team")
                                .value_name("TEAM")
                                .required(true)
                                .help("Team name or ID"),
                        )
                        .arg(
                            Arg::new("iteration_id")
                                .value_name("ITERATION_ID")
                                .required(true)
                                .help("Iteration identifier (UUID)"),
                        ),
                )
                .subcommand(
                    Command::new("create")
                        .about(
                            "Create a new iteration (sprint) for a team. Without --start-date and --finish-date, the iteration has no time bounds (acts as a backlog bucket).",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("team")
                                .value_name("TEAM")
                                .required(true)
                                .help("Team name or ID"),
                        )
                        .arg(
                            Arg::new("name")
                                .long("name")
                                .value_name("NAME")
                                .required(true)
                                .help("Iteration name (e.g. 'Sprint 23', 'Q1 2026')"),
                        )
                        .arg(
                            Arg::new("start_date")
                                .long("start-date")
                                .value_name("DATE")
                                .help("Sprint start date in ISO 8601 (YYYY-MM-DD, e.g. '2026-01-15')"),
                        )
                        .arg(
                            Arg::new("finish_date")
                                .long("finish-date")
                                .value_name("DATE")
                                .help(
                                    "Sprint end date in ISO 8601 (YYYY-MM-DD, e.g. '2026-01-29'). Should be after start_date.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("update")
                        .about(
                            "Modify an existing iteration's name, start date, or finish date. Pass at least one option. Existing work-item assignments are preserved when the dates change.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("team")
                                .value_name("TEAM")
                                .required(true)
                                .help("Team name or ID"),
                        )
                        .arg(
                            Arg::new("iteration_id")
                                .value_name("ITERATION_ID")
                                .required(true)
                                .help("Iteration identifier (UUID)"),
                        )
                        .arg(
                            Arg::new("name")
                                .long("name")
                                .value_name("NAME")
                                .help("New iteration name"),
                        )
                        .arg(
                            Arg::new("start_date")
                                .long("start-date")
                                .value_name("DATE")
                                .help("New start date (YYYY-MM-DD)"),
                        )
                        .arg(
                            Arg::new("finish_date")
                                .long("finish-date")
                                .value_name("DATE")
                                .help("New finish date (YYYY-MM-DD)"),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .about(
                            "Delete an iteration. Fails if there are work items still assigned to it; reassign them to a different iteration first (use `ado workitems update --iteration`).",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("team")
                                .value_name("TEAM")
                                .required(true)
                                .help("Team name or ID"),
                        )
                        .arg(
                            Arg::new("iteration_id")
                                .value_name("ITERATION_ID")
                                .required(true)
                                .help("Iteration identifier (UUID)"),
                        ),
                ),
        )
        .subcommand(
            Command::new("projects")
                .about(
                    "Manage Azure DevOps projects. A project is the top-level container for repos, pipelines, work items, and teams. Every Azure DevOps organization has at least one project.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List all projects in the organization. Output is a table (Name, ID, State, Visibility). Returns top 100 by default; use --top to change the page size. Pass --json for raw data.",
                        )
                        .arg(
                            Arg::new("state")
                                .long("state")
                                .value_name("STATE")
                                .help(
                                    "Project lifecycle state. Valid: wellFormed (default — healthy and usable), creating, deleting, new (just created, being initialized), all (every state including soft-deleted)",
                                ),
                        )
                        .arg(
                            Arg::new("top")
                                .long("top")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help("Maximum number of projects to return. Default 100, max 1000."),
                        )
                        .arg(
                            Arg::new("skip")
                                .long("skip")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help(
                                    "Number of projects to skip (for pagination). Use with --top to page through results.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show details of a single project: ID, name, description, state, visibility, process template, and capabilities (with --capabilities). The argument accepts either the project name (e.g. 'MyApp') or the GUID.",
                        )
                        .arg(
                            Arg::new("project_id")
                                .value_name("PROJECT_ID")
                                .required(true)
                                .help(
                                    "Project name (e.g. 'MyApp') or GUID. Both are accepted; names are case-insensitive.",
                                ),
                        )
                        .arg(
                            Arg::new("capabilities")
                                .long("capabilities")
                                .action(ArgAction::SetTrue)
                                .help(
                                    "Include the project's capability map (whether version control, boards, pipelines, test plans are enabled). Adds ~30 lines of output.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("create")
                        .about(
                            "Create a new project. The project becomes 'wellFormed' within 30-60 seconds; the CLI does not wait. Use the resulting name with other commands.",
                        )
                        .arg(
                            Arg::new("name")
                                .value_name("NAME")
                                .required(true)
                                .help(
                                    "Project name. Must be unique within the org, 3-64 chars, alphanumeric with hyphens (no spaces). Cannot be changed without deleting and recreating.",
                                ),
                        )
                        .arg(
                            Arg::new("description")
                                .long("description")
                                .value_name("DESC")
                                .help(
                                    "Project description shown in the project picker. Multi-word values do not need quoting.",
                                ),
                        )
                        .arg(
                            Arg::new("visibility")
                                .long("visibility")
                                .value_name("VISIBILITY")
                                .help(
                                    "Who can see the project. Valid: private (default — only invited members), public (anyone on the internet can view, including non-Azure-DevOps users). Note: public projects require AAD and org-level enabling.",
                                ),
                        )
                        .arg(
                            Arg::new("process")
                                .long("process")
                                .value_name("PROCESS")
                                .help(
                                    "Process template that defines work item types and states. Common values: 'Agile', 'Scrum', 'CMMI', 'Basic'. Default depends on the org.",
                                ),
                        )
                        .arg(
                            Arg::new("source_control")
                                .long("source-control")
                                .value_name("TYPE")
                                .help(
                                    "Initial source control type. Valid: 'Git' (default — modern, distributed), 'Tfvc' (legacy Team Foundation Version Control). Cannot be changed after creation.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("update")
                        .about(
                            "Update a project's name or description. The name change propagates to all URLs (old URLs redirect for a grace period).",
                        )
                        .arg(
                            Arg::new("project_id")
                                .value_name("PROJECT_ID")
                                .required(true)
                                .help("Project name or GUID"),
                        )
                        .arg(
                            Arg::new("name")
                                .long("name")
                                .value_name("NAME")
                                .help("New project name (must be unique, same constraints as create)"),
                        )
                        .arg(
                            Arg::new("description")
                                .long("description")
                                .value_name("DESC")
                                .help("New project description"),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .about(
                            "Permanently delete a project. This is IRREVERSIBLE: all repos, work items, pipelines, and history are erased. Use --force to skip the interactive confirmation (the CLI will still ask via the API). Plan for a 30-90 day soft-delete window if you change your mind.",
                        )
                        .arg(
                            Arg::new("project_id")
                                .value_name("PROJECT_ID")
                                .required(true)
                                .help("Project name or GUID"),
                        )
                        .arg(
                            Arg::new("force")
                                .long("force")
                                .action(ArgAction::SetTrue)
                                .help(
                                    "Skip the interactive confirmation prompt (useful in scripts). The Azure DevOps API itself does not require a separate confirmation.",
                                ),
                        ),
                ),
        )
        .subcommand(
            Command::new("repos")
                .about(
                    "Manage Azure DevOps Git repositories. A repository holds source code, branches, commits, tags, and pull requests. The CLI manages metadata and refs; clone/push/commit is left to the `git` command itself.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List all Git repositories in a project. Output is a table (ID, Name, Default Branch). Pass --include-links to also see web URLs. Pass --json for raw data.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("include_links")
                                .long("include-links")
                                .action(ArgAction::SetTrue)
                                .help(
                                    "Include reference links (web, ssh, remote URLs) in the output. Adds 3 columns to the table.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show details of a specific repository: ID, name, default branch, size in bytes, project, and URLs (SSH + web). Use the repo name (not the GUID) as the argument.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help(
                                    "Repository name (preferred) or GUID. Names are case-sensitive in the URL.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("create")
                        .about(
                            "Create a new empty Git repository. The repo is uninitialized (no commits) until you push to it. The default branch is created on first push.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("name")
                                .value_name("NAME")
                                .required(true)
                                .help(
                                    "Repository name. Must be unique within the project, 1-64 chars. Allowed: alphanumerics, hyphens, underscores, periods; no spaces.",
                                ),
                        )
                        .arg(
                            Arg::new("default_branch")
                                .long("default-branch")
                                .value_name("BRANCH")
                                .help(
                                    "Default branch NAME (short form, e.g. 'main' or 'master'). The 'refs/heads/' prefix is added automatically. Default: 'main'. The branch is NOT created until the first push — the setting is applied when the first commit lands on it.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .about(
                            "Permanently delete a repository. IRREVERSIBLE: all commits, branches, tags, PRs, and policies are erased. Use --force in scripts to skip the interactive confirmation prompt.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help("Repository name or GUID"),
                        )
                        .arg(
                            Arg::new("force")
                                .long("force")
                                .action(ArgAction::SetTrue)
                                .help("Skip the interactive confirmation prompt (use in scripts/CI)."),
                        ),
                )
                .subcommand(
                    Command::new("branches")
                        .about(
                            "List branches in a repository. Output is a table (Name, Object ID / commit SHA). Use --filter to limit to branches whose names start with a substring (e.g. --filter feature).",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help(
                                    "Repository name (preferred) or GUID. Names are case-sensitive in the URL.",
                                ),
                        )
                        .arg(
                            Arg::new("filter")
                                .long("filter")
                                .value_name("PATTERN")
                                .help(
                                    "Substring to match against branch names. Default 'heads/' (all branches). Use 'feature' to match 'refs/heads/feature/*', 'users/alice/' for personal branches.",
                                ),
                        ),
                ),
        )
        .subcommand(
            Command::new("prs")
                .about(
                    "Manage Azure DevOps pull requests (PRs). A PR is a request to merge code from one branch (source) into another (target), with required reviewers, policies, and discussion threads.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List pull requests in a repository. Output is a table (ID, Title, Status, Source, Target, Creator). Use --status to filter (default: active). Pass --json for raw data.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help("Repository name or ID"),
                        )
                        .arg(
                            Arg::new("status")
                                .long("status")
                                .value_name("STATUS")
                                .help(
                                    "PR status filter. Valid: active (open, default — includes drafts), completed (merged or closed), abandoned (closed without merging), all (every status). For active PRs only, omit this flag.",
                                ),
                        )
                        .arg(
                            Arg::new("creator")
                                .long("creator")
                                .value_name("USER")
                                .help(
                                    "Filter by creator's email or display name (substring match, case-insensitive). Use the exact email for a single user.",
                                ),
                        )
                        .arg(
                            Arg::new("top")
                                .long("top")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help("Maximum number of PRs to return. Default 50, max 1000."),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show full details of a single pull request: title, description, source/target branches, creator, status, reviewers, labels, policies, merge status, and links. Pass --json for the raw API response (best for scripting).",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help("Repository name or ID"),
                        )
                        .arg(
                            Arg::new("pr_id")
                                .value_name("PR_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric pull request ID"),
                        ),
                )
                .subcommand(
                    Command::new("create")
                        .about(
                            "Create a new pull request. The source and target branches must exist; the source must be different from the target. Returns the new PR ID and web URL.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help("Repository name or ID"),
                        )
                        .arg(
                            Arg::new("title")
                                .long("title")
                                .value_name("TITLE")
                                .required(true)
                                .help(
                                    "PR title (required). Shown in the PR list and as the merge commit subject (depending on merge strategy). Multi-word values do not need quoting.",
                                ),
                        )
                        .arg(
                            Arg::new("description")
                                .long("description")
                                .value_name("DESC")
                                .help(
                                    "PR description (markdown supported). Shown in the PR overview. Multi-word values do not need quoting.",
                                ),
                        )
                        .arg(
                            Arg::new("source")
                                .long("source")
                                .value_name("BRANCH")
                                .required(true)
                                .help(
                                    "Source branch as a full ref (e.g. 'refs/heads/feature/my-branch'). Use the short name ('my-branch') — 'refs/heads/' is added automatically.",
                                ),
                        )
                        .arg(
                            Arg::new("target")
                                .long("target")
                                .value_name("BRANCH")
                                .required(true)
                                .help(
                                    "Target branch as a full ref (e.g. 'refs/heads/main') or short name ('main'). Default: the repo's default branch (usually 'main' or 'master').",
                                ),
                        )
                        .arg(
                            Arg::new("draft")
                                .long("draft")
                                .action(ArgAction::SetTrue)
                                .help(
                                    "Create as a draft PR. Drafts are visible in lists but cannot be completed (merged) until you click 'Ready for review' in the UI.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("complete")
                        .about(
                            "Complete (merge) a pull request. Fails if any required policies haven't passed (builds, required reviewers, branch policies). The merge is non-atomic: the API may return success but the actual merge can take seconds to minutes.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help("Repository name or ID"),
                        )
                        .arg(
                            Arg::new("pr_id")
                                .value_name("PR_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric PR ID"),
                        )
                        .arg(
                            Arg::new("delete-source")
                                .long("delete-source")
                                .action(ArgAction::SetTrue)
                                .help(
                                    "Delete the source branch after the merge succeeds. Useful for keeping the repo clean; if the merge fails, the branch is not deleted.",
                                ),
                        )
                        .arg(
                            Arg::new("merge-strategy")
                                .long("merge-strategy")
                                .value_name("STRATEGY")
                                .help(
                                    "Merge strategy. Valid: 'squash' (combine all commits into one on target, default for most repos), 'rebase' (replay commits without merge), 'noFastForward' (preserve all commits with a merge commit). The strategy must be enabled in the repo's branch policies.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("approve")
                        .about(
                            "Approve a pull request (records a +10 vote on your behalf). If you're not already a reviewer, the API auto-adds you as one. The approval counts toward branch policies that require N approvals.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help("Repository name or ID"),
                        )
                        .arg(
                            Arg::new("pr_id")
                                .value_name("PR_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric PR ID"),
                        ),
                )
                .subcommand(
                    Command::new("vote")
                        .about(
                            "Record a vote on a pull request with a specific value. Use `ado prs approve` as a shortcut for +10. To change or remove your vote, simply vote again with the new value.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help("Repository name or ID"),
                        )
                        .arg(
                            Arg::new("pr_id")
                                .value_name("PR_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric PR ID"),
                        )
                        .arg(
                            Arg::new("vote")
                                .long("vote")
                                .value_name("VOTE")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help(
                                    "Vote value. Valid: 10 (approve), 5 (approve with suggestions, still allows merge), 0 (reset/withdraw your vote), -5 (wait for author, blocks merge), -10 (reject, blocks merge).",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("abandon")
                        .about(
                            "Abandon a pull request (close without merging). The PR stays in the list with status 'abandoned'; the source branch is preserved. The action is reversible in the web UI but not from the CLI.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help("Repository name or ID"),
                        )
                        .arg(
                            Arg::new("pr_id")
                                .value_name("PR_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric PR ID"),
                        ),
                )
                .subcommand(
                    Command::new("diff")
                        .about(
                            "Show the diff for a pull request in one of three modes. Default: table of changed files (path, change type, +/- counts) — fast, no file content fetched. --file PATH: full unified diff for one file (like `git diff <path>`). --unified: single concatenated diff stream for all files (pipe to `less`, `delta`, `code --diff`). --iteration N: inspect an earlier iteration (default: latest = N-1 for a non-draft PR).",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("repo_id")
                                .value_name("REPO_ID")
                                .required(true)
                                .help("Repository name or ID"),
                        )
                        .arg(
                            Arg::new("pr_id")
                                .value_name("PR_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric PR ID"),
                        )
                        .arg(
                            Arg::new("file")
                                .long("file")
                                .value_name("PATH")
                                .help(
                                    "Show the full unified diff for a single path (relative to repo root, with or without leading slash). Must match a file in the default view's path column.",
                                ),
                        )
                        .arg(
                            Arg::new("iteration")
                                .long("iteration")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64).range(1..))
                                .help(
                                    "Iteration number to inspect (default: latest). Iteration 1 is the first push, 2 is the first 'push' after a review, etc. Useful for reviewing earlier versions after force-pushes.",
                                ),
                        )
                        .arg(
                            Arg::new("unified")
                                .long("unified")
                                .action(ArgAction::SetTrue)
                                .help(
                                    "Output a single concatenated unified diff stream for ALL changed files (like `git diff` on the whole PR). Pipe to a pager or syntax highlighter.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("comments")
                        .about(
                            "Manage pull request review comments. Subcommands: add (create thread or reply), list (view threads), update (edit content or status), delete (remove comment or close thread), resolve (mark thread as fixed). A 'thread' is the top-level comment; a 'comment' is a reply within a thread.",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List review threads on a pull request. Default output is a compact table of thread headers (ID, status, file, line, author). Use --all to expand each thread with full comment content, file paths, and reply markers.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("repo_id")
                                        .value_name("REPO_ID")
                                        .required(true)
                                        .help("Repository name or ID"),
                                )
                                .arg(
                                    Arg::new("pr_id")
                                        .value_name("PR_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric PR ID"),
                                )
                                .arg(
                                    Arg::new("all")
                                        .long("all")
                                        .action(ArgAction::SetTrue)
                                        .help(
                                            "Show full comment content, file paths, and reply markers for each thread (verbose mode). Default shows just thread headers.",
                                        ),
                                ),
                        )
                        .subcommand(
                            Command::new("update")
                                .about(
                                    "Update a comment or thread. Pass --content to edit a comment's text, --status to change a thread's resolution state, or both. --content supports @<file> and - (stdin) for multi-line input.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("repo_id")
                                        .value_name("REPO_ID")
                                        .required(true)
                                        .help("Repository name or ID"),
                                )
                                .arg(
                                    Arg::new("pr_id")
                                        .value_name("PR_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric PR ID"),
                                )
                                .arg(
                                    Arg::new("thread_id")
                                        .value_name("THREAD_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Thread ID (from `comments list`)"),
                                )
                                .arg(
                                    Arg::new("comment_id")
                                        .value_name("COMMENT_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Comment ID within the thread (from `comments list --all`)"),
                                )
                                .arg(
                                    Arg::new("content")
                                        .long("content")
                                        .value_name("TEXT")
                                        .help(
                                            "New comment content. Use @<file> to read from a file or `-` to read from stdin. Omit to update status only.",
                                        ),
                                )
                                .arg(
                                    Arg::new("status")
                                        .long("status")
                                        .value_name("STATUS")
                                        .help(
                                            "New thread status. Valid: active (default — open thread), fixed (resolved, hides from active view), wontFix (acknowledged but won't fix), closed (admin-closed), byDesign (working as intended). Omit to update content only.",
                                        ),
                                )
                                .arg(
                                    Arg::new("resolved-by-me")
                                        .long("resolved-by-me")
                                        .action(ArgAction::SetTrue)
                                        .help(
                                            "When --status is set, also set the thread's resolvedBy field to the currently-authenticated user's GUID. Makes an extra GET to /_apis/connectionData to look up your ID.",
                                        ),
                                )
                                .arg(
                                    Arg::new("dry-run")
                                        .long("dry-run")
                                        .action(ArgAction::SetTrue)
                                        .help(
                                            "Print the API request(s) that would be made (method, path, body) as JSON, then exit. Makes no network calls. Useful for previewing the patch before applying.",
                                        ),
                                ),
                        )
                        .subcommand(
                            Command::new("add")
                                .about(
                                    "Add a review comment to a pull request. By default creates a new thread; pass --thread-id to reply to an existing one. For an inline code comment, also pass --file-path and --line.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("repo_id")
                                        .value_name("REPO_ID")
                                        .required(true)
                                        .help("Repository name or ID"),
                                )
                                .arg(
                                    Arg::new("pr_id")
                                        .value_name("PR_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric PR ID"),
                                )
                                .arg(
                                    Arg::new("content")
                                        .long("content")
                                        .value_name("TEXT")
                                        .required(true)
                                        .help(
                                            "Comment text (markdown supported in the web UI). Use @<file> to read from a file or `-` to read from stdin.",
                                        ),
                                )
                                .arg(
                                    Arg::new("file-path")
                                        .long("file-path")
                                        .value_name("PATH")
                                        .help(
                                            "File path for an inline comment (e.g. 'src/foo.ex'). Omit for a general PR comment (not attached to a file).",
                                        ),
                                )
                                .arg(
                                    Arg::new("line")
                                        .long("line")
                                        .value_name("N")
                                        .value_parser(clap::value_parser!(i64))
                                        .allow_negative_numbers(true)
                                        .help(
                                            "Starting line number for an inline comment. Requires --file-path. Use --end-line to comment on a range of lines.",
                                        ),
                                )
                                .arg(
                                    Arg::new("end-line")
                                        .long("end-line")
                                        .value_name("N")
                                        .value_parser(clap::value_parser!(i64))
                                        .allow_negative_numbers(true)
                                        .help(
                                            "Ending line number for a multi-line (codeblock) comment. Requires --file-path and --line.",
                                        ),
                                )
                                .arg(
                                    Arg::new("thread-id")
                                        .long("thread-id")
                                        .value_name("THREAD_ID")
                                        .value_parser(clap::value_parser!(i64))
                                        .allow_negative_numbers(true)
                                        .help(
                                            "Reply to an existing thread (the comment is added as a new reply). Without this flag, a NEW thread is created.",
                                        ),
                                )
                                .arg(
                                    Arg::new("comment-id")
                                        .long("comment-id")
                                        .value_name("COMMENT_ID")
                                        .value_parser(clap::value_parser!(i64))
                                        .allow_negative_numbers(true)
                                        .help(
                                            "Parent comment to reply to (requires --thread-id). Use 0 to start a new top-level comment in the thread (default behavior if --thread-id is set but --comment-id is not).",
                                        ),
                                )
                                .arg(
                                    Arg::new("status")
                                        .long("status")
                                        .value_name("STATUS")
                                        .help(
                                            "Thread status when creating a new thread. Valid: active (default), fixed, wontFix, closed, byDesign.",
                                        ),
                                ),
                        )
                        .subcommand(
                            Command::new("delete")
                                .about(
                                    "Delete a review comment or close a thread. Pass --comment-id to delete a specific comment (HTTP DELETE). Without --comment-id, the thread is closed (PATCH status=closed). Use --force to skip the confirmation prompt.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("repo_id")
                                        .value_name("REPO_ID")
                                        .required(true)
                                        .help("Repository name or ID"),
                                )
                                .arg(
                                    Arg::new("pr_id")
                                        .value_name("PR_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric PR ID"),
                                )
                                .arg(
                                    Arg::new("thread_id")
                                        .value_name("THREAD_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Thread ID (from `comments list`)"),
                                )
                                .arg(
                                    Arg::new("comment-id")
                                        .long("comment-id")
                                        .value_name("COMMENT_ID")
                                        .value_parser(clap::value_parser!(i64))
                                        .allow_negative_numbers(true)
                                        .help(
                                            "Comment ID within the thread to delete. Omit to delete the entire thread.",
                                        ),
                                )
                                .arg(
                                    Arg::new("force")
                                        .long("force")
                                        .action(ArgAction::SetTrue)
                                        .help("Skip confirmation prompt."),
                                ),
                        )
                        .subcommand(
                            Command::new("resolve")
                                .about(
                                    "Resolve a review thread by setting its status. This is a convenience wrapper around `comments update --status` that does not require a comment ID. Default status is 'fixed'. Use --resolved-by-me to attribute the resolution to yourself.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("repo_id")
                                        .value_name("REPO_ID")
                                        .required(true)
                                        .help("Repository name or ID"),
                                )
                                .arg(
                                    Arg::new("pr_id")
                                        .value_name("PR_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric PR ID"),
                                )
                                .arg(
                                    Arg::new("thread_id")
                                        .value_name("THREAD_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Thread ID (from `comments list`)"),
                                )
                                .arg(
                                    Arg::new("status")
                                        .long("status")
                                        .value_name("STATUS")
                                        .default_value("fixed")
                                        .help(
                                            "Resolution status. Valid: fixed (resolved), wontFix (won't fix), closed (admin-closed), byDesign (working as intended), active (reopen).",
                                        ),
                                )
                                .arg(
                                    Arg::new("resolved-by-me")
                                        .long("resolved-by-me")
                                        .action(ArgAction::SetTrue)
                                        .help(
                                            "Attribute the resolution to the currently-authenticated user. Makes an extra GET to /_apis/connectionData to look up your GUID.",
                                        ),
                                ),
                        ),
                )
                .subcommand(
                    Command::new("reviewers")
                        .about(
                            "Manage pull request reviewers. Reviewers receive notifications, can vote (approve/reject/wait), and count toward branch policies that require N approvals.",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List reviewers on a pull request. Output is a table (Display Name, Email, Vote, Status). Vote values: 10 (approved), 5 (approved w/ suggestions), -5 (waiting), -10 (rejected), 0 (no vote, or reset). Use --search for fuzzy filtering by name or email (client-side, since the API returns all reviewers).",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("repo_id")
                                        .value_name("REPO_ID")
                                        .required(true)
                                        .help("Repository name or ID"),
                                )
                                .arg(
                                    Arg::new("pr_id")
                                        .value_name("PR_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric PR ID"),
                                )
                                .arg(
                                    Arg::new("search")
                                        .long("search")
                                        .value_name("QUERY")
                                        .help(
                                            "Fuzzy-filter reviewers by display name or email. Supports substring and subsequence (fzf-style) matching. Case-insensitive.",
                                        ),
                                ),
                        )
                        .subcommand(
                            Command::new("add")
                                .about(
                                    "Add a reviewer to a pull request. The reviewer receives a notification email and shows up in the PR's reviewer list. The --reviewer value can be either a user GUID (most reliable) or an email address (resolved to a GUID by the API).",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("repo_id")
                                        .value_name("REPO_ID")
                                        .required(true)
                                        .help("Repository name or ID"),
                                )
                                .arg(
                                    Arg::new("pr_id")
                                        .value_name("PR_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric PR ID"),
                                )
                                .arg(
                                    Arg::new("reviewer")
                                        .long("reviewer")
                                        .value_name("USER")
                                        .required(true)
                                        .help(
                                            "Reviewer identifier. Accepts a user GUID (preferred — e.g. from `ado users show alice@example.com`) or an email address. GUIDs are case-insensitive.",
                                        ),
                                )
                                .arg(
                                    Arg::new("required")
                                        .long("required")
                                        .action(ArgAction::SetTrue)
                                        .help(
                                            "Mark as a required reviewer (default: optional). The PR cannot be completed until all required reviewers have voted (vote != 0).",
                                        ),
                                ),
                        )
                        .subcommand(
                            Command::new("remove")
                                .about(
                                    "Remove a reviewer from a pull request. The user's vote is discarded. Does NOT notify the user (unlike adding).",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("repo_id")
                                        .value_name("REPO_ID")
                                        .required(true)
                                        .help("Repository name or ID"),
                                )
                                .arg(
                                    Arg::new("pr_id")
                                        .value_name("PR_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric PR ID"),
                                )
                                .arg(
                                    Arg::new("reviewer")
                                        .long("reviewer")
                                        .value_name("USER")
                                        .required(true)
                                        .help(
                                            "Reviewer identifier (GUID or email — see `add`). Use a GUID for unambiguous removal.",
                                        ),
                                ),
                        ),
                ),
        )
        .subcommand(
            Command::new("workitems")
                .about(
                    "Manage Azure DevOps work items (bugs, tasks, user stories, epics, issues). Full CRUD plus state transitions, comments, attachments, and WIQL queries. Use --json for structured output.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List work items in a project with optional filtering by type, assigned user, state. Output is a table (ID, Title, Type, State, Assigned To). Use --top to limit, --type to filter.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("type")
                                .long("type")
                                .value_name("TYPE")
                                .help("Work item type (Bug, Task, User Story)"),
                        )
                        .arg(
                            Arg::new("assigned-to")
                                .long("assigned-to")
                                .value_name("USER")
                                .help(
                                    "Filter to items assigned to a specific user (substring match on display name or email)",
                                ),
                        )
                        .arg(
                            Arg::new("state")
                                .long("state")
                                .value_name("STATE")
                                .help(
                                    "Filter by work item state: New, Active, Resolved, Closed, etc. State values depend on the process template.",
                                ),
                        )
                        .arg(
                            Arg::new("top")
                                .long("top")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help(
                                    "Max items per page. Default is 100; increase for broader queries, decrease for faster responses.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show a single work item by numeric ID. Returns all system fields, custom fields, and relations (parent/child links). Use --expand=all for full details.",
                        )
                        .arg(
                            Arg::new("id")
                                .value_name("ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Work item ID"),
                        )
                        .arg(
                            Arg::new("expand")
                                .long("expand")
                                .value_name("LEVEL")
                                .default_value("all")
                                .help("Expand level"),
                        ),
                )
                .subcommand(
                    Command::new("query")
                        .about(
                            "Execute a WIQL (Work Item Query Language) query against a project. WIQL is SQL-like: SELECT [System.Id] FROM WorkItems WHERE [System.State] = Active. Use --top to limit.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("wiql")
                                .long("wiql")
                                .value_name("WIQL")
                                .help(
                                    "WIQL query (SQL-like syntax). SELECT ... FROM WorkItems WHERE ... ORDER BY ... . Required fields: [System.Id], [System.Title].",
                                ),
                        )
                        .arg(
                            Arg::new("top")
                                .long("top")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help("Maximum number of results"),
                        ),
                )
                .subcommand(
                    Command::new("create")
                        .about(
                            "Create a new work item. Requires --type and --title. Optional: --description, --assigned-to, --state, --priority (1-4), --tags (comma-separated).",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("type")
                                .long("type")
                                .value_name("TYPE")
                                .required(true)
                                .help("Work item type (Bug, Task, User Story, Epic, Issue)"),
                        )
                        .arg(
                            Arg::new("title")
                                .long("title")
                                .value_name("TITLE")
                                .required(true)
                                .help(
                                    "Title for the work item. Keep it concise (shown in list views, boards, and queries).",
                                ),
                        )
                        .arg(
                            Arg::new("description")
                                .long("description")
                                .value_name("DESC")
                                .help(
                                    "Description body (markdown supported). Multi-word values do not need quoting.",
                                ),
                        )
                        .arg(
                            Arg::new("assigned-to")
                                .long("assigned-to")
                                .value_name("USER")
                                .help(
                                    "User display name or email to assign the item to. The user must be a member of the project.",
                                ),
                        )
                        .arg(
                            Arg::new("state")
                                .long("state")
                                .value_name("STATE")
                                .help(
                                    "Initial state: New, Active, Proposed, etc. Depends on process template. Default is the first state in the workflow.",
                                ),
                        )
                        .arg(
                            Arg::new("priority")
                                .long("priority")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help(
                                    "Priority level: 1 (highest), 2 (high), 3 (medium), 4 (low). Default depends on process template.",
                                ),
                        )
                        .arg(
                            Arg::new("tags")
                                .long("tags")
                                .value_name("TAGS")
                                .help(
                                    "Tags as comma-separated list (e.g. frontend,ui,regression). Case-insensitive. Existing tags are auto-created.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("update")
                        .about(
                            "Update a work item's fields. Pass only the fields you want to change. Setting a field to its current value is a no-op but still creates a revision entry.",
                        )
                        .arg(
                            Arg::new("id")
                                .value_name("ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Work item ID"),
                        )
                        .arg(
                            Arg::new("title")
                                .long("title")
                                .value_name("TITLE")
                                .help("Replacement title. Leave unset to keep the current title."),
                        )
                        .arg(
                            Arg::new("description")
                                .long("description")
                                .value_name("DESC")
                                .help("Replacement description. Leave unset to keep current."),
                        )
                        .arg(
                            Arg::new("state")
                                .long("state")
                                .value_name("STATE")
                                .help(
                                    "Target state. Must be a valid transition from the current state. Use the web UI to discover valid states for your process template.",
                                ),
                        )
                        .arg(
                            Arg::new("assigned-to")
                                .long("assigned-to")
                                .value_name("USER")
                                .help("Assign to user"),
                        )
                        .arg(
                            Arg::new("priority")
                                .long("priority")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help("Priority (1-4)"),
                        )
                        .arg(
                            Arg::new("tags")
                                .long("tags")
                                .value_name("TAGS")
                                .help("Comma-separated tags (replaces all)"),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .about(
                            "Permanently delete a work item. This is irreversible. By default work items can be moved to the Recycle Bin instead; deletion requires special permissions.",
                        )
                        .arg(
                            Arg::new("id")
                                .value_name("ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Work item ID"),
                        ),
                )
                .subcommand(
                    Command::new("comments")
                        .about(
                            "Add, list, or update discussion comments on a work item. Comments are threaded; each update creates a revision.",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List all discussion comments on a work item. Returns comment ID, author, date, and text. Use for auditing or review.",
                                )
                                .arg(
                                    Arg::new("id")
                                        .value_name("ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Work item ID"),
                                ),
                        )
                        .subcommand(
                            Command::new("add")
                                .about(
                                    "Add a new discussion comment to a work item. Comment text supports markdown. The comment appears in the Discussion section.",
                                )
                                .arg(
                                    Arg::new("id")
                                        .value_name("ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Work item ID"),
                                )
                                .arg(
                                    Arg::new("text")
                                        .long("text")
                                        .value_name("TEXT")
                                        .required(true)
                                        .help("Comment text"),
                                ),
                        )
                        .subcommand(
                            Command::new("update")
                                .about(
                                    "Edit an existing discussion comment by revision ID. Only the comment body can be changed; author and timestamp are preserved. Use list to find comment IDs.",
                                )
                                .arg(
                                    Arg::new("id")
                                        .value_name("ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Work item ID"),
                                )
                                .arg(
                                    Arg::new("comment_id")
                                        .value_name("COMMENT_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help(
                                            "Comment revision number (from the list command). Each edit creates a new revision.",
                                        ),
                                )
                                .arg(
                                    Arg::new("text")
                                        .long("text")
                                        .value_name("TEXT")
                                        .required(true)
                                        .help("Replacement comment text."),
                                ),
                        ),
                )
                .subcommand(
                    Command::new("attachments")
                        .about(
                            "Upload, list, or download file attachments on a work item. Attachments are stored in Azure DevOps with the work item.",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List all file attachments on a work item: filename, size, and attachment ID.",
                                )
                                .arg(
                                    Arg::new("id")
                                        .value_name("ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Work item ID"),
                                ),
                        )
                        .subcommand(
                            Command::new("download")
                                .about(
                                    "Download a single attachment to a local file. Default filename matches the original attachment name. Use --output to specify a custom path.",
                                )
                                .arg(
                                    Arg::new("id")
                                        .value_name("ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Work item ID"),
                                )
                                .arg(
                                    Arg::new("attachment_id")
                                        .value_name("ATTACHMENT_ID")
                                        .required(true)
                                        .help("Attachment ID"),
                                )
                                .arg(
                                    Arg::new("output")
                                        .long("output")
                                        .value_name("PATH")
                                        .help("Output file path (default: attachment filename)"),
                                ),
                        ),
                ),
        )
        .subcommand(
            Command::new("teams")
                .about(
                    "Manage Azure DevOps teams (groups of members with shared area paths and iterations). Teams are the unit for sprint planning and work item assignment.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List all teams in a project. Output is a table (ID, Name, Description). Use --top to limit.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("top")
                                .long("top")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help("Maximum number to return"),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show a single team: ID, name, description, identity URL, and project context. Accepts name or GUID.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("team_id")
                                .value_name("TEAM_ID")
                                .required(true)
                                .help("Team name or ID"),
                        ),
                )
                .subcommand(
                    Command::new("create")
                        .about(
                            "Create a new team in a project. The team inherits the project default area path and iteration. Members are added separately with the members subcommand.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("name")
                                .long("name")
                                .value_name("NAME")
                                .required(true)
                                .help("Team name"),
                        )
                        .arg(
                            Arg::new("description")
                                .long("description")
                                .value_name("DESC")
                                .help("Team description"),
                        ),
                )
                .subcommand(
                    Command::new("update")
                        .about("Update a team.")
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("team_id")
                                .value_name("TEAM_ID")
                                .required(true)
                                .help("Team name or ID"),
                        )
                        .arg(
                            Arg::new("name")
                                .long("name")
                                .value_name("NAME")
                                .help("New team name"),
                        )
                        .arg(
                            Arg::new("description")
                                .long("description")
                                .value_name("DESC")
                                .help("New description"),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .about(
                            "Delete a team. Members are not removed from the org; just the team container is deleted.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("team_id")
                                .value_name("TEAM_ID")
                                .required(true)
                                .help("Team name or ID"),
                        ),
                )
                .subcommand(
                    Command::new("members")
                        .about(
                            "Add/remove users to/from a team. Team membership is separate from project membership; users must be in the project first.",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List all members of a team. Output shows display name, unique name (email), and member ID.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("team_id")
                                        .value_name("TEAM_ID")
                                        .required(true)
                                        .help("Team name or ID"),
                                ),
                        ),
                ),
        )
        .subcommand(
            Command::new("users")
                .about(
                    "Manage user access levels and entitlements (licenses, extensions, project memberships). Requires Project Collection Administrator permissions.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List all users in the organization with their access level, last login, and project memberships. Output is a table by default; use --json for raw data. Use --top to limit.",
                        )
                        .arg(
                            Arg::new("top")
                                .long("top")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help("Maximum number to return"),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show details of a single user: email, display name, access level (Stakeholder/Basic/Basic+Test Plans/VS Enterprise), date created, last accessed, and project/group memberships.",
                        )
                        .arg(
                            Arg::new("user_id")
                                .value_name("USER_ID")
                                .required(true)
                                .help("User ID or email"),
                        ),
                )
                .subcommand(
                    Command::new("add")
                        .about("Add a user to the organization.")
                        .arg(
                            Arg::new("email")
                                .long("email")
                                .value_name("EMAIL")
                                .required(true)
                                .help("User email address"),
                        )
                        .arg(
                            Arg::new("license")
                                .long("license")
                                .value_name("LICENSE")
                                .help(
                                    "Access level: express (Basic, 5 free users), professional (Basic, paid), stakeholder (free, limited). Default: express.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("remove")
                        .about(
                            "Remove a user from the organization entirely. Revokes all licenses and memberships. The user is immediately blocked from accessing any project.",
                        )
                        .arg(
                            Arg::new("user_id")
                                .value_name("USER_ID")
                                .required(true)
                                .help("User ID or email"),
                        ),
                ),
        )
        .subcommand(
            Command::new("pipelines")
                .about(
                    "Manage Azure DevOps YAML pipelines and variable groups. Pipelines define CI/CD workflows as code in azure-pipelines.yml; variable groups are shared sets of KEY=VALUE pairs that pipelines can reference.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List all pipelines in a project. Output is a table (ID, Name, Folder). Use --folder to scope to a subtree (e.g. 'MyTeam/Frontend'); use --top to limit. Pass --json for raw data.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("top")
                                .long("top")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help(
                                    "Maximum number of pipelines to return. Default 100, max 1000.",
                                ),
                        )
                        .arg(
                            Arg::new("folder")
                                .long("folder")
                                .value_name("PATH")
                                .help(
                                    "Filter to pipelines in a specific folder (e.g. 'MyTeam/Frontend' or '/' for root)",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show details of a single pipeline: ID, name, folder, YAML path, repository, and web URL. The pipeline ID is a stable integer — use it with `run`.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("pipeline_id")
                                .value_name("PIPELINE_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help(
                                    "Numeric pipeline ID (from `list`). The ID is project-scoped — the same number may refer to a different pipeline in another project.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("run")
                        .about(
                            "Trigger a new run of a pipeline. Returns the run ID and a link to monitor it (use `ado ci watch` for live streaming). Variables are scoped to this run only; use variable groups for shared config.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("pipeline_id")
                                .value_name("PIPELINE_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric pipeline ID to run"),
                        )
                        .arg(
                            Arg::new("branch")
                                .long("branch")
                                .value_name("BRANCH")
                                .help(
                                    "Branch to run on (short name, e.g. 'main' or 'feature/foo'; 'refs/heads/' is added automatically). Default: the pipeline's default branch.",
                                ),
                        )
                        .arg(
                            Arg::new("variables")
                                .long("variables")
                                .value_name("VARS")
                                .help(
                                    "Run-time variables as comma-separated KEY=VALUE pairs (e.g. 'ENV=staging,DEBUG=true'). These override pipeline-defined variables for this run only.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("create")
                        .about(
                            "Create a new YAML pipeline that points to an existing azure-pipelines.yml file in a repository. The YAML file must already exist; this command registers the pipeline definition.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("name")
                                .long("name")
                                .value_name("NAME")
                                .required(true)
                                .help("Display name for the pipeline (e.g. 'MyApp CI')"),
                        )
                        .arg(
                            Arg::new("repo")
                                .long("repo")
                                .value_name("REPO")
                                .required(true)
                                .help(
                                    "Repository name or ID containing the YAML file (must be in the same project)",
                                ),
                        )
                        .arg(
                            Arg::new("path")
                                .long("path")
                                .value_name("PATH")
                                .required(true)
                                .help(
                                    "Path to the YAML file in the repo, relative to the repo root (e.g. 'pipelines/ci.yml' or 'azure-pipelines.yml')",
                                ),
                        )
                        .arg(
                            Arg::new("folder")
                                .long("folder")
                                .value_name("FOLDER")
                                .help(
                                    "Folder to place the pipeline in (e.g. 'MyTeam/Frontend'). Use '/' for the root. The folder is auto-created if it doesn't exist.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("update")
                        .about(
                            "Update a pipeline's name or YAML path. Cannot change the repository; delete and recreate to switch repos. Pass at least one of --name or --path.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("pipeline_id")
                                .value_name("PIPELINE_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric pipeline ID"),
                        )
                        .arg(
                            Arg::new("name")
                                .long("name")
                                .value_name("NAME")
                                .help("New display name for the pipeline"),
                        )
                        .arg(
                            Arg::new("path")
                                .long("path")
                                .value_name("PATH")
                                .help(
                                    "New YAML file path in the repo. Doesn't move the file on disk; just changes what the pipeline definition points to.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .about(
                            "Delete a pipeline definition. This is irreversible: the run history is preserved (Azure DevOps retains it), but the pipeline can no longer be triggered and new runs cannot be created from this definition.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("pipeline_id")
                                .value_name("PIPELINE_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric pipeline ID"),
                        ),
                )
                .subcommand(
                    Command::new("vars")
                        .about(
                            "Manage variable groups (a.k.a. library variable groups). Variable groups are shared KEY=VALUE sets that multiple pipelines can reference — perfect for environment-specific config (DB URLs, API keys).",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List all variable groups in a project. Output is a table (ID, Name, Description, variable count). Use --top to limit.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("top")
                                        .long("top")
                                        .value_name("N")
                                        .value_parser(clap::value_parser!(i64))
                                        .allow_negative_numbers(true)
                                        .help("Maximum number to return. Default 50."),
                                ),
                        )
                        .subcommand(
                            Command::new("show")
                                .about(
                                    "Show details of a variable group: ID, name, description, type, and the list of variable names. SECRET VALUES ARE NEVER DISPLAYED (they show as ' [secret]'). Use --json to confirm a variable's key/value is being read correctly without exposing secrets.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("group_id")
                                        .value_name("GROUP_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric variable group ID (from `list`)"),
                                ),
                        )
                        .subcommand(
                            Command::new("create")
                                .about(
                                    "Create a new variable group. Use --secret to mark specific keys as secret (the API stores them encrypted; they cannot be retrieved later).",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("name")
                                        .long("name")
                                        .value_name("NAME")
                                        .required(true)
                                        .help(
                                            "Display name for the variable group (e.g. 'prod-secrets', 'ci-shared')",
                                        ),
                                )
                                .arg(
                                    Arg::new("description")
                                        .long("description")
                                        .value_name("DESC")
                                        .help("Human-readable description of the group's purpose"),
                                )
                                .arg(
                                    Arg::new("variables")
                                        .long("variables")
                                        .value_name("VARS")
                                        .help(
                                            "Initial variables as comma-separated KEY=VALUE pairs (e.g. 'DB_HOST=db.example.com,DB_USER=app')",
                                        ),
                                )
                                .arg(
                                    Arg::new("secret")
                                        .long("secret")
                                        .value_name("KEYS")
                                        .help(
                                            "Comma-separated list of variable names to mark as secret (e.g. 'DB_PASS,API_KEY'). The values are stored encrypted and cannot be retrieved via the API after creation.",
                                        ),
                                ),
                        )
                        .subcommand(
                            Command::new("update")
                                .about(
                                    "Update an existing variable group. Pass --variables to merge new values (existing variables not in the list are kept). Pass --secret to change which keys are secret. Note: changing a secret's value requires re-passing the value; the original encrypted value cannot be read back.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("group_id")
                                        .value_name("GROUP_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric variable group ID"),
                                )
                                .arg(
                                    Arg::new("name")
                                        .long("name")
                                        .value_name("NAME")
                                        .help("New display name"),
                                )
                                .arg(
                                    Arg::new("description")
                                        .long("description")
                                        .value_name("DESC")
                                        .help("New description"),
                                )
                                .arg(
                                    Arg::new("variables")
                                        .long("variables")
                                        .value_name("VARS")
                                        .help("Variables to merge, as comma-separated KEY=VALUE pairs"),
                                )
                                .arg(
                                    Arg::new("secret")
                                        .long("secret")
                                        .value_name("KEYS")
                                        .help(
                                            "Comma-separated list of keys to mark as secret. Replaces the previous secret list.",
                                        ),
                                ),
                        )
                        .subcommand(
                            Command::new("delete")
                                .about(
                                    "Delete a variable group. Pipelines that reference it will fail at runtime until their YAML is updated to remove the reference.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("group_id")
                                        .value_name("GROUP_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric variable group ID"),
                                ),
                        ),
                )
                .subcommand(
                    Command::new("variables")
                        .about(
                            "Manage per-pipeline variables (user-defined variables stored on a single pipeline definition, not shared). Prefer variable groups for shared config — these are scoped to one pipeline.",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List all variables defined directly on a pipeline (not those in referenced variable groups). Output is a table (Key, Value, Secret).",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("pipeline_id")
                                        .value_name("PIPELINE_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric pipeline ID"),
                                ),
                        )
                        .subcommand(
                            Command::new("create")
                                .about(
                                    "Add a single variable to a pipeline. For multiple variables, prefer `ado pipelines vars create` (variable groups) which can be shared across pipelines.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("pipeline_id")
                                        .value_name("PIPELINE_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric pipeline ID"),
                                )
                                .arg(
                                    Arg::new("key")
                                        .long("key")
                                        .value_name("KEY")
                                        .required(true)
                                        .help("Variable name (env-var friendly: uppercase, no spaces)"),
                                )
                                .arg(
                                    Arg::new("value")
                                        .long("value")
                                        .value_name("VALUE")
                                        .required(true)
                                        .help(
                                            "Variable value. Marked secret if --secret is passed (value is then stored encrypted and cannot be retrieved later).",
                                        ),
                                )
                                .arg(
                                    Arg::new("secret")
                                        .long("secret")
                                        .action(ArgAction::SetTrue)
                                        .help(
                                            "Mark the variable as secret. Once set, the value cannot be retrieved via the API — only overwritten with a new value.",
                                        ),
                                ),
                        )
                        .subcommand(
                            Command::new("delete")
                                .about(
                                    "Remove a variable from a pipeline. Pipelines referencing this variable will fail; update the pipeline YAML or use a default value.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("pipeline_id")
                                        .value_name("PIPELINE_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric pipeline ID"),
                                )
                                .arg(
                                    Arg::new("key")
                                        .long("key")
                                        .value_name("KEY")
                                        .required(true)
                                        .help("Variable name to remove (must match exactly, case-sensitive)"),
                                ),
                        ),
                )
                .subcommand(
                    Command::new("secure_files")
                        .about(
                            "Manage Secure Files in the Pipeline Library. Secure files are binary blobs (certificates, kubeconfigs, signing keys) referenced by pipelines via the DownloadSecureFile@1 task. (The 'download' subcommand is intentionally absent — see CHANGELOG for the Azure DevOps platform gap that prevents ticket issuance for personal Microsoft accounts.)",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List all Secure Files in a project. Output is a table (ID, Name, Size, Modified). Use --top to limit. Pass --json for raw data.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("top")
                                        .long("top")
                                        .value_name("N")
                                        .value_parser(clap::value_parser!(i64))
                                        .allow_negative_numbers(true)
                                        .help("Maximum number to return. Default 50."),
                                ),
                        )
                        .subcommand(
                            Command::new("show")
                                .about(
                                    "Show details of a single Secure File: ID, name, size, created/modified by and on. The ID is a GUID string (from `list`).",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("secure_file_id")
                                        .value_name("SECURE_FILE_ID")
                                        .required(true)
                                        .help("Secure File ID (GUID string, from `list`)"),
                                ),
                        )
                        .subcommand(
                            Command::new("upload")
                                .about(
                                    "Upload a local file as a Secure File to the Library. The file content is sent as raw bytes (application/octet-stream). If a Secure File with the same name already exists and --allow-exists is passed, the existing one is deleted first; otherwise the command fails with a 409 error.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("name")
                                        .value_name("NAME")
                                        .required(true)
                                        .help(
                                            "Name for the Secure File (e.g. 'prod-cert.pem'). Must be unique within the project.",
                                        ),
                                )
                                .arg(
                                    Arg::new("file")
                                        .long("file")
                                        .value_name("PATH")
                                        .required(true)
                                        .help("Path to the local file to upload"),
                                )
                                .arg(
                                    Arg::new("allow_exists")
                                        .long("allow-exists")
                                        .action(ArgAction::SetTrue)
                                        .help(
                                            "If a Secure File with this name already exists, delete it first before uploading. Without this flag, the command fails on name conflict.",
                                        ),
                                ),
                        )
                        .subcommand(
                            Command::new("delete")
                                .about(
                                    "Permanently delete a Secure File. Pipelines referencing it will fail until updated.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("secure_file_id")
                                        .value_name("SECURE_FILE_ID")
                                        .required(true)
                                        .help("Secure File ID (GUID string, from `list`)"),
                                )
                                .arg(
                                    Arg::new("force")
                                        .long("force")
                                        .action(ArgAction::SetTrue)
                                        .help(
                                            "Proceed with the delete. Without it the command refuses, writes the guard to stderr and sends nothing.",
                                        ),
                                ),
                        ),
                )
        )
        .subcommand(
            Command::new("pipelines-folders")
                .about(
                    "Manage pipeline folders. Folders organize pipelines in the web UI (like directories) and help with permissions and discoverability. They are purely organizational — they don't change pipeline behavior.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List pipeline folders in a project with the count of pipelines in each. Use --path to scope to a subtree. Output is a table (Folder, Pipelines). Pass --json for raw pipeline data.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("path")
                                .long("path")
                                .value_name("PATH")
                                .help("Subtree to list (e.g. 'MyTeam/Frontend'). Omit to list the whole project."),
                        ),
                )
                .subcommand(
                    Command::new("create")
                        .about(
                            "Create a new pipeline folder. Use forward slashes for nesting (e.g. 'MyTeam/Frontend'). Parent folders are created automatically. Idempotent: returns success if the folder already exists.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("path")
                                .long("path")
                                .value_name("PATH")
                                .required(true)
                                .help("Folder path (forward-slash separated; nested paths are auto-created)"),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .about(
                            "Delete a folder AND all pipelines within it. This is a hard delete — pipelines inside are removed (not moved to root). Refuses if the folder doesn't exist or if it contains builds/runs that the API considers blocking.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("path")
                                .long("path")
                                .value_name("PATH")
                                .required(true)
                                .help("Folder path to delete (must match exactly as shown by `list`)"),
                        ),
                ),
        )
        .subcommand(
            Command::new("pipelines-builds")
                .about(
                    "Manage Azure Pipelines classic (XAML) builds. Most modern pipelines use YAML and should use the `ado pipelines` commands instead; this command group is for legacy build definitions.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List recent builds in a project as a table (ID, Definition, Status, Result, Branch). Use --definitions to filter to specific definitions (comma-separated IDs). Default page size is 50; use --top to change.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("top")
                                .long("top")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
                                .allow_negative_numbers(true)
                                .help("Maximum number of builds to return. Default 50, max 1000."),
                        )
                        .arg(
                            Arg::new("definitions")
                                .long("definitions")
                                .value_name("IDS")
                                .help(
                                    "Filter to specific definition IDs (comma-separated, e.g. '5,12,18')",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about(
                            "Show details of a specific build: ID, definition, status (inProgress/completed/cancelling/etc.), result (succeeded/failed/partiallySucceeded), branch, requester, queue time, and web URL.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("build_id")
                                .value_name("BUILD_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric build ID"),
                        ),
                )
                .subcommand(
                    Command::new("queue")
                        .about(
                            "Queue a new classic build. The build is added to the queue and starts as soon as an agent is available. Returns the new build ID and a link to monitor it.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("definition")
                                .long("definition")
                                .value_name("ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help(
                                    "Numeric ID of the classic build definition to run (use `ado pipelines-builds definitions list` to find it)",
                                ),
                        )
                        .arg(
                            Arg::new("branch")
                                .long("branch")
                                .value_name("BRANCH")
                                .help(
                                    "Source branch to build. Pass the short name (e.g. 'main'); 'refs/heads/' is added automatically. Default: main.",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("cancel")
                        .about(
                            "Cancel a running or queued build. Sets status to 'cancelling'; the build will stop after the current step completes.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("build_id")
                                .value_name("BUILD_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric build ID"),
                        ),
                )
                .subcommand(
                    Command::new("tags")
                        .about(
                            "Manage tags on a build. Tags are free-form labels useful for marking release builds, hotfixes, or environment deployments.",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List all tags on a build. Output is a comma-separated list of tag names (or 'No tags.' if none).",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("build_id")
                                        .value_name("BUILD_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric build ID"),
                                ),
                        )
                        .subcommand(
                            Command::new("add")
                                .about(
                                    "Add one or more tags to a build. Comma-separated values, e.g. --tags 'release,prod,v1.2.3'. Existing tags are preserved (this is additive, not a replace).",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                )
                                .arg(
                                    Arg::new("build_id")
                                        .value_name("BUILD_ID")
                                        .required(true)
                                        .value_parser(clap::value_parser!(i64))
                                        .help("Numeric build ID"),
                                )
                                .arg(
                                    Arg::new("tags")
                                        .long("tags")
                                        .value_name("TAGS")
                                        .required(true)
                                        .help("Tags to add (comma-separated)"),
                                ),
                        ),
                )
                .subcommand(
                    Command::new("definitions")
                        .about(
                            "Manage classic (XAML) build definitions. For modern YAML pipelines, use `ado pipelines` instead.",
                        )
                        .subcommand(
                            Command::new("list")
                                .about(
                                    "List classic build definitions in a project. Output is a table (ID, Name, Queue). Use the IDs with `queue --definition` to start a build.",
                                )
                                .arg(
                                    Arg::new("project")
                                        .value_name("PROJECT")
                                        .required(true)
                                        .help("Project name or ID"),
                                ),
                        ),
                ),
        )
        .subcommand(
            Command::new("pipelines-artifacts")
                .about(
                    "Manage pipeline run artifacts. Artifacts are files produced by a pipeline run (build outputs, test results, coverage reports, logs) that downstream pipelines or release definitions can consume.",
                )
                .subcommand(
                    Command::new("list")
                        .about(
                            "List all artifacts produced by a single pipeline run. Output is a table (Name, Size in bytes). Use this to discover artifact names before downloading.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("pipeline_id")
                                .value_name("PIPELINE_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric pipeline definition ID"),
                        )
                        .arg(
                            Arg::new("run_id")
                                .value_name("RUN_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help(
                                    "Numeric run ID (from `ci watch` output, the build number, or `pipelines runs` if available)",
                                ),
                        ),
                )
                .subcommand(
                    Command::new("download")
                        .about(
                            "Download a single artifact to a local file. By default saves to './<artifact-name>.zip' in the current directory; pass --output to choose a different path. Useful for retrieving build outputs for inspection or local testing.",
                        )
                        .arg(
                            Arg::new("project")
                                .value_name("PROJECT")
                                .required(true)
                                .help("Project name or ID"),
                        )
                        .arg(
                            Arg::new("pipeline_id")
                                .value_name("PIPELINE_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric pipeline definition ID"),
                        )
                        .arg(
                            Arg::new("run_id")
                                .value_name("RUN_ID")
                                .required(true)
                                .value_parser(clap::value_parser!(i64))
                                .help("Numeric run ID"),
                        )
                        .arg(
                            Arg::new("artifact_name")
                                .value_name("ARTIFACT_NAME")
                                .required(true)
                                .help("Exact artifact name (from `list`). Names are case-sensitive."),
                        )
                        .arg(
                            Arg::new("output")
                                .long("output")
                                .value_name("PATH")
                                .help(
                                    "Local file path to write to. Default: ./<artifact-name>.zip in the current directory. Parent directories are NOT auto-created.",
                                ),
                        ),
                ),
        )
        .subcommand(
            Command::new("schema")
                .about("Dump the CLI command tree as structured JSON for LLM agents.")
                .arg(
                    Arg::new("name")
                        .value_name("NAME")
                        .help("Optional: dump only this command + descendants"),
                ),
        )
        .subcommand(
            Command::new("completion")
                .about(
                    "Generate a shell completion script for the ado CLI.\n\n\
                     Usage:\n  \
                     eval \"$(ado completion bash)\"          # bash\n  \
                     ado completion zsh > \"${fpath[1]}/_ado\"  # zsh\n  \
                     ado completion fish | source            # fish\n  \
                     ado completion powershell | Out-String | Invoke-Expression  # pwsh",
                )
                .arg(
                    Arg::new("shell")
                        .value_name("SHELL")
                        .help("Shell to generate completion for: bash, zsh, fish, powershell. Default: bash"),
                )
                .arg(
                    Arg::new("write-to-file")
                        .short('w')
                        .long("write-to-file")
                        .value_name("PATH")
                        .help(
                            "Write the script to this file path instead of stdout. Useful for installing to a system fpath (e.g. `ado completion zsh -w ~/.zsh/completions/_ado`).",
                        ),
                ),
        )
}
