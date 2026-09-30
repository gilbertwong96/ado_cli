//! `ado test-results list|show|publish` — the whole of
//! `lib/ado_cli/cli/test_results.ex`: the `_apis/test/runs` surface, the list
//! table's stat selection, the run detail, and the three-request publish chain.
//!
//! Four captured shapes decide the code:
//!
//!   * `--top` is the module's `if t = Map.get(parsed.options, :top)`: absent is
//!     no pair, while `0` and a negative are present options (the oracle's
//!     `OptionParser` takes a negative integer) and send `$top`;
//!   * the two hyphen-declared filters (`--build-id`, `--min-last-updated`) and
//!     `publish --build-id` are advertised and unreachable in the frozen parser
//!     (Ruling 4(a): `OptionParser` normalises `--a-b` to `:a_b`, so a
//!     declaration key with a hyphen can never match); this build accepts each as
//!     its help advertises and sends the module's intended `buildIds`,
//!     `minLastUpdatedDate` pair and `build` link;
//!   * a list body without `value` is a silent exit 0 in the oracle (the
//!     `error ->` clause's no-op formatter); this build wraps the whole body as
//!     the single item (D41's class);
//!   * `publish` prints the module's own document under `--json`
//!     (`{"ok":true,"run":{…}}`, D38's class) and its block plus the
//!     `halt_success("Done.")` marker in human mode.
//!
//! The module's third publish request is the request this build sends, not the
//! captured bytes: the frozen `Client.post/3` receives its content-type map as
//! **query params** (`Content-Type=application%2Foctet-stream` glued after the
//! path's own `?api-version=7.1-preview.1&fileName=…`, D25's family) and
//! `JSON.encode!`s the file into a JSON string, where the module's comment says
//! "a simple binary POST with the file content as the raw body". This build sends
//! the bytes with `application/octet-stream` and the two pairs the path meant; the
//! row and the harness case pin both spellings. A non-UTF-8 file is the one
//! captured consequence: the oracle's encode raises after its PATCH and the
//! rescue exits 0 silently, where this build uploads.
//!
//! The frozen list interpolates the project **raw** (`"/#{project}/_apis/test/runs"`),
//! so a space makes Finch refuse the request target before anything leaves; this
//! build escapes every segment strictly (D22) and the integration suite pins both
//! sites. The 90-column rule and the 8/40/12 pads are the module's own rendering;
//! this build's `Report::Table` is §8 surface and keeps the full name (the
//! `extensions`/`branch-policies` precedent).

