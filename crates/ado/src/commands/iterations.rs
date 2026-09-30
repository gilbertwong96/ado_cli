//! `ado iterations list|show|create|update|delete` — the sprint half of
//! `lib/ado_cli/cli/iterations.ex`: a team's `work/teamsettings/iterations`
//! surface, the table and detail views, and the module's own "Iteration not
//! found" wording. The date options are the wave's D39 repair: the frozen
//! `put_in/3` crashed before sending, so this build sends the body that crash
//! intended (`{"name": …, "attributes": {"startDate": …, "finishDate": …}}`).
//! `delete` never prompts (R1).

use ado_core::client::encode_path_segment;
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Map, Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The module's guard wording when `update` carries no option at all.
const NO_OPTIONS: &str = "At least one option is required.";

/// `ado iterations list`: `GET …/iterations`, with `$timeframe=current` when
/// `--current` is given. `Client.list` unwraps the `value` array and the value
/// envelope carries it; the human path is the module's four-column table.
pub fn list(
    context: &mut Context,
    project: &str,
    team: &str,
    current: bool,
) -> Result<Report, AdoError> {
    let params = current_params(current);
    let iterations = items(
        context
            .client()?
            .list(&iterations_path(project, team), &params)?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(iterations.clone())), || {
            iterations_table(&iterations)
        }),
    )
}

