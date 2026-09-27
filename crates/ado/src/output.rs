use std::io::{self, Write};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
