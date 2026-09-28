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
                    "Manage Azure DevOps pull requests (PRs). A PR is a request to merge code from one branch (source) into another, with required reviewers, policies, and discussion threads.",
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
