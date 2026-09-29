//! `ado pipelines-builds list|show|queue|cancel|tags list|tags add|definitions
//! list` — the read and write paths of `lib/ado_cli/cli/builds.ex`: the same REST
//! surface, the same params and bodies, and the same human layouts.
//!
//! `queue`, `cancel` and `tags add` have no question and no `--force` (captured);
//! `queue` without `--definition` and `tags add` without `--tags` are silent exit
//! 0 in the oracle (R4/D34) and usage errors here.

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// `queue_build/1`: `POST /{project}/_apis/build/builds` with the definition id
/// and the branch as a fully-qualified ref (`refs/heads/` added to the short name,
/// `main` when absent). The queued build is the value envelope under `--json`
/// (D33); the human path is the module's three lines, whose URL line reads a
/// missing `_links` as empty (captured).
pub fn queue(
    context: &mut Context,
    project: &str,
    definition: i64,
    branch: Option<String>,
) -> Result<Report, AdoError> {
    let body = queue_body(definition, branch.as_deref());
    let path = builds_collection_path(project);
    let build = context.client()?.post(&path, &body, &[])?;

    Ok(context.json_or_report(ok_value(build.clone()), || queue_lines(&build)))
}

/// `cancel_build/1`: `PATCH /{project}/_apis/build/builds/{build_id}` with
/// `{"status": "cancelling"}`. The build the API answers is the value envelope
/// under `--json` (D33).
pub fn cancel(context: &mut Context, project: &str, build_id: i64) -> Result<Report, AdoError> {
    let path = build_path(project, build_id);
    let build = context
        .client()?
        .patch(&path, &json!({"status": "cancelling"}), &[])?;

    Ok(context.json_or_report(ok_value(build.clone()), || {
        Report::Text(format!("Build #{} cancelled.", id_cell(&build, "id")))
    }))
}

/// `add_tags/1`: `PUT /{project}/_apis/build/builds/{build_id}/tags` with the tags
/// split on commas and each trimmed, as a bare JSON array (captured, including
/// the trim of `'trimmed, spaced '`). The tag-list answer is the value envelope
/// under `--json` (D33); the module's success line names the build id from argv.
pub fn tags_add(
    context: &mut Context,
    project: &str,
    build_id: i64,
    tags: &str,
) -> Result<Report, AdoError> {
    let body = tags_body(tags);
    let path = format!("{}/tags", build_path(project, build_id));
    let response = context.client()?.put(&path, &body, &[])?;
    let message = format!("Tags added to build #{build_id}.");

    Ok(context.json_or_report(ok_value(response.clone()), || Report::Text(message)))
}

/// The module's `queue_build/1` body: the definition by id and the branch with
/// its `refs/heads/` prefix, `main` when `--branch` is absent. An empty branch is
/// present and becomes `refs/heads/` (Elixir's `Map.get` default only applies to
/// an absent option).
fn queue_body(definition: i64, branch: Option<&str>) -> Value {
    json!({
        "definition": {"id": definition},
        "sourceBranch": format!("refs/heads/{}", branch.unwrap_or("main")),
    })
}

/// The module's `add_tags/1` body: `String.split(tags, ",")` then `String.trim/1`
/// on each, so an empty segment between two commas stays an empty tag.
fn tags_body(tags: &str) -> Value {
    Value::Array(
        tags.split(',')
            .map(|tag| Value::String(tag.trim().to_owned()))
            .collect(),
    )
}

/// The module's `queue_build/1` success lines: the queued build's id, status and
/// `_links.web.href`, the last empty when the answer carries neither.
fn queue_lines(build: &Value) -> Report {
    let url = build
        .get("_links")
        .and_then(|links| links.get("web"))
        .and_then(|web| web.get("href"))
        .map(text)
        .unwrap_or_default();

    Report::Text(format!(
        "Build #{} queued.\n  Status: {}\n  URL:    {url}",
        id_cell(build, "id"),
        or_empty(build, "status"),
    ))
}

fn builds_collection_path(project: &str) -> String {
    format!("/{}/_apis/build/builds", encode_path_segment(project))
}

fn build_path(project: &str, build_id: i64) -> String {
    format!("{}/{build_id}", builds_collection_path(project))
}

/// `list_builds/1`: `GET /{project}/_apis/build/builds` with the module's `$top`
/// and `definitions` filters. Under `--json` the body is the value envelope — a
/// bare array under `result`, the kind the module's `Helpers.json_or_format` picks
/// (W1-R12).
pub fn list(
    context: &mut Context,
    project: &str,
    top: Option<i64>,
    definitions: Option<String>,
) -> Result<Report, AdoError> {
    let path = format!("/{}/_apis/build/builds", encode_path_segment(project));
    let builds = items(
        context
            .client()?
            .list(&path, &list_params(top, definitions))?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(builds.clone())), || {
            builds_table(&builds)
        }),
    )
}

