//! `ado pipelines list|show` — the read paths of `lib/ado_cli/cli/pipelines.ex`:
//! the same REST surface, the same filters, and the same human layouts.
//!
//! `run`, `create`, `update`, `delete`, `vars`, `variables` and `secure-files`
//! are Wave 2 and deliberately absent.

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::Value;

use crate::context::Context;
use crate::output::Report;

/// `list_pipelines/1`: `GET /{project}/_apis/pipelines` with the module's `$top`
/// and `folder` filters. Under `--json` the body is the value envelope — a bare
/// array under `result`, the kind the module's `Helpers.json_or_format` picks
/// (W1-R12).
pub fn list(
    context: &mut Context,
    project: &str,
    top: Option<i64>,
    folder: Option<String>,
) -> Result<Report, AdoError> {
    let path = format!("/{}/_apis/pipelines", encode_path_segment(project));
    let pipelines = items(context.client()?.list(&path, &list_params(top, folder))?);

    Ok(
        context.json_or_report(ok_value(Value::Array(pipelines.clone())), || {
            pipelines_table(&pipelines)
        }),
    )
}

/// `show_pipeline/1`: `GET /{project}/_apis/pipelines/{pipeline_id}`, with no params
/// beyond the version. The module answers a 404 with its own message.
pub fn show(context: &mut Context, project: &str, pipeline_id: i64) -> Result<Report, AdoError> {
    let path = format!(
        "/{}/_apis/pipelines/{pipeline_id}",
        encode_path_segment(project)
    );
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(pipeline) => {
            Ok(context.json_or_report(ok_value(pipeline.clone()), || pipeline_detail(&pipeline)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pipeline #{pipeline_id} not found in project '{project}'"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// The module's `list_pipelines/1` params: `$top` then `folder`, each added only
/// when the option is present. `if value` is truthy for `0` and `""`, so an
/// explicit zero top and an explicit empty folder both reach the wire.
fn list_params(top: Option<i64>, folder: Option<String>) -> Vec<(String, String)> {
    let mut params = Vec::new();

    if let Some(top) = top {
        params.push(("$top".to_owned(), top.to_string()));
    }
    if let Some(folder) = folder {
        params.push(("folder".to_owned(), folder));
    }

    params
}

/// The Elixir's `Client.list/2` unwraps the `value` array; anything else is
/// wrapped as a single element, so the value envelope always carries an array —
/// including a `null` body, which `List.wrap/1` would drop instead.
fn items(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        other => vec![other],
    }
}

/// The module's `print_pipelines_table/1`: the formatter's columns are ID, Name and
/// Folder, and its empty answer is the module's own message.
fn pipelines_table(pipelines: &[Value]) -> Report {
    if pipelines.is_empty() {
        return Report::Text("No pipelines found.".to_owned());
    }

    let rows = pipelines
        .iter()
        .map(|pipeline| {
            vec![
                id_cell(pipeline),
                field(pipeline, "name"),
                folder_cell(pipeline),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Name".to_owned(), "Folder".to_owned()],
        rows,
    }
}

/// The module's `print_pipeline_detail/1`, minus the colour: the labels and
/// fallbacks are the formatter's, and the optional Type/Path/Web lines follow its
/// truthy guards.
fn pipeline_detail(pipeline: &Value) -> Report {
    let mut detail = String::from("\n");

    detail.push_str("Pipeline Details\n");
    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:     {}\n", id_cell(pipeline)));
    detail.push_str(&format!("  Name:   {}\n", field(pipeline, "name")));
    detail.push_str(&format!("  Folder: {}\n", folder_cell(pipeline)));
    detail.push_str(&format!("  URL:    {}\n", field(pipeline, "url")));

    if let Some(configuration) = pipeline.get("configuration").filter(|value| truthy(value)) {
        detail.push_str(&format!(
            "  Type:   {}\n",
            field_or(configuration, "type", "?")
        ));

        if let Some(path) = configuration.get("path").filter(|value| truthy(value)) {
            detail.push_str(&format!("  Path:   {}\n", text(path)));
        }
    }

    if let Some(web) = pipeline
        .get("_links")
        .and_then(|links| links.get("web"))
        .filter(|value| truthy(value))
    {
        detail.push_str(&format!("  Web:    {}\n", field(web, "href")));
    }

    detail.push('\n');

    Report::Text(detail)
}

/// `to_string(p["id"] || "")`: a numeric id reads as its text, a string id as
/// itself, and a missing, null or false id is empty.
fn id_cell(pipeline: &Value) -> String {
    match pipeline.get("id") {
        Some(Value::String(id)) => id.clone(),
        Some(Value::Null) | Some(Value::Bool(false)) | None => String::new(),
        Some(id) => id.to_string(),
    }
}

/// `p["folder"] || "/"`: `nil` and `false` fall back to the root marker, while an
/// explicit empty folder is truthy in Elixir and stays empty.
fn folder_cell(pipeline: &Value) -> String {
    match pipeline.get("folder") {
        Some(value) if truthy(value) => text(value),
        _ => "/".to_owned(),
    }
}

/// The `||` fallback of the module's `configuration["type"] || "?"`: only `nil` and
/// `false` fall through, so an empty type stays empty.
fn field_or(value: &Value, key: &str, fallback: &str) -> String {
    match value.get(key) {
        Some(value) if truthy(value) => text(value),
        _ => fallback.to_owned(),
    }
}

/// Elixir's truthiness for a decoded JSON value: only `null` and `false` are falsy.
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

/// A JSON value as the text `#{}` would interpolate: a string as itself, anything
/// else by its JSON text.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `value["key"] || ""`: a missing or non-string field is empty.
fn field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn list_params_keeps_present_values_in_the_modules_order() {
        assert_eq!(
            list_params(None, None),
            Vec::new(),
            "an absent options map sends no param beyond api-version"
        );
        assert_eq!(
            list_params(Some(5), Some("MyTeam/Frontend".to_owned())),
            vec![
                ("$top".to_owned(), "5".to_owned()),
                ("folder".to_owned(), "MyTeam/Frontend".to_owned()),
            ],
            "the module's own order: $top, then folder"
        );
        assert_eq!(
            list_params(Some(0), Some(String::new())),
            vec![
                ("$top".to_owned(), "0".to_owned()),
                ("folder".to_owned(), String::new()),
            ],
            "present means sent: an explicit zero top and an empty folder"
        );
        assert_eq!(
            list_params(Some(-1), None),
            vec![("$top".to_owned(), "-1".to_owned())],
            "a negative top is sent, as the oracle sends it"
        );
    }

    #[test]
    fn pipelines_table_uses_the_module_columns() {
        let pipelines = vec![
            json!({"id": 12, "name": "Alpha CI", "folder": "\\MyTeam\\Frontend"}),
            json!({"id": 7, "name": "Alpha Nightly", "folder": "\\"}),
        ];

        assert_eq!(
            pipelines_table(&pipelines),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "Folder".to_owned()],
                rows: vec![
                    vec![
                        "12".to_owned(),
                        "Alpha CI".to_owned(),
                        "\\MyTeam\\Frontend".to_owned(),
                    ],
                    vec!["7".to_owned(), "Alpha Nightly".to_owned(), "\\".to_owned()],
                ],
            },
            "the formatter's cell values, with the folder as the API sends it"
        );
    }

    #[test]
    fn pipelines_table_reads_missing_fields_as_the_module_does() {
        let pipelines = vec![json!({"id": "7", "name": null}), json!({"folder": ""})];

        assert_eq!(
            pipelines_table(&pipelines),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "Folder".to_owned()],
                rows: vec![
                    vec!["7".to_owned(), String::new(), "/".to_owned()],
                    vec![String::new(), String::new(), String::new()],
                ],
            },
            "a string id reads as itself; a missing folder is '/', an explicit empty one is not"
        );
    }

    #[test]
    fn pipelines_table_of_nothing_is_the_module_message() {
        assert_eq!(
            pipelines_table(&[]),
            Report::Text("No pipelines found.".to_owned())
        );
    }

    #[test]
    fn pipeline_detail_prints_the_labels_and_the_optional_lines() {
        let pipeline = json!({
            "_links": {
                "web": {"href": "https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=12"},
            },
            "configuration": {"path": "pipelines/ci.yml", "type": "yaml"},
            "folder": "\\MyTeam\\Frontend",
            "id": 12,
            "name": "Alpha CI",
            "url": "https://dev.azure.com/myorg/Alpha/_apis/pipelines/12?revision=4",
        });

        let Report::Text(detail) = pipeline_detail(&pipeline) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.starts_with("\nPipeline Details\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(&format!("{}\n", "─".repeat(60))),
            "detail: {detail}"
        );
        assert!(detail.contains("  ID:     12\n"), "detail: {detail}");
        assert!(detail.contains("  Name:   Alpha CI\n"), "detail: {detail}");
        assert!(
            detail.contains("  Folder: \\MyTeam\\Frontend\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(
                "  URL:    https://dev.azure.com/myorg/Alpha/_apis/pipelines/12?revision=4\n"
            ),
            "detail: {detail}"
        );
        assert!(detail.contains("  Type:   yaml\n"), "detail: {detail}");
        assert!(
            detail.contains("  Path:   pipelines/ci.yml\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(
                "  Web:    https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=12\n"
            ),
            "detail: {detail}"
        );
        assert!(detail.ends_with("definitionId=12\n\n"), "detail: {detail}");
    }

    /// The module's guards are truthy-based: a missing or null `configuration`
    /// prints no Type/Path line, a `configuration` without a type prints `?`, and
    /// the Web line needs a truthy `_links.web`.
    #[test]
    fn pipeline_detail_guards_the_optional_lines_like_the_module() {
        for pipeline in [json!({}), json!({"configuration": null})] {
            let Report::Text(detail) = pipeline_detail(&pipeline) else {
                panic!("the detail is a text report");
            };
            assert!(!detail.contains("  Type:"), "detail: {detail}");
            assert!(!detail.contains("  Path:"), "detail: {detail}");
            assert!(!detail.contains("  Web:"), "detail: {detail}");
        }

        let Report::Text(empty_configuration) = pipeline_detail(&json!({"configuration": {}}))
        else {
            panic!("the detail is a text report");
        };
        assert!(
            empty_configuration.contains("  Type:   ?\n"),
            "an empty configuration is truthy in Elixir: {empty_configuration}"
        );
        assert!(
            !empty_configuration.contains("  Path:"),
            "detail: {empty_configuration}"
        );

        let Report::Text(web) = pipeline_detail(&json!({"_links": {"web": {}}})) else {
            panic!("the detail is a text report");
        };
        assert!(
            web.contains("  Web:    \n"),
            "an empty web object is truthy, so the label prints empty: {web}"
        );

        for pipeline in [json!({"_links": {}}), json!({"_links": {"web": null}})] {
            let Report::Text(detail) = pipeline_detail(&pipeline) else {
                panic!("the detail is a text report");
            };
            assert!(!detail.contains("  Web:"), "detail: {detail}");
        }
    }

    /// `#{pipeline["id"]}` and `pipeline["folder"] || "/"` for values that are not
    /// the usual strings.
    #[test]
    fn pipeline_detail_keeps_an_explicit_empty_type_and_an_empty_folder() {
        let pipeline = json!({
            "configuration": {"type": ""},
            "folder": "",
        });

        let Report::Text(detail) = pipeline_detail(&pipeline) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.contains("  Type:   \n"),
            "an empty type is truthy, not the '?' fallback: {detail}"
        );
        assert!(
            detail.contains("  Folder: \n"),
            "an empty folder is truthy, not the '/' fallback: {detail}"
        );
    }

    #[test]
    fn id_cell_reads_numbers_strings_and_missing_ids() {
        assert_eq!(id_cell(&json!({"id": 12})), "12");
        assert_eq!(id_cell(&json!({"id": "12"})), "12");
        assert_eq!(id_cell(&json!({"id": null})), "");
        assert_eq!(id_cell(&json!({"id": false})), "");
        assert_eq!(id_cell(&json!({})), "");
    }

    #[test]
    fn folder_cell_falls_back_only_for_nil_and_false() {
        assert_eq!(
            folder_cell(&json!({"folder": "\\MyTeam\\Frontend"})),
            "\\MyTeam\\Frontend"
        );
        assert_eq!(folder_cell(&json!({"folder": "\\"})), "\\");
        assert_eq!(folder_cell(&json!({"folder": ""})), "");
        assert_eq!(folder_cell(&json!({"folder": null})), "/");
        assert_eq!(folder_cell(&json!({"folder": false})), "/");
        assert_eq!(folder_cell(&json!({})), "/");
    }
}
