use std::process::ExitCode;

use ado::cli;
use ado::commands;
use ado::output::{Report, WriteFailure, render};

fn main() -> ExitCode {
    let matches = cli::command().get_matches();

    let (report, json) = match matches.subcommand() {
        Some(("version", version_matches)) => {
            let json = version_matches.get_flag("json");
            (commands::version::run(json), json)
        }
        _ => (
            Report::Text(cli::command().render_help().to_string()),
            false,
        ),
    };

    match render(&report, json) {
        Ok(()) | Err(WriteFailure::BrokenPipe) => ExitCode::SUCCESS,
        Err(WriteFailure::Other(message)) => {
            eprintln!("ado: {message}");
            ExitCode::FAILURE
        }
    }
}
