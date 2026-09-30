//! The poll loop `lib/ado_cli/ci/watcher.ex` runs, ported request for request.
//!
//! One tick is the frozen loop's own: fetch the build, render its status, fetch
//! the timeline and print its records, stop when the build is terminal, and
//! otherwise fetch the timeline **again** to stream the logs of every record that
//! is `inProgress` and carries a `log`. The two timeline fetches per
//! non-terminal tick are the frozen chain, not an optimisation: the harness's
//! `sequence`-driven cases assert it (captured: build, timeline, timeline, log,
//! build, timeline, timeline, log, build, timeline for a three-tick watch).
//!
//! Two frozen behaviours are ported deliberately because no ruling reaches them,
//! and both are visible in the captures:
//!
//!   * **Every timeline record re-prints on every tick.** The frozen
//!     `render_timeline_diff/2` returns `%{state | last_timeline: records}` and the
//!     loop **discards** the return value, so the dedup against `last_timeline`
//!     never sees a non-empty baseline. The module's comment claims otherwise; the
//!     capture (`w8-logs`) shows both records on all three ticks.
//!   * **The catch-all status line distinguishes an absent `result` from a `null`
//!     one**: Elixir's `%{"result" => nil}` pattern requires the key. A body with
//!     `"status": "inProgress"` and no result key renders
//!     `? Build 132 · status=inProgress result=nil · <1s` (captured,
//!     `w8-no-result-key`).
//!
//! Ctrl+C is this build's repair (Ruling 3): `signal_hook::flag::register` stores
//! into an `AtomicBool` the loop checks between every request, so a watch exits 2
//! instead of the frozen's ignored signal (captured: the frozen kept polling and
//! had to be `kill -9`ed).

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ado_core::client::{Client, RawBody, encode_path_segment};
use ado_core::error::AdoError;
use serde_json::Value;
use signal_hook::consts::SIGINT;
use signal_hook::flag;

/// The floor the frozen `watch/4` clamps to (`max(Keyword.get(opts, :poll_ms,
/// 2000), 250)`); the CLI's documented rule has already turned anything below it
/// into the default, so this only keeps the library entry point honest.
pub const MIN_POLL_MS: u64 = 250;

/// How the watch ended. Ruling 3's repair splits the frozen's single `:ok` into
/// the three outcomes the command documents, plus the interruption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// `result` succeeded, partiallySucceeded, or absent/unknown on a completed
    /// build — the doc's "0 on success" reads a partial success as one.
    Succeeded,
    /// `result` failed.
    Failed,
    /// `result` canceled, or `status` cancelling (a cancellation in flight).
    Cancelled,
    /// Ctrl+C: the watch stopped before the build did.
    Interrupted,
}

/// Why the loop stopped early.
pub enum Failure {
    /// An API or transport failure the caller renders as the classified error.
    Api(AdoError),
    /// The stream's reader went away; the run is a silent success (spec §6.4).
    BrokenPipe,
}

impl From<AdoError> for Failure {
    fn from(error: AdoError) -> Failure {
        Failure::Api(error)
    }
}

/// Watches `build_id` in `project` until it reaches a terminal state. `out`
/// receives the frozen renderings verbatim — the status line, the record lines and
/// the log content; the final sentence belongs to the caller, which knows the exit
/// pair Ruling 3 promises.
pub fn watch(
    client: &Client,
    build_id: &str,
    project: &str,
    poll_ms: u64,
    out: &mut dyn Write,
) -> Result<Ended, Failure> {
    let poll_ms = poll_ms.max(MIN_POLL_MS);
    let cancel = Arc::new(AtomicBool::new(false));
    // A registration failure only means Ctrl+C keeps the process default; the
    // watch itself is unaffected, so it is not worth failing the run over.
    let _ = flag::register(SIGINT, Arc::clone(&cancel));

    let started_at = Instant::now();
    let mut last_log_lines: HashMap<String, usize> = HashMap::new();

    loop {
        if interrupted(&cancel) {
            return Ok(Ended::Interrupted);
        }

        let build = fetch_build(client, project, build_id)?;
        write_str(out, &render_status(&build, started_at.elapsed()))?;
        write_str(out, "\n")?;

        print_timeline_records(client, project, build_id, out)?;

        if terminal(&build) {
            write_str(out, final_message(&build))?;
            return Ok(ended(&build));
        }

        stream_active_logs(client, project, build_id, &mut last_log_lines, out)?;

        if sleep_interruptible(poll_ms, &cancel) {
            return Ok(Ended::Interrupted);
        }
    }
}

