use std::ffi::OsStr;
use std::io::{self, IsTerminal, Write};

use ado_core::envelope;
use ado_core::error::AdoError;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Table};
use serde_json::Value;

/// What a command produced, before the output layer writes it.
#[derive(Debug, PartialEq)]
pub enum Report {
    /// A JSON payload, emitted **only** when `--json` is set: a command that also
    /// has a human form picks [`Report::Text`] when `Context::json()` is false,
    /// because the renderer has no `--json` to consult — its other branch is the
    /// pretty printer the renderer's own tests use (spec §6.1).
    Json(Value),
    /// Human text, written with a trailing newline when non-empty.
    Text(String),
    /// Bytes written verbatim, including any newline the caller provides.
    Raw(String),
    /// A human table, rendered by [`render_table`]. Human-only by construction:
    /// commands pick it behind `Context::json_or_report`, and the renderer
    /// refuses it under `--json` (W1-3).
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

#[derive(Debug, PartialEq)]
pub enum WriteFailure {
    BrokenPipe,
    Other(String),
}

pub fn write_bytes(writer: &mut (impl Write + ?Sized), bytes: &[u8]) -> Result<(), WriteFailure> {
    match writer.write_all(bytes) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Err(WriteFailure::BrokenPipe),
        Err(error) => Err(WriteFailure::Other(error.to_string())),
    }
}

pub fn render(report: &Report, json: bool) -> Result<(), WriteFailure> {
    render_to(&mut io::stdout().lock(), report, json)
}

/// [`render`] against the writers a caller owns, so binary-level failure
/// handling can be driven by a test without touching the process's own streams.
pub fn render_to(writer: &mut impl Write, report: &Report, json: bool) -> Result<(), WriteFailure> {
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
        Report::Table { headers, rows } => {
            if json {
                Err(WriteFailure::Other(
                    "internal error: a table cannot be written under --json".to_owned(),
                ))
            } else {
                let table = render_table(
                    headers,
                    rows,
                    terminal_form(
                        io::stdout().is_terminal(),
                        std::env::var_os("NO_COLOR").as_deref(),
                    ),
                );

                write_bytes(writer, with_trailing_newline(table).as_bytes())
            }
        }
    }
}

/// The human table in one of its two forms: `comfy-table`'s bordered table on a
/// terminal, plain padded lines everywhere else, so a pipe or CI reads the same
/// columns without borders or ANSI. No cell style is ever applied, so the table
/// never emits an escape byte, whatever the environment says.
fn render_table(headers: &[String], rows: &[Vec<String>], terminal: bool) -> String {
    if terminal {
        comfy_table(headers, rows)
    } else {
        plain_table(headers, rows)
    }
}

fn comfy_table(headers: &[String], rows: &[Vec<String>]) -> String {
    let mut table = Table::new();
    table
        .load_style(UTF8_FULL.with_rounded_corners())
        .set_header(headers.iter().map(Cell::new));

    for row in rows {
        table.add_row(row.iter().map(Cell::new));
    }

    table.to_string()
}

fn plain_table(headers: &[String], rows: &[Vec<String>]) -> String {
    let widths = column_widths(headers, rows);
    let mut lines = vec![plain_line(headers, &widths), plain_rule(&widths)];

    for row in rows {
        lines.push(plain_line(row, &widths));
    }

    lines.join("\n")
}

fn column_widths(headers: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths = headers
        .iter()
        .map(|header| header.chars().count())
        .collect::<Vec<_>>();

    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if widths.len() <= index {
                widths.push(0);
            }
            widths[index] = widths[index].max(cell.chars().count());
        }
    }

    widths
}

fn plain_line(cells: &[String], widths: &[usize]) -> String {
    let mut line = String::new();

    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            line.push_str("  ");
        }
        match widths.get(index) {
            Some(width) => line.push_str(&format!("{cell:<width$}")),
            None => line.push_str(cell),
        }
    }

    line.trim_end().to_owned()
}

fn plain_rule(widths: &[usize]) -> String {
    widths
        .iter()
        .map(|width| "-".repeat(*width))
        .collect::<Vec<_>>()
        .join("  ")
}

