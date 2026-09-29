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
