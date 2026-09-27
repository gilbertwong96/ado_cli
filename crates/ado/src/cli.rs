use clap::{Arg, ArgAction, Command};

pub fn command() -> Command {
    Command::new("ado")
        .about("Azure DevOps CLI - Manage Azure DevOps projects, repos, work items, and pipelines from the terminal.")
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .subcommand(
            Command::new("version")
                .about("Print the ado version and exit.")
                .arg(
                    Arg::new("json")
                        .long("json")
                        .action(ArgAction::SetTrue)
                        .help("Output as JSON envelope"),
                ),
        )
}
