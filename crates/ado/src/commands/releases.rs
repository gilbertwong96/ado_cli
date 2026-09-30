//! `ado releases list|show` — the whole of `lib/ado_cli/cli/releases.ex`: the
//! classic-release surface under `/{project}/_apis/release/releases`, the
//! table and detail views, and the three list filters.
//!
//! The detail view is byte-for-byte the module's (its box-drawing rules, its
//! `Status:`/`Definition:`/`Created By:`/`Environments:` blocks, and the
//! `definitionEnvironmentId`/`unknown` fallbacks); the list table is this build's
//! style (D37's precedent) over the module's four columns.

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::Value;

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// `ado releases list`: `GET /{project}/_apis/release/releases` with the module's
/// three filters, each sent only when the caller named it. The value array
/// unwraps to the value envelope.
pub fn list(
    context: &mut Context,
    project: &str,
    top: Option<i64>,
    definition_id: Option<i64>,
    status: Option<String>,
) -> Result<Report, AdoError> {
    let releases = items(context.client()?.list(
        &releases_path(project),
        &list_params(top, definition_id, status),
    )?);

    Ok(
        context.json_or_report(ok_value(Value::Array(releases.clone())), || {
            releases_table(&releases)
        }),
    )
}

/// `ado releases show`: `GET …/releases/{release_id}`; a 404 takes the module's
/// `Release #<id> not found in project '<project>'` wording (D4's class).
pub fn show(context: &mut Context, project: &str, release_id: i64) -> Result<Report, AdoError> {
    let path = format!("{}/{release_id}", releases_path(project));

    match context.client()?.get(&path, &[]) {
        Ok(release) => Ok(context.json_or_report(ok_value(release.clone()), || {
            Report::Text(release_detail(&release))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Release #{release_id} not found in project '{project}'"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// The collection path; the module encodes the project with `URI.encode/1` and
/// this build escapes every segment strictly (D22).
fn releases_path(project: &str) -> String {
    format!("/{}/_apis/release/releases", encode_path_segment(project))
}

/// The module's `put_if` chain: `$top`, `definitionId`, `statusFilter`, each only
/// when the option is present. The schema's own names (`definition_id`) are
/// display names; the runnable flags are `--definition-id` (D17).
fn list_params(
    top: Option<i64>,
    definition_id: Option<i64>,
    status: Option<String>,
) -> Vec<(String, String)> {
    let mut params = Vec::new();

    if let Some(top) = top {
        params.push(("$top".to_owned(), top.to_string()));
    }
    if let Some(definition_id) = definition_id {
        params.push(("definitionId".to_owned(), definition_id.to_string()));
    }
    if let Some(status) = status {
        params.push(("statusFilter".to_owned(), status));
    }

    params
}

/// The module's `print_releases_table/1` columns (ID, Name, Status, Created),
/// with its "No releases found." when empty and its ten-byte date truncation.
fn releases_table(releases: &[Value]) -> Report {
    if releases.is_empty() {
        return Report::Text("No releases found.".to_owned());
    }

    let rows = releases
        .iter()
        .map(|release| {
            vec![
                value_text(release.get("id")),
                value_text(release.get("name")),
                status_text(release),
                truncate_date(release.get("createdOn")),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Name".to_owned(),
            "Status".to_owned(),
            "Created".to_owned(),
        ],
        rows,
    }
}

/// The module's `r["status"] || "unknown"`: an absent status is `unknown`.
fn status_text(release: &Value) -> String {
    match release.get("status") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "unknown".to_owned(),
        Some(value) => value_text(Some(value)),
    }
}

/// The module's `truncate_date/1`: nil is empty, a string longer than ten bytes
/// keeps its first ten, anything else prints as it is.
fn truncate_date(date: Option<&Value>) -> String {
    match date {
        Some(Value::String(text)) if text.len() > 10 => text[..10].to_owned(),
        Some(value) => value_text(Some(value)),
        None => String::new(),
    }
}

/// The module's `print_release_detail/1`: the box-drawing rule, the four
/// unconditional fields, and the three conditional blocks (`Definition:`,
/// `Created By:`, `Environments:`).
fn release_detail(release: &Value) -> String {
    let mut detail = String::from("\nRelease Details\n\n");

    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!(
        "  ID:          {}\n",
        value_text(release.get("id"))
    ));
    detail.push_str(&format!(
        "  Name:        {}\n",
        value_text(release.get("name"))
    ));
    detail.push_str(&format!(
        "  Status:      {}\n",
        value_text(release.get("status"))
    ));

    if let Some(definition) = release.get("releaseDefinition") {
        detail.push_str(&format!("  Definition:  {}\n", definition_name(definition)));
    }

    detail.push_str(&format!(
        "  Created On:  {}\n",
        value_text(release.get("createdOn"))
    ));

    if let Some(created_by) = release.get("createdBy") {
        detail.push_str(&format!(
            "  Created By:  {}\n",
            value_text(created_by.get("displayName"))
        ));
    }

    detail.push_str(&format!(
        "  URL:         {}\n",
        value_text(release.get("url"))
    ));

    if let Some(environments) = release.get("environments") {
        detail.push_str("\n  Environments:\n");

        for environment in environments.as_array().into_iter().flatten() {
            detail.push_str(&format!(
                "    - {}: {}\n",
                env_name(environment),
                env_status(environment)
            ));
        }
    }

    detail.push('\n');

    detail
}

/// `definition["name"] || definition["id"]` — only nil/`false` fall through.
fn definition_name(definition: &Value) -> String {
    match definition.get("name") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => value_text(definition.get("id")),
        Some(value) => value_text(Some(value)),
    }
}

/// `env["name"] || env["definitionEnvironmentId"]` — a numeric environment id
/// prints as its number, which is what the capture's third environment shows.
fn env_name(environment: &Value) -> String {
    match environment.get("name") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => {
            value_text(environment.get("definitionEnvironmentId"))
        }
        Some(value) => value_text(Some(value)),
    }
}

/// `env["status"] || "unknown"`.
fn env_status(environment: &Value) -> String {
    match environment.get("status") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "unknown".to_owned(),
        Some(value) => value_text(Some(value)),
    }
}

/// Elixir's `#{term}` interpolation for the JSON scalars these fields carry.
fn value_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_params_name_the_schema_options_and_stay_absent_when_unset() {
        assert_eq!(list_params(None, None, None), Vec::new());
        assert_eq!(
            list_params(Some(5), Some(3), Some("active".to_owned())),
            vec![
                ("$top".to_owned(), "5".to_owned()),
                ("definitionId".to_owned(), "3".to_owned()),
                ("statusFilter".to_owned(), "active".to_owned()),
            ]
        );
    }

    #[test]
    fn the_date_truncation_keeps_ten_bytes_and_passes_short_values_through() {
        assert_eq!(
            truncate_date(Some(&json!("2026-09-20T12:34:56.789Z"))),
            "2026-09-20"
        );
        assert_eq!(truncate_date(Some(&json!("2026-09-18"))), "2026-09-18");
        assert_eq!(truncate_date(None), "");
    }

    #[test]
    fn the_detail_falls_back_for_the_environment_fields() {
        let detail = release_detail(&json!({
            "id": 101,
            "name": "Release-101",
            "status": "active",
            "environments": [
                {"name": "Dev", "status": "succeeded"},
                {"definitionEnvironmentId": 7, "status": "notStarted"},
                {"name": "Bare"},
            ],
        }));

        assert!(
            detail.contains("    - Dev: succeeded\n"),
            "detail: {detail:?}"
        );
        assert!(
            detail.contains("    - 7: notStarted\n"),
            "detail: {detail:?}"
        );
        assert!(
            detail.contains("    - Bare: unknown\n"),
            "detail: {detail:?}"
        );
    }

    #[test]
    fn the_detail_omits_absent_blocks_but_keeps_empty_fields() {
        let detail = release_detail(&json!({"id": 102, "name": "Release-102"}));

        assert!(!detail.contains("Definition:"), "detail: {detail:?}");
        assert!(!detail.contains("Created By:"), "detail: {detail:?}");
        assert!(!detail.contains("Environments:"), "detail: {detail:?}");
        assert!(detail.contains("  Status:      \n"), "detail: {detail:?}");
        assert!(
            detail.ends_with("\n\n"),
            "the trailing blank line: {detail:?}"
        );
    }
}
