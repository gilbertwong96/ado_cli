//! `ado projects list|show|create|update|delete` — the read and write paths of
//! `lib/ado_cli/cli/projects.ex`: the same REST surface, the same filters, and
//! the same human layout. `delete` asks the first confirmation of the wave
//! (spec §4.1).

use ado_core::client::encode_path_segment;
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The collection every path here builds on; the module's `"/_apis/projects"`.
const PROJECTS_PATH: &str = "/_apis/projects";

/// The `sourceControlType` the module sends when `--source-control` is absent.
const DEFAULT_SOURCE_CONTROL: &str = "Git";

/// The helper's refusal wording (`Helpers.confirm_delete/2`'s
/// `halt_error("Aborted.")`); it is this build's §8 wording, printed on stderr.
const ABORTED: &str = "Aborted.";

/// `ado projects create`: `POST /_apis/projects` with the module's body — the
/// two capability maps always, `description`/`visibility` only when given — and
/// the module's success line. Under `--json` the created project is the value
/// envelope where the frozen CLI prints that line even under `--json` (D33).
pub fn create(
    context: &mut Context,
    name: &str,
    description: Option<String>,
    visibility: Option<String>,
    process: Option<String>,
    source_control: Option<String>,
) -> Result<Report, AdoError> {
    let body = create_body(name, description, visibility, process, source_control);
    let project = context.client()?.post(PROJECTS_PATH, &body, &[])?;

    Ok(context.json_or_report(ok_value(project.clone()), || {
        Report::Text(format!(
            "Project '{}' created (ID: {}).\n  Status: {}\n  URL:    {}",
            field(&project, "name"),
            field(&project, "id"),
            field(&project, "status"),
            field(&project, "url"),
        ))
    }))
}

/// The module's `create_project/1` body, captured: the capability maps always
/// carry a value (`Git`, and the process template id), and an absent
/// `--description`/`--visibility` is an absent key.
fn create_body(
    name: &str,
    description: Option<String>,
    visibility: Option<String>,
    process: Option<String>,
    source_control: Option<String>,
) -> Value {
    let mut body = json!({
        "name": name,
        "capabilities": {
            "versioncontrol": {
                "sourceControlType": source_control.unwrap_or_else(|| DEFAULT_SOURCE_CONTROL.to_owned()),
            },
            "processTemplate": {"templateTypeId": process_template_id(process.as_deref())},
        }
    });
    let fields = body.as_object_mut().expect("the body is an object");

    if let Some(description) = description {
        fields.insert("description".to_owned(), json!(description));
    }
    if let Some(visibility) = visibility {
        fields.insert("visibility".to_owned(), json!(visibility));
    }

    body
}

/// The frozen `process_template_id/1`: `nil` and `scrum` are the same id, the
/// three other lowercase spellings are mapped, and anything else — including
/// the capitalised names the option's own help text suggests — is sent as the
/// template id verbatim, which is the captured behaviour (`--process Agile`
/// sends `"Agile"`).
fn process_template_id(process: Option<&str>) -> String {
    match process {
        None | Some("scrum") => "6b724908-ef14-45cf-84f8-768b5384da45".to_owned(),
        Some("agile") => "adcc42ab-9882-485e-a3ed-7678f01f66bc".to_owned(),
        Some("basic") => "b8a3a935-7e91-48b8-a94c-606d37c3e9f2".to_owned(),
        Some("cmmi") => "27450541-8e31-4150-9947-dc59f998fc01".to_owned(),
        Some(unknown) => unknown.to_owned(),
    }
}

