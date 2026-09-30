//! `ado ci watch` — the whole of `lib/ado_cli/cli/ci.ex` and its
//! `AdoCli.CI.Watcher`: the `--latest` resolution, the poll loop's request chain
//! (build → timeline → the active logs' `?id=N` content), the status and timeline
//! renderings, and the two repairs this build carries (Rulings 4(a) and 3):
//!
//!   * `--poll-interval` **works as its help documents** — the frozen option is
//!     declared with a hyphen in its key, so `OptionParser`'s dash→underscore
//!     normalisation makes every spelling `invalid option --poll-interval`
//!     (captured). The documented rule is `poll_ms/1`'s own: absent or below the
//!     250 ms floor means the 2000 ms default.
//!   * the exit statuses are the command's documented 0/1/2 — the frozen watcher
//!     returns `:ok` for every terminal state, so a failed build exits 0 and its
//!     Ctrl+C flag is set nowhere (captured: SIGINT is ignored). A succeeded build
//!     exits 0, a failed one 1, a cancelled build or an interrupted watch 2.
//!
//! The live stream is the command's product, so it is written as the loop runs
//! (the `login` announce precedent). Under `--json` the stream is suppressed and
//! stdout carries exactly one document — the final message envelope — because the
//! frozen's `writeln` stream would leave a JSON consumer with prose (the D33
//! convention).

pub mod watcher;

use std::io::Write;

use ado_core::error::AdoError;
use serde_json::Value;

use crate::context::Context;

/// The documented default and floor, verbatim from the frozen help text and
/// `poll_ms/1`: "Default 2000 (2s). Values below 250 are clamped to 2000."
pub const DEFAULT_POLL_MS: i64 = 2000;
pub const MIN_POLL_MS: i64 = 250;

/// The command line as `cli.rs` parsed it. The six inputs are exactly the frozen
/// node's `args` + `opts` (Annex A).
pub struct WatchArgs<'a> {
    pub project: &'a str,
    pub build_id: Option<i64>,
    pub latest: bool,
    pub definition: Option<i64>,
    pub branch: Option<&'a str>,
    pub poll_interval: Option<i64>,
}

/// What the watch ended with: the final sentence and the process status Ruling 3
/// promises. `message` is empty when the stream hit a closed pipe, where the run
/// is the silent success spec §6.4 records.
pub struct WatchOutcome {
    pub message: String,
    pub exit_code: u8,
}

/// Resolves the build, runs the loop against `out` and reports how it ended.
pub fn watch(
    context: &mut Context,
    args: WatchArgs<'_>,
    out: &mut dyn Write,
) -> Result<WatchOutcome, AdoError> {
    let build_id = resolve_build_id(context, &args)?;
    let poll_ms = poll_ms(args.poll_interval);

    // Under `--json` the live stream is suppressed so stdout carries exactly one
    // document (the module docs); the watch itself is unchanged.
    let mut sink = std::io::sink();
    let stream: &mut dyn Write = if context.json() { &mut sink } else { out };

    let ended = match watcher::watch(context.client()?, &build_id, args.project, poll_ms, stream) {
        Ok(ended) => ended,
        Err(watcher::Failure::Api(error)) => return Err(error),
        Err(watcher::Failure::BrokenPipe) => {
            return Ok(WatchOutcome {
                message: String::new(),
                exit_code: 0,
            });
        }
    };

    Ok(outcome(&build_id, ended))
}

/// The frozen `resolve_build_id/3`: a positive positional wins; otherwise
/// `--latest` resolves the newest build; otherwise the module's own refusal. The
/// guard is `is_integer(id) and id > 0`, so `0` and `-1` take the refusal path
/// (captured: `ci watch Alpha -1`, whose `-1` the frozen parser leaves as a
/// positional — D46's negative-number rule).
fn resolve_build_id(context: &mut Context, args: &WatchArgs<'_>) -> Result<String, AdoError> {
    if let Some(id) = args.build_id
        && id > 0
    {
        return Ok(id.to_string());
    }

    if args.latest {
        return fetch_latest_build(context, args);
    }

    Err(AdoError::validation(
        "no build ID given; pass BUILD_ID as the second positional argument or use --latest",
    ))
}