/// `show_build/1`: `GET /{project}/_apis/build/builds/{build_id}`, with no params
/// beyond the version. The module answers a 404 with its own message.
pub fn show(context: &mut Context, project: &str, build_id: i64) -> Result<Report, AdoError> {
    let path = format!(
        "/{}/_apis/build/builds/{build_id}",
        encode_path_segment(project)
    );
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(build) => Ok(context.json_or_report(ok_value(build.clone()), || build_detail(&build))),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Build #{build_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `list_tags/1`: `GET /{project}/_apis/build/builds/{build_id}/tags`. The array
/// answers the value envelope under `--json`; the human path is the module's joined
/// `Tags: …` line, or `No tags.` for an empty array. The module's empty check runs
/// before its `json_or_format`, so the frozen binary prints `No tags.` where the
/// JSON contract promises the envelope — the D21 class, ruled at Task 5: the
/// contract wins and the human message stays.
pub fn tags(context: &mut Context, project: &str, build_id: i64) -> Result<Report, AdoError> {
    let path = format!(
        "/{}/_apis/build/builds/{build_id}/tags",
        encode_path_segment(project)
    );
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(Value::Array(tags)) if tags.is_empty() => Ok(context
            .json_or_report(ok_value(Value::Array(Vec::new())), || {
                Report::Text("No tags.".to_owned())
            })),
        Ok(tags) => Ok(context.json_or_report(ok_value(tags.clone()), || tag_list(&tags))),
        Err(error) => Err(error),
    }
}

/// `list_definitions/1`: `GET /{project}/_apis/build/definitions`, with no params
/// beyond the version.
pub fn definitions(context: &mut Context, project: &str) -> Result<Report, AdoError> {
    let path = format!("/{}/_apis/build/definitions", encode_path_segment(project));
    let classic = items(context.client()?.list(&path, &[])?);

    Ok(
        context.json_or_report(ok_value(Value::Array(classic.clone())), || {
            definitions_table(&classic)
        }),
    )
}

/// The module's `list_builds/1` params: `$top` then `definitions`, each added only
/// when the option is present. `if value` is truthy for `0` and `""`, so an
/// explicit zero top and an explicit empty definitions string both reach the wire.
fn list_params(top: Option<i64>, definitions: Option<String>) -> Vec<(String, String)> {
    let mut params = Vec::new();

    if let Some(top) = top {
        params.push(("$top".to_owned(), top.to_string()));
    }
    if let Some(definitions) = definitions {
        params.push(("definitions".to_owned(), definitions));
    }

    params
}