/// `ado projects update`: `PATCH /_apis/projects/{id}` with only the options
/// given, after the module's own guard — no `--name` and no `--description` is a
/// `validation_error` with no request (captured).
pub fn update(
    context: &mut Context,
    project_id: &str,
    name: Option<String>,
    description: Option<String>,
) -> Result<Report, AdoError> {
    if name.is_none() && description.is_none() {
        return Err(AdoError::validation(
            "At least one of --name or --description is required.",
        ));
    }

    let mut body = serde_json::Map::new();

    if let Some(name) = name {
        body.insert("name".to_owned(), json!(name));
    }
    if let Some(description) = description {
        body.insert("description".to_owned(), json!(description));
    }

    let path = format!("{PROJECTS_PATH}/{}", encode_path_segment(project_id));

    match context.client()?.patch(&path, &Value::Object(body), &[]) {
        Ok(project) => Ok(context.json_or_report(ok_value(project.clone()), || {
            Report::Text(format!("Project updated: {}", field(&project, "name")))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Project '{project_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado projects delete`: the wave's first confirmation — asked before any
/// credential is resolved or request is built — then
/// `DELETE /_apis/projects/{id}`. `--force` skips the question; a "no" or EOF
/// returns the refusal, which exits 1 and sends nothing (D30/D32).
pub fn delete(context: &mut Context, project_id: &str, force: bool) -> Result<Report, AdoError> {
    if !force && !context.confirm(&delete_question(project_id)) {
        return Err(AdoError::cancelled(ABORTED));
    }

    let path = format!("{PROJECTS_PATH}/{}", encode_path_segment(project_id));

    match context.client()?.delete(&path, &[]) {
        Ok(()) => {
            let message = format!("Project '{project_id}' queued for deletion.");

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Project '{project_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `Helpers.confirm_delete("project", id)`'s question, verbatim: the command
/// owns it, so the seam hard-codes no single question.
fn delete_question(project_id: &str) -> String {
    format!("Delete project '{project_id}'? This cannot be undone. [y/N] ")
}

/// `ado projects list`: `GET /_apis/projects` with `stateFilter`, `$top` and
/// `$skip`, exactly as `list_projects/1` builds them. Under `--json` the body is
/// the value envelope — a bare array under `result`, the kind the module's
/// `Helpers.json_or_format` picks (W1-R12) — not the `count`/`items` list form.
pub fn list(
    context: &mut Context,
    state: Option<String>,
    top: Option<i64>,
    skip: Option<i64>,
) -> Result<Report, AdoError> {
    let params = list_params(state, top, skip);
    let items = items(context.client()?.list(PROJECTS_PATH, &params)?);

    Ok(
        context.json_or_report(ok_value(Value::Array(items.clone())), || {
            projects_table(&items)
        }),
    )
}

/// `ado projects show`: `GET /_apis/projects/{id}`, with `includeCapabilities`
/// when `--capabilities` is set. The module answers a 404 with its own message.
pub fn show(
    context: &mut Context,
    project_id: &str,
    capabilities: bool,
) -> Result<Report, AdoError> {
    let path = format!("{PROJECTS_PATH}/{}", encode_path_segment(project_id));
    let params = show_params(capabilities);
    let response = context.client()?.get(&path, &params);

    match response {
        Ok(project) => {
            Ok(context.json_or_report(ok_value(project.clone()), || project_detail(&project)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Project '{project_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// The Elixir's `build_params/3` includes every present option: `if value` is
/// truthy for `0` and `""` (only `nil`/`false` are falsy), so `--top 0`,
/// `--skip 0` and `--state ''` all reach the wire while an absent option does
/// not. Its mapping table — and the help text — name `stateFilter`/`$top`/
/// `$skip`; the lookup misses and the frozen CLI sends `state`/`top`/`skip`
/// instead, which Azure ignores (D19).
fn list_params(
    state: Option<String>,
    top: Option<i64>,
    skip: Option<i64>,
) -> Vec<(String, String)> {
    let mut params = Vec::new();

    if let Some(state) = state {
        params.push(("stateFilter".to_owned(), state));
    }
    if let Some(top) = top {
        params.push(("$top".to_owned(), top.to_string()));
    }
    if let Some(skip) = skip {
        params.push(("$skip".to_owned(), skip.to_string()));
    }

    params
}

fn show_params(capabilities: bool) -> Vec<(String, String)> {
    if capabilities {
        vec![("includeCapabilities".to_owned(), "true".to_owned())]
    } else {
        Vec::new()
    }
}

/// The module's `print_projects_table/1`: columns ID, Name, State, with its
/// "No projects found." for an empty list.
fn projects_table(projects: &[Value]) -> Report {
    if projects.is_empty() {
        return Report::Text("No projects found.".to_owned());
    }

    let rows = projects
        .iter()
        .map(|project| {
            vec![
                field(project, "id"),
                field(project, "name"),
                field(project, "state"),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Name".to_owned(), "State".to_owned()],
        rows,
    }
}

/// The module's `print_project_detail/1`, minus the colour; capabilities are
/// only shown by the JSON path, exactly as the Elixir leaves them.
fn project_detail(project: &Value) -> Report {
    let mut detail = String::from("\nProject Details\n\n");

    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:          {}\n", field(project, "id")));
    detail.push_str(&format!("  Name:        {}\n", field(project, "name")));
    detail.push_str(&format!("  Description: {}\n", description(project)));
    detail.push_str(&format!("  State:       {}\n", field(project, "state")));
    detail.push_str(&format!(
        "  Visibility:  {}\n",
        field(project, "visibility")
    ));
    detail.push_str(&format!("  URL:         {}\n", field(project, "url")));

    if let Some(team) = project.get("defaultTeam") {
        detail.push_str(&format!(
            "  Default Team: {} ({})\n",
            field(team, "name"),
            field(team, "id")
        ));
    }

    detail.push('\n');

    Report::Text(detail)
}

/// `project["description"] || "(none)"`.
fn description(project: &Value) -> String {
    project
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("(none)")
        .to_owned()
}

/// `p["id"] || ""`: a missing or non-string field is empty.
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
    fn list_params_use_the_mapped_names() {
        assert_eq!(list_params(None, None, None), Vec::new());
        assert_eq!(
            list_params(Some("wellFormed".to_owned()), Some(10), Some(5)),
            vec![
                ("stateFilter".to_owned(), "wellFormed".to_owned()),
                ("$top".to_owned(), "10".to_owned()),
                ("$skip".to_owned(), "5".to_owned()),
            ],
            "the names Azure reads, not the frozen CLI's state/top/skip (D19)"
        );
        assert_eq!(
            list_params(Some(String::new()), Some(0), Some(0)),
            vec![
                ("stateFilter".to_owned(), String::new()),
                ("$top".to_owned(), "0".to_owned()),
                ("$skip".to_owned(), "0".to_owned()),
            ],
            "a present option is sent even when empty or zero: the Elixir's `if value` is truthy for both"
        );
        assert_eq!(
            list_params(None, Some(-1), Some(-1)),
            vec![
                ("$top".to_owned(), "-1".to_owned()),
                ("$skip".to_owned(), "-1".to_owned()),
            ],
            "a negative value is a value, not a flag"
        );
    }

    #[test]
    fn show_params_only_carry_capabilities_when_asked() {
        assert_eq!(show_params(false), Vec::new());
        assert_eq!(
            show_params(true),
            vec![("includeCapabilities".to_owned(), "true".to_owned())],
            "the Elixir's includeCapabilities is a boolean written as true"
        );
    }

    #[test]
    fn projects_table_uses_the_module_columns() {
        let projects = vec![json!({
            "id": "p1",
            "name": "Alpha",
            "state": "wellFormed",
            "visibility": "private",
        })];

        assert_eq!(
            projects_table(&projects),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "State".to_owned()],
                rows: vec![vec![
                    "p1".to_owned(),
                    "Alpha".to_owned(),
                    "wellFormed".to_owned()
                ]],
            }
        );
    }

    #[test]
    fn projects_table_of_nothing_is_the_module_message() {
        assert_eq!(
            projects_table(&[]),
            Report::Text("No projects found.".to_owned())
        );
    }

    #[test]
    fn project_detail_prints_the_labels_and_the_default_team() {
        let project = json!({
            "id": "p1",
            "name": "Alpha",
            "description": "The first project",
            "state": "wellFormed",
            "visibility": "private",
            "url": "https://example.test/p1",
            "defaultTeam": {"id": "t1", "name": "Alpha Team"},
        });

        let Report::Text(detail) = project_detail(&project) else {
            panic!("the detail is a text report");
        };

        assert!(detail.contains("Project Details\n"), "detail: {detail}");
        assert!(detail.contains("  ID:          p1\n"), "detail: {detail}");
        assert!(
            detail.contains("  Description: The first project\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Default Team: Alpha Team (t1)\n"),
            "detail: {detail}"
        );
    }

    #[test]
    fn project_detail_says_none_without_a_description() {
        let Report::Text(detail) = project_detail(&json!({"id": "p1"})) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.contains("  Description: (none)\n"),
            "detail: {detail}"
        );
    }

    #[test]
    fn process_template_id_maps_the_lowercase_spellings_and_passes_others_through() {
        assert_eq!(
            process_template_id(None),
            "6b724908-ef14-45cf-84f8-768b5384da45"
        );
        assert_eq!(
            process_template_id(Some("scrum")),
            "6b724908-ef14-45cf-84f8-768b5384da45",
            "nil and scrum are the same id"
        );
        assert_eq!(
            process_template_id(Some("agile")),
            "adcc42ab-9882-485e-a3ed-7678f01f66bc"
        );
        assert_eq!(
            process_template_id(Some("basic")),
            "b8a3a935-7e91-48b8-a94c-606d37c3e9f2"
        );
        assert_eq!(
            process_template_id(Some("cmmi")),
            "27450541-8e31-4150-9947-dc59f998fc01"
        );
        assert_eq!(
            process_template_id(Some("Agile")),
            "Agile",
            "the frozen mapping is lowercase-only; the captured capitalised spelling passes through"
        );
    }

    #[test]
    fn create_body_defaults_the_capabilities_and_only_includes_present_options() {
        assert_eq!(
            create_body("Minimal", None, None, None, None),
            json!({
                "name": "Minimal",
                "capabilities": {
                    "versioncontrol": {"sourceControlType": "Git"},
                    "processTemplate": {"templateTypeId": "6b724908-ef14-45cf-84f8-768b5384da45"},
                },
            })
        );
        assert_eq!(
            create_body(
                "Created",
                Some("The created project".to_owned()),
                Some("public".to_owned()),
                Some("agile".to_owned()),
                Some("Tfvc".to_owned()),
            ),
            json!({
                "name": "Created",
                "capabilities": {
                    "versioncontrol": {"sourceControlType": "Tfvc"},
                    "processTemplate": {"templateTypeId": "adcc42ab-9882-485e-a3ed-7678f01f66bc"},
                },
                "description": "The created project",
                "visibility": "public",
            })
        );
    }

    #[test]
    fn delete_question_is_the_module_wording() {
        assert_eq!(
            delete_question("Alpha"),
            "Delete project 'Alpha'? This cannot be undone. [y/N] ",
            "the command owns its question; the seam adds nothing"
        );
    }
}