// ── the fetches ─────────────────────────────────────────────────────────────

fn fetch_build(client: &Client, project: &str, build_id: &str) -> Result<Value, Failure> {
    Ok(client.get(&build_path(project, build_id), &[])?)
}

fn fetch_timeline(client: &Client, project: &str, build_id: &str) -> Result<Value, Failure> {
    Ok(client.get(&format!("{}/timeline", build_path(project, build_id)), &[])?)
}

fn build_path(project: &str, build_id: &str) -> String {
    format!(
        "/{}/_apis/build/builds/{build_id}",
        encode_path_segment(project)
    )
}

/// The frozen `render_timeline_diff/2`, minus its dead dedup: every record of the
/// fetched timeline prints. A timeline failure prints nothing — the frozen
/// swallows it and the watch continues (`w8-timeline-500`).
fn print_timeline_records(
    client: &Client,
    project: &str,
    build_id: &str,
    out: &mut dyn Write,
) -> Result<(), Failure> {
    let body = match fetch_timeline(client, project, build_id) {
        Ok(body) => body,
        Err(Failure::Api(_)) => return Ok(()),
        Err(Failure::BrokenPipe) => return Err(Failure::BrokenPipe),
    };

    for record in records_of(&body) {
        write_str(out, &record_line(record))?;
    }

    Ok(())
}

/// The frozen `stream_active_logs/2`: fetch the timeline again, take every record
/// that is `inProgress` **and** carries a truthy `log`, and stream its new
/// content. A timeline failure is swallowed like the first fetch's.
fn stream_active_logs(
    client: &Client,
    project: &str,
    build_id: &str,
    last_log_lines: &mut HashMap<String, usize>,
    out: &mut dyn Write,
) -> Result<(), Failure> {
    let body = match fetch_timeline(client, project, build_id) {
        Ok(body) => body,
        Err(Failure::Api(_)) => return Ok(()),
        Err(Failure::BrokenPipe) => return Err(Failure::BrokenPipe),
    };

    for record in records_of(&body) {
        if record.get("state").and_then(Value::as_str) != Some("inProgress") {
            continue;
        }

        let Some(log) = record.get("log").filter(|value| truthy(value)) else {
            continue;
        };

        let log_id = log.get("id");
        let key = log_key(log_id);
        let last = last_log_lines.get(&key).copied().unwrap_or(0);

        stream_log(client, project, build_id, log_id, last, last_log_lines, out)?;
    }

    Ok(())
}

/// The frozen `stream_log/4`: `GET …/logs/{logId}?id={last + 1}` for the body,
/// `\r\n` normalised to `\n` before printing, and the cursor advanced by the raw
/// body's line count (`count_newlines/1`). An empty body or a failed fetch keeps
/// the cursor where it was, so the next tick retries the same range.
fn stream_log(
    client: &Client,
    project: &str,
    build_id: &str,
    log_id: Option<&Value>,
    last_line: usize,
    last_log_lines: &mut HashMap<String, usize>,
    out: &mut dyn Write,
) -> Result<(), Failure> {
    let key = log_key(log_id);
    let path = format!("{}/logs/{}", build_path(project, build_id), text(log_id));
    let url = client.url_for(&path, &[("id".to_owned(), (last_line + 1).to_string())]);

    let content = match client.get_raw(&url) {
        Ok(mut body) => match read_raw_text(&mut body) {
            Ok(content) => content,
            Err(_) => {
                last_log_lines.entry(key.clone()).or_insert(last_line);
                return Ok(());
            }
        },
        Err(_) => {
            last_log_lines.entry(key.clone()).or_insert(last_line);
            return Ok(());
        }
    };

    if content.is_empty() {
        last_log_lines.entry(key).or_insert(last_line);
    } else {
        write_str(out, &content.replace("\r\n", "\n"))?;
        last_log_lines.insert(key, last_line + count_newlines(&content));
    }

    Ok(())
}