/// The frozen `fetch_latest_build/3`: `GET /{project}/_apis/build/builds` with
/// `$top=1` and the two optional filters in the module's order, then the first
/// entry's `id`. The id is carried as its `#{}` text, because the frozen
/// interpolates whatever the payload carries — a `null` id watches `/builds/`
/// (captured, `w8-latest-null-id`).
fn fetch_latest_build(context: &mut Context, args: &WatchArgs<'_>) -> Result<String, AdoError> {
    let mut params = vec![("$top".to_owned(), "1".to_owned())];

    if let Some(definition) = args.definition {
        params.push(("definitions".to_owned(), definition.to_string()));
    }
    if let Some(branch) = args.branch {
        params.push(("branchName".to_owned(), branch.to_owned()));
    }

    let path = format!(
        "/{}/_apis/build/builds",
        ado_core::client::encode_path_segment(args.project)
    );
    let body = context.client()?.get(&path, &params)?;

    // The frozen's three clauses, in its order: a non-empty `value` list whose
    // first entry carries an `id` resolves it; an **empty** `value` list is the
    // command's own sentence; anything else — a missing `value`, a first entry
    // without an `id` key — is the unexpected-response branch.
    let entries = body.get("value").and_then(Value::as_array);

    match entries {
        Some(entries) if entries.is_empty() => Err(AdoError::not_found(
            "no builds found for this project/definition/branch",
        )),
        Some(entries) => match entries.first().and_then(|entry| entry.get("id")) {
            Some(id) => Ok(watcher::text(Some(id))),
            None => Err(unexpected_latest_response(&body)),
        },
        None => Err(unexpected_latest_response(&body)),
    }
}

fn unexpected_latest_response(body: &Value) -> AdoError {
    AdoError {
        code: ado_core::error::ErrorCode::ApiError,
        status: None,
        message: format!("the builds endpoint answered an unexpected response: {body}"),
        details: None,
    }
}

/// `poll_ms/1`'s rule (Ruling 4(a), the documented clamp): an interval at or above
/// the floor is used as given, and absent or below-floor is the 2000 ms default.
fn poll_ms(interval: Option<i64>) -> u64 {
    match interval {
        Some(ms) if ms >= MIN_POLL_MS => ms as u64,
        _ => DEFAULT_POLL_MS as u64,
    }
}

/// The final sentence and the status pair (Ruling 3). The three terminal kinds
/// carry a message naming the build; an interrupted watch names itself because the
/// build is still running.
fn outcome(build_id: &str, ended: watcher::Ended) -> WatchOutcome {
    match ended {
        watcher::Ended::Succeeded => WatchOutcome {
            message: format!("✓ Build {build_id} completed."),
            exit_code: 0,
        },
        watcher::Ended::Failed => WatchOutcome {
            message: format!("✗ Build {build_id} failed."),
            exit_code: 1,
        },
        watcher::Ended::Cancelled => WatchOutcome {
            message: format!("✗ Build {build_id} canceled."),
            exit_code: 2,
        },
        watcher::Ended::Interrupted => WatchOutcome {
            message: "✗ Watch cancelled.".to_owned(),
            exit_code: 2,
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn poll_ms_is_the_documented_clamp() {
        assert_eq!(poll_ms(None), 2000, "the default");
        assert_eq!(poll_ms(Some(250)), 250, "the floor is accepted");
        assert_eq!(poll_ms(Some(2000)), 2000);
        assert_eq!(
            poll_ms(Some(249)),
            2000,
            "below the floor means the default, as the help says — not the floor"
        );
        assert_eq!(poll_ms(Some(0)), 2000);
        assert_eq!(
            poll_ms(Some(-1)),
            2000,
            "a negative interval is below the floor"
        );
    }

    #[test]
    fn the_outcomes_carry_the_documented_status_pair() {
        assert_eq!(outcome("123", watcher::Ended::Succeeded).exit_code, 0);
        assert_eq!(outcome("123", watcher::Ended::Failed).exit_code, 1);
        assert_eq!(outcome("123", watcher::Ended::Cancelled).exit_code, 2);
        assert_eq!(outcome("123", watcher::Ended::Interrupted).exit_code, 2);
    }

    #[test]
    fn the_outcome_messages_name_the_build_or_the_watch() {
        assert_eq!(
            outcome("123", watcher::Ended::Succeeded).message,
            "✓ Build 123 completed."
        );
        assert_eq!(
            outcome("123", watcher::Ended::Failed).message,
            "✗ Build 123 failed."
        );
        assert_eq!(
            outcome("123", watcher::Ended::Cancelled).message,
            "✗ Build 123 canceled."
        );
        assert_eq!(
            outcome("", watcher::Ended::Interrupted).message,
            "✗ Watch cancelled."
        );
    }

    #[test]
    fn a_null_latest_id_carries_the_empty_text_the_frozen_interpolates() {
        assert_eq!(watcher::text(Some(&json!(null))), "");
        assert_eq!(watcher::text(Some(&json!("abc"))), "abc");
        assert_eq!(watcher::text(Some(&json!(7))), "7");
    }
}