/// Whether the table takes its terminal form: stdout is a terminal and
/// `NO_COLOR` is unset or empty (https://no-color.org).
fn terminal_form(tty: bool, no_color: Option<&OsStr>) -> bool {
    tty && no_color.is_none_or(|value| value.is_empty())
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

/// [`render_error`] against the writers a caller owns; see [`render_to`].
pub fn render_error_to(
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

    fn table() -> Report {
        Report::Table {
            headers: vec!["ID".to_owned(), "Name".to_owned(), "State".to_owned()],
            rows: vec![
                vec![
                    "6a1f8f6e".to_owned(),
                    "Alpha".to_owned(),
                    "wellFormed".to_owned(),
                ],
                vec![
                    "b7c2d0a4".to_owned(),
                    "Beta".to_owned(),
                    "wellFormed".to_owned(),
                ],
            ],
        }
    }

    #[test]
    fn render_refuses_a_table_under_json() {
        let mut output = Vec::new();

        assert_eq!(
            render_to(&mut output, &table(), true),
            Err(WriteFailure::Other(
                "internal error: a table cannot be written under --json".to_owned()
            ))
        );
        assert!(output.is_empty(), "nothing may reach stdout: {output:?}");
    }

    #[test]
    fn render_table_terminal_form_is_a_comfy_table() {
        let rendered = render_table(&["ID".to_owned()], &[vec!["p1".to_owned()]], true);

        assert!(
            rendered.contains('╭') && rendered.contains('│'),
            "the comfy-table borders are missing: {rendered}"
        );
        assert!(rendered.contains("ID") && rendered.contains("p1"));
    }

    #[test]
    fn render_table_pipe_form_is_plain_padded_lines() {
        let rendered = render_table(
            &["ID".to_owned(), "Name".to_owned()],
            &[
                vec!["p1".to_owned(), "Alpha".to_owned()],
                vec!["longer-id".to_owned(), "Beta".to_owned()],
            ],
            false,
        );
        let lines = rendered.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), 4, "header, rule, two rows: {rendered}");
        assert!(lines[0].starts_with("ID"), "header: {rendered}");
        assert!(lines[0].ends_with("Name"), "header: {rendered}");
        assert!(
            lines[1].chars().all(|c| c == '-' || c == ' '),
            "rule: {rendered}"
        );
        assert_eq!(lines[2], "p1         Alpha", "row: {rendered}");
        assert_eq!(lines[3], "longer-id  Beta", "row: {rendered}");
        for marker in ['┌', '│', '─', '\u{1b}'] {
            assert!(
                !rendered.contains(marker),
                "the pipe form must stay plain: {rendered:?}"
            );
        }
    }

    #[test]
    fn render_table_pads_every_column_to_its_widest_cell() {
        let rendered = render_table(
            &["ID".to_owned(), "State".to_owned()],
            &[vec!["p1".to_owned(), "wellFormed".to_owned()]],
            false,
        );

        assert_eq!(rendered.lines().nth(2), Some("p1  wellFormed"));
        assert_eq!(rendered.lines().nth(1), Some("--  ----------"));
    }

    #[test]
    fn render_writes_the_table_form_with_a_trailing_newline() {
        let mut output = Vec::new();

        render_to(&mut output, &table(), false).unwrap();

        let text = String::from_utf8(output).expect("utf-8");
        assert!(text.ends_with('\n'), "text: {text:?}");
        assert!(text.contains("Alpha") && text.contains("Beta"));
    }

    #[test]
    fn render_propagates_broken_pipe_for_a_table() {
        assert_eq!(
            render_to(&mut BrokenPipeWriter, &table(), false),
            Err(WriteFailure::BrokenPipe)
        );
    }

    #[test]
    fn terminal_form_needs_a_terminal_and_no_no_color() {
        assert!(terminal_form(true, None));
        assert!(terminal_form(true, Some(OsStr::new(""))));
        assert!(!terminal_form(true, Some(OsStr::new("1"))));
        assert!(!terminal_form(false, None));
        assert!(!terminal_form(false, Some(OsStr::new(""))));
    }
}
