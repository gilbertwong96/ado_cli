//! `ado prs list|show` — the read paths of `lib/ado_cli/cli/pull_requests.ex`: the
//! same REST surface, the same filters, and the same human layouts.
//!
//! `create`, `complete`, `abandon`, `approve`, `vote`, `diff`, `comments` and
//! `reviewers` are Wave 2 and deliberately absent.

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::Value;

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// `list_prs/1`: `GET /{project}/_apis/git/repositories/{repo_id}/pullrequests`
/// with the module's search criteria. Under `--json` the body is the value
/// envelope — a bare array under `result`, the kind the module's
/// `Helpers.json_or_format` picks (W1-R12).
pub fn list(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    status: Option<String>,
    creator: Option<String>,
    top: Option<i64>,
) -> Result<Report, AdoError> {
    let path = collection_path(project, repo_id);
    let pull_requests = items(
        context
            .client()?
            .list(&path, &list_params(status, creator, top))?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(pull_requests.clone())), || {
            pull_requests_table(&pull_requests)
        }),
    )
}

/// `show_pr/1`: `GET …/pullrequests/{pr_id}`, with no params beyond the version.
/// The module answers a 404 with its own message.
pub fn show(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
) -> Result<Report, AdoError> {
    let path = format!("{}/{}", collection_path(project, repo_id), pr_id);
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(pull_request) => Ok(context.json_or_report(ok_value(pull_request.clone()), || {
            pull_request_detail(&pull_request)
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pull request #{pr_id} not found in {project}/{repo_id}"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// The module's
/// `"/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests"`,
/// with the stricter segment encoder (D22).
fn collection_path(project: &str, repo_id: &str) -> String {
    format!(
        "/{}/_apis/git/repositories/{}/pullrequests",
        encode_path_segment(project),
        encode_path_segment(repo_id)
    )
}

/// The module's `list_prs/1` criteria: `searchCriteria.status` always — its
/// `Map.get(parsed.options, :status, "active")` default — then `creator` and `top`
/// when present. `if value` is truthy for `0` and `""`, so only an absent option is
/// omitted; an explicit empty `--status`/`--creator` and a zero or negative
/// `--top` all reach the wire.
fn list_params(
    status: Option<String>,
    creator: Option<String>,
    top: Option<i64>,
) -> Vec<(String, String)> {
    let mut params = vec![(
        "searchCriteria.status".to_owned(),
        status.unwrap_or_else(|| "active".to_owned()),
    )];

    if let Some(creator) = creator {
        params.push(("searchCriteria.creatorId".to_owned(), creator));
    }
    if let Some(top) = top {
        params.push(("$top".to_owned(), top.to_string()));
    }

    params
}

/// The module's `print_prs_table/1`: the formatter's row carries ID, Title,
/// `source -> target` and Status, so the table splits the branch pair into its two
/// columns (spec D9 renderer shape). The help text's Creator column is absent
/// because the formatter has none.
fn pull_requests_table(pull_requests: &[Value]) -> Report {
    if pull_requests.is_empty() {
        return Report::Text("No pull requests found.".to_owned());
    }

    let rows = pull_requests
        .iter()
        .map(|pull_request| {
            vec![
                id_cell(pull_request),
                field(pull_request, "title"),
                branch_cell(pull_request, "sourceRefName"),
                branch_cell(pull_request, "targetRefName"),
                field(pull_request, "status"),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Title".to_owned(),
            "Source".to_owned(),
            "Target".to_owned(),
            "Status".to_owned(),
        ],
        rows,
    }
}

/// The module's `print_pr_detail/1`, minus the colour: the labels and fallbacks are
/// the formatter's. The Reviewers line prints whenever the payload carries an array
/// — the module's `if reviewers` guard, where an empty array is truthy and a null
/// payload is not.
fn pull_request_detail(pull_request: &Value) -> Report {
    let mut detail = String::from("\n");

    detail.push_str(&format!("Pull Request #{}\n", id_cell(pull_request)));
    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!(
        "  Title:       {}\n",
        field(pull_request, "title")
    ));
    detail.push_str(&format!(
        "  Description: {}\n",
        description_cell(pull_request)
    ));
    detail.push_str(&format!(
        "  Status:      {}\n",
        field(pull_request, "status")
    ));
    detail.push_str(&format!(
        "  Source:      {}\n",
        field(pull_request, "sourceRefName")
    ));
    detail.push_str(&format!(
        "  Target:      {}\n",
        field(pull_request, "targetRefName")
    ));
    detail.push_str(&format!(
        "  Created By:  {}\n",
        created_by_cell(pull_request)
    ));
    detail.push_str(&format!(
        "  Created:     {}\n",
        field(pull_request, "creationDate")
    ));

    if let Some(reviewers) = pull_request.get("reviewers").and_then(Value::as_array) {
        detail.push_str(&format!("  Reviewers:   {}\n", reviewers_cell(reviewers)));
    }

    detail.push_str(&format!("  URL:         {}\n", field(pull_request, "url")));
    detail.push('\n');

    Report::Text(detail)
}

/// `to_string(pr["pullRequestId"] || "")`: a numeric id reads as its text, a string
/// id as itself, and a missing, null or false id is empty.
fn id_cell(pull_request: &Value) -> String {
    match pull_request.get("pullRequestId") {
        Some(Value::String(id)) => id.clone(),
        Some(Value::Null) | Some(Value::Bool(false)) | None => String::new(),
        Some(id) => id.to_string(),
    }
}

/// `String.replace_prefix(pr["sourceRefName"] || "", "refs/heads/", "")` — the same
/// for the target.
fn branch_cell(pull_request: &Value, key: &str) -> String {
    let branch = field(pull_request, key);

    branch
        .strip_prefix("refs/heads/")
        .unwrap_or(&branch)
        .to_owned()
}

/// `pr["description"] || "(none)"`.
fn description_cell(pull_request: &Value) -> String {
    pull_request
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("(none)")
        .to_owned()
}

/// `get_in(pr, ["createdBy", "displayName"]) || "?"`.
fn created_by_cell(pull_request: &Value) -> String {
    pull_request
        .get("createdBy")
        .and_then(|created_by| created_by.get("displayName"))
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_owned()
}

/// `Enum.map_join(reviewers, ", ", & &1["displayName"])`: a reviewer without a
/// display name contributes an empty cell.
fn reviewers_cell(reviewers: &[Value]) -> String {
    reviewers
        .iter()
        .map(|reviewer| {
            reviewer
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(", ")
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
    fn list_params_default_the_status_and_keep_present_values() {
        assert_eq!(
            list_params(None, None, None),
            vec![("searchCriteria.status".to_owned(), "active".to_owned())],
            "the module's Map.get default is the only value that reaches the wire when nothing is given"
        );
        assert_eq!(
            list_params(
                Some("completed".to_owned()),
                Some("alice@example.test".to_owned()),
                Some(5)
            ),
            vec![
                ("searchCriteria.status".to_owned(), "completed".to_owned()),
                (
                    "searchCriteria.creatorId".to_owned(),
                    "alice@example.test".to_owned()
                ),
                ("$top".to_owned(), "5".to_owned()),
            ],
            "the module's own order: status, creator, top"
        );
        assert_eq!(
            list_params(Some(String::new()), Some(String::new()), Some(0)),
            vec![
                ("searchCriteria.status".to_owned(), String::new()),
                ("searchCriteria.creatorId".to_owned(), String::new()),
                ("$top".to_owned(), "0".to_owned()),
            ],
            "present means sent: an explicit empty status or creator and a zero top"
        );
        assert_eq!(
            list_params(Some("active".to_owned()), None, Some(-1)),
            vec![
                ("searchCriteria.status".to_owned(), "active".to_owned()),
                ("$top".to_owned(), "-1".to_owned()),
            ],
            "an absent creator is omitted; a negative top is sent"
        );
    }

    #[test]
    fn pull_requests_table_uses_the_module_columns() {
        let pull_requests = vec![
            json!({
                "pullRequestId": 137,
                "title": "Add payment retries",
                "sourceRefName": "refs/heads/feature/payments",
                "targetRefName": "refs/heads/main",
                "status": "active",
                "createdBy": {"displayName": "Alice Example"},
            }),
            json!({
                "pullRequestId": 138,
                "title": "Retire the legacy ledger",
                "sourceRefName": "refs/heads/feature/ledger-retirement",
                "targetRefName": "refs/heads/main",
                "status": "completed",
            }),
        ];

        assert_eq!(
            pull_requests_table(&pull_requests),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Title".to_owned(),
                    "Source".to_owned(),
                    "Target".to_owned(),
                    "Status".to_owned(),
                ],
                rows: vec![
                    vec![
                        "137".to_owned(),
                        "Add payment retries".to_owned(),
                        "feature/payments".to_owned(),
                        "main".to_owned(),
                        "active".to_owned(),
                    ],
                    vec![
                        "138".to_owned(),
                        "Retire the legacy ledger".to_owned(),
                        "feature/ledger-retirement".to_owned(),
                        "main".to_owned(),
                        "completed".to_owned(),
                    ],
                ],
            },
            "the formatter's cell values, with the branch pair split and no Creator column"
        );
    }

    #[test]
    fn pull_requests_table_reads_missing_fields_as_empty_cells() {
        let pull_requests = vec![json!({"title": null}), json!({})];

        assert_eq!(
            pull_requests_table(&pull_requests),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Title".to_owned(),
                    "Source".to_owned(),
                    "Target".to_owned(),
                    "Status".to_owned(),
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
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new()
                    ],
                ],
            }
        );
    }

    #[test]
    fn pull_requests_table_of_nothing_is_the_module_message() {
        assert_eq!(
            pull_requests_table(&[]),
            Report::Text("No pull requests found.".to_owned())
        );
    }

    #[test]
    fn pull_request_detail_prints_the_labels_and_the_reviewers() {
        let pull_request = json!({
            "pullRequestId": 137,
            "title": "Add payment retries",
            "description": "Retries soft declines up to three times before failing the order.",
            "status": "active",
            "sourceRefName": "refs/heads/feature/payments",
            "targetRefName": "refs/heads/main",
            "createdBy": {"displayName": "Alice Example"},
            "creationDate": "2026-09-14T09:31:07.83Z",
            "reviewers": [
                {"displayName": "Bob Example", "vote": 10},
                {"displayName": "Carol Example", "vote": 0},
            ],
            "url": "https://dev.azure.com/myorg/Alpha/_apis/git/repositories/repo/pullRequests/137",
        });

        let Report::Text(detail) = pull_request_detail(&pull_request) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.starts_with("\nPull Request #137\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(&format!("{}\n", "─".repeat(60))),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Title:       Add payment retries\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(
                "  Description: Retries soft declines up to three times before failing the order.\n"
            ),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Status:      active\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Source:      refs/heads/feature/payments\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Target:      refs/heads/main\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Created By:  Alice Example\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Created:     2026-09-14T09:31:07.83Z\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Reviewers:   Bob Example, Carol Example\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  URL:         https://dev.azure.com/myorg/Alpha/_apis/git/repositories/repo/pullRequests/137\n"),
            "detail: {detail}"
        );
        assert!(detail.ends_with("pullRequests/137\n\n"), "detail: {detail}");
    }

    /// The module's `if reviewers = pr["reviewers"]` is truthy for `[]`, so an empty
    /// array prints the label with an empty join; a null or missing payload prints no
    /// line at all.
    #[test]
    fn pull_request_detail_prints_the_reviewers_line_only_for_an_array() {
        let Report::Text(empty) = pull_request_detail(&json!({"reviewers": []})) else {
            panic!("the detail is a text report");
        };
        assert!(
            empty.contains("  Reviewers:   \n"),
            "an empty array is truthy in Elixir: {empty}"
        );

        for pull_request in [json!({"reviewers": null}), json!({})] {
            let Report::Text(detail) = pull_request_detail(&pull_request) else {
                panic!("the detail is a text report");
            };
            assert!(
                !detail.contains("  Reviewers:"),
                "a null or missing reviewers prints no line: {detail}"
            );
        }
    }

    #[test]
    fn pull_request_detail_says_none_and_question_mark_without_the_fields() {
        let Report::Text(detail) =
            pull_request_detail(&json!({"pullRequestId": 7, "description": null}))
        else {
            panic!("the detail is a text report");
        };

        assert!(detail.contains("Pull Request #7\n"), "detail: {detail}");
        assert!(
            detail.contains("  Description: (none)\n"),
            "detail: {detail}"
        );
        assert!(detail.contains("  Created By:  ?\n"), "detail: {detail}");
        assert!(!detail.contains("  Reviewers:"), "detail: {detail}");
    }

    /// The Elixir's `||` is truthy-based: an empty description is truthy and prints
    /// empty, not `(none)`.
    #[test]
    fn pull_request_detail_keeps_an_explicitly_empty_description() {
        let Report::Text(detail) = pull_request_detail(&json!({"description": ""})) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.contains("  Description: \n"),
            "an empty description is not the missing one: {detail}"
        );
    }

    #[test]
    fn id_cell_reads_numbers_strings_and_missing_ids() {
        assert_eq!(id_cell(&json!({"pullRequestId": 137})), "137");
        assert_eq!(id_cell(&json!({"pullRequestId": "137"})), "137");
        assert_eq!(id_cell(&json!({"pullRequestId": null})), "");
        assert_eq!(id_cell(&json!({"pullRequestId": false})), "");
        assert_eq!(id_cell(&json!({})), "");
    }

    #[test]
    fn branch_cell_strips_the_heads_prefix() {
        assert_eq!(
            branch_cell(
                &json!({"sourceRefName": "refs/heads/feature/payments"}),
                "sourceRefName"
            ),
            "feature/payments"
        );
        assert_eq!(
            branch_cell(&json!({"sourceRefName": "refs/tags/v1"}), "sourceRefName"),
            "refs/tags/v1"
        );
        assert_eq!(branch_cell(&json!({}), "sourceRefName"), "");
    }
}
