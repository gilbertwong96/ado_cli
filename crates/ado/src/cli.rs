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
                                .help("Maximum number of projects to return. Default 100, max 1000."),
                        )
                        .arg(
                            Arg::new("skip")
                                .long("skip")
                                .value_name("N")
                                .value_parser(clap::value_parser!(i64))
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