/// The module's `print_builds_table/1`: ID, Definition, Status, Result and Branch.
/// Its rule and the trailing `<n> build(s)` line are the module's own.
fn builds_table(builds: &[Value]) -> Report {
    if builds.is_empty() {
        return Report::Text("No builds found.".to_owned());
    }

    let rows = builds
        .iter()
        .map(|build| {
            vec![
                id_cell(build, "id"),
                nested_or_empty(build, "definition", "name"),
                or_empty(build, "status"),
                or_empty(build, "result"),
                or_empty(build, "sourceBranch"),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Definition".to_owned(),
            "Status".to_owned(),
            "Result".to_owned(),
            "Branch".to_owned(),
        ],
        rows,
    }
}

/// The module's `print_build_detail/1`, minus the colour: the labels and the
/// module's **ASCII** 60-dash rule (unlike `pipelines`' box-drawing one), with the
/// Web line only for a truthy `_links.web`.
fn build_detail(build: &Value) -> Report {
    let mut detail = String::from("\n");

    detail.push_str("Build Details\n");
    detail.push_str(&"-".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:         {}\n", id_cell(build, "id")));
    detail.push_str(&format!(
        "  Definition: {}\n",
        nested_or_empty(build, "definition", "name")
    ));
    detail.push_str(&format!("  Status:     {}\n", or_empty(build, "status")));
    detail.push_str(&format!("  Result:     {}\n", or_empty(build, "result")));
    detail.push_str(&format!(
        "  Branch:     {}\n",
        or_empty(build, "sourceBranch")
    ));
    detail.push_str(&format!(
        "  Requested:  {}\n",
        nested_or_empty(build, "requestedFor", "displayName")
    ));
    detail.push_str(&format!("  Queue:      {}\n", or_empty(build, "queueTime")));

    if let Some(web) = build
        .get("_links")
        .and_then(|links| links.get("web"))
        .filter(|value| truthy(value))
    {
        detail.push_str(&format!(
            "  Web:        {}\n",
            web.get("href").map(text).unwrap_or_default()
        ));
    }

    detail.push('\n');

    Report::Text(detail)
}

/// The module's `list_tags/1` non-empty branch: the names joined with `", "`.
fn tag_list(tags: &Value) -> Report {
    let names = match tags {
        Value::Array(tags) => tags.iter().map(text).collect::<Vec<_>>().join(", "),
        other => text(other),
    };

    Report::Text(format!("Tags: {names}"))
}

/// The module's `print_definitions_table/1`: ID, Name and Queue.
fn definitions_table(definitions: &[Value]) -> Report {
    if definitions.is_empty() {
        return Report::Text("No classic build definitions found.".to_owned());
    }

    let rows = definitions
        .iter()
        .map(|definition| {
            vec![
                id_cell(definition, "id"),
                or_empty(definition, "name"),
                queue_cell(definition),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Name".to_owned(), "Queue".to_owned()],
        rows,
    }
}

/// `d["queue"]["name"] || d["queueStatus"] || ""` — the Elixir `||` is truthy-based,
/// so only a missing, null or false queue name falls through to the status, and an
/// explicit empty name stays empty.
fn queue_cell(definition: &Value) -> String {
    if let Some(name) = definition
        .get("queue")
        .and_then(|queue| queue.get("name"))
        .filter(|value| truthy(value))
    {
        return text(name);
    }

    definition
        .get("queueStatus")
        .filter(|value| truthy(value))
        .map(text)
        .unwrap_or_default()
}

/// `to_string(value["key"])`: `nil` reads as empty, `false` as `"false"`, and every
/// other scalar as its text — the module's two formatters call `to_string/1`
/// directly on the id, with no `|| ""` fallback.
fn id_cell(value: &Value, key: &str) -> String {
    value.get(key).map(text).unwrap_or_default()
}

/// `value["key"] || ""`: only a missing, null or false field is empty.
fn or_empty(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(value) if truthy(value) => text(value),
        _ => String::new(),
    }
}

/// `value["parent"]["key"] || ""`, the nested form the table and the detail use.
fn nested_or_empty(value: &Value, parent: &str, key: &str) -> String {
    value
        .get(parent)
        .and_then(|parent| parent.get(key))
        .filter(|value| truthy(value))
        .map(text)
        .unwrap_or_default()
}

/// Elixir's truthiness for a decoded JSON value: only `null` and `false` are falsy.
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

/// A JSON value as `#{}` would interpolate it: `nil` as empty, a string as itself,
/// anything else by its JSON text.
fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn queue_body_defaults_the_branch_and_prefixes_the_short_name() {
        assert_eq!(
            queue_body(5, None),
            json!({"definition": {"id": 5}, "sourceBranch": "refs/heads/main"}),
            "an absent branch is main, fully qualified"
        );
        assert_eq!(
            queue_body(7, Some("feature/foo")),
            json!({"definition": {"id": 7}, "sourceBranch": "refs/heads/feature/foo"}),
            "the short name is prefixed, never replaced"
        );
        assert_eq!(
            queue_body(5, Some("")),
            json!({"definition": {"id": 5}, "sourceBranch": "refs/heads/"}),
            "an explicit empty branch is present and becomes the bare prefix (Map.get's default applies only to an absent option)"
        );
    }

    #[test]
    fn tags_body_splits_on_commas_and_trims_each_tag() {
        assert_eq!(
            tags_body("release,prod,v1.2.3"),
            json!(["release", "prod", "v1.2.3"])
        );
        assert_eq!(
            tags_body("trimmed, spaced "),
            json!(["trimmed", "spaced"]),
            "the capture's own pair"
        );
        assert_eq!(
            tags_body("a,,b"),
            json!(["a", "", "b"]),
            "String.split keeps the empty segment; the oracle sends it (captured)"
        );
    }

    #[test]
    fn queue_lines_read_a_missing_links_chain_as_empty() {
        let build = json!({
            "id": 200,
            "status": "notStarted",
            "_links": {"web": {"href": "https://example.test/200"}},
        });

        assert_eq!(
            queue_lines(&build),
            Report::Text(
                "Build #200 queued.\n  Status: notStarted\n  URL:    https://example.test/200"
                    .to_owned()
            )
        );

        assert_eq!(
            queue_lines(&json!({"id": 202, "status": "notStarted"})),
            Report::Text("Build #202 queued.\n  Status: notStarted\n  URL:    ".to_owned()),
            "the oracle's Access chain reads nil as empty rather than raising (captured)"
        );
        assert_eq!(
            queue_lines(&json!({"id": null, "_links": {"web": {}}})),
            Report::Text("Build # queued.\n  Status: \n  URL:    ".to_owned()),
            "to_string(nil) is empty and a missing status reads empty"
        );
    }

    #[test]
    fn list_params_keeps_present_values_in_the_modules_order() {
        assert_eq!(
            list_params(None, None),
            Vec::new(),
            "an absent options map sends no param beyond api-version"
        );
        assert_eq!(
            list_params(Some(5), Some("5,12".to_owned())),
            vec![
                ("$top".to_owned(), "5".to_owned()),
                ("definitions".to_owned(), "5,12".to_owned()),
            ],
            "the module's own order: $top, then definitions"
        );
        assert_eq!(
            list_params(Some(0), Some(String::new())),
            vec![
                ("$top".to_owned(), "0".to_owned()),
                ("definitions".to_owned(), String::new()),
            ],
            "present means sent: an explicit zero top and an empty definitions string"
        );
        assert_eq!(
            list_params(Some(-1), None),
            vec![("$top".to_owned(), "-1".to_owned())],
            "a negative top is sent, as the oracle sends it"
        );
    }

    #[test]
    fn builds_table_uses_the_module_columns() {
        let builds = vec![
            json!({
                "id": 128,
                "definition": {"name": "Alpha CI"},
                "status": "completed",
                "result": "succeeded",
                "sourceBranch": "refs/heads/main",
            }),
            json!({
                "id": 127,
                "definition": {"name": "Alpha Nightly"},
                "status": "completed",
                "result": "failed",
                "sourceBranch": "refs/heads/release/1.2",
            }),
        ];

        assert_eq!(
            builds_table(&builds),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Definition".to_owned(),
                    "Status".to_owned(),
                    "Result".to_owned(),
                    "Branch".to_owned(),
                ],
                rows: vec![
                    vec![
                        "128".to_owned(),
                        "Alpha CI".to_owned(),
                        "completed".to_owned(),
                        "succeeded".to_owned(),
                        "refs/heads/main".to_owned(),
                    ],
                    vec![
                        "127".to_owned(),
                        "Alpha Nightly".to_owned(),
                        "completed".to_owned(),
                        "failed".to_owned(),
                        "refs/heads/release/1.2".to_owned(),
                    ],
                ],
            },
            "the formatter's cell values, with the branch as the API sends it"
        );
    }

    #[test]
    fn builds_table_reads_missing_fields_as_the_module_does() {
        let builds = vec![json!({"id": null}), json!({"id": false})];

        assert_eq!(
            builds_table(&builds),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Definition".to_owned(),
                    "Status".to_owned(),
                    "Result".to_owned(),
                    "Branch".to_owned(),
                ],
                rows: vec![
                    vec![
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new()
                    ],
                    vec![
                        "false".to_owned(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new()
                    ],
                ],
            },
            "to_string(nil) is empty and to_string(false) is \"false\" (the module has no || \"\" there)"
        );
    }

    #[test]
    fn builds_table_of_nothing_is_the_module_message() {
        assert_eq!(
            builds_table(&[]),
            Report::Text("No builds found.".to_owned())
        );
    }

    #[test]
    fn build_detail_prints_the_labels_and_the_web_line() {
        let build = json!({
            "_links": {
                "self": {"href": "https://dev.azure.com/myorg/Alpha/_apis/build/Builds/128"},
                "web": {"href": "https://dev.azure.com/myorg/Alpha/_build/results?buildId=128"},
            },
            "definition": {"name": "Alpha CI"},
            "id": 128,
            "queueTime": "2026-09-27T09:12:04.1533333Z",
            "requestedFor": {"displayName": "Alice Example"},
            "result": "succeeded",
            "sourceBranch": "refs/heads/main",
            "status": "completed",
        });

        let Report::Text(detail) = build_detail(&build) else {
            panic!("the detail is a text report");
        };

        assert!(detail.starts_with("\nBuild Details\n"), "detail: {detail}");
        assert!(
            detail.contains(&format!("{}\n", "-".repeat(60))),
            "the module's ASCII rule: {detail}"
        );
        assert!(detail.contains("  ID:         128\n"), "detail: {detail}");
        assert!(
            detail.contains("  Definition: Alpha CI\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Status:     completed\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Result:     succeeded\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Branch:     refs/heads/main\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Requested:  Alice Example\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Queue:      2026-09-27T09:12:04.1533333Z\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(
                "  Web:        https://dev.azure.com/myorg/Alpha/_build/results?buildId=128\n"
            ),
            "detail: {detail}"
        );
        assert!(detail.ends_with("buildId=128\n\n"), "detail: {detail}");
    }

    /// The module's Web guard is truthy-based: a missing or null `_links.web` prints
    /// no line, while an empty web object prints the label empty.
    #[test]
    fn build_detail_guards_the_web_line_like_the_module() {
        for build in [
            json!({}),
            json!({"_links": {}}),
            json!({"_links": {"web": null}}),
        ] {
            let Report::Text(detail) = build_detail(&build) else {
                panic!("the detail is a text report");
            };
            assert!(!detail.contains("  Web:"), "detail: {detail}");
        }

        let Report::Text(web) = build_detail(&json!({"_links": {"web": {}}})) else {
            panic!("the detail is a text report");
        };
        assert!(
            web.contains("  Web:        \n"),
            "an empty web object is truthy, so the label prints empty: {web}"
        );
    }

    #[test]
    fn build_detail_keeps_a_missing_definition_and_queue_empty() {
        let Report::Text(detail) = build_detail(&json!({"id": null})) else {
            panic!("the detail is a text report");
        };

        assert!(detail.contains("  ID:         \n"), "detail: {detail}");
        assert!(detail.contains("  Definition: \n"), "detail: {detail}");
        assert!(detail.contains("  Requested:  \n"), "detail: {detail}");
        assert!(detail.contains("  Queue:      \n"), "detail: {detail}");
    }

    #[test]
    fn definitions_table_uses_the_module_columns_and_the_queue_fallback() {
        let definitions = vec![
            json!({"id": 5, "name": "Alpha CI", "queue": {"name": "Azure Pipelines"}}),
            json!({"id": 7, "name": "Alpha Nightly", "queueStatus": "disabled"}),
        ];

        assert_eq!(
            definitions_table(&definitions),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "Queue".to_owned()],
                rows: vec![
                    vec![
                        "5".to_owned(),
                        "Alpha CI".to_owned(),
                        "Azure Pipelines".to_owned()
                    ],
                    vec![
                        "7".to_owned(),
                        "Alpha Nightly".to_owned(),
                        "disabled".to_owned()
                    ],
                ],
            },
            "the queue name wins, and queueStatus fills in when there is none"
        );
    }

    #[test]
    fn definitions_table_of_nothing_is_the_module_message() {
        assert_eq!(
            definitions_table(&[]),
            Report::Text("No classic build definitions found.".to_owned())
        );
    }

    #[test]
    fn queue_cell_falls_back_only_for_a_missing_null_or_false_name() {
        assert_eq!(
            queue_cell(&json!({"queue": {"name": "Azure Pipelines"}})),
            "Azure Pipelines"
        );
        assert_eq!(
            queue_cell(&json!({"queue": {"name": null}, "queueStatus": "disabled"})),
            "disabled"
        );
        assert_eq!(
            queue_cell(&json!({"queue": {"name": false}, "queueStatus": "disabled"})),
            "disabled"
        );
        assert_eq!(
            queue_cell(&json!({"queue": {}, "queueStatus": "disabled"})),
            "disabled",
            "a missing name is not an empty one"
        );
        assert_eq!(
            queue_cell(&json!({"queue": {"name": ""}, "queueStatus": "disabled"})),
            "",
            "an explicit empty name is truthy in Elixir and stays empty"
        );
        assert_eq!(queue_cell(&json!({})), "");
    }

    #[test]
    fn id_cell_reads_numbers_strings_and_the_scalars_the_module_stringifies() {
        assert_eq!(id_cell(&json!({"id": 128}), "id"), "128");
        assert_eq!(id_cell(&json!({"id": "128"}), "id"), "128");
        assert_eq!(id_cell(&json!({"id": null}), "id"), "");
        assert_eq!(id_cell(&json!({"id": false}), "id"), "false");
        assert_eq!(id_cell(&json!({}), "id"), "");
    }

    #[test]
    fn tag_list_joins_the_names_with_a_comma_and_a_space() {
        assert_eq!(
            tag_list(&json!(["release", "prod", "v1.2.3"])),
            Report::Text("Tags: release, prod, v1.2.3".to_owned())
        );
        assert_eq!(
            tag_list(&json!(["release"])),
            Report::Text("Tags: release".to_owned())
        );
    }
}
