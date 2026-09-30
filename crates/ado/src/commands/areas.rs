//! `ado areas list|show|create|update|delete` — the area-path half of
//! `lib/ado_cli/cli/areas.ex`: the classification-node REST surface under
//! `wit/classificationNodes/areas`, the tree and detail views, and the module's
//! own 404 wording for `show`. `delete` never prompts (R1): the frozen CLI sends
//! its DELETE without asking.

use ado_core::client::encode_path_segment;
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::context::Context;
use crate::output::Report;

/// `ado areas list`: `GET /{project}/_apis/wit/classificationNodes/areas`, with
/// the module's `$depth` when `--depth` is given. The response is the root node;
/// `--json` emits it whole under the value envelope, and the human path prints
/// the module's two-space tree.
pub fn list(context: &mut Context, project: &str, depth: Option<i64>) -> Result<Report, AdoError> {
    let params = depth_params(depth);
    let root = context.client()?.get(&areas_path(project), &params)?;

    Ok(context.json_or_report(ok_value(root.clone()), || Report::Text(area_tree(&root))))
}

/// `ado areas show`: `GET /{project}/_apis/wit/classificationNodes/areas/{path}`,
/// where the whole area path is one percent-encoded segment. A 404 takes the
/// module's own wording.
pub fn show(context: &mut Context, project: &str, area_path: &str) -> Result<Report, AdoError> {
    let path = area_node_path(project, area_path);

    match context.client()?.get(&path, &[]) {
        Ok(area) => {
            Ok(context.json_or_report(ok_value(area.clone()), || Report::Text(area_detail(&area))))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Area path '{area_path}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado areas create`: `POST` the root collection, or the `--parent` node, with
/// the module's `{"name" => name}` body. The response names the created node, so
/// the human line carries its id.
pub fn create(
    context: &mut Context,
    project: &str,
    name: &str,
    parent: Option<String>,
) -> Result<Report, AdoError> {
    let path = match parent {
        Some(parent) => area_node_path(project, &parent),
        None => areas_path(project),
    };
    let area = context
        .client()?
        .post(&path, &json!({ "name": name }), &[])?;

    Ok(context.json_or_report(ok_value(area.clone()), || {
        Report::Text(format!(
            "Area '{}' created (ID: {}).",
            value_text(area.get("name")),
            value_text(area.get("id")),
        ))
    }))
}

/// `ado areas update`: `PATCH` the node with the module's `{"name" => new_name}`.
pub fn update(
    context: &mut Context,
    project: &str,
    area_path: &str,
    name: &str,
) -> Result<Report, AdoError> {
    let path = area_node_path(project, area_path);
    let area = context
        .client()?
        .patch(&path, &json!({ "name": name }), &[])?;

    Ok(context.json_or_report(ok_value(area.clone()), || {
        Report::Text(format!(
            "Area renamed to '{}'.",
            value_text(area.get("name"))
        ))
    }))
}

/// `ado areas delete`: the module's plain `DELETE`, without a prompt (R1).
pub fn delete(context: &mut Context, project: &str, area_path: &str) -> Result<Report, AdoError> {
    let path = area_node_path(project, area_path);
    context.client()?.delete(&path, &[])?;

    let message = format!("Area '{area_path}' deleted.");

    Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
}

/// The collection path: `/{project}/_apis/wit/classificationNodes/areas`.
fn areas_path(project: &str) -> String {
    format!(
        "/{}/_apis/wit/classificationNodes/areas",
        encode_path_segment(project)
    )
}

/// One node below the collection: the area path is a single segment, so a name
/// that carries a separator cannot change the URL's structure (D22).
fn area_node_path(project: &str, area_path: &str) -> String {
    format!("{}/{}", areas_path(project), encode_path_segment(area_path))
}

/// The module's `params = if d = Map.get(parsed.options, :depth), do: %{"$depth" => d}, else: %{}`.
fn depth_params(depth: Option<i64>) -> Vec<(String, String)> {
    depth
        .map(|depth| vec![("$depth".to_owned(), depth.to_string())])
        .unwrap_or_default()
}

/// The module's `print_area_tree/2`: two spaces per level, ` ▾` exactly on a node
/// whose `children` is a non-empty list, and the root printed first.
fn area_tree(root: &Value) -> String {
    let mut out = String::new();

    push_area_node(&mut out, root, 0);

    out
}

fn push_area_node(out: &mut String, node: &Value, depth: usize) {
    let children = node.get("children").and_then(Value::as_array);
    let branch = if children.is_some_and(|children| !children.is_empty()) {
        " ▾"
    } else {
        ""
    };

    if depth > 0 {
        out.push('\n');
    }
    out.push_str(&"  ".repeat(depth));
    out.push_str(&value_text(node.get("name")));
    out.push_str(branch);

    for child in children.into_iter().flatten() {
        push_area_node(out, child, depth + 1);
    }
}

/// The module's `print_area_detail/1`: a blank line, the heading, an ASCII rule,
/// the four labelled fields, and the URL only when the node carries one.
fn area_detail(area: &Value) -> String {
    let mut detail = String::from("\nArea Path Details\n\n");

    detail.push_str(&"-".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:        {}\n", value_text(area.get("id"))));
    detail.push_str(&format!("  Name:      {}\n", value_text(area.get("name"))));
    detail.push_str(&format!("  Path:      {}\n", value_text(area.get("path"))));
    detail.push_str(&format!("  Structure: {}\n", structure_type(area)));

    if let Some(url) = area
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
    {
        detail.push_str(&format!("  URL:       {url}\n"));
    }

    detail.push('\n');

    detail
}

/// The module's `area["structureType"] || "hierarchy"`: `nil` falls back, and an
/// empty string is a value like any other (only `nil` and `false` are falsy).
fn structure_type(area: &Value) -> String {
    match area.get("structureType") {
        None | Some(Value::Null) => "hierarchy".to_owned(),
        Some(value) => value_text(Some(value)),
    }
}

/// Elixir's `#{term}` interpolation for the JSON scalars these fields carry:
/// strings verbatim, numbers and booleans as they print, and `nil`/absent as the
/// empty string.
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
    fn depth_params_only_carry_a_given_depth() {
        assert_eq!(depth_params(None), Vec::new());
        assert_eq!(
            depth_params(Some(2)),
            vec![("$depth".to_owned(), "2".to_owned())]
        );
        assert_eq!(
            depth_params(Some(0)),
            vec![("$depth".to_owned(), "0".to_owned())],
            "zero is a value: the Elixir's `if value` is truthy for 0"
        );
    }

    #[test]
    fn the_tree_marks_only_non_empty_children() {
        let root = json!({
            "name": "Alpha",
            "children": [
                {"name": "Team", "children": [{"name": "Feature"}]},
                {"name": "Sub", "children": []},
                {"name": "Leaf"},
            ],
        });

        assert_eq!(
            area_tree(&root),
            "Alpha ▾\n  Team ▾\n    Feature\n  Sub\n  Leaf",
            "` ▾` is for a non-empty children list; an empty one and an absent one are leaves"
        );
    }

    #[test]
    fn a_root_without_children_is_its_own_line() {
        assert_eq!(area_tree(&json!({"name": "Empty"})), "Empty");
        assert_eq!(
            area_tree(&json!({"name": "Empty", "children": []})),
            "Empty"
        );
    }

    #[test]
    fn the_detail_prints_the_frozen_labels_and_falls_back_to_hierarchy() {
        let detail = area_detail(&json!({"id": 2, "name": "Team", "path": "\\Alpha\\Team"}));

        assert_eq!(
            detail,
            format!(
                "\nArea Path Details\n\n{}\n  ID:        2\n  Name:      Team\n  Path:      \\Alpha\\Team\n  Structure: hierarchy\n\n",
                "-".repeat(60)
            )
        );
    }

    #[test]
    fn the_detail_omits_a_missing_url_but_prints_an_empty_structure_verbatim() {
        let detail = area_detail(&json!({"id": 2, "structureType": "", "url": ""}));

        assert!(detail.contains("  Structure: \n"), "detail: {detail:?}");
        assert!(!detail.contains("  URL:"), "detail: {detail:?}");
    }

    #[test]
    fn the_paths_encode_each_segment_strictly() {
        assert_eq!(
            areas_path("Alpha Beta"),
            "/Alpha%20Beta/_apis/wit/classificationNodes/areas"
        );
        assert_eq!(
            area_node_path("Alpha", "Alpha/Team"),
            "/Alpha/_apis/wit/classificationNodes/areas/Alpha%2FTeam",
            "a slash cannot split the path (D22)"
        );
        assert_eq!(
            area_node_path("Alpha", "Alpha\\Team"),
            "/Alpha/_apis/wit/classificationNodes/areas/Alpha%5CTeam"
        );
    }
}
