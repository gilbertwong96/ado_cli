//! `ado workitems list|show|query|create|update|delete` — the read and write
//! paths of `lib/ado_cli/cli/work_items.ex`: the same WIQL surface, the same
//! filters, the same JSON-patch bodies, and the same human layouts.
//!
//! Both writes send a **JSON-patch array** under `application/json-patch+json`
//! (captured; the frozen CLI's `build_json_patch/1`): `create`'s six `add`
//! operations follow the module's field order, `update`'s five follow *its* own
//! (state before assigned-to) with a `replace` for tags prepended. `delete` was
//! re-verified against the mock with `n` on stdin and on EOF (R5): the DELETE
//! goes out on both, so this module asks no question and the tree has no
//! `--force`.

use ado_core::client::encode_path_segment;
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::context::Context;
use crate::output::Report;

/// The columns every `list` query selects, from the module's `list_work_items/1`.
const LIST_SELECT: &str = "SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], [System.AssignedTo] FROM WorkItems WHERE ";

/// The batch endpoint is org-scoped: no project segment, exactly as
/// `fetch_work_items_batch/2` calls it.
const WORK_ITEMS_PATH: &str = "/_apis/wit/workitems";

const WORK_ITEM_FIELDS: &str =
    "System.Id,System.Title,System.State,System.WorkItemType,System.AssignedTo";

/// `ado workitems list`: the module builds a WIQL query from the project and the
/// filters and runs the shared two-request flow. Its clauses are joined validly
/// where the frozen binary sends `WHERE AND …` (D20).
pub fn list(
    context: &mut Context,
    project: &str,
    item_type: Option<String>,
    assigned_to: Option<String>,
    state: Option<String>,
    top: Option<i64>,
) -> Result<Report, AdoError> {
    let wiql = list_wiql(
        project,
        item_type.as_deref(),
        assigned_to.as_deref(),
        state.as_deref(),
    );

    run_wiql(context, project, &wiql, top)
}

