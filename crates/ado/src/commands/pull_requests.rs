//! `ado prs list|show|create|complete|abandon|approve|vote` — the read and write
//! paths of `lib/ado_cli/cli/pull_requests.ex`: the same REST surface, the same
//! filters, the same merge and vote bodies, and the same human layouts.
//!
//! `diff`, `comments` and `reviewers` are Tasks 10 and 11 and deliberately absent.
//!
//! The captures settled four things a reader of the frozen help would get wrong.
//! `complete` is a **two-request** command: it reads the PR for
//! `lastMergeSourceCommit.commitId` and only then PATCHes the completion body,
//! whose `mergeStrategy` key is absent when the option is absent and whose value
//! maps `squash`/`rebase` onto the API's camelCase names (an unknown value passes
//! through). `approve` and `vote` are also two requests: the frozen
//! `resolve_reviewer_id/3` fetches `GET /{org}/_apis/connectionData` with **no
//! `api-version` at all** and votes as `authenticatedUser.id`. The flags the
//! invocations table lists as `--delete_source`/`--merge_strategy` are rejected by
//! the frozen parser; the runnable spellings are hyphenated (D17). And none of the
//! five prompts: every one was re-run against the mock with `n` on stdin and on
//! EOF (R5), and `abandon` and `complete` change a pull request's state without
//! asking. The oracle's `create` without `--description` (or without
//! `--title`/`--source`/`--target`) exits 0 silently, its `opts.*` access raising a
//! swallowed `KeyError`; this build requires the three and omits an absent
//! description from the body (D34 for the missing required flags, D35 for the
//! absent optional one).

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

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
    let path = pull_request_path(project, repo_id, pr_id);
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

/// One pull request under the collection path.
fn pull_request_path(project: &str, repo_id: &str, pr_id: i64) -> String {
    format!("{}/{pr_id}", collection_path(project, repo_id))
}

/// `create_pr/1`'s options. Grouped so the command's own signature stays under
/// clippy's argument ceiling, the `workitems` `WorkItemOptions` precedent.
#[derive(Debug, Default, PartialEq)]
pub struct CreateOptions {
    pub title: String,
    pub description: Option<String>,
    pub source: String,
    pub target: String,
    pub draft: bool,
}

/// `create_pr/1`: `POST …/pullrequests` with the module's body. Under `--json` the
/// created pull request is the value envelope (D33). The frozen flow crashes on an
/// absent `--description` and exits 0 having sent nothing — the D35 row — so this
/// build omits the key instead.
pub fn create(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    options: CreateOptions,
) -> Result<Report, AdoError> {
    let body = create_body(&options);
    let pull_request = context
        .client()?
        .post(&collection_path(project, repo_id), &body, &[])?;

    Ok(context.json_or_report(ok_value(pull_request.clone()), || {
        created_lines(&pull_request)
    }))
}

/// `build_pr_body/1`: the four keys always present — `isDraft` is the option's
/// boolean, defaulted `false` by the frozen parser — and `description` only when
/// the flag was given (the capture's five-key body when it was).
fn create_body(options: &CreateOptions) -> Value {
    let mut body = json!({
        "title": options.title,
        "sourceRefName": ensure_ref_prefix(&options.source),
        "targetRefName": ensure_ref_prefix(&options.target),
        "isDraft": options.draft,
    });

    if let Some(description) = &options.description {
        body["description"] = json!(description);
    }

    body
}

/// `ensure_ref_prefix/1`: a `refs/`-prefixed ref is kept verbatim, any other
/// branch name gains `refs/heads/`.
fn ensure_ref_prefix(branch: &str) -> String {
    if branch.starts_with("refs/") {
        branch.to_owned()
    } else {
        format!("refs/heads/{branch}")
    }
}

