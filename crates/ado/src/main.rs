use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use ado::args::GlobalOpts;
use ado::argv;
use ado::cli;
use ado::commands;
use ado::context::Context;
use ado::output::{Report, WriteFailure, render_error_to, render_to, write_bytes};
use ado_core::error::AdoError;

/// CliMate's framework wording for a bare `ado`; the Elixir oracle prints the
/// same line, and nothing in `lib/` carries the string.
const MISSING_SUBCOMMAND: &str = "missing sub-command";

fn main() -> ExitCode {
    let mut raw_args = std::env::args_os();
    let program = raw_args.next().unwrap_or_else(|| OsString::from("ado"));

    let args = match to_utf8(raw_args.collect()) {
        Ok(args) => args,
        Err(arg) => {
            return fail(format!(
                "invalid UTF-8 in argument: {}",
                arg.to_string_lossy()
            ));
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
        Some(("projects", sub)) => {
            let mut context = Context::load(globals);

            match sub.subcommand() {
                Some(("list", list)) => commands::projects::list(
                    &mut context,
                    list.get_one::<String>("state").cloned(),
                    list.get_one::<i64>("top").copied(),
                    list.get_one::<i64>("skip").copied(),
                ),
                Some(("show", show)) => commands::projects::show(
                    &mut context,
                    show.get_one::<String>("project_id")
                        .expect("the positional is required")
                        .as_str(),
                    show.get_flag("capabilities"),
                ),
                _ => Err(AdoError::validation(MISSING_SUBCOMMAND)),
            }
        }
        Some(("repos", sub)) => {
            let mut context = Context::load(globals);

            match sub.subcommand() {
                Some(("list", list)) => commands::repos::list(
                    &mut context,
                    list.get_one::<String>("project")
                        .expect("the positional is required")
                        .as_str(),
                    list.get_flag("include_links"),
                ),
                Some(("show", show)) => commands::repos::show(
                    &mut context,
                    show.get_one::<String>("project")
                        .expect("the positional is required")
                        .as_str(),
                    show.get_one::<String>("repo_id")
                        .expect("the positional is required")
                        .as_str(),
                ),
                Some(("branches", branches)) => commands::repos::branches(
                    &mut context,
                    branches
                        .get_one::<String>("project")
                        .expect("the positional is required")
                        .as_str(),
                    branches
                        .get_one::<String>("repo_id")
                        .expect("the positional is required")
                        .as_str(),
                    branches.get_one::<String>("filter").cloned(),
                ),
                _ => Err(AdoError::validation(MISSING_SUBCOMMAND)),
            }
        }
        Some(("prs", sub)) => {
            let mut context = Context::load(globals);

            match sub.subcommand() {
                Some(("list", list)) => commands::pull_requests::list(
                    &mut context,
                    list.get_one::<String>("project")
                        .expect("the positional is required")
                        .as_str(),
                    list.get_one::<String>("repo_id")
                        .expect("the positional is required")
                        .as_str(),
                    list.get_one::<String>("status").cloned(),
                    list.get_one::<String>("creator").cloned(),
                    list.get_one::<i64>("top").copied(),
                ),
                Some(("show", show)) => commands::pull_requests::show(
                    &mut context,
                    show.get_one::<String>("project")
                        .expect("the positional is required")
                        .as_str(),
                    show.get_one::<String>("repo_id")
                        .expect("the positional is required")
                        .as_str(),
                    show.get_one::<i64>("pr_id")
                        .copied()
                        .expect("the positional is required"),
                ),
                _ => Err(AdoError::validation(MISSING_SUBCOMMAND)),
            }
        }
        Some(("workitems", sub)) => {
            let mut context = Context::load(globals);

            match sub.subcommand() {
                Some(("list", list)) => commands::workitems::list(
                    &mut context,
                    list.get_one::<String>("project")
                        .expect("the positional is required")
                        .as_str(),
                    list.get_one::<String>("type").cloned(),
                    list.get_one::<String>("assigned-to").cloned(),
                    list.get_one::<String>("state").cloned(),
                    list.get_one::<i64>("top").copied(),
                ),
                Some(("show", show)) => commands::workitems::show(
                    &mut context,
                    show.get_one::<i64>("id")
                        .copied()
                        .expect("the positional is required"),
                    show.get_one::<String>("expand")
                        .expect("the default is set")
                        .as_str(),
                ),
                Some(("query", query)) => commands::workitems::query(
                    &mut context,
                    query
                        .get_one::<String>("project")
                        .expect("the positional is required")
                        .as_str(),
                    query.get_one::<String>("wiql").cloned(),
                    query.get_one::<i64>("top").copied(),
                ),
                _ => Err(AdoError::validation(MISSING_SUBCOMMAND)),
            }
        }
        Some(("schema", sub)) => {
            commands::schema::run(json, sub.get_one::<String>("name").map(String::as_str))
        }
        Some(("completion", sub)) => commands::completion::run(
            sub.get_one::<String>("shell").map(String::as_str),
            sub.get_one::<String>("write-to-file").map(Path::new),
        ),
        Some((name, _)) => Err(AdoError::validation(format!("unknown command '{name}'"))),
        None => return missing_subcommand(json),
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

/// Bare `ado` is a missing subcommand: root help on stdout and the labelled error
/// line on stderr, exit 1. Under `--json` only the envelope is written, so stdout
/// stays parseable JSON.
fn missing_subcommand(json: bool) -> ExitCode {
    let error = AdoError::validation(MISSING_SUBCOMMAND);

    if json {
        return emit_error(&error, true);
    }

    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let help = Report::Text(cli::command().render_help().to_string());

    match render_to(&mut stdout, &help, false) {
        Ok(()) => emit_error_to(&mut stdout, &mut stderr, &error, false),
        Err(WriteFailure::BrokenPipe) => ExitCode::SUCCESS,
        Err(WriteFailure::Other(message)) => fail_to(&mut stderr, &message),
    }
}

fn emit(report: &Report, json: bool) -> ExitCode {
    emit_to(
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
        report,
        json,
    )
}

fn emit_to(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    report: &Report,
    json: bool,
) -> ExitCode {
    match render_to(stdout, report, json) {
        Ok(()) | Err(WriteFailure::BrokenPipe) => ExitCode::SUCCESS,
        Err(WriteFailure::Other(message)) => fail_to(stderr, &message),
    }
}

fn emit_error(error: &AdoError, json: bool) -> ExitCode {
    emit_error_to(
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
        error,
        json,
    )
}

fn emit_error_to(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    error: &AdoError,
    json: bool,
) -> ExitCode {
    match render_error_to(stdout, stderr, error, json) {
        Ok(()) => ExitCode::FAILURE,
        Err(WriteFailure::BrokenPipe) => ExitCode::SUCCESS,
        Err(WriteFailure::Other(message)) => fail_to(stderr, &message),
    }
}

/// A diagnostic on stderr, then exit 1.
fn fail(message: impl AsRef<str>) -> ExitCode {
    fail_to(&mut io::stderr().lock(), message.as_ref())
}

/// The write goes through [`write_bytes`]: `eprintln!` panics on an unwritable
/// stream, which aborts under `panic = "abort"` instead of exiting 1 (R19). The
/// failure being reported is already fatal, so a second one leaves nothing to do
/// with but exit 1.
fn fail_to(stderr: &mut impl Write, message: &str) -> ExitCode {
    let _ = write_bytes(stderr, format!("ado: {message}\n").as_bytes());
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FailingWriter(io::ErrorKind);

    impl Write for FailingWriter {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(self.0, "write failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn failing(kind: io::ErrorKind) -> FailingWriter {
        FailingWriter(kind)
    }

    fn report() -> Report {
        Report::Text(format!("ado {}", ado::VERSION))
    }

    fn written(bytes: Vec<u8>) -> String {
        String::from_utf8(bytes).expect("utf-8")
    }

    #[test]
    fn a_failed_render_is_reported_on_stderr() {
        let (mut stdout, mut stderr) = (failing(io::ErrorKind::PermissionDenied), Vec::new());

        let code = emit_to(&mut stdout, &mut stderr, &report(), false);

        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(written(stderr), "ado: write failed\n");
    }

    #[test]
    fn a_failed_error_render_is_reported_on_stderr() {
        let (mut stdout, mut stderr) = (failing(io::ErrorKind::PermissionDenied), Vec::new());
        let error = AdoError::validation("no shell");

        let code = emit_error_to(&mut stdout, &mut stderr, &error, true);

        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(written(stderr), "ado: write failed\n");
    }

    #[test]
    fn a_failed_diagnostic_still_exits_one() {
        let (mut stdout, mut stderr) = (
            failing(io::ErrorKind::PermissionDenied),
            failing(io::ErrorKind::PermissionDenied),
        );

        assert_eq!(
            emit_to(&mut stdout, &mut stderr, &report(), false),
            ExitCode::FAILURE
        );
        assert_eq!(
            emit_error_to(&mut stdout, &mut stderr, &AdoError::validation("x"), true),
            ExitCode::FAILURE
        );
    }

    #[test]
    fn a_broken_pipe_is_a_silent_success() {
        let (mut stdout, mut stderr) = (failing(io::ErrorKind::BrokenPipe), Vec::new());

        assert_eq!(
            emit_to(&mut stdout, &mut stderr, &report(), false),
            ExitCode::SUCCESS
        );
        assert_eq!(
            emit_error_to(
                &mut failing(io::ErrorKind::BrokenPipe),
                &mut stderr,
                &AdoError::validation("x"),
                true
            ),
            ExitCode::SUCCESS
        );
        assert!(stderr.is_empty(), "stderr: {}", written(stderr));
    }
}