/// The body of a log fetch, streamed and decoded lossily — the frozen prints the
/// raw binary and this build's writer carries text (§8).
fn read_raw_text(body: &mut RawBody) -> Result<String, AdoError> {
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = Vec::new();

    loop {
        let read = body.read_chunk(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ── rendering ───────────────────────────────────────────────────────────────

/// The frozen `render_status/3`'s five clauses, in its order. The `elapsed`
/// argument is the loop's own `format_duration/1` rendering.
fn render_status(build: &Value, elapsed: Duration) -> String {
    let id = text(build.get("id"));
    let definition_name = text(
        build
            .get("definition")
            .and_then(|definition| definition.get("name")),
    );
    let branch = text(build.get("sourceBranch"));
    let elapsed = format_duration(elapsed);
    let status = build.get("status").and_then(Value::as_str);

    match (status, build.get("result")) {
        (Some("inProgress"), Some(Value::Null)) => {
            format!("* Build {id} · {definition_name} · {branch} · running for {elapsed}")
        }
        (Some("completed"), Some(result)) => format!(
            "✓ Build {id} · {definition_name} · {branch} · {} in {elapsed}",
            text(Some(result))
        ),
        (Some("cancelling"), _) => format!("* Build {id} · cancelling… ({elapsed})"),
        (Some("postponed"), _) => format!("* Build {id} · postponed (waiting for resources)"),
        _ => format!(
            "? Build {id} · status={} result={} · {elapsed}",
            text(build.get("status")),
            elixir_inspect(build.get("result"))
        ),
    }
}

/// The frozen `render_final/2`, verbatim: the terminal result's sentence, or
/// nothing for a result it does not name.
fn final_message(build: &Value) -> &'static str {
    match build.get("result").and_then(Value::as_str) {
        Some("succeeded") => "\n  Build succeeded.\n",
        Some("partiallySucceeded") => "\n  Build partially succeeded (some warnings).\n",
        Some("failed") => "\n  Build failed.\n",
        Some("canceled") => "\n  Build was canceled.\n",
        _ => "",
    }
}

/// Ruling 3's classification of a terminal build. Only an explicit `failed` is a
/// failure and only an explicit cancellation is one; anything else — including a
/// missing result, which the frozen's `render_final` also leaves silent — is the
/// doc's success.
fn ended(build: &Value) -> Ended {
    match build.get("result").and_then(Value::as_str) {
        Some("failed") => Ended::Failed,
        Some("canceled") => Ended::Cancelled,
        _ => match build.get("status").and_then(Value::as_str) {
            Some("cancelling") => Ended::Cancelled,
            _ => Ended::Succeeded,
        },
    }
}

/// The frozen `terminal?/1`: `completed` and `cancelling` are the two states the
/// watch stops on (`cancelling` is the cancellation in flight).
fn terminal(build: &Value) -> bool {
    matches!(
        build.get("status").and_then(Value::as_str),
        Some("completed") | Some("cancelling")
    )
}

/// The frozen `print_record/2`: `"  {icon} [{type}] {name}\n"`, with
/// `name || type || "?"` and `type || "record"` under Elixir's truthiness.
fn record_line(record: &Value) -> String {
    let icon = record_icon(record.get("state"));
    let name = record
        .get("name")
        .filter(|value| truthy(value))
        .or_else(|| record.get("type").filter(|value| truthy(value)))
        .map(|value| text(Some(value)))
        .unwrap_or_else(|| "?".to_owned());
    let record_type = record
        .get("type")
        .filter(|value| truthy(value))
        .map(|value| text(Some(value)))
        .unwrap_or_else(|| "record".to_owned());

    format!("  {icon} [{record_type}] {name}\n")
}

/// The frozen `record_icon/1`, which matches the state string exactly — a
/// non-string state takes the default icon, and the strings themselves carry the
/// frozen two-space indent (the caller adds two more).
fn record_icon(state: Option<&Value>) -> &'static str {
    match state.and_then(Value::as_str) {
        Some("completed") => "  ✔",
        Some("inProgress") => "  *",
        Some("failed") => "  ✗",
        Some("skipped") => "  —",
        _ => "  ·",
    }
}

/// The frozen `format_duration/1`, verbatim.
fn format_duration(elapsed: Duration) -> String {
    let ms = elapsed.as_millis();

    if ms < 1000 {
        return "<1s".to_owned();
    }

    let seconds = ms / 1000;

    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

// ── the small frozen predicates ─────────────────────────────────────────────

/// The records of a timeline body; a missing or non-array `records` member is the
/// frozen `{:error, _}` branch's empty list.
fn records_of(body: &Value) -> &[Value] {
    body.get("records")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Elixir's truthiness for a decoded JSON value: only `null` and `false` are
/// falsy.
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

/// `#{term}` interpolation for the JSON scalars these payloads carry: `nil` is
/// empty, a string is itself, everything else its JSON text.
pub fn text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// `inspect/1` of a decoded JSON value, for the catch-all status line. This is a
/// **forced rendering** of the frozen's Elixir term syntax — the captured case is
/// `result=nil`; the container forms are pinned by this build's unit tests (§8).
fn elixir_inspect(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "nil".to_owned(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(text)) => format!("\"{}\"", escape(text)),
        Some(Value::Array(items)) => {
            let items = items
                .iter()
                .map(|item| elixir_inspect(Some(item)))
                .collect::<Vec<_>>()
                .join(", ");

            format!("[{items}]")
        }
        Some(Value::Object(entries)) => {
            let entries = entries
                .iter()
                .map(|(key, value)| {
                    format!("\"{}\" => {}", escape(key), elixir_inspect(Some(value)))
                })
                .collect::<Vec<_>>()
                .join(", ");

            format!("%{{{entries}}}")
        }
    }
}

/// Elixir's inspect escaping for the characters a JSON string can carry.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }

    escaped
}

/// The frozen `count_newlines/1`: split on `\r\n` and `\n`, drop the empty
/// segments, count what remains. The count runs on the **raw** body, before the
/// `\r\n` normalisation, which is why a CRLF line counts once. Rust's `Pattern`
/// has no multi-string split, so the first separator is folded into the second
/// — the replace cannot merge two lines, because a `\r\n` match can never
/// overlap a `\n` one.
fn count_newlines(content: &str) -> usize {
    content
        .replace("\r\n", "\n")
        .split('\n')
        .filter(|line| !line.is_empty())
        .count()
}

/// A log id as the cursor map's key: the value's JSON text, so the number `7` and
/// the string `"7"` stay distinct keys like the frozen's term keys, and a missing
/// id and a `null` one both key `"null"` like the frozen's `nil`.
fn log_key(log_id: Option<&Value>) -> String {
    log_id
        .map(Value::to_string)
        .unwrap_or_else(|| "null".to_owned())
}

// ── the loop's plumbing ─────────────────────────────────────────────────────

fn interrupted(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

/// Sleeps the poll interval in short slices so a signal is noticed promptly; a
/// return of `true` means the watch was cancelled during the sleep.
fn sleep_interruptible(poll_ms: u64, cancel: &AtomicBool) -> bool {
    let deadline = Instant::now() + Duration::from_millis(poll_ms);

    while Instant::now() < deadline {
        if interrupted(cancel) {
            return true;
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }

    interrupted(cancel)
}

fn write_str(out: &mut dyn Write, text: &str) -> Result<(), Failure> {
    out.write_all(text.as_bytes()).map_err(write_failure)
}

fn write_failure(error: std::io::Error) -> Failure {
    if error.kind() == std::io::ErrorKind::BrokenPipe {
        Failure::BrokenPipe
    } else {
        Failure::Api(AdoError::validation(format!(
            "cannot write the watch output: {error}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn completed(result: Value) -> Value {
        json!({
            "id": 123,
            "status": "completed",
            "result": result,
            "sourceBranch": "refs/heads/main",
            "definition": {"name": "Alpha CI"},
        })
    }

    #[test]
    fn the_status_lines_are_the_frozen_five_clauses() {
        let running = json!({"id": 1, "status": "inProgress", "result": null,
            "sourceBranch": "refs/heads/main", "definition": {"name": "CI"}});
        assert_eq!(
            render_status(&running, Duration::from_millis(100)),
            "* Build 1 · CI · refs/heads/main · running for <1s"
        );

        assert_eq!(
            render_status(&completed(json!("succeeded")), Duration::from_millis(2000)),
            "✓ Build 123 · Alpha CI · refs/heads/main · succeeded in 2s"
        );

        let cancelling = json!({"id": 9, "status": "cancelling"});
        assert_eq!(
            render_status(&cancelling, Duration::from_millis(100)),
            "* Build 9 · cancelling… (<1s)"
        );

        let postponed = json!({"id": 9, "status": "postponed"});
        assert_eq!(
            render_status(&postponed, Duration::from_millis(100)),
            "* Build 9 · postponed (waiting for resources)"
        );

        let other = json!({"id": 9, "status": "notStarted"});
        assert_eq!(
            render_status(&other, Duration::from_millis(100)),
            "? Build 9 · status=notStarted result=nil · <1s",
            "the catch-all inspects the missing result as nil"
        );
    }

    #[test]
    fn an_absent_result_is_not_a_null_one() {
        let absent = json!({"id": 132, "status": "inProgress"});
        assert_eq!(
            render_status(&absent, Duration::from_millis(100)),
            "? Build 132 · status=inProgress result=nil · <1s",
            "Elixir's map pattern requires the key, so the inProgress clause misses"
        );

        let null = json!({"id": 132, "status": "inProgress", "result": null});
        assert_eq!(
            render_status(&null, Duration::from_millis(100)),
            "* Build 132 ·  ·  · running for <1s",
            "with the key present the inProgress clause matches and the missing fields read empty"
        );
    }

    #[test]
    fn the_final_messages_are_the_frozen_four() {
        assert_eq!(
            final_message(&completed(json!("succeeded"))),
            "\n  Build succeeded.\n"
        );
        assert_eq!(
            final_message(&completed(json!("partiallySucceeded"))),
            "\n  Build partially succeeded (some warnings).\n"
        );
        assert_eq!(
            final_message(&completed(json!("failed"))),
            "\n  Build failed.\n"
        );
        assert_eq!(
            final_message(&completed(json!("canceled"))),
            "\n  Build was canceled.\n"
        );
        assert_eq!(final_message(&completed(json!(null))), "");
    }

    #[test]
    fn the_exit_classification_is_ruling_threes_contract() {
        assert_eq!(ended(&completed(json!("succeeded"))), Ended::Succeeded);
        assert_eq!(
            ended(&completed(json!("partiallySucceeded"))),
            Ended::Succeeded,
            "a partial success is not the doc's build failure"
        );
        assert_eq!(ended(&completed(json!("failed"))), Ended::Failed);
        assert_eq!(ended(&completed(json!("canceled"))), Ended::Cancelled);
        assert_eq!(
            ended(&json!({"status": "cancelling"})),
            Ended::Cancelled,
            "the terminal cancelling state is a cancellation"
        );
        assert_eq!(ended(&completed(json!(null))), Ended::Succeeded);
    }

    #[test]
    fn terminal_is_completed_or_cancelling() {
        assert!(terminal(&json!({"status": "completed"})));
        assert!(terminal(&json!({"status": "cancelling"})));
        assert!(!terminal(&json!({"status": "inProgress"})));
        assert!(!terminal(&json!({"status": "postponed"})));
        assert!(!terminal(&json!({})));
    }

    #[test]
    fn the_record_lines_use_the_modules_icon_and_fallbacks() {
        assert_eq!(
            record_line(
                &json!({"id": "r1", "state": "completed", "type": "Job", "name": "Compile"})
            ),
            "    ✔ [Job] Compile\n"
        );
        assert_eq!(
            record_line(&json!({"id": "r2", "state": "weirdState", "type": "Task"})),
            "    · [Task] Task\n",
            "a missing name falls back to the type"
        );
        assert_eq!(
            record_line(&json!({"id": "r3", "state": "skipped"})),
            "    — [record] ?\n"
        );
        assert_eq!(
            record_line(&json!({"id": "r4", "state": "failed", "name": "Test"})),
            "    ✗ [record] Test\n"
        );
        assert_eq!(record_line(&json!({"state": 5})), "    · [record] ?\n");
    }

    #[test]
    fn durations_are_the_frozen_format_duration() {
        for (ms, expected) in [
            (0u64, "<1s"),
            (999, "<1s"),
            (1000, "1s"),
            (2000, "2s"),
            (59_999, "59s"),
            (60_000, "1m0s"),
            (119_000, "1m59s"),
            (3_600_000, "1h0m"),
            (3_661_000, "1h1m"),
        ] {
            assert_eq!(
                format_duration(Duration::from_millis(ms)),
                expected,
                "{ms} ms"
            );
        }
    }

    #[test]
    fn newline_counting_matches_the_frozen_split() {
        assert_eq!(count_newlines("Step A\r\nStep B\r\n"), 2);
        assert_eq!(count_newlines("Step C\r\n"), 1);
        assert_eq!(
            count_newlines("a\n\nb"),
            2,
            "trim: true drops the empty segment"
        );
        assert_eq!(count_newlines("\n"), 0);
        assert_eq!(count_newlines(""), 0);
        assert_eq!(count_newlines("one line, no newline"), 1);
        assert_eq!(count_newlines("a\r\nb\nc"), 3, "both separators split");
    }

    #[test]
    fn the_elixir_inspect_covers_the_captured_case_and_the_scalars() {
        assert_eq!(elixir_inspect(None), "nil");
        assert_eq!(elixir_inspect(Some(&json!(null))), "nil");
        assert_eq!(elixir_inspect(Some(&json!(true))), "true");
        assert_eq!(elixir_inspect(Some(&json!(5))), "5");
        assert_eq!(elixir_inspect(Some(&json!(5.5))), "5.5");
        assert_eq!(elixir_inspect(Some(&json!("failed"))), "\"failed\"");
        assert_eq!(
            elixir_inspect(Some(&json!({"a": [1, "b"]}))),
            "%{\"a\" => [1, \"b\"]}"
        );
        assert_eq!(elixir_inspect(Some(&json!("a\"b\n"))), "\"a\\\"b\\n\"");
    }

    #[test]
    fn a_timeline_without_records_is_the_empty_list() {
        assert!(records_of(&json!({})).is_empty());
        assert!(records_of(&json!({"records": 5})).is_empty());
        assert_eq!(records_of(&json!({"records": [{"id": "r"}]})).len(), 1);
    }

    #[test]
    fn the_log_keys_keep_numbers_and_strings_apart_and_nulls_together() {
        assert_eq!(log_key(Some(&json!(7))), "7");
        assert_eq!(log_key(Some(&json!("7"))), "\"7\"");
        assert_eq!(log_key(Some(&json!(null))), "null");
        assert_eq!(log_key(None), "null");
    }

    #[test]
    fn text_interpolates_like_the_frozen_hash_notation() {
        assert_eq!(text(None), "");
        assert_eq!(text(Some(&json!(null))), "");
        assert_eq!(text(Some(&json!("main"))), "main");
        assert_eq!(text(Some(&json!(123))), "123");
        assert_eq!(text(Some(&json!(false))), "false");
    }

    #[test]
    fn a_non_broken_pipe_write_error_is_an_api_failure() {
        match write_failure(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        )) {
            Failure::Api(error) => assert!(
                error.message.starts_with("cannot write the watch output: "),
                "{}",
                error.message
            ),
            Failure::BrokenPipe => panic!("a permission failure is not a broken pipe"),
        }
    }
}
