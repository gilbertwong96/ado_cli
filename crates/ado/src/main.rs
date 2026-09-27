use std::ffi::OsString;
use std::process::ExitCode;

use ado::args::GlobalOpts;
use ado::argv;
use ado::cli;
use ado::commands;
use ado::output::{Report, WriteFailure, render};

fn main() -> ExitCode {
    let mut raw_args = std::env::args_os();
    let program = raw_args.next().unwrap_or_else(|| OsString::from("ado"));

    let args = match to_utf8(raw_args.collect()) {
        Ok(args) => args,
        Err(arg) => {
            eprintln!("ado: invalid UTF-8 in argument: {}", arg.to_string_lossy());
            return ExitCode::FAILURE;
        }
    };

    if argv::is_version_flag(&args) {
        return emit(&Report::Text(format!("ado {}", ado::VERSION)), false);
    }

    let cli_args = std::iter::once(program)
        .chain(argv::normalize(args).into_iter().map(OsString::from))
        .collect::<Vec<_>>();

    let matches = match cli::command().try_get_matches_from(cli_args) {
        Ok(matches) => matches,
        Err(error) => return clap_exit(&error),
    };

    let globals = GlobalOpts::from_matches(&matches);

    let report = match matches.subcommand() {
        Some(("version", _)) => commands::version::run(globals.json),
        Some((name, _)) => {
            eprintln!("ado: unknown command '{name}'");
            return ExitCode::FAILURE;
        }
        None => Report::Text(cli::command().render_help().to_string()),
    };

    emit(&report, globals.json)
}

fn to_utf8(args: Vec<OsString>) -> Result<Vec<String>, OsString> {
    args.into_iter().map(OsString::into_string).collect()
}

fn clap_exit(error: &clap::Error) -> ExitCode {
    let _ = error.print();

    if error.use_stderr() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn emit(report: &Report, json: bool) -> ExitCode {
    match render(report, json) {
        Ok(()) | Err(WriteFailure::BrokenPipe) => ExitCode::SUCCESS,
        Err(WriteFailure::Other(message)) => {
            eprintln!("ado: {message}");
            ExitCode::FAILURE
        }
    }
}
