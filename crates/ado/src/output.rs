use std::io::{self, Write};

use ado_core::envelope;
use ado_core::error::AdoError;
use serde_json::Value;

/// What a command produced, before the output layer writes it.
#[derive(Debug, PartialEq)]
pub enum Report {
    /// A JSON payload: compact and newline-terminated under `--json`, pretty otherwise.
    Json(Value),
    /// Human text, written with a trailing newline when non-empty.
    Text(String),
    /// Bytes written verbatim, including any newline the caller provides.
    Raw(String),
}

#[derive(Debug, PartialEq)]
pub enum WriteFailure {
    BrokenPipe,
    Other(String),
}

pub fn write_bytes(writer: &mut impl Write, bytes: &[u8]) -> Result<(), WriteFailure> {
    match writer.write_all(bytes) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Err(WriteFailure::BrokenPipe),
        Err(error) => Err(WriteFailure::Other(error.to_string())),
    }
}

pub fn render(report: &Report, json: bool) -> Result<(), WriteFailure> {
    render_to(&mut io::stdout().lock(), report, json)
}

fn render_to(writer: &mut impl Write, report: &Report, json: bool) -> Result<(), WriteFailure> {
    match report {
        Report::Json(value) => {
            let serialized = if json {
                serde_json::to_string(value)
            } else {
                serde_json::to_string_pretty(value)
            };

            match serialized {
                Ok(text) => write_bytes(writer, with_trailing_newline(text).as_bytes()),
                Err(error) => Err(WriteFailure::Other(error.to_string())),
            }
        }
        Report::Text(text) => write_bytes(writer, with_trailing_newline(text.clone()).as_bytes()),
        Report::Raw(raw) => write_bytes(writer, raw.as_bytes()),
    }
}

fn with_trailing_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// Render a failure for the binary: the error envelope on stdout under `--json`
/// (agents parse it there), a plain labelled line on stderr otherwise.
pub fn render_error(error: &AdoError, json: bool) -> Result<(), WriteFailure> {
    render_error_to(
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
        error,
        json,
    )
}

fn render_error_to(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    error: &AdoError,
    json: bool,
) -> Result<(), WriteFailure> {
    if json {
        match serde_json::to_string(&envelope::error(error)) {
            Ok(text) => write_bytes(stdout, with_trailing_newline(text).as_bytes()),
            Err(serialize_error) => Err(WriteFailure::Other(serialize_error.to_string())),
        }
    } else {
        let line = format!("[{}] {}\n", error.code.label(), error.message);
        write_bytes(stderr, line.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ado_core::error::AdoError;

    struct BrokenPipeWriter;

    impl Write for BrokenPipeWriter {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the pipe is closed",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_bytes_maps_broken_pipe() {
        assert_eq!(
            write_bytes(&mut BrokenPipeWriter, b"x"),
            Err(WriteFailure::BrokenPipe)
        );
    }

    #[test]
    fn render_propagates_broken_pipe_without_panicking() {
        let report = Report::Text("ado 1.0.0-rc.0".to_owned());

        assert_eq!(
            render_to(&mut BrokenPipeWriter, &report, false),
            Err(WriteFailure::BrokenPipe)
        );
    }

    #[test]
    fn render_text_ends_with_a_newline() {
        let mut output = Vec::new();

        render_to(
            &mut output,
            &Report::Text("ado 1.0.0-rc.0".to_owned()),
            false,
        )
        .unwrap();

        assert_eq!(output, b"ado 1.0.0-rc.0\n");
    }

    #[test]
    fn render_empty_text_writes_nothing() {
        let mut output = Vec::new();

        render_to(&mut output, &Report::Text(String::new()), false).unwrap();

        assert!(output.is_empty());
    }

    #[test]
    fn render_error_json_writes_the_envelope_to_stdout() {
        let error = AdoError::not_found("gone");
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());

        render_error_to(&mut stdout, &mut stderr, &error, true).unwrap();

        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            "{\"error\":{\"code\":\"not_found\",\"message\":\"gone\"},\"ok\":false}\n"
        );
        assert!(stderr.is_empty());
    }

    #[test]
    fn render_error_plain_writes_the_labelled_line_to_stderr() {
        let error = AdoError::not_found("gone");
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());

        render_error_to(&mut stdout, &mut stderr, &error, false).unwrap();

        assert_eq!(String::from_utf8(stderr).unwrap(), "[Not found] gone\n");
        assert!(stdout.is_empty());
    }

    #[test]
    fn render_error_propagates_broken_pipe() {
        let error = AdoError::validation("no shell");

        assert_eq!(
            render_error_to(&mut BrokenPipeWriter, &mut BrokenPipeWriter, &error, true),
            Err(WriteFailure::BrokenPipe)
        );
        assert_eq!(
            render_error_to(&mut BrokenPipeWriter, &mut BrokenPipeWriter, &error, false),
            Err(WriteFailure::BrokenPipe)
        );
    }
}