use std::fs;
use std::io::ErrorKind;

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::AdoError;
use serde_json::{Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The project-scoped collection every path here builds on.
const TEST_RUNS_PATH: &str = "/_apis/test/runs";

/// The preview version the attachments path carries (the module's own literal).
const ATTACHMENTS_API_VERSION: &str = "7.1-preview.1";

/// `ado test-results list PROJECT [--top N] [--build-id ID] [--min-last-updated DATE]`:
/// `GET …/test/runs` with the module's pairs. The two filters are Ruling 4(a)'s
/// repairs — the frozen declaration keys are hyphenated and can never match.
pub fn list(
    context: &mut Context,
    project: &str,
    top: Option<i64>,
    build_id: Option<i64>,
    min_last_updated: Option<&str>,
) -> Result<Report, AdoError> {
    let runs = items(context.client()?.list(
        &collection_path(project),
        &list_params(top, build_id, min_last_updated),
    )?);

    Ok(context.json_or_report(ok_value(Value::Array(runs.clone())), || runs_table(&runs)))
}

/// `ado test-results show PROJECT RUN_ID`: `GET …/test/runs/{run_id}`, with the
/// whole response body as the value envelope's `result`.
pub fn show(context: &mut Context, project: &str, run_id: i64) -> Result<Report, AdoError> {
    let run = context
        .client()?
        .get(&run_path(project, &run_id.to_string()), &[])?;

    Ok(context.json_or_report(ok_value(run.clone()), || Report::Text(run_detail(&run))))
}

/// `ado test-results publish PROJECT --name NAME --file PATH [--build-id ID]`:
/// the module's three requests — create the run, mark it completed (the result is
/// discarded), upload the file — then its document or block. `--build-id` is
/// Ruling 4(a)'s third repair.
pub fn publish(
    context: &mut Context,
    project: &str,
    name: &str,
    file_path: &str,
    build_id: Option<i64>,
) -> Result<Report, AdoError> {
    let content = read_results_file(file_path)?;

    let (run_id, run_id_text) = {
        let client = context.client()?;
        let run = client.post(&collection_path(project), &create_body(name, build_id), &[])?;
        let run_id_text = value_text(run.get("id"));
        let run_id = run.get("id").cloned().unwrap_or(Value::Null);

        // The module discards the PATCH's result (`_ = Client.patch(...)`): a
        // failed completion does not fail the publish (captured).
        let _ = client.patch(
            &run_path(project, &run_id_text),
            &json!({"state": "Completed"}),
            &[],
        );

        client.post_binary(
            &attachments_path(project, &run_id_text),
            &content,
            &attachment_params(file_path),
        )?;

        (run_id, run_id_text)
    };

    Ok(context.json_or_report(publish_document(&run_id, name), || {
        Report::Text(published_block(&run_id_text, name, file_path))
    }))
}

/// The module's `"/#{URI.encode(project)}/_apis/test/runs"`; the frozen path
/// interpolates the project raw, and this build escapes the segment strictly
/// (D22).
fn collection_path(project: &str) -> String {
    format!("/{}{TEST_RUNS_PATH}", encode_path_segment(project))
}

/// One run below the collection, encoded the same way. `run_id` is the response's
/// `id` as the module interpolates it, so an absent id leaves the segment empty.
fn run_path(project: &str, run_id: &str) -> String {
    format!(
        "{}/{}",
        collection_path(project),
        encode_path_segment(run_id)
    )
}

/// The upload target below one run.
fn attachments_path(project: &str, run_id: &str) -> String {
    format!("{}/attachments", run_path(project, run_id))
}

/// The module's `build_list_params/1` in its own order: `$top`, then the two
/// repaired filters. Each is present only when its option is (Elixir's `if x =
/// Map.get(...)` over an integer or a string sends `0` and `""` too).
fn list_params(
    top: Option<i64>,
    build_id: Option<i64>,
    min_last_updated: Option<&str>,
) -> Vec<(String, String)> {
    let mut params = Vec::new();

    if let Some(top) = top {
        params.push(("$top".to_owned(), top.to_string()));
    }

    if let Some(build_id) = build_id {
        params.push(("buildIds".to_owned(), build_id.to_string()));
    }

    if let Some(min_last_updated) = min_last_updated {
        params.push(("minLastUpdatedDate".to_owned(), min_last_updated.to_owned()));
    }

    params
}

/// `create_test_run/3`'s body: the three members, plus the build link when the
/// repaired `--build-id` is given.
fn create_body(name: &str, build_id: Option<i64>) -> Value {
    let mut body = json!({
        "name": name,
        "isAutomated": true,
        "state": "InProgress",
    });

    if let Some(build_id) = build_id {
        body["build"] = json!({"id": build_id});
    }

    body
}

/// The attachments query the module builds and means: the preview version pair
/// and `fileName=<Path.basename/1>`.
fn attachment_params(file_path: &str) -> Vec<(String, String)> {
    vec![
        ("api-version".to_owned(), ATTACHMENTS_API_VERSION.to_owned()),
        ("fileName".to_owned(), basename(file_path)),
    ]
}

/// Elixir's `Path.basename/1` for the forms a path may take: trailing separators
/// are dropped and the last segment is the name (`"/"` and `""` are empty).
fn basename(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_owned()
}

/// The module's `read_file/1`: `{:error, :enoent}` keeps its own sentence, and
/// any other io error takes the D44 family's `Cannot read …` wording (the
/// oracle's `:eisdir` is the inspected reason, captured).
fn read_results_file(path: &str) -> Result<Vec<u8>, AdoError> {
    match fs::read(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Err(AdoError::validation(format!("File not found: {path}")))
        }
        Err(error) => Err(AdoError::validation(format!(
            "Cannot read results file \"{path}\": {error}"
        ))),
    }
}

