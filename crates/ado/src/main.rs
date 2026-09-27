use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::ExitCode;

use ado::args::GlobalOpts;
use ado::argv;
use ado::cli;
use ado::commands;
use ado::context::Context;
use ado::output::{Report, WriteFailure, render, render_error, write_bytes};
use ado_core::error::AdoError;

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
    let json = globals.json;

    let result = match matches.subcommand() {
        Some(("version", _)) => commands::version::run(json),
        Some(("whoami", _)) => commands::whoami::run(&Context::load(globals)),
        Some(("schema", sub)) => {
            commands::schema::run(json, sub.get_one::<String>("name").map(String::as_str))
        }
        Some(("completion", sub)) => commands::completion::run(
            sub.get_one::<String>("shell").map(String::as_str),
            sub.get_one::<String>("write-to-file").map(Path::new),
        ),
        Some((name, _)) => Err(AdoError::validation(format!("unknown command '{name}'"))),
        None => Ok(Report::Text(cli::command().render_help().to_string())),
    };

    match result {
        Ok(report) => emit(&report, json),
        Err(error) => emit_error(&error, json),
    }
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

fn emit_error(error: &AdoError, json: bool) -> ExitCode {
    match render_error(error, json) {
        Ok(()) => ExitCode::FAILURE,
        Err(WriteFailure::BrokenPipe) => ExitCode::SUCCESS,
        Err(WriteFailure::Other(message)) => {
            let _ = write_bytes(
                &mut io::stderr().lock(),
                format!("ado: {message}\n").as_bytes(),
            );
            ExitCode::FAILURE
        }
    }
}
