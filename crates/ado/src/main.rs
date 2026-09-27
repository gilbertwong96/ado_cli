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

    fn unwell(kind: io::ErrorKind) -> FailingWriter {
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
        let (mut stdout, mut stderr) = (unwell(io::ErrorKind::PermissionDenied), Vec::new());

        let code = emit_to(&mut stdout, &mut stderr, &report(), false);

        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(written(stderr), "ado: write failed\n");
    }

    #[test]
    fn a_failed_error_render_is_reported_on_stderr() {
        let (mut stdout, mut stderr) = (unwell(io::ErrorKind::PermissionDenied), Vec::new());
        let error = AdoError::validation("no shell");

        let code = emit_error_to(&mut stdout, &mut stderr, &error, true);

        assert_eq!(code, ExitCode::FAILURE);
        assert_eq!(written(stderr), "ado: write failed\n");
    }

    #[test]
    fn a_failed_diagnostic_still_exits_one() {
        let (mut stdout, mut stderr) = (
            unwell(io::ErrorKind::PermissionDenied),
            unwell(io::ErrorKind::PermissionDenied),
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
        let (mut stdout, mut stderr) = (unwell(io::ErrorKind::BrokenPipe), Vec::new());

        assert_eq!(
            emit_to(&mut stdout, &mut stderr, &report(), false),
            ExitCode::SUCCESS
        );
        assert_eq!(
            emit_error_to(
                &mut unwell(io::ErrorKind::BrokenPipe),
                &mut stderr,
                &AdoError::validation("x"),
                true
            ),
            ExitCode::SUCCESS
        );
        assert!(stderr.is_empty(), "stderr: {}", written(stderr));
    }
}