/// The module's JSON branch: `%{ok: true, run: %{id: run["id"], name: name}}`,
/// the raw response id (a missing one is `null`).
fn publish_document(run_id: &Value, name: &str) -> Value {
    json!({"ok": true, "run": {"id": run_id, "name": name}})
}

/// The module's human block, its hardcoded `_test` URL, the path as given, and
/// the `halt_success("Done.")` marker the capture shows (the `test-coverage`
/// no-data branch's precedent: the marker is part of the frozen human stdout).
fn published_block(run_id: &str, name: &str, file_path: &str) -> String {
    format!(
        concat!(
            "\n✓ Test run #{} created: {}\n",
            "  File {} attached.\n",
            "\n",
            "  View run: https://dev.azure.com/_test/runs?runId={}\n",
            "\n",
            "Done.\n",
        ),
        run_id, name, file_path, run_id,
    )
}

/// `print_imports_table/1`'s sibling for the run list: the module's four columns.
/// No "nothing found" sentence — the frozen view prints the header for an empty
/// list. The frozen pads (8/40/12) and the 90-column rule are its rendering.
fn runs_table(runs: &[Value]) -> Report {
    let rows = runs
        .iter()
        .map(|run| {
            let stats = run
                .get("runStatistics")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();

            vec![
                value_text(run.get("id")),
                value_text(run.get("name")),
                or_default(run.get("state"), "?"),
                format!(
                    "{} / {} / {}",
                    stat_count(stats, "TotalTests").unwrap_or_else(|| "?".to_owned()),
                    stat_count(stats, "Passed").unwrap_or_else(|| "0".to_owned()),
                    stat_count(stats, "Failed").unwrap_or_else(|| "0".to_owned()),
                ),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Name".to_owned(),
            "State".to_owned(),
            "Total / Passed / Failed".to_owned(),
        ],
        rows,
    }
}

/// The module's `find_stat/2`: the first stat whose `outcome` or `state` equals
/// the label, then its `count` — `nil` (or a `false`) falls back, while `0` and
/// `""` are values because Elixir's `||` treats only those two as falsy.
fn stat_count(stats: &[Value], label: &str) -> Option<String> {
    let stat = stats.iter().find(|stat| {
        stat.get("outcome").and_then(Value::as_str) == Some(label)
            || stat.get("state").and_then(Value::as_str) == Some(label)
    })?;

    truthy_text(stat.get("count"))
}

/// `print_run_detail/1`: the module's lines, its `outcome || state || "?"` label
/// padded to 20, a conditional `Build:` line, and the trailing blank line.
fn run_detail(run: &Value) -> String {
    let mut detail = format!(
        concat!(
            "\n",
            "Test Run #{}\n",
            "  Name:        {}\n",
            "  State:       {}\n",
            "  Started:     {}\n",
            "  Completed:   {}\n",
            "  Results:\n",
        ),
        value_text(run.get("id")),
        value_text(run.get("name")),
        value_text(run.get("state")),
        value_text(run.get("startedDate")),
        value_text(run.get("completedDate")),
    );

    for stat in run
        .get("runStatistics")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        detail.push_str(&format!(
            "    {:<20} {}\n",
            stat_label(stat),
            value_text(stat.get("count")),
        ));
    }

    // The module's `if build = run["build"]`: only nil and `false` are falsy, so
    // an empty object prints a line with an empty id.
    if let Some(build) = truthy(run.get("build")) {
        detail.push_str(&format!("  Build:       {}\n", value_text(build.get("id"))));
    }

    detail.push('\n');

    detail
}

/// `s["outcome"] || s["state"] || "?"`.
fn stat_label(stat: &Value) -> String {
    truthy(stat.get("outcome"))
        .or_else(|| truthy(stat.get("state")))
        .map(|value| value_text(Some(value)))
        .unwrap_or_else(|| "?".to_owned())
}

/// Elixir's truthiness: only `nil` and `false` are falsy.
fn truthy(value: Option<&Value>) -> Option<&Value> {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => None,
        Some(value) => Some(value),
    }
}

/// The text of a truthy value, or `None` when the value is falsy.
fn truthy_text(value: Option<&Value>) -> Option<String> {
    truthy(value).map(|value| value_text(Some(value)))
}

