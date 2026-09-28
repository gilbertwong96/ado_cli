//! `ado repos list|show|branches` — the read paths of `lib/ado_cli/cli/repos.ex`:
//! the same REST surface, the same filter, and the same human layouts.

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::Value;

use crate::context::Context;
use crate::output::Report;

/// `ado repos list`: `GET /{project}/_apis/git/repositories`, with `includeLinks`
/// when `--include-links` is set. Under `--json` the body is the value envelope —
/// a bare array under `result`, the kind the module's `Helpers.json_or_format`
/// picks (W1-R12).
pub fn list(context: &mut Context, project: &str, include_links: bool) -> Result<Report, AdoError> {
    let path = format!("/{}/_apis/git/repositories", encode_path_segment(project));
    let repos = items(context.client()?.list(&path, &list_params(include_links))?);

    Ok(
        context.json_or_report(ok_value(Value::Array(repos.clone())), || {
            repositories_table(&repos)
        }),
    )
}

/// `ado repos show`: `GET /{project}/_apis/git/repositories/{repo_id}`, with no
/// params beyond the version. The module answers a 404 with its own message.
pub fn show(context: &mut Context, project: &str, repo_id: &str) -> Result<Report, AdoError> {
    let path = format!(
        "/{}/_apis/git/repositories/{}",
        encode_path_segment(project),
        encode_path_segment(repo_id)
    );
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(repo) => Ok(context.json_or_report(ok_value(repo.clone()), || repository_detail(&repo))),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Repository '{repo_id}' not found in project '{project}'"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado repos branches`: `GET /{project}/_apis/git/repositories/{repo_id}/refs`
/// with the module's `filter`, defaulting to `heads/`, then the module's own
/// `refs/heads/` filter on the answer.
pub fn branches(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    filter: Option<String>,
) -> Result<Report, AdoError> {
    let path = format!(
        "/{}/_apis/git/repositories/{}/refs",
        encode_path_segment(project),
        encode_path_segment(repo_id)
    );
    let refs = head_refs(&items(
        context.client()?.list(&path, &branches_params(filter))?,
    ));

    Ok(
        context.json_or_report(ok_value(Value::Array(refs.clone())), || {
            branches_table(&refs)
        }),
    )
}

fn list_params(include_links: bool) -> Vec<(String, String)> {
    if include_links {
        vec![("includeLinks".to_owned(), "true".to_owned())]
    } else {
        Vec::new()
    }
}

/// The Elixir's `Map.get(parsed.options, :filter, "heads/")`: the default applies
/// only when `--filter` is absent, so an explicitly empty pattern is sent as
/// `filter=`.
fn branches_params(filter: Option<String>) -> Vec<(String, String)> {
    vec![(
        "filter".to_owned(),
        filter.unwrap_or_else(|| "heads/".to_owned()),
    )]
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

/// The module's `Enum.filter(refs, &String.starts_with?(&1["name"] || "",
/// "refs/heads/"))`: a ref without a name falls away with the tags.
fn head_refs(refs: &[Value]) -> Vec<Value> {
    refs.iter()
        .filter(|reference| field(reference, "name").starts_with("refs/heads/"))
        .cloned()
        .collect()
}

/// The module's `print_repos_table/1`: columns ID, Name, Default Branch, with
/// its "No repositories found." for an empty list. `--include-links` adds a query
/// param but no column, exactly as the module's formatter ignores the links.
fn repositories_table(repos: &[Value]) -> Report {
    if repos.is_empty() {
        return Report::Text("No repositories found.".to_owned());
    }

    let rows = repos
        .iter()
        .map(|repo| {
            vec![
                field(repo, "id"),
                field(repo, "name"),
                default_branch_cell(repo),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Name".to_owned(),
            "Default Branch".to_owned(),
        ],
        rows,
    }
}

/// `get_in(r, ["defaultBranch"]) || "(none)"` with the module's
/// `String.replace_prefix(branch, "refs/heads/", "")`.
fn default_branch_cell(repo: &Value) -> String {
    let branch = repo
        .get("defaultBranch")
        .and_then(Value::as_str)
        .unwrap_or("(none)");

    branch
        .strip_prefix("refs/heads/")
        .unwrap_or(branch)
        .to_owned()
}

/// The module's `print_repo_detail/1`, minus the colour: the full ref for the
/// default branch, the size as bytes (or 0), and the project line only when the
/// repository carries one.
fn repository_detail(repo: &Value) -> Report {
    let mut detail = String::from("\nRepository Details\n\n");

    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:             {}\n", field(repo, "id")));
    detail.push_str(&format!("  Name:           {}\n", field(repo, "name")));
    detail.push_str(&format!("  Default Branch: {}\n", default_branch_raw(repo)));
    detail.push_str(&format!(
        "  Size:           {} bytes\n",
        repo.get("size").and_then(Value::as_i64).unwrap_or(0)
    ));
    detail.push_str(&format!("  SSH URL:        {}\n", field(repo, "sshUrl")));
    detail.push_str(&format!("  Web URL:        {}\n", field(repo, "webUrl")));

    if let Some(project) = repo.get("project").filter(|project| !project.is_null()) {
        detail.push_str(&format!(
            "  Project:        {} ({})\n",
            field(project, "name"),
            field(project, "id")
        ));
    }

    detail.push('\n');

    Report::Text(detail)
}

/// `get_in(repo, ["defaultBranch"]) || "(none)"`, unstripped: the detail shows
/// the full ref.
fn default_branch_raw(repo: &Value) -> String {
    repo.get("defaultBranch")
        .and_then(Value::as_str)
        .unwrap_or("(none)")
        .to_owned()
}

/// The module's `print_branches_table/1`: columns Name (prefix stripped) and
/// Object ID, with its "No branches found." for an empty list.
fn branches_table(branches: &[Value]) -> Report {
    if branches.is_empty() {
        return Report::Text("No branches found.".to_owned());
    }

    let rows = branches
        .iter()
        .map(|branch| vec![branch_name_cell(branch), field(branch, "objectId")])
        .collect();

    Report::Table {
        headers: vec!["Name".to_owned(), "Object ID".to_owned()],
        rows,
    }
}

/// `String.replace_prefix(b["name"] || "", "refs/heads/", "")` for the branches
/// table.
fn branch_name_cell(branch: &Value) -> String {
    let name = field(branch, "name");

    name.strip_prefix("refs/heads/").unwrap_or(&name).to_owned()
}

/// `v["key"] || ""`: a missing or non-string field is empty.
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
    fn list_params_only_carry_the_links_when_asked() {
        assert_eq!(list_params(false), Vec::new());
        assert_eq!(
            list_params(true),
            vec![("includeLinks".to_owned(), "true".to_owned())],
            "the Elixir's include_links is a boolean written as true"
        );
    }

    #[test]
    fn branches_params_default_to_heads_and_keep_an_explicit_empty() {
        assert_eq!(
            branches_params(None),
            vec![("filter".to_owned(), "heads/".to_owned())],
            "the module's Map.get default"
        );
        assert_eq!(
            branches_params(Some("feature".to_owned())),
            vec![("filter".to_owned(), "feature".to_owned())]
        );
        assert_eq!(
            branches_params(Some(String::new())),
            vec![("filter".to_owned(), String::new())],
            "an explicit empty --filter is not the absent option"
        );
    }

    #[test]
    fn repositories_table_uses_the_module_columns() {
        let repos = vec![
            json!({
                "id": "a1",
                "name": "Alpha.Core",
                "defaultBranch": "refs/heads/main",
                "sshUrl": "git@example.test:Alpha.Core",
            }),
            json!({"id": "b2", "name": "Alpha.Docs", "defaultBranch": null}),
        ];

        assert_eq!(
            repositories_table(&repos),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Name".to_owned(),
                    "Default Branch".to_owned()
                ],
                rows: vec![
                    vec!["a1".to_owned(), "Alpha.Core".to_owned(), "main".to_owned()],
                    vec![
                        "b2".to_owned(),
                        "Alpha.Docs".to_owned(),
                        "(none)".to_owned()
                    ],
                ],
            },
            "the table is ID/Name/stripped branch, never the links"
        );
    }

    #[test]
    fn repositories_table_of_nothing_is_the_module_message() {
        assert_eq!(
            repositories_table(&[]),
            Report::Text("No repositories found.".to_owned())
        );
    }

    #[test]
    fn head_refs_keep_only_the_heads() {
        let refs = vec![
            json!({"name": "refs/heads/main"}),
            json!({"name": "refs/tags/v1.0.0"}),
            json!({"name": "refs/heads/feature/payments"}),
            json!({"objectId": "no-name"}),
        ];

        assert_eq!(
            head_refs(&refs),
            vec![
                json!({"name": "refs/heads/main"}),
                json!({"name": "refs/heads/feature/payments"}),
            ],
            "the module's starts_with? drops tags and nameless refs"
        );
    }

    #[test]
    fn branches_table_uses_the_module_columns() {
        let branches = vec![json!({
            "name": "refs/heads/feature/payments",
            "objectId": "2222222222222222222222222222222222222222",
            "creator": {"displayName": "Bob Example"},
        })];

        assert_eq!(
            branches_table(&branches),
            Report::Table {
                headers: vec!["Name".to_owned(), "Object ID".to_owned()],
                rows: vec![vec![
                    "feature/payments".to_owned(),
                    "2222222222222222222222222222222222222222".to_owned()
                ]],
            }
        );
    }

    #[test]
    fn branches_table_of_nothing_is_the_module_message() {
        assert_eq!(
            branches_table(&[]),
            Report::Text("No branches found.".to_owned())
        );
    }

    #[test]
    fn repository_detail_prints_the_labels_and_the_project() {
        let repo = json!({
            "id": "a1",
            "name": "Alpha.Core",
            "defaultBranch": "refs/heads/main",
            "size": 204800,
            "sshUrl": "git@example.test:Alpha.Core",
            "webUrl": "https://example.test/Alpha.Core",
            "project": {"id": "p1", "name": "Alpha"},
        });

        let Report::Text(detail) = repository_detail(&repo) else {
            panic!("the detail is a text report");
        };

        assert!(detail.contains("Repository Details\n"), "detail: {detail}");
        assert!(
            detail.contains("  ID:             a1\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Default Branch: refs/heads/main\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Size:           204800 bytes\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Project:        Alpha (p1)\n"),
            "detail: {detail}"
        );
    }

    #[test]
    fn repository_detail_says_none_and_zero_without_the_fields() {
        let Report::Text(detail) = repository_detail(&json!({"id": "a1"})) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.contains("  Default Branch: (none)\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Size:           0 bytes\n"),
            "detail: {detail}"
        );
        assert!(
            !detail.contains("  Project:"),
            "a repository without a project prints no project line: {detail}"
        );
    }
}