/// `ado workitems show`: `GET /_apis/wit/workitems/{id}` with `$expand`,
/// defaulting to `all`. The module answers a 404 with its own message.
pub fn show(context: &mut Context, id: i64, expand: &str) -> Result<Report, AdoError> {
    let path = format!("/_apis/wit/workitems/{id}");
    let params = vec![("$expand".to_owned(), expand.to_owned())];

    let response = {
        let client = context.client()?;
        client.get(&path, &params)
    };

    match response {
        Ok(work_item) => Ok(
            context.json_or_report(ok_value(work_item.clone()), || work_item_detail(&work_item))
        ),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Work item #{id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado workitems query`: the user's WIQL verbatim. `--wiql` is optional in the
/// frozen tree (`required: false`), so its absence is the module's own validation
/// message rather than a clap parse error.
pub fn query(
    context: &mut Context,
    project: &str,
    wiql: Option<String>,
    top: Option<i64>,
) -> Result<Report, AdoError> {
    let wiql =
        wiql.ok_or_else(|| AdoError::validation("--wiql is required for the query command"))?;

    run_wiql(context, project, &wiql, top)
}

/// The five optional field flags both write paths share; `None` means the flag was
/// not given, so the field's operation is left out of the patch. `title` stays a
/// parameter of its own because `create` requires it and `update` does not.
#[derive(Debug, Default, PartialEq)]
pub struct WorkItemOptions {
    pub description: Option<String>,
    pub state: Option<String>,
    pub assigned_to: Option<String>,
    pub priority: Option<i64>,
    pub tags: Option<String>,
}

impl WorkItemOptions {
    /// No field at all was given; `update`'s guard refuses that.
    pub fn is_empty(&self) -> bool {
        self.description.is_none()
            && self.state.is_none()
            && self.assigned_to.is_none()
            && self.priority.is_none()
            && self.tags.is_none()
    }
}

/// `ado workitems create`: `POST /{project}/_apis/wit/workitems/${type}` with the
/// module's JSON-patch array, under the API's JSON-patch content type. Under
/// `--json` the created work item is the value envelope where the frozen CLI
/// prints its success line even under `--json` (D33).
pub fn create(
    context: &mut Context,
    project: &str,
    item_type: &str,
    title: &str,
    options: WorkItemOptions,
) -> Result<Report, AdoError> {
    let patch = create_patch(title, options);
    let path = format!(
        "/{}/_apis/wit/workitems/${}",
        encode_path_segment(project),
        encode_path_segment(item_type)
    );
    let work_item = context
        .client()?
        .post_json_patch(&path, &Value::Array(patch), &[])?;

    Ok(context.json_or_report(ok_value(work_item.clone()), || created_lines(&work_item)))
}

/// The module's `create_work_item/1` patch, captured: one `add` per option given,
/// in the order the module lists them — title, description, assigned-to, state,
/// priority, tags — with `--priority` an integer, not a string.
fn create_patch(title: &str, options: WorkItemOptions) -> Vec<Value> {
    [
        ("/fields/System.Title", Some(Value::from(title))),
        (
            "/fields/System.Description",
            options.description.map(Value::from),
        ),
        (
            "/fields/System.AssignedTo",
            options.assigned_to.map(Value::from),
        ),
        ("/fields/System.State", options.state.map(Value::from)),
        (
            "/fields/Microsoft.VSTS.Common.Priority",
            options.priority.map(Value::from),
        ),
        ("/fields/System.Tags", options.tags.map(Value::from)),
    ]
    .into_iter()
    .filter_map(|(path, value)| value.map(|value| add_op(path, value)))
    .collect()
}

/// `ado workitems update`: `PATCH /_apis/wit/workitems/{id}` with the module's
/// JSON-patch array, after its own guard — no field at all is a `validation_error`
/// with no request (captured). A 404 answers the module's own message; every other
/// error is the shared envelope.
pub fn update(
    context: &mut Context,
    id: i64,
    title: Option<String>,
    options: WorkItemOptions,
) -> Result<Report, AdoError> {
    if title.is_none() && options.is_empty() {
        return Err(AdoError::validation(
            "At least one field to update is required (--title, --state, --assigned-to, etc.)",
        ));
    }

    let patch = update_patch(title, options);
    let path = format!("/_apis/wit/workitems/{id}");

    match context
        .client()?
        .patch_json_patch(&path, &Value::Array(patch), &[])
    {
        Ok(work_item) => {
            Ok(context.json_or_report(ok_value(work_item.clone()), || updated_lines(&work_item)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Work item #{id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// The module's `update_work_item/1` patch, captured: five `add` operations in
/// the module's own order — title, description, **state, assigned-to**, priority
/// — and, when `--tags` is given, one `replace` for the tag list **first**, which
/// is how `[tags | patch]` prepends it.
fn update_patch(title: Option<String>, options: WorkItemOptions) -> Vec<Value> {
    let mut patch = [
        ("/fields/System.Title", title.map(Value::from)),
        (
            "/fields/System.Description",
            options.description.map(Value::from),
        ),
        ("/fields/System.State", options.state.map(Value::from)),
        (
            "/fields/System.AssignedTo",
            options.assigned_to.map(Value::from),
        ),
        (
            "/fields/Microsoft.VSTS.Common.Priority",
            options.priority.map(Value::from),
        ),
    ]
    .into_iter()
    .filter_map(|(path, value)| value.map(|value| add_op(path, value)))
    .collect::<Vec<_>>();

    if let Some(tags) = options.tags {
        patch.insert(
            0,
            json!({"op": "replace", "path": "/fields/System.Tags", "value": tags}),
        );
    }

    patch
}

/// One `add` operation of the module's `build_json_patch/1`.
fn add_op(path: &str, value: Value) -> Value {
    json!({"op": "add", "path": path, "value": value})
}

/// `ado workitems delete`: `DELETE /_apis/wit/workitems/{id}`. No prompt and no
/// `--force` (R5, captured): the frozen CLI sends the DELETE on `n` and on EOF.
/// A 404 answers the module's own message; every other error is the shared
/// envelope.
pub fn delete(context: &mut Context, id: i64) -> Result<Report, AdoError> {
    let path = format!("/_apis/wit/workitems/{id}");

    match context.client()?.delete(&path, &[]) {
        Ok(()) => {
            let message = format!("Work item #{id} deleted.");

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Work item #{id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// The module's `create_work_item/1` success lines, minus the colour: the created
/// id and title, then type, state and the `_links.html.href` (empty when the link
/// is missing, exactly as the frozen Access chain reads it).
fn created_lines(work_item: &Value) -> Report {
    let fields = fields_of(work_item);

    Report::Text(format!(
        "Work item #{} created: {}\n  Type:  {}\n  State: {}\n  URL:   {}",
        id_cell(work_item),
        field(fields, "System.Title").unwrap_or_default(),
        field(fields, "System.WorkItemType").unwrap_or_default(),
        field(fields, "System.State").unwrap_or_default(),
        html_url(work_item),
    ))
}

/// The module's `update_work_item/1` success lines: the id, then the answered
/// title and state.
fn updated_lines(work_item: &Value) -> Report {
    let fields = fields_of(work_item);

    Report::Text(format!(
        "Work item #{} updated.\n  Title: {}\n  State: {}",
        id_cell(work_item),
        field(fields, "System.Title").unwrap_or_default(),
        field(fields, "System.State").unwrap_or_default(),
    ))
}

/// `wi["_links"]["html"]["href"] || ""`: a missing link chain reads as empty.
fn html_url(work_item: &Value) -> String {
    work_item
        .get("_links")
        .and_then(|links| links.get("html"))
        .and_then(|html| html.get("href"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The module's `run_wiql_query/3`: POST the WIQL, slice the ids with `--top`,
/// then fetch the batch. An empty slice is the module's "No work items found."
/// on the human path and the value envelope under `--json` (D21).
fn run_wiql(
    context: &mut Context,
    project: &str,
    wiql: &str,
    top: Option<i64>,
) -> Result<Report, AdoError> {
    let items = wiql_items(context, project, wiql, top)?;

    if items.is_empty() {
        return Ok(
            context.json_or_report(ok_value(Value::Array(Vec::new())), || {
                Report::Text("No work items found.".to_owned())
            }),
        );
    }

    let work_items = fetch_work_items(context, &items)?;

    Ok(
        context.json_or_report(ok_value(Value::Array(work_items.clone())), || {
            work_items_table(&work_items)
        }),
    )
}

/// `POST /{project}/_apis/wit/wiql` with `{"query": …}`, then `Enum.take/2` for
/// `--top`. The slice is client-side: no `$top` reaches the wire.
fn wiql_items(
    context: &mut Context,
    project: &str,
    wiql: &str,
    top: Option<i64>,
) -> Result<Vec<Value>, AdoError> {
    let path = format!("/{}/_apis/wit/wiql", encode_path_segment(project));
    let body = json!({ "query": wiql });

    let response = {
        let client = context.client()?;
        client.post(&path, &body, &[])?
    };

    let items = response
        .get("workItems")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(slice_top(items, top))
}

/// The module's `fetch_work_items_batch/2`: `GET /_apis/wit/workitems` for the
/// ids and fields. Its deliberate fallback keeps the WIQL `{id,url}` items when
/// the batch fetch fails or answers without a `value` array — it catches every
/// error, transport ones included (a W4 candidate for tightening).
fn fetch_work_items(context: &mut Context, items: &[Value]) -> Result<Vec<Value>, AdoError> {
    let ids = items.iter().map(id_cell).collect::<Vec<_>>().join(",");
    let params = vec![
        ("ids".to_owned(), ids),
        ("fields".to_owned(), WORK_ITEM_FIELDS.to_owned()),
    ];

    let fetched = {
        let client = context.client()?;
        client.get(WORK_ITEMS_PATH, &params)
    };

    match fetched {
        Ok(body) => match body.get("value").and_then(Value::as_array) {
            Some(work_items) => Ok(work_items.clone()),
            None => Ok(items.to_vec()),
        },
        Err(_) => Ok(items.to_vec()),
    }
}

/// `Enum.take/2`: a positive count takes from the front, zero takes nothing, and
/// a negative count takes from the end. An absent `--top` keeps everything.
fn slice_top(items: Vec<Value>, top: Option<i64>) -> Vec<Value> {
    match top {
        None => items,
        Some(top) if top >= 0 => items.into_iter().take(top as usize).collect(),
        Some(top) => {
            let count = top.unsigned_abs() as usize;
            let skip = items.len().saturating_sub(count);

            items.into_iter().skip(skip).collect()
        }
    }
}

/// The module's `list_work_items/1` WIQL: the team-project clause first, then one
/// clause per filter in the order the module reads them. Its `add_wiql_filter/3`
/// prepends `"AND …"` and joins with `" AND "`, so the frozen wire carries a
/// leading `AND` (and `AND AND` between filters) — a WHERE Azure cannot parse;
/// this joins valid clauses instead (D20). An explicitly empty filter is truthy
/// in Elixir and still becomes a clause, exactly as the module sends it.
fn list_wiql(
    project: &str,
    item_type: Option<&str>,
    assigned_to: Option<&str>,
    state: Option<&str>,
) -> String {
    let mut clauses = vec![format!("[System.TeamProject] = '{}'", escape_wiql(project))];

    for (value, field) in [
        (item_type, "System.WorkItemType"),
        (assigned_to, "System.AssignedTo"),
        (state, "System.State"),
    ] {
        if let Some(value) = value {
            clauses.push(format!("[{field}] = '{}'", escape_wiql(value)));
        }
    }

    format!(
        "{LIST_SELECT}{} ORDER BY [System.Id] DESC",
        clauses.join(" AND ")
    )
}

/// `String.replace(str, "'", "''")`, the module's `escape_wiql/1`.
fn escape_wiql(value: &str) -> String {
    value.replace('\'', "''")
}

/// The module's `print_work_items_table/1`: columns ID, Type, Title, State —
/// not the help text's "(ID, Title, Type, State, Assigned To)". There is no
/// empty-list message here: the empty branch answers before the formatter runs,
/// and a batch that answers `value: []` prints the header with no rows.
fn work_items_table(items: &[Value]) -> Report {
    let rows = items
        .iter()
        .map(|item| {
            let fields = fields_of(item);

            vec![
                id_cell(item),
                field(fields, "System.WorkItemType").unwrap_or_default(),
                field(fields, "System.Title").unwrap_or_default(),
                field(fields, "System.State").unwrap_or_default(),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Type".to_owned(),
            "Title".to_owned(),
            "State".to_owned(),
        ],
        rows,
    }
}

/// The module's `print_work_item_detail/1`, minus the colour: the labels and
/// fallbacks are the formatter's, the description is sliced at 200 graphemes with
/// the module's ellipsis, and the URL is the top-level `url`.
fn work_item_detail(work_item: &Value) -> Report {
    let fields = fields_of(work_item);
    let mut detail = String::from("\n");

    detail.push_str(&format!("Work Item #{}\n", id_cell(work_item)));
    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!(
        "  Type:        {}\n",
        field_or(fields, "System.WorkItemType", "?")
    ));
    detail.push_str(&format!(
        "  Title:       {}\n",
        field_or(fields, "System.Title", "?")
    ));
    detail.push_str(&format!(
        "  State:       {}\n",
        field_or(fields, "System.State", "?")
    ));
    detail.push_str(&format!(
        "  Assigned To: {}\n",
        display_name(fields, "System.AssignedTo", "(unassigned)")
    ));
    detail.push_str(&format!(
        "  Created By:  {}\n",
        display_name(fields, "System.CreatedBy", "?")
    ));
    detail.push_str(&format!(
        "  Created:     {}\n",
        field_or(fields, "System.CreatedDate", "?")
    ));

    if let Some(description) = field(fields, "System.Description") {
        let sliced = description.chars().take(200).collect::<String>();

        detail.push_str(&format!("  Description: {sliced}...\n"));
    }

    detail.push_str(&format!(
        "  URL:         {}\n",
        work_item
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
    ));
    detail.push('\n');

    Report::Text(detail)
}

/// `item["fields"] || %{}`: a missing, null or non-object fields map reads as
/// empty, so every cell falls back.
fn fields_of(work_item: &Value) -> Option<&Value> {
    work_item.get("fields").filter(|fields| fields.is_object())
}

/// `fields["key"] || ""` for the table: a missing, null or non-string field is
/// empty.
fn field(fields: Option<&Value>, key: &str) -> Option<String> {
    fields
        .and_then(|fields| fields.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The detail's `|| "?"` form of [`field`], where a non-string value is empty
/// too exactly as the table's is.
fn field_or(fields: Option<&Value>, key: &str, default: &str) -> String {
    field(fields, key).unwrap_or_else(|| default.to_owned())
}

/// `display_name/2`: only a map carrying `displayName` has a name; anything else
/// — a plain string field included — takes the default.
fn display_name(fields: Option<&Value>, key: &str, default: &str) -> String {
    fields
        .and_then(|fields| fields.get(key))
        .and_then(|identity| identity.get("displayName"))
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_owned()
}

/// `to_string(item["id"] || "")`: the batch's numeric id reads as its text, a
/// string id as itself, and a missing or null id is empty.
fn id_cell(item: &Value) -> String {
    match item.get("id") {
        Some(Value::String(id)) => id.clone(),
        Some(Value::Null) | None => String::new(),
        Some(id) => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn list_wiql_joins_one_valid_clause_per_filter() {
        assert_eq!(
            list_wiql("Alpha", None, None, None),
            format!("{LIST_SELECT}[System.TeamProject] = 'Alpha' ORDER BY [System.Id] DESC"),
            "the unfiltered query is the frozen binary's, byte for byte"
        );
        assert_eq!(
            list_wiql("Alpha", Some("Bug"), None, None),
            format!(
                "{LIST_SELECT}[System.TeamProject] = 'Alpha' AND [System.WorkItemType] = 'Bug' ORDER BY [System.Id] DESC"
            ),
            "one filter"
        );
        assert_eq!(
            list_wiql("Alpha", Some("Bug"), Some("alice"), None),
            format!(
                "{LIST_SELECT}[System.TeamProject] = 'Alpha' AND [System.WorkItemType] = 'Bug' AND [System.AssignedTo] = 'alice' ORDER BY [System.Id] DESC"
            ),
            "two filters"
        );
        assert_eq!(
            list_wiql("Alpha", Some("Bug"), Some("alice"), Some("Active")),
            format!(
                "{LIST_SELECT}[System.TeamProject] = 'Alpha' AND [System.WorkItemType] = 'Bug' AND [System.AssignedTo] = 'alice' AND [System.State] = 'Active' ORDER BY [System.Id] DESC"
            ),
            "three filters, joined without the frozen leading AND (D20)"
        );
    }

    #[test]
    fn list_wiql_escapes_quotes_and_keeps_an_explicit_empty_filter() {
        assert_eq!(
            list_wiql("Alpha's", Some(""), None, None),
            format!(
                "{LIST_SELECT}[System.TeamProject] = 'Alpha''s' AND [System.WorkItemType] = '' ORDER BY [System.Id] DESC"
            ),
            "an explicit empty --type is truthy in Elixir and still a clause"
        );
    }

    #[test]
    fn slice_top_mirrors_enum_take() {
        let items = vec![json!({"id": 1}), json!({"id": 2}), json!({"id": 3})];
        let ids = |items: Vec<Value>| items.iter().map(id_cell).collect::<Vec<_>>();

        assert_eq!(ids(slice_top(items.clone(), None)), ["1", "2", "3"]);
        assert_eq!(ids(slice_top(items.clone(), Some(0))), Vec::<String>::new());
        assert_eq!(ids(slice_top(items.clone(), Some(2))), ["1", "2"]);
        assert_eq!(ids(slice_top(items.clone(), Some(-1))), ["3"]);
        assert_eq!(ids(slice_top(items.clone(), Some(-5))), ["1", "2", "3"]);
    }

    #[test]
    fn id_cell_reads_numbers_strings_and_missing_ids() {
        assert_eq!(id_cell(&json!({"id": 42})), "42");
        assert_eq!(id_cell(&json!({"id": "42"})), "42");
        assert_eq!(id_cell(&json!({"id": null})), "");
        assert_eq!(id_cell(&json!({})), "");
    }

    #[test]
    fn work_items_table_uses_the_module_columns() {
        let items = vec![json!({
            "id": 42,
            "fields": {
                "System.WorkItemType": "Bug",
                "System.Title": "Checkout fails on expired cards",
                "System.State": "Active",
            },
        })];

        assert_eq!(
            work_items_table(&items),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Type".to_owned(),
                    "Title".to_owned(),
                    "State".to_owned()
                ],
                rows: vec![vec![
                    "42".to_owned(),
                    "Bug".to_owned(),
                    "Checkout fails on expired cards".to_owned(),
                    "Active".to_owned(),
                ]],
            }
        );
    }

    #[test]
    fn work_items_table_reads_an_empty_fields_map_as_empty_cells() {
        let items = vec![json!({"id": 7}), json!({"id": 8, "fields": null})];

        assert_eq!(
            work_items_table(&items),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Type".to_owned(),
                    "Title".to_owned(),
                    "State".to_owned()
                ],
                rows: vec![
                    vec!["7".to_owned(), String::new(), String::new(), String::new()],
                    vec!["8".to_owned(), String::new(), String::new(), String::new()],
                ],
            }
        );
    }

    #[test]
    fn work_items_table_of_nothing_keeps_the_header() {
        assert_eq!(
            work_items_table(&[]),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Type".to_owned(),
                    "Title".to_owned(),
                    "State".to_owned()
                ],
                rows: Vec::new(),
            },
            "a 2xx batch answer of value: [] still renders the module's header"
        );
    }

    #[test]
    fn work_item_detail_prints_the_labels_and_the_ellipsis() {
        let work_item = json!({
            "id": 42,
            "fields": {
                "System.WorkItemType": "Bug",
                "System.Title": "Checkout fails on expired cards",
                "System.State": "Active",
                "System.AssignedTo": {"displayName": "Alice Example"},
                "System.CreatedBy": {"displayName": "Bob Example"},
                "System.CreatedDate": "2026-09-14T09:31:07.83Z",
                "System.Description": "Short description",
            },
            "url": "https://dev.azure.com/myorg/_apis/wit/workItems/42",
        });

        let Report::Text(detail) = work_item_detail(&work_item) else {
            panic!("the detail is a text report");
        };

        assert!(detail.starts_with("\nWork Item #42\n"), "detail: {detail}");
        assert!(detail.contains("  Type:        Bug\n"), "detail: {detail}");
        assert!(
            detail.contains("  Assigned To: Alice Example\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Created By:  Bob Example\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Description: Short description...\n"),
            "the module appends its ellipsis to the 200-grapheme slice: {detail}"
        );
        assert!(
            detail.contains("  URL:         https://dev.azure.com/myorg/_apis/wit/workItems/42\n"),
            "detail: {detail}"
        );
    }

    #[test]
    fn work_item_detail_falls_back_without_the_fields() {
        let Report::Text(detail) = work_item_detail(&json!({"id": 7})) else {
            panic!("the detail is a text report");
        };

        assert!(detail.contains("  Type:        ?\n"), "detail: {detail}");
        assert!(
            detail.contains("  Assigned To: (unassigned)\n"),
            "detail: {detail}"
        );
        assert!(detail.contains("  Created By:  ?\n"), "detail: {detail}");
        assert!(
            !detail.contains("  Description:"),
            "a missing description prints no line: {detail}"
        );
    }

    /// A plain-string identity is the module's `display_name(_, default)` clause:
    /// it takes the default even though the field is present.
    #[test]
    fn work_item_detail_reads_a_string_identity_as_unassigned() {
        let Report::Text(detail) = work_item_detail(&json!({
            "id": 7,
            "fields": {"System.AssignedTo": "Alice <alice@example.test>"},
        })) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.contains("  Assigned To: (unassigned)\n"),
            "detail: {detail}"
        );
    }

    #[test]
    fn create_patch_is_the_captured_field_order_and_skips_absent_options() {
        assert_eq!(
            create_patch("T", WorkItemOptions::default()),
            vec![json!({"op": "add", "path": "/fields/System.Title", "value": "T"})],
            "an absent option is an absent operation"
        );
        assert_eq!(
            create_patch(
                "T",
                WorkItemOptions {
                    description: Some("D".to_owned()),
                    state: Some("Active".to_owned()),
                    assigned_to: Some("alice".to_owned()),
                    priority: Some(2),
                    tags: Some("frontend,ui".to_owned()),
                },
            ),
            vec![
                json!({"op": "add", "path": "/fields/System.Title", "value": "T"}),
                json!({"op": "add", "path": "/fields/System.Description", "value": "D"}),
                json!({"op": "add", "path": "/fields/System.AssignedTo", "value": "alice"}),
                json!({"op": "add", "path": "/fields/System.State", "value": "Active"}),
                json!({"op": "add", "path": "/fields/Microsoft.VSTS.Common.Priority", "value": 2}),
                json!({"op": "add", "path": "/fields/System.Tags", "value": "frontend,ui"}),
            ],
            "captured: title, description, assigned-to, state, priority, tags"
        );
        assert_eq!(
            create_patch(
                "T",
                WorkItemOptions {
                    priority: Some(3),
                    ..WorkItemOptions::default()
                },
            ),
            vec![
                json!({"op": "add", "path": "/fields/System.Title", "value": "T"}),
                json!({"op": "add", "path": "/fields/Microsoft.VSTS.Common.Priority", "value": 3}),
            ],
            "--priority is an integer operation"
        );
    }

    #[test]
    fn update_patch_puts_tags_first_and_uses_the_modules_own_field_order() {
        assert_eq!(
            update_patch(Some("T".to_owned()), WorkItemOptions::default()),
            vec![json!({"op": "add", "path": "/fields/System.Title", "value": "T"})]
        );
        assert_eq!(
            update_patch(
                Some("T".to_owned()),
                WorkItemOptions {
                    description: Some("D".to_owned()),
                    state: Some("Closed".to_owned()),
                    assigned_to: Some("bob".to_owned()),
                    priority: Some(1),
                    tags: Some("a,b".to_owned()),
                },
            ),
            vec![
                json!({"op": "replace", "path": "/fields/System.Tags", "value": "a,b"}),
                json!({"op": "add", "path": "/fields/System.Title", "value": "T"}),
                json!({"op": "add", "path": "/fields/System.Description", "value": "D"}),
                json!({"op": "add", "path": "/fields/System.State", "value": "Closed"}),
                json!({"op": "add", "path": "/fields/System.AssignedTo", "value": "bob"}),
                json!({"op": "add", "path": "/fields/Microsoft.VSTS.Common.Priority", "value": 1}),
            ],
            "captured: tags first with replace, then title, description, state, assigned-to, priority"
        );
        assert_eq!(
            update_patch(
                None,
                WorkItemOptions {
                    tags: Some("solo".to_owned()),
                    ..WorkItemOptions::default()
                },
            ),
            vec![json!({"op": "replace", "path": "/fields/System.Tags", "value": "solo"})],
            "tags alone is still a non-empty patch"
        );
        assert!(
            WorkItemOptions::default().is_empty(),
            "the guard reads no field at all from the options"
        );
    }

    #[test]
    fn created_lines_print_the_captured_success_line() {
        let Report::Text(text) = created_lines(&json!({
            "id": 42,
            "fields": {
                "System.Title": "Checkout fails on expired cards",
                "System.WorkItemType": "Bug",
                "System.State": "New",
            },
            "_links": {"html": {"href": "https://dev.azure.com/myorg/Alpha/_workitems/edit/42"}},
        })) else {
            panic!("the success line is a text report");
        };

        assert_eq!(
            text,
            "Work item #42 created: Checkout fails on expired cards\n  Type:  Bug\n  State: New\n  URL:   https://dev.azure.com/myorg/Alpha/_workitems/edit/42"
        );
    }

    #[test]
    fn created_lines_read_a_missing_link_and_missing_fields_as_empty() {
        let Report::Text(text) = created_lines(&json!({"id": 7})) else {
            panic!("the success line is a text report");
        };

        assert_eq!(
            text, "Work item #7 created: \n  Type:  \n  State: \n  URL:   ",
            "the frozen Access chain reads nil as empty, and interpolation prints nothing"
        );
    }

    #[test]
    fn updated_lines_print_the_captured_success_line() {
        let Report::Text(text) = updated_lines(&json!({
            "id": 42,
            "fields": {"System.Title": "Renamed", "System.State": "Active"},
        })) else {
            panic!("the success line is a text report");
        };

        assert_eq!(
            text,
            "Work item #42 updated.\n  Title: Renamed\n  State: Active"
        );
    }
}
