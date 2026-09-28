//! `ado projects list|show` — the read paths of `lib/ado_cli/cli/projects.ex`:
//! the same REST surface, the same filters, and the same human layout.

use ado_core::envelope::{ok_list, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::Value;

use crate::context::Context;
use crate::output::Report;

/// The collection both subcommands build on; the module's `"/_apis/projects"`.
const PROJECTS_PATH: &str = "/_apis/projects";

/// `ado projects list`: `GET /_apis/projects` with `stateFilter`, `$top` and
/// `$skip`, exactly as `list_projects/1` builds them.
pub fn list(
    context: &mut Context,
    state: Option<String>,
    top: Option<i64>,
    skip: Option<i64>,
) -> Result<Report, AdoError> {
    let params = list_params(state, top, skip);
    let items = items(context.client()?.list(PROJECTS_PATH, &params)?);

    Ok(context.json_or_report(ok_list(items.clone()), || projects_table(&items)))
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

/// The Elixir's `build_params/3` tests each value's truthiness, so an empty
/// `--state` and a zero `--top`/`--skip` are not sent at all. Its mapping table
/// — and the help text — name `stateFilter`/`$top`/`$skip`; the lookup misses
/// and the frozen CLI sends `state`/`top`/`skip` instead, which Azure ignores
/// (D19 in `docs/rust-rewrite/contract-inventory.md`).
fn list_params(
    state: Option<String>,
    top: Option<i64>,
    skip: Option<i64>,
) -> Vec<(String, String)> {
    let mut params = Vec::new();

    if let Some(state) = state.filter(|state| !state.is_empty()) {
        params.push(("stateFilter".to_owned(), state));
    }
    if let Some(top) = top.filter(|top| *top != 0) {
        params.push(("$top".to_owned(), top.to_string()));
    }
    if let Some(skip) = skip.filter(|skip| *skip != 0) {
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

/// The Elixir's `Client.list/2` unwraps the `value` array; anything else is
/// wrapped the way `List.wrap/1` wraps it, so the envelope stays a list.
fn items(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        other => vec![other],
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

/// `URI.encode/1` for one path segment: the unreserved set survives, everything
/// else is percent-encoded with uppercase hex, and a space is `%20` — not the
/// query encoder's `+`.
fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }

    encoded
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
            Vec::new(),
            "the Elixir's `if value` drops an empty string and zeroes"
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
    fn encode_path_segment_encodes_like_uri_encode() {
        assert_eq!(encode_path_segment("Alpha"), "Alpha");
        assert_eq!(encode_path_segment("My Project"), "My%20Project");
        assert_eq!(encode_path_segment("a/b+c"), "a%2Fb%2Bc");
        assert_eq!(
            encode_path_segment("6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c"),
            "6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c"
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
}