/// `ado iterations show`: `GET …/iterations/{id}`; a 404 takes the module's own
/// wording.
pub fn show(
    context: &mut Context,
    project: &str,
    team: &str,
    iteration_id: &str,
) -> Result<Report, AdoError> {
    let path = iteration_path(project, team, iteration_id);

    match context.client()?.get(&path, &[]) {
        Ok(iteration) => Ok(context.json_or_report(ok_value(iteration.clone()), || {
            Report::Text(iteration_detail(&iteration))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: "Iteration not found".to_owned(),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado iterations create`: `POST` with the name and, when either date is given,
/// the `attributes` map the frozen `put_in/3` intended (D39).
pub fn create(
    context: &mut Context,
    project: &str,
    team: &str,
    name: &str,
    start_date: Option<String>,
    finish_date: Option<String>,
) -> Result<Report, AdoError> {
    let body = create_body(name, start_date.as_deref(), finish_date.as_deref());
    let iteration = context
        .client()?
        .post(&iterations_path(project, team), &body, &[])?;

    Ok(context.json_or_report(ok_value(iteration.clone()), || {
        Report::Text(format!(
            "Iteration '{}' created.",
            value_text(iteration.get("name"))
        ))
    }))
}

/// `ado iterations update`: `PATCH` with only the fields given; no field at all
/// is the module's guard, which precedes any request. A 404 takes the module's
/// own wording.
pub fn update(
    context: &mut Context,
    project: &str,
    team: &str,
    iteration_id: &str,
    name: Option<String>,
    start_date: Option<String>,
    finish_date: Option<String>,
) -> Result<Report, AdoError> {
    if name.is_none() && start_date.is_none() && finish_date.is_none() {
        return Err(AdoError::validation(NO_OPTIONS));
    }

    let body = update_body(
        name.as_deref(),
        start_date.as_deref(),
        finish_date.as_deref(),
    );
    let path = iteration_path(project, team, iteration_id);

    match context.client()?.patch(&path, &body, &[]) {
        Ok(iteration) => Ok(context.json_or_report(ok_value(iteration.clone()), || {
            Report::Text(format!(
                "Iteration '{}' updated.",
                value_text(iteration.get("name"))
            ))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: "Iteration not found".to_owned(),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado iterations delete`: the module's plain `DELETE`, without a prompt (R1).
/// A 404 takes the module's own wording.
pub fn delete(
    context: &mut Context,
    project: &str,
    team: &str,
    iteration_id: &str,
) -> Result<Report, AdoError> {
    let path = iteration_path(project, team, iteration_id);

    match context.client()?.delete(&path, &[]) {
        Ok(()) => {
            let message = "Iteration deleted.".to_owned();

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: "Iteration not found".to_owned(),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// The collection path: `/{project}/{team}/_apis/work/teamsettings/iterations`.
fn iterations_path(project: &str, team: &str) -> String {
    format!(
        "/{}/{}/_apis/work/teamsettings/iterations",
        encode_path_segment(project),
        encode_path_segment(team)
    )
}

/// One iteration below the collection; the id is a single segment.
fn iteration_path(project: &str, team: &str, iteration_id: &str) -> String {
    format!(
        "{}/{}",
        iterations_path(project, team),
        encode_path_segment(iteration_id)
    )
}

/// The module's current-only path glue expressed as a query pair: the frozen CLI
/// writes `?$timeframe=current` into the path and then appends `?api-version=7.1`,
/// producing one broken pair (D25); this build sends the two (D25's second site).
fn current_params(current: bool) -> Vec<(String, String)> {
    if current {
        vec![("$timeframe".to_owned(), "current".to_owned())]
    } else {
        Vec::new()
    }
}

fn create_body(name: &str, start_date: Option<&str>, finish_date: Option<&str>) -> Value {
    let mut body = json!({ "name": name });

    insert_attributes(&mut body, start_date, finish_date);

    body
}

fn update_body(name: Option<&str>, start_date: Option<&str>, finish_date: Option<&str>) -> Value {
    let mut body = Map::new();

    if let Some(name) = name {
        body.insert("name".to_owned(), json!(name));
    }

    let mut body = Value::Object(body);

    insert_attributes(&mut body, start_date, finish_date);

    body
}

/// The `attributes` map the frozen `put_in(body, ["attributes", key], value)`
/// intended; absent when neither date is given, so a names-only body stays
/// `{"name": …}`.
fn insert_attributes(body: &mut Value, start_date: Option<&str>, finish_date: Option<&str>) {
    if start_date.is_none() && finish_date.is_none() {
        return;
    }

    let mut attributes = Map::new();

    if let Some(start_date) = start_date {
        attributes.insert("startDate".to_owned(), json!(start_date));
    }
    if let Some(finish_date) = finish_date {
        attributes.insert("finishDate".to_owned(), json!(finish_date));
    }

    body.as_object_mut()
        .expect("the iteration body is an object")
        .insert("attributes".to_owned(), Value::Object(attributes));
}

/// The module's `print_iterations_table/1` table: ID, Name, Start, Finish, with
/// the module's "No iterations found." when empty.
fn iterations_table(iterations: &[Value]) -> Report {
    if iterations.is_empty() {
        return Report::Text("No iterations found.".to_owned());
    }

    let rows = iterations
        .iter()
        .map(|iteration| {
            let attributes = iteration.get("attributes").and_then(Value::as_object);

            vec![
                value_text(iteration.get("id")),
                value_text(iteration.get("name")),
                value_text(attributes.and_then(|attributes| attributes.get("startDate"))),
                value_text(attributes.and_then(|attributes| attributes.get("finishDate"))),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Name".to_owned(),
            "Start".to_owned(),
            "Finish".to_owned(),
        ],
        rows,
    }
}

/// The module's `print_iteration_detail/1`, with its `─` rule and the
/// `attrs[key]` reads (a missing attribute prints as the empty string).
fn iteration_detail(iteration: &Value) -> String {
    let attributes = iteration.get("attributes").and_then(Value::as_object);
    let mut detail = String::from("\nIteration Details\n\n");

    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:    {}\n", value_text(iteration.get("id"))));
    detail.push_str(&format!("  Name:  {}\n", value_text(iteration.get("name"))));
    detail.push_str(&format!("  Path:  {}\n", value_text(iteration.get("path"))));
    detail.push_str(&format!(
        "  Start: {}\n",
        value_text(attributes.and_then(|attributes| attributes.get("startDate")))
    ));
    detail.push_str(&format!(
        "  Finish: {}\n",
        value_text(attributes.and_then(|attributes| attributes.get("finishDate")))
    ));

    detail.push('\n');

    detail
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

    #[test]
    fn current_params_only_carry_the_timeframe_pair() {
        assert_eq!(current_params(false), Vec::new());
        assert_eq!(
            current_params(true),
            vec![("$timeframe".to_owned(), "current".to_owned())],
            "a real query pair, not the frozen path glue (D25)"
        );
    }

    #[test]
    fn a_names_only_create_body_has_no_attributes_key() {
        assert_eq!(
            create_body("Sprint 26", None, None),
            json!({"name": "Sprint 26"})
        );
    }

    #[test]
    fn a_dated_create_body_carries_both_attributes() {
        assert_eq!(
            create_body("Sprint 26", Some("2026-03-01"), Some("2026-03-14")),
            json!({
                "name": "Sprint 26",
                "attributes": {"startDate": "2026-03-01", "finishDate": "2026-03-14"},
            })
        );
    }

    #[test]
    fn an_update_body_carries_only_the_given_fields() {
        assert_eq!(
            update_body(Some("Sprint 24b"), None, None),
            json!({"name": "Sprint 24b"})
        );
        assert_eq!(
            update_body(None, None, Some("2026-03-14")),
            json!({"attributes": {"finishDate": "2026-03-14"}})
        );
        assert_eq!(
            update_body(None, Some("2026-03-01"), Some("2026-03-14")),
            json!({"attributes": {"startDate": "2026-03-01", "finishDate": "2026-03-14"}})
        );
    }

    #[test]
    fn the_table_uses_the_module_columns_and_its_empty_sentence() {
        assert_eq!(
            iterations_table(&[]),
            Report::Text("No iterations found.".to_owned())
        );

        let iterations = vec![json!({
            "id": "i1",
            "name": "Sprint 24",
            "attributes": {"startDate": "2026-01-15T00:00:00Z", "finishDate": "2026-01-29T00:00:00Z"},
        })];

        assert_eq!(
            iterations_table(&iterations),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Name".to_owned(),
                    "Start".to_owned(),
                    "Finish".to_owned()
                ],
                rows: vec![vec![
                    "i1".to_owned(),
                    "Sprint 24".to_owned(),
                    "2026-01-15T00:00:00Z".to_owned(),
                    "2026-01-29T00:00:00Z".to_owned(),
                ]],
            }
        );
    }

    #[test]
    fn the_detail_prints_the_frozen_labels_and_empty_missing_fields() {
        let detail = iteration_detail(&json!({"id": "i1", "name": "Sprint 24"}));

        assert_eq!(
            detail,
            format!(
                "\nIteration Details\n\n{}\n  ID:    i1\n  Name:  Sprint 24\n  Path:  \n  Start: \n  Finish: \n\n",
                "─".repeat(60)
            ),
            "a missing attribute interpolates as the empty string, trailing space and all"
        );
    }

    #[test]
    fn the_paths_encode_each_segment_strictly() {
        assert_eq!(
            iterations_path("Alpha Beta", "Team One"),
            "/Alpha%20Beta/Team%20One/_apis/work/teamsettings/iterations"
        );
        assert_eq!(
            iteration_path("Alpha", "Team", "a/b"),
            "/Alpha/Team/_apis/work/teamsettings/iterations/a%2Fb"
        );
    }
}