/// Elixir's `value || default` for a rendered field: only `nil` and `false`
/// fall back, so `""` prints as itself.
fn or_default(value: Option<&Value>, default: &str) -> String {
    truthy_text(value).unwrap_or_else(|| default.to_owned())
}

/// Elixir's `#{term}` interpolation for the JSON scalars these fields carry: a
/// missing or `false` value reads empty (the module's `x || ""`), a string as
/// itself, a number in its JSON spelling.
fn value_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paths_escape_the_project_and_the_run_id_as_one_segment_each() {
        assert_eq!(collection_path("Alpha"), "/Alpha/_apis/test/runs");
        assert_eq!(
            collection_path("Alpha Beta"),
            "/Alpha%20Beta/_apis/test/runs",
            "the frozen path interpolates the project raw (D22)"
        );
        assert_eq!(run_path("Alpha", "501"), "/Alpha/_apis/test/runs/501");
        assert_eq!(
            attachments_path("Alpha", "501"),
            "/Alpha/_apis/test/runs/501/attachments"
        );
    }

    #[test]
    fn the_list_params_are_the_modules_three_pairs_in_order() {
        assert!(list_params(None, None, None).is_empty());
        assert_eq!(
            list_params(Some(1), None, None),
            vec![("$top".to_owned(), "1".to_owned())]
        );
        assert_eq!(
            list_params(Some(0), Some(42), Some("2026-01-01")),
            vec![
                ("$top".to_owned(), "0".to_owned()),
                ("buildIds".to_owned(), "42".to_owned()),
                ("minLastUpdatedDate".to_owned(), "2026-01-01".to_owned()),
            ],
            "zero is a present option and the two filters follow in the module's order"
        );
        assert_eq!(
            list_params(None, None, Some("")),
            vec![("minLastUpdatedDate".to_owned(), String::new())],
            "a present empty value is sent"
        );
    }

    #[test]
    fn the_create_body_carries_the_build_only_when_the_option_is_given() {
        assert_eq!(
            create_body("Nightly", None),
            json!({"name": "Nightly", "isAutomated": true, "state": "InProgress"})
        );
        assert_eq!(
            create_body("Nightly", Some(42)),
            json!({
                "name": "Nightly",
                "isAutomated": true,
                "state": "InProgress",
                "build": {"id": 42}
            }),
            "the build link Ruling 4(a) repairs"
        );
    }

    #[test]
    fn the_attachments_query_is_the_preview_version_and_the_basename() {
        assert_eq!(
            attachment_params("sub/results.xml"),
            vec![
                ("api-version".to_owned(), "7.1-preview.1".to_owned()),
                ("fileName".to_owned(), "results.xml".to_owned()),
            ]
        );
    }

    #[test]
    fn the_basename_is_elixirs_path_basename() {
        assert_eq!(basename("results.xml"), "results.xml");
        assert_eq!(basename("sub/results.xml"), "results.xml");
        assert_eq!(basename("a/b/"), "b", "trailing separators are dropped");
        assert_eq!(basename("/"), "");
        assert_eq!(basename(""), "");
        assert_eq!(basename("a/.."), "..");
    }

    #[test]
    fn the_stat_lookup_matches_outcome_or_state_and_falls_back_on_a_falsy_count() {
        let stats = json!([
            {"state": "TotalTests", "count": 12},
            {"state": "Passed", "count": null},
            {"outcome": "Failed"},
            {"outcome": "TotalTests", "count": 99}
        ]);
        let stats = stats.as_array().expect("an array");

        assert_eq!(
            stat_count(stats, "TotalTests"),
            Some("12".to_owned()),
            "the first matching stat wins, and `state` matches too"
        );
        assert_eq!(
            stat_count(stats, "Passed"),
            None,
            "a null count is falsy and falls back"
        );
        assert_eq!(stat_count(stats, "Failed"), None, "a missing count too");
        assert_eq!(stat_count(stats, "Skipped"), None, "no match at all");
        assert_eq!(
            stat_count(
                json!([{"outcome": "Failed", "count": 0}])
                    .as_array()
                    .expect("an array"),
                "Failed"
            ),
            Some("0".to_owned()),
            "zero is a value in Elixir"
        );
        assert_eq!(
            stat_count(
                json!([{"outcome": "Failed", "count": ""}])
                    .as_array()
                    .expect("an array"),
                "Failed"
            ),
            Some(String::new()),
            "an empty string is truthy and prints as itself"
        );
    }

    #[test]
    fn the_run_table_reads_the_modules_columns_and_fallbacks() {
        let runs = json!([
            {"id": 101, "name": "Nightly", "state": "Completed",
             "runStatistics": [{"outcome": "TotalTests", "count": 42},
                               {"outcome": "Passed", "count": 40},
                               {"outcome": "Failed", "count": 2}]},
            {"id": 102, "name": "No stats"}
        ]);
        let rows = match runs_table(runs.as_array().expect("an array")) {
            Report::Table { rows, .. } => rows,
            other => panic!("a table: {other:?}"),
        };

        assert_eq!(rows[0], vec!["101", "Nightly", "Completed", "42 / 40 / 2"]);
        assert_eq!(
            rows[1],
            vec!["102", "No stats", "?", "? / 0 / 0"],
            "a missing state is `?`; a missing runStatistics is `? / 0 / 0`"
        );
    }

    #[test]
    fn the_stats_text_reads_a_false_state_as_the_question_mark() {
        let runs = json!([{"id": 1, "name": "x", "state": false}]);
        let rows = match runs_table(runs.as_array().expect("an array")) {
            Report::Table { rows, .. } => rows,
            other => panic!("a table: {other:?}"),
        };

        assert_eq!(rows[0][2], "?", "`false || \"?\"` is the question mark");
    }

    #[test]
    fn the_detail_prints_the_modules_lines_and_an_empty_count() {
        let run = json!({
            "id": 44, "name": "State-labelled", "state": "InProgress",
            "startedDate": null, "completedDate": "2026-09-26T22:31:07Z",
            "runStatistics": [{"state": "TotalTests", "count": 9},
                              {"state": "Passed", "count": null},
                              {"outcome": "Failed"}],
            "build": {"id": 501}
        });

        assert_eq!(
            run_detail(&run),
            concat!(
                "\n",
                "Test Run #44\n",
                "  Name:        State-labelled\n",
                "  State:       InProgress\n",
                "  Started:     \n",
                "  Completed:   2026-09-26T22:31:07Z\n",
                "  Results:\n",
                "    TotalTests           9\n",
                "    Passed               \n",
                "    Failed               \n",
                "  Build:       501\n",
                "\n",
            )
        );
    }

    #[test]
    fn the_detail_omits_the_build_line_for_a_falsy_build() {
        let bare = json!({"id": 43});

        assert_eq!(
            run_detail(&bare),
            concat!(
                "\n",
                "Test Run #43\n",
                "  Name:        \n",
                "  State:       \n",
                "  Started:     \n",
                "  Completed:   \n",
                "  Results:\n",
                "\n",
            )
        );

        let empty_build = json!({"id": 43, "build": {}});
        assert!(
            run_detail(&empty_build).contains("  Build:       \n"),
            "an empty object is truthy and prints an empty id"
        );
    }

    #[test]
    fn the_publish_document_carries_the_raw_response_id() {
        assert_eq!(
            publish_document(&json!(501), "Nightly"),
            json!({"ok": true, "run": {"id": 501, "name": "Nightly"}})
        );
        assert_eq!(
            publish_document(&Value::Null, "Nightly"),
            json!({"ok": true, "run": {"id": null, "name": "Nightly"}}),
            "a missing id is null in the document, empty in the block"
        );
    }

    #[test]
    fn the_published_block_keeps_the_modules_shape_and_marker() {
        assert_eq!(
            published_block("501", "Nightly Regression", "sub/results.xml"),
            concat!(
                "\n",
                "✓ Test run #501 created: Nightly Regression\n",
                "  File sub/results.xml attached.\n",
                "\n",
                "  View run: https://dev.azure.com/_test/runs?runId=501\n",
                "\n",
                "Done.\n",
            ),
            "the path prints as given and the marker closes the block"
        );
    }
}