/// The module's `success`/`writeln` block for a created pull request. The URL is
/// the response's `_links.web.href`; the oracle builds it from its compile-time
/// config, which the capture shows printing the literal `{org}` when no
/// organization was set at build time (§8 surface).
fn created_lines(pull_request: &Value) -> Report {
    Report::Text(format!(
        "Pull request #{} created: {}\n  Status:    {}\n  Source:    {}\n  Target:    {}\n  Created:   {}\n  URL:       {}",
        id_cell(pull_request),
        field(pull_request, "title"),
        field(pull_request, "status"),
        field(pull_request, "sourceRefName"),
        field(pull_request, "targetRefName"),
        field(pull_request, "creationDate"),
        web_url(pull_request),
    ))
}

/// The response's `_links.web.href`, empty when the payload has no link.
fn web_url(pull_request: &Value) -> String {
    pull_request
        .pointer("/_links/web/href")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// `complete_pr/1`: read the PR for `lastMergeSourceCommit.commitId`, then PATCH
/// the completion body. The captured GET 404 is the generic error envelope while
/// the PATCH's is the module's own message, so only the second is remapped.
/// Under `--json` the completed pull request is the value envelope (D33).
pub fn complete(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    delete_source: bool,
    merge_strategy: Option<String>,
) -> Result<Report, AdoError> {
    let path = pull_request_path(project, repo_id, pr_id);
    let pull_request = context.client()?.get(&path, &[])?;
    let Some(commit_id) = pull_request
        .pointer("/lastMergeSourceCommit/commitId")
        .filter(|commit_id| !commit_id.is_null())
        .cloned()
    else {
        return Err(AdoError {
            code: ErrorCode::ApiError,
            status: None,
            message: format!(
                "Cannot complete PR #{pr_id}: no lastMergeSourceCommit.commitId in the PR data."
            ),
            details: None,
        });
    };

    let body = complete_body(commit_id, delete_source, merge_strategy.as_deref());

    match context.client()?.patch(&path, &body, &[]) {
        Ok(completed) => Ok(context.json_or_report(ok_value(completed.clone()), || {
            Report::Text(format!(
                "Pull request #{} completed (merged).",
                id_cell(&completed)
            ))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pull request #{pr_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `build_complete_body/2`: the three keys always present, plus `mergeStrategy`
/// only when the option was given — an explicit empty value sends `""` while an
/// absent one sends no key, which is the capture's distinction.
fn complete_body(commit_id: Value, delete_source: bool, merge_strategy: Option<&str>) -> Value {
    let mut body = json!({
        "status": "completed",
        "lastMergeSourceCommit": {"commitId": commit_id},
        "deleteSourceBranch": delete_source,
    });

    if let Some(strategy) = merge_strategy {
        body["mergeStrategy"] = json!(merge_api_strategy(strategy));
    }

    body
}

/// `merge_strategy/1`: the two short names map onto the API's camelCase values,
/// `noFastForward` and anything else pass through verbatim.
fn merge_api_strategy(strategy: &str) -> &str {
    match strategy {
        "squash" => "squashMerge",
        "rebase" => "rebaseMerge",
        other => other,
    }
}

/// `abandon_pr/1`: `PATCH …/pullrequests/{id}` with `{"status": "abandoned"}` and
/// no read first. Under `--json` the abandoned pull request is the value envelope
/// (D33).
pub fn abandon(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
) -> Result<Report, AdoError> {
    let body = json!({"status": "abandoned"});
    let path = pull_request_path(project, repo_id, pr_id);

    match context.client()?.patch(&path, &body, &[]) {
        Ok(abandoned) => Ok(context.json_or_report(ok_value(abandoned.clone()), || {
            Report::Text(format!("Pull request #{} abandoned.", id_cell(&abandoned)))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pull request #{pr_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `approve_pr/1` is `vote_pr/1` with the vote forced to `+10`.
pub fn approve(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
) -> Result<Report, AdoError> {
    cast_vote(context, project, repo_id, pr_id, 10)
}

/// `vote_pr/1`: resolve the authenticated user's id, then `PUT` the reviewer path
/// with the vote. Under `--json` the returned reviewer is the value envelope
/// (D33).
pub fn vote(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    vote: i64,
) -> Result<Report, AdoError> {
    cast_vote(context, project, repo_id, pr_id, vote)
}

fn cast_vote(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    vote: i64,
) -> Result<Report, AdoError> {
    let reviewer_id = current_user_id(context, pr_id)?;
    let path = format!(
        "{}/reviewers/{}",
        pull_request_path(project, repo_id, pr_id),
        encode_path_segment(&reviewer_id)
    );
    let body = json!({"vote": vote});

    match context.client()?.put(&path, &body, &[]) {
        Ok(reviewer) => Ok(context.json_or_report(ok_value(reviewer.clone()), || {
            Report::Text(format!("Voted {} on PR #{pr_id}.", vote_label(vote)))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pull request #{pr_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `resolve_reviewer_id/3`: `AdoCli.Auth.current_user_id/0`'s
/// `GET /_apis/connectionData` — the captured request carries no `api-version` —
/// read for `authenticatedUser.id`. The module's own wording is kept on both
/// failure shapes: an HTTP or transport failure keeps the client's message under
/// the identity prefix, and a 2xx body without a usable id is
/// `auth_required`.
fn current_user_id(context: &mut Context, pr_id: i64) -> Result<String, AdoError> {
    let fetch = context
        .client()?
        .get_without_version("/_apis/connectionData");

    let identity = fetch.map_err(|error| AdoError {
        code: error.code,
        status: error.status,
        message: format!(
            "Cannot determine authenticated user identity for PR #{pr_id}: {}",
            error.message
        ),
        details: error.details,
    })?;
    let user_id = identity
        .pointer("/authenticatedUser/id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());

    match user_id {
        Some(id) => Ok(id.to_owned()),
        None => Err(AdoError {
            code: ErrorCode::AuthRequired,
            status: None,
            message: format!(
                "Cannot determine authenticated user identity for PR #{pr_id}: Connection data did not include an authenticated user ID"
            ),
            details: None,
        }),
    }
}

/// `vote_label/1`: the five named values and the bare integer otherwise.
fn vote_label(vote: i64) -> String {
    match vote {
        10 => "+10 (approved)".to_owned(),
        5 => "+5 (approved with suggestions)".to_owned(),
        0 => "0 (reset)".to_owned(),
        -5 => "-5 (waiting for author)".to_owned(),
        -10 => "-10 (rejected)".to_owned(),
        other => other.to_string(),
    }
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

    /// The captured create body: the four keys always, `description` only when the
    /// option was given (the oracle's own body when it has one).
    #[test]
    fn create_body_omits_an_absent_description() {
        let full = CreateOptions {
            title: "Add checkout retries".to_owned(),
            description: Some("Retries soft declines.".to_owned()),
            source: "feature/payments".to_owned(),
            target: "main".to_owned(),
            draft: true,
        };
        assert_eq!(
            create_body(&full),
            json!({
                "title": "Add checkout retries",
                "description": "Retries soft declines.",
                "sourceRefName": "refs/heads/feature/payments",
                "targetRefName": "refs/heads/main",
                "isDraft": true,
            })
        );

        let minimal = CreateOptions {
            description: None,
            draft: false,
            ..full
        };
        let body = create_body(&minimal);
        assert_eq!(
            body,
            json!({
                "title": "Add checkout retries",
                "sourceRefName": "refs/heads/feature/payments",
                "targetRefName": "refs/heads/main",
                "isDraft": false,
            }),
            "the absent description leaves no key (D35)"
        );
    }

    #[test]
    fn ensure_ref_prefix_keeps_refs_and_prefixes_short_names() {
        assert_eq!(
            ensure_ref_prefix("refs/heads/feature/payments"),
            "refs/heads/feature/payments"
        );
        assert_eq!(ensure_ref_prefix("refs/tags/v1"), "refs/tags/v1");
        assert_eq!(
            ensure_ref_prefix("feature/payments"),
            "refs/heads/feature/payments"
        );
        assert_eq!(ensure_ref_prefix(""), "refs/heads/");
    }

    /// The captured `merge_strategy/1` table, including the identity case and the
    /// pass-through an unknown value gets.
    #[test]
    fn merge_api_strategy_maps_and_passes_through() {
        assert_eq!(merge_api_strategy("squash"), "squashMerge");
        assert_eq!(merge_api_strategy("rebase"), "rebaseMerge");
        assert_eq!(merge_api_strategy("noFastForward"), "noFastForward");
        assert_eq!(merge_api_strategy("merge"), "merge");
        assert_eq!(merge_api_strategy(""), "");
    }

    /// `put_if_key/3` skips only `nil`: an explicit empty strategy sends the key
    /// with an empty value while an absent option sends no key at all.
    #[test]
    fn complete_body_carries_the_merge_strategy_only_when_given() {
        let commit_id = json!("1111111111111111111111111111111111111111");

        assert_eq!(
            complete_body(commit_id.clone(), false, None),
            json!({
                "status": "completed",
                "lastMergeSourceCommit": {"commitId": "1111111111111111111111111111111111111111"},
                "deleteSourceBranch": false,
            })
        );
        assert_eq!(
            complete_body(commit_id.clone(), true, Some("squash")),
            json!({
                "status": "completed",
                "lastMergeSourceCommit": {"commitId": "1111111111111111111111111111111111111111"},
                "deleteSourceBranch": true,
                "mergeStrategy": "squashMerge",
            })
        );
        assert_eq!(
            complete_body(commit_id, false, Some("")),
            json!({
                "status": "completed",
                "lastMergeSourceCommit": {"commitId": "1111111111111111111111111111111111111111"},
                "deleteSourceBranch": false,
                "mergeStrategy": "",
            })
        );
    }

    /// The captured `vote_label/1` table: the five named values and the bare
    /// integer otherwise.
    #[test]
    fn vote_label_names_the_five_values_and_passes_others_through() {
        assert_eq!(vote_label(10), "+10 (approved)");
        assert_eq!(vote_label(5), "+5 (approved with suggestions)");
        assert_eq!(vote_label(0), "0 (reset)");
        assert_eq!(vote_label(-5), "-5 (waiting for author)");
        assert_eq!(vote_label(-10), "-10 (rejected)");
        assert_eq!(vote_label(7), "7");
        assert_eq!(vote_label(-7), "-7");
    }

    /// The created line names the response's fields and its own `_links.web.href`;
    /// a payload without the link prints an empty cell rather than failing.
    #[test]
    fn created_lines_use_the_response_fields_and_web_link() {
        let pull_request = json!({
            "pullRequestId": 145,
            "title": "Add checkout retries",
            "status": "active",
            "sourceRefName": "refs/heads/feature/payments",
            "targetRefName": "refs/heads/main",
            "creationDate": "2026-09-27T10:00:00.000Z",
            "_links": {"web": {"href": "https://dev.azure.com/myorg/Alpha/_git/Alpha.Core/pullrequest/145"}},
        });
        let Report::Text(line) = created_lines(&pull_request) else {
            panic!("the created line is a text report");
        };

        assert_eq!(
            line,
            "Pull request #145 created: Add checkout retries\n  Status:    active\n  Source:    refs/heads/feature/payments\n  Target:    refs/heads/main\n  Created:   2026-09-27T10:00:00.000Z\n  URL:       https://dev.azure.com/myorg/Alpha/_git/Alpha.Core/pullrequest/145"
        );

        let Report::Text(bare) = created_lines(&json!({"pullRequestId": 7})) else {
            panic!("the created line is a text report");
        };
        assert!(
            bare.ends_with("  URL:       "),
            "a missing link is an empty cell: {bare}"
        );
    }
}
