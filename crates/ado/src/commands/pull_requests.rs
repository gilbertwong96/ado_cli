//! `ado prs list|show|create|complete|abandon|approve|vote|diff|comments|reviewers …`
//! — the read, write and diff paths of `lib/ado_cli/cli/pull_requests.ex`: the
//! same REST surface, the same filters, the same merge and vote bodies, the same
//! request chain for `diff`, the same comment-thread surface, and the same human
//! layouts.
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
//! `--title`/`--source`/`--target`) exits 0 silently: its `opts.*` read raises a
//! raw `{:badkey, …}` Erlang reason, which the rescue's `ErlangError` wildcard
//! swallows into exit 0; this build requires the three and omits an absent
//! description from the body (D34 for the missing required flags, D35 for the
//! absent optional one).
//!
//! `diff` is a rendering task as much as a request one: the frozen chain is
//! `GET …/pullRequests/{id}/iterations` (skipped when `--iteration N` is given),
//! `GET …/iterations/{n}/changes`, and — only for `--file`/`--unified` — the
//! iteration list again for its two commit ids, `GET …/items` per revision, and a
//! locally computed unified diff (D36's captures). The human bytes are this
//! build's own table and raw diff text (§8); the shapes — a file list, one
//! unified diff on stdout, exactly one JSON document under `--json` — are
//! contract.

use ado_core::client::{RawBody, encode_path_segment};
use ado_core::envelope::{ok_list, ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::fuzzy;
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
    fetch_user_id(context).map_err(|error| AdoError {
        message: format!(
            "Cannot determine authenticated user identity for PR #{pr_id}: {}",
            error.message
        ),
        ..error
    })
}

/// `AdoCli.Auth.current_user_id/0`: `GET /_apis/connectionData` — the captured
/// request carries no `api-version` — read for `authenticatedUser.id`. The body
/// without a usable id is `auth_required`. Callers add their own context's
/// prefix to the message.
fn fetch_user_id(context: &mut Context) -> Result<String, AdoError> {
    let identity = context
        .client()?
        .get_without_version("/_apis/connectionData")?;
    let user_id = identity
        .pointer("/authenticatedUser/id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());

    match user_id {
        Some(id) => Ok(id.to_owned()),
        None => Err(AdoError {
            code: ErrorCode::AuthRequired,
            status: None,
            message: "Connection data did not include an authenticated user ID".to_owned(),
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

// ── `prs comments` (Task 11a) ────────────────────────────────────────────────
//
// Captured against the standalone mock (`captures/task11a/`). `list` and `update`
// spell `pullRequests` with a capital R; `add`, `delete` and `resolve` spell
// `pullrequests` — both kept. The write paths' `--json` documents are the frozen
// ones and are mirrored (D38) instead of rewritten into the `{ok,result}`
// envelope, because the frozen CLI already emits a document there; `resolve`
// passes an unknown status through (captured — the schema's "valid" list is not
// enforced); `--dry-run` prints its actions document even without `--json`. The
// delete prompt is the wave's third: the command owns the question and the
// refusal, the seam owns the stream and the exit status (D30/D31/D32).

/// `list_comments/1`: `GET …/pullRequests/{pr_id}/threads` (capital R), the
/// module's compact listing or its `--all` view, and the value envelope under
/// `--json` (W1-R12, the oracle's captured document is the same shape).
pub fn comments_list(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    all: bool,
) -> Result<Report, AdoError> {
    let path = comments_list_path(project, repo_id, pr_id);
    let threads = items(context.client()?.list(&path, &[])?);

    Ok(
        context.json_or_report(ok_value(Value::Array(threads.clone())), || {
            threads_text(&threads, all)
        }),
    )
}

/// `add_comment/1`'s options, grouped for clippy's argument ceiling.
#[derive(Debug, Default, PartialEq)]
pub struct CommentAddOptions {
    pub content: String,
    pub file_path: Option<String>,
    pub line: Option<i64>,
    pub end_line: Option<i64>,
    pub thread_id: Option<i64>,
    pub comment_id: Option<i64>,
    pub status: Option<String>,
}

/// `add_comment/1`: reply to a thread when `--thread-id` is given, an inline
/// thread when `--file-path` and `--line` are, and a general thread otherwise.
/// The captured bodies, the canonical leading slash and the reply's default
/// `parentCommentId 0` are the module's; a reply reads the first comment of the
/// response for the comment id and reports `null` when there is none.
pub fn comments_add(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    options: CommentAddOptions,
) -> Result<Report, AdoError> {
    let content = resolve_comment_content(&options.content)?;
    let status = validate_comment_status(options.status.as_deref().unwrap_or(""))?;

    if let Some(thread_id) = options.thread_id {
        let path = format!(
            "{}/comments",
            comments_lower_thread_path(project, repo_id, pr_id, thread_id)
        );
        let body = json!({
            "commentType": "text",
            "content": content,
            "parentCommentId": options.comment_id.unwrap_or(0),
        });
        let result = context.client()?.post(&path, &body, &[])?;

        return Ok(render_add_result(
            context,
            result,
            &format!("Reply added to thread {thread_id}."),
        ));
    }

    if let (Some(file_path), Some(line)) = (options.file_path.as_deref(), options.line) {
        let canonical_path = ensure_leading_slash(file_path);
        let end_line = options.end_line.unwrap_or(line);
        let range = if end_line != line {
            format!("{line}-{end_line}")
        } else {
            line.to_string()
        };
        let path = comments_add_path(project, repo_id, pr_id);
        let body = json!({
            "comments": [{"commentType": "text", "content": content, "parentCommentId": 0}],
            "status": status,
            "threadContext": {
                "filePath": canonical_path,
                "rightFileStart": {"line": line, "offset": 1},
                "rightFileEnd": {"line": end_line, "offset": 1},
            },
        });
        let result = context.client()?.post(&path, &body, &[])?;

        return Ok(render_add_result(
            context,
            result,
            &format!("Comment added to {canonical_path}:{range}."),
        ));
    }

    let path = comments_add_path(project, repo_id, pr_id);
    let body = json!({
        "comments": [{"commentType": "text", "content": content, "parentCommentId": 0}],
        "status": status,
    });
    let result = context.client()?.post(&path, &body, &[])?;

    Ok(render_add_result(context, result, "Comment added."))
}

/// `update_comment/1`'s options, grouped like [`CommentAddOptions`].
#[derive(Debug, Default, PartialEq)]
pub struct CommentUpdateOptions {
    pub content: Option<String>,
    pub status: Option<String>,
    pub resolved_by_me: bool,
    pub dry_run: bool,
}

/// `update_comment/1`: `--content` PATCHes the comment, `--status` the thread,
/// both PATCH the thread first. `--resolved-by-me` reads the authenticated user
/// before anything else — the dry run included — and `--dry-run` prints the
/// would-be actions document and sends nothing. The frozen render always asks
/// the *response* for an id where it has one and the arguments otherwise; the
/// captured documents are mirrored exactly, including the status-only path's
/// `status: null`.
pub fn comments_update(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    thread_id: i64,
    comment_id: i64,
    options: CommentUpdateOptions,
) -> Result<Report, AdoError> {
    let wants_content = options
        .content
        .as_deref()
        .is_some_and(|raw| !raw.is_empty());
    let wants_status = options.status.as_deref().is_some_and(|raw| !raw.is_empty());

    if !wants_content && !wants_status {
        return Err(AdoError::validation(
            "Must pass --content and/or --status. Pass --content to edit a comment, --status to change a thread's resolution state, or both.",
        ));
    }

    let content = resolve_comment_content(options.content.as_deref().unwrap_or(""))?;
    let status = validate_comment_status(options.status.as_deref().unwrap_or(""))?.to_owned();
    let user_id = if options.resolved_by_me {
        Some(comment_user_id(
            context,
            "Cannot resolve thread as current user",
        )?)
    } else {
        None
    };

    let thread_path = comments_update_thread_path(project, repo_id, pr_id, thread_id);
    let comment_path = comments_update_comment_path(project, repo_id, pr_id, thread_id, comment_id);

    if options.dry_run {
        return Ok(Report::Raw(format!(
            "{}\n",
            dry_run_document(
                wants_content,
                wants_status,
                &thread_path,
                &comment_path,
                &status,
                user_id.as_deref(),
                &content,
            )
        )));
    }

    if wants_content && wants_status {
        let thread = context.client()?.patch(
            &thread_path,
            &comment_thread_body(&status, user_id.as_deref()),
            &[],
        )?;
        let comment = context
            .client()?
            .patch(&comment_path, &json!({"content": content}), &[])?;

        return Ok(update_report(
            context,
            thread.get("id").cloned().unwrap_or(Value::Null),
            comment.get("id").cloned().unwrap_or(Value::Null),
            thread.get("status").cloned().unwrap_or(Value::Null),
            true,
            "Comment and thread status updated.",
        ));
    }

    if wants_content {
        let comment = context
            .client()?
            .patch(&comment_path, &json!({"content": content}), &[])?;

        return Ok(update_report(
            context,
            json!(thread_id),
            comment.get("id").cloned().unwrap_or(Value::Null),
            Value::Null,
            true,
            "Comment updated.",
        ));
    }

    context.client()?.patch(
        &thread_path,
        &comment_thread_body(&status, user_id.as_deref()),
        &[],
    )?;

    Ok(update_report(
        context,
        json!(thread_id),
        json!(comment_id),
        Value::Null,
        false,
        "Thread status updated.",
    ))
}

/// `delete_comment/1`: `--comment-id` DELETEs the comment, otherwise the thread
/// is closed with `PATCH {"status":"closed"}`. The command owns the question and
/// the `Cancelled.` refusal; the seam writes them to stderr and the refusal exits
/// 1 (R2/D31/D32), where the oracle prints `Cancelled.` on stdout and exits 0.
pub fn comments_delete(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    thread_id: i64,
    comment_id: Option<i64>,
    force: bool,
) -> Result<Report, AdoError> {
    let label = match comment_id {
        Some(comment_id) => format!("comment {comment_id} in thread {thread_id}"),
        None => format!("thread {thread_id}"),
    };

    if !force && !context.confirm(&format!("Close {label}? [y/N] ")) {
        return Err(AdoError::cancelled("Cancelled."));
    }

    let thread_path = comments_lower_thread_path(project, repo_id, pr_id, thread_id);

    match comment_id {
        Some(comment_id) => {
            context
                .client()?
                .delete(&format!("{thread_path}/comments/{comment_id}"), &[])?;
        }
        None => {
            context
                .client()?
                .patch(&thread_path, &json!({"status": "closed"}), &[])?;
        }
    }

    Ok(
        context.json_or_report(json!({"ok": true, "closed": label}), || {
            Report::Text(format!("Closed {label}."))
        }),
    )
}

/// `resolve_thread/1`: PATCH the thread path (lower case, captured) with the
/// status, plus `resolvedBy` when `--resolved-by-me` read the identity. The
/// frozen command does not validate `--status` and neither does this one.
pub fn comments_resolve(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    thread_id: i64,
    status: &str,
    resolved_by_me: bool,
) -> Result<Report, AdoError> {
    let user_id = if resolved_by_me {
        Some(comment_user_id(
            context,
            "Cannot determine authenticated user identity",
        )?)
    } else {
        None
    };

    let path = comments_lower_thread_path(project, repo_id, pr_id, thread_id);
    context
        .client()?
        .patch(&path, &comment_thread_body(status, user_id.as_deref()), &[])?;

    let attribution = if resolved_by_me { " (by you)" } else { "" };

    Ok(context.json_or_report(
        json!({"ok": true, "thread": thread_id, "status": status}),
        || {
            Report::Text(format!(
                "Thread {thread_id} resolved as '{status}'{attribution}."
            ))
        },
    ))
}

/// `/…/git/repositories/{repo_id}/pullRequests/{pr_id}/threads` — the list path's
/// captured spelling (capital R, unlike the add/delete/resolve paths).
fn comments_list_path(project: &str, repo_id: &str, pr_id: i64) -> String {
    format!(
        "{}/pullRequests/{pr_id}/threads",
        repository_path(project, repo_id)
    )
}

/// The `update` thread path: the captured capital-R spelling.
fn comments_update_thread_path(project: &str, repo_id: &str, pr_id: i64, thread_id: i64) -> String {
    format!(
        "{}/pullRequests/{pr_id}/threads/{thread_id}",
        repository_path(project, repo_id)
    )
}

/// The `update` comment path: the captured capital-R spelling.
fn comments_update_comment_path(
    project: &str,
    repo_id: &str,
    pr_id: i64,
    thread_id: i64,
    comment_id: i64,
) -> String {
    format!(
        "{}/comments/{comment_id}",
        comments_update_thread_path(project, repo_id, pr_id, thread_id)
    )
}

/// `/…/git/repositories/{repo_id}/pullrequests/{pr_id}/threads` — the add path's
/// captured spelling (lower case, unlike the list/update paths).
fn comments_add_path(project: &str, repo_id: &str, pr_id: i64) -> String {
    format!(
        "{}/pullrequests/{pr_id}/threads",
        repository_path(project, repo_id)
    )
}

/// The `delete`/`resolve` thread path: the captured lower-case spelling.
fn comments_lower_thread_path(project: &str, repo_id: &str, pr_id: i64, thread_id: i64) -> String {
    format!(
        "{}/pullrequests/{pr_id}/threads/{thread_id}",
        repository_path(project, repo_id)
    )
}

/// The captured `--status` validation: an absent value reads `active`, the five
/// names pass, everything else is a validation error naming the list.
const COMMENT_STATUSES: [&str; 5] = ["active", "fixed", "wontFix", "closed", "byDesign"];

fn validate_comment_status(status: &str) -> Result<&str, AdoError> {
    if status.is_empty() {
        return Ok("active");
    }

    if COMMENT_STATUSES.contains(&status) {
        return Ok(status);
    }

    Err(AdoError::validation(format!(
        "Invalid --status '{status}'. Must be one of: active, fixed, wontFix, closed, byDesign."
    )))
}

/// `resolve_content/1`: `-` reads stdin, `@<path>` a file (both stripped of
/// trailing newlines), anything else is the literal text.
fn resolve_comment_content(raw: &str) -> Result<String, AdoError> {
    if raw == "-" {
        let mut content = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut content);

        return Ok(strip_trailing_newlines(&content));
    }

    if let Some(path) = raw.strip_prefix('@') {
        let content = std::fs::read_to_string(path).map_err(|error| {
            AdoError::validation(format!("Cannot read comment file '{path}': {error}"))
        })?;

        return Ok(strip_trailing_newlines(&content));
    }

    Ok(raw.to_owned())
}

/// The module's `~r/\n+\z/`: trailing newlines only, not spaces.
fn strip_trailing_newlines(content: &str) -> String {
    content.trim_end_matches('\n').to_owned()
}

/// `render_add_result/3`: the captured document's keys, the first comment's id
/// (nil when the response has no comments), and the module's two human lines.
fn render_add_result(context: &Context, result: Value, message: &str) -> Report {
    if !result.is_object() {
        return context.json_or_report(ok_value(result), || Report::Text(message.to_owned()));
    }

    let thread_id = result.get("id").cloned().unwrap_or(Value::Null);
    let comment_id = result
        .get("comments")
        .and_then(Value::as_array)
        .and_then(|comments| comments.first())
        .and_then(|comment| comment.get("id"))
        .cloned()
        .unwrap_or(Value::Null);

    context.json_or_report(
        json!({
            "ok": true,
            "thread_id": thread_id,
            "comment_id": comment_id,
            "message": message,
        }),
        || {
            Report::Text(format!(
                "{message}\n  thread_id:  {}\n  comment_id: {}",
                text_of(&thread_id),
                text_of(&comment_id)
            ))
        },
    )
}

/// `render_update_result/5`'s document and human lines: the status lines only
/// when the thread response carried a status, the comment line when the comment
/// PATCH answered.
fn update_report(
    context: &Context,
    thread_id: Value,
    comment_id: Value,
    status: Value,
    show_comment: bool,
    message: &str,
) -> Report {
    let human = {
        let mut text = message.to_owned();

        if !status.is_null() {
            text.push_str(&format!(
                "\n  thread_id: {}\n  status:    {}",
                text_of(&thread_id),
                text_of(&status)
            ));
        }

        if show_comment {
            text.push_str(&format!("\n  comment_id: {}", text_of(&comment_id)));
        }

        text
    };

    context.json_or_report(
        json!({
            "ok": true,
            "thread_id": thread_id,
            "comment_id": comment_id,
            "status": status,
            "message": message,
        }),
        || Report::Text(human),
    )
}

/// The thread PATCH body: `resolvedBy` only when `--resolved-by-me` supplied an
/// id.
fn comment_thread_body(status: &str, user_id: Option<&str>) -> Value {
    match user_id {
        Some(user_id) => json!({"status": status, "resolvedBy": {"id": user_id}}),
        None => json!({"status": status}),
    }
}

/// `print_dry_run/6`'s payload: one `{method,path,body}` per would-be PATCH, the
/// thread first when both flags are present.
fn dry_run_document(
    wants_content: bool,
    wants_status: bool,
    thread_path: &str,
    comment_path: &str,
    status: &str,
    user_id: Option<&str>,
    content: &str,
) -> Value {
    let thread_action = json!({
        "method": "PATCH",
        "path": thread_path,
        "body": comment_thread_body(status, user_id),
    });
    let comment_action = json!({
        "method": "PATCH",
        "path": comment_path,
        "body": {"content": content},
    });

    let actions = match (wants_content, wants_status) {
        (true, true) => vec![thread_action, comment_action],
        (true, false) => vec![comment_action],
        (false, true) => vec![thread_action],
        (false, false) => Vec::new(),
    };

    json!({"ok": true, "dry_run": true, "actions": actions})
}

/// The identity lookup the comments paths share, with their own message prefixes
/// (`Approved by AdoCli.Auth.current_user_id/0`).
fn comment_user_id(context: &mut Context, prefix: &str) -> Result<String, AdoError> {
    fetch_user_id(context).map_err(|error| AdoError {
        message: format!("{prefix}: {}", error.message),
        ..error
    })
}

/// `list_comments/1`'s two views, byte for byte: a leading blank line, one
/// thread block each, a blank line after every block.
fn threads_text(threads: &[Value], all: bool) -> Report {
    let mut text = String::from("\n");

    for thread in threads {
        if all {
            push_full_thread(&mut text, thread);
        } else {
            push_compact_thread(&mut text, thread);
        }
    }

    Report::Text(text)
}

fn push_compact_thread(text: &mut String, thread: &Value) {
    text.push_str(&format!(
        "  Thread {} [{}]\n",
        text_of(thread.get("id").unwrap_or(&Value::Null)),
        thread_status(thread)
    ));

    for comment in comments_of(thread) {
        text.push_str(&format!(
            "    [{}] {}: {}\n",
            text_of(comment.get("id").unwrap_or(&Value::Null)),
            comment_author(comment),
            truncate_chars(&text_of(comment.get("content").unwrap_or(&Value::Null)), 80)
        ));
    }

    text.push('\n');
}

fn push_full_thread(text: &mut String, thread: &Value) {
    let id = text_of(thread.get("id").unwrap_or(&Value::Null));
    let status = thread_status(thread);

    match thread.pointer("/threadContext/filePath") {
        Some(Value::String(path)) => {
            text.push_str(&format!("  Thread {id} [{status}] on {path}\n"))
        }
        _ => text.push_str(&format!("  Thread {id} [{status}]\n")),
    }

    for comment in comments_of(thread) {
        let comment_id = text_of(comment.get("id").unwrap_or(&Value::Null));
        let author = comment_author(comment);
        let parent_id = comment
            .get("parentCommentId")
            .and_then(Value::as_i64)
            .unwrap_or(0);

        if parent_id > 0 {
            text.push_str(&format!(
                "    [{comment_id}] (reply to {parent_id}) {author}:\n"
            ));
        } else {
            text.push_str(&format!("    [{comment_id}] {author}:\n"));
        }

        for line in text_of(comment.get("content").unwrap_or(&Value::Null)).split('\n') {
            if line.is_empty() {
                text.push('\n');
            } else {
                text.push_str(&format!("      {line}\n"));
            }
        }
    }

    text.push('\n');
}

fn comments_of(thread: &Value) -> &[Value] {
    thread
        .get("comments")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn thread_status(thread: &Value) -> String {
    match thread.get("status") {
        None | Some(Value::Null) => "unknown".to_owned(),
        Some(status) => text_of(status),
    }
}

fn comment_author(comment: &Value) -> String {
    match comment.pointer("/author/displayName") {
        None | Some(Value::Null) => "unknown".to_owned(),
        Some(name) => text_of(name),
    }
}

/// The frozen `String.slice(content, 0, 80)`.
fn truncate_chars(content: &str, limit: usize) -> String {
    content.chars().take(limit).collect()
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => String::new(),
    }
}

// ── `prs reviewers` (Task 11b) ───────────────────────────────────────────────
//
// Captured against the standalone mock (`captures/task11b/`). All three leaves
// spell `pullrequests` (lower case, like `vote`); `--reviewer` is both the
// path's last segment and the body's `id`, and `--required` adds
// `isRequired: true` — the frozen body omits the key entirely when it is not
// given. `--search` is the module's client-side fuzzy filter over `displayName`
// and `uniqueName` (substring or subsequence, case-insensitive; an absent or
// empty query is no filter). `list` is the first consumer of `ok_list` (C4): the
// frozen `list_reviewers/1` reaches `json_or_format_list/3`, whose captured
// document is `{"ok":true,"count":N,"items":[…]}`. The two writes' `--json`
// documents are this build's (D33) — the oracle prints its human success line in
// both modes — and a missing `--reviewer` is D34's silent exit 0 in the oracle
// (`Map.fetch!` inside `AdoCli.CLI.run/1`'s swallowed rescue) where this build's
// clap is loud.

/// `list_reviewers/1`: `GET …/pullrequests/{pr_id}/reviewers`, the client-side
/// `--search` filter, and the list envelope. The frozen command reads the raw
/// `value` array itself (`Client.get/2`), so a body without that key dies inside
/// the swallowed rescue; this build's shared `Client::list`/`items` path wraps
/// it instead (D41).
pub fn reviewers_list(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    search: Option<&str>,
) -> Result<Report, AdoError> {
    let path = reviewers_list_path(project, repo_id, pr_id);
    let reviewers = items(context.client()?.list(&path, &[])?);
    let reviewers = fuzzy::match_fields(&reviewers, search, &["displayName", "uniqueName"]);

    Ok(context.json_or_report(ok_list(reviewers.clone()), || reviewers_table(&reviewers)))
}

/// `add_reviewer/1`: `PUT …/reviewers/{reviewer}` with the id body and
/// `isRequired: true` under `--required`. The frozen 404 branch has its own
/// wording; every other failure is the shared `Helpers.bail/2` envelope.
pub fn reviewers_add(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    reviewer: &str,
    required: bool,
) -> Result<Report, AdoError> {
    let path = reviewer_item_path(project, repo_id, pr_id, reviewer);
    let body = if required {
        json!({"id": reviewer, "isRequired": true})
    } else {
        json!({"id": reviewer})
    };

    match context.client()?.put(&path, &body, &[]) {
        Ok(answer) => Ok(context.json_or_report(ok_value(answer), || {
            Report::Text(reviewer_added_line(reviewer, required))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!(
                "Reviewer not found: {reviewer}. Use the user's GUID from Azure DevOps."
            ),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `remove_reviewer/1`: `DELETE …/reviewers/{reviewer}`. The oracle's non-404
/// fallback prints `xx  Remove failed: …` prose on stdout and no envelope; this
/// build returns the client's error, so `--json` stays a document and the human
/// mode keeps the labelled line on stderr (D4).
pub fn reviewers_remove(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    reviewer: &str,
) -> Result<Report, AdoError> {
    let path = reviewer_item_path(project, repo_id, pr_id, reviewer);

    match context.client()?.delete(&path, &[]) {
        Ok(()) => {
            let message = format!("Reviewer {reviewer} removed.");

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Reviewer not found: {reviewer}"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `/…/pullrequests/{pr_id}/reviewers` — the list path, lower case like `vote`.
fn reviewers_list_path(project: &str, repo_id: &str, pr_id: i64) -> String {
    format!("{}/reviewers", pull_request_path(project, repo_id, pr_id))
}

/// The item route both writes address; the reviewer segment is percent-encoded
/// (D22), unlike the frozen `URI.encode/1` which left an email's `@` alone.
fn reviewer_item_path(project: &str, repo_id: &str, pr_id: i64, reviewer: &str) -> String {
    format!(
        "{}/{}",
        reviewers_list_path(project, repo_id, pr_id),
        encode_path_segment(reviewer)
    )
}

/// `success("Reviewer #{reviewer} added #{label}.\n")` — the module's wording
/// (§8) with its `(optional)`/`(required)` label.
fn reviewer_added_line(reviewer: &str, required: bool) -> String {
    let label = if required { "required" } else { "optional" };

    format!("Reviewer {reviewer} added ({label}).")
}

/// The frozen `print_reviewer_row/1`'s columns and fallbacks, in this build's
/// table style (§8): `?` for an absent name, `0` for an absent vote, and `no`
/// for an absent or false `isRequired`. An empty list keeps the module's own
/// sentence rather than a header with no rows.
fn reviewers_table(reviewers: &[Value]) -> Report {
    if reviewers.is_empty() {
        return Report::Text("No reviewers.".to_owned());
    }

    Report::Table {
        headers: ["Display Name", "Email", "Vote", "Required"]
            .map(str::to_owned)
            .to_vec(),
        rows: reviewers
            .iter()
            .map(|reviewer| {
                vec![
                    reviewer_cell(reviewer.get("displayName")),
                    reviewer_cell(reviewer.get("uniqueName")),
                    reviewer_vote(reviewer.get("vote")),
                    reviewer_required(reviewer.get("isRequired")),
                ]
            })
            .collect(),
    }
}

/// `r["displayName"] || "?"` — an absent or null value is the module's `?`.
fn reviewer_cell(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "?".to_owned(),
        Some(value) => text_of(value),
    }
}

/// `to_string(r["vote"] || 0)` — a false, null or absent vote reads `0`; every
/// other value stringifies the way the frozen `to_string/1` does.
fn reviewer_vote(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "0".to_owned(),
        Some(value) => text_of(value),
    }
}

/// `if r["isRequired"], do: "yes", else: "no"` — Elixir's truthiness, so only
/// `false` and `nil` (and an absent key) are `no`.
fn reviewer_required(value: Option<&Value>) -> String {
    let required = !matches!(value, None | Some(Value::Null) | Some(Value::Bool(false)));

    if required { "yes" } else { "no" }.to_owned()
}

/// `diff`'s options, grouped for the same reason as [`CreateOptions`].
#[derive(Debug, Default, PartialEq)]
pub struct DiffOptions {
    pub file: Option<String>,
    pub iteration: Option<i64>,
    pub unified: bool,
}

/// `diff_pr/1`: one pull request's changed files in three modes. The captured
/// request chain is the module's: resolve the iteration (`--iteration N` is used
/// as given; otherwise the **last** entry of `GET …/pullRequests/{id}/iterations`),
/// read `GET …/iterations/{n}/changes`, and only then — for `--file` and
/// `--unified` — read the iteration list again for its two commit ids, fetch the
/// file revisions from `GET …/items`, and render the unified diff locally. The
/// default view fetches no content at all, and `--file` with `--unified` is
/// refused before any request (captured: zero requests).
pub fn diff(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    options: DiffOptions,
) -> Result<Report, AdoError> {
    if options.file.is_some() && options.unified {
        return Err(AdoError::validation(
            "Pass either --file or --unified, not both.",
        ));
    }

    let iteration_id = resolve_iteration(context, project, repo_id, pr_id, options.iteration)?;
    let changes = fetch_changes(
        context,
        &changes_path(project, repo_id, pr_id, iteration_id),
    )?;

    match &options.file {
        Some(file) => file_diff(
            context,
            project,
            repo_id,
            pr_id,
            iteration_id,
            &changes,
            file,
        ),
        None if options.unified => render_unified(
            context,
            project,
            repo_id,
            pr_id,
            iteration_id,
            changes.len(),
        ),
        None => Ok(
            context.json_or_report(file_list_envelope(iteration_id, &changes), || {
                file_list_table(&changes)
            }),
        ),
    }
}

/// `resolve_iteration/2` with a number given is that number, with no request;
/// otherwise the iteration list's last entry names it.
fn resolve_iteration(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    iteration: Option<i64>,
) -> Result<i64, AdoError> {
    if let Some(iteration) = iteration {
        return Ok(iteration);
    }

    let iterations = fetch_iterations(context, &iterations_path(project, repo_id, pr_id))?;
    let Some(latest) = iterations.last() else {
        return Err(AdoError {
            code: ErrorCode::ApiError,
            status: None,
            message: format!("PR #{pr_id} has no iterations (nothing to diff)."),
            details: None,
        });
    };

    latest
        .get("id")
        .or_else(|| latest.get("number"))
        .and_then(Value::as_i64)
        .ok_or_else(|| AdoError {
            code: ErrorCode::ApiError,
            status: None,
            message: "Could not determine latest iteration ID".to_owned(),
            details: None,
        })
}

/// `fetch_changes/2`: the module reads `changeEntries` first and `value` second;
/// a body carrying neither reads as no changes here where the frozen case clause
/// would crash (a swallowed exit 0).
fn fetch_changes(context: &mut Context, path: &str) -> Result<Vec<Value>, AdoError> {
    let response = context.client()?.get(path, &[])?;

    Ok(response
        .get("changeEntries")
        .or_else(|| response.get("value"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn fetch_iterations(context: &mut Context, path: &str) -> Result<Vec<Value>, AdoError> {
    let response = context.client()?.get(path, &[])?;

    Ok(response
        .get("value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// `fetch_iteration_data/2`: a second read of the iteration list (the capture
/// shows the repeated request), looked up by `id`.
fn fetch_iteration_data(
    context: &mut Context,
    path: &str,
    iteration_id: i64,
) -> Result<Value, AdoError> {
    fetch_iterations(context, path)?
        .into_iter()
        .find(|iteration| iteration.get("id").and_then(Value::as_i64) == Some(iteration_id))
        .ok_or_else(|| AdoError::not_found(format!("Iteration {iteration_id} not found")))
}

/// `render_file_diff/5`: match the path, read the iteration for its commits, fetch
/// both revisions, and emit the locally rendered unified diff.
fn file_diff(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    iteration_id: i64,
    changes: &[Value],
    file: &str,
) -> Result<Report, AdoError> {
    let change = find_change(changes, file).ok_or_else(|| {
        AdoError::validation(format!(
            "No change matches --file '{file}'. Use 'ado prs diff' (no flags) to list files."
        ))
    })?;
    let iteration = fetch_iteration_data(
        context,
        &iterations_path(project, repo_id, pr_id),
        iteration_id,
    )?;
    let (base, target) = commit_pair(&iteration)?;
    let path = change_path(change);
    let change_type = change_type(change);
    let old = fetch_revision(
        context,
        project,
        repo_id,
        &path,
        &base,
        &change_type,
        Side::Base,
    )?;
    let new = fetch_revision(
        context,
        project,
        repo_id,
        &path,
        &target,
        &change_type,
        Side::Target,
    )?;
    let content = unified_diff_text(&path, &old, &new);

    Ok(context.json_or_report(
        json!({
            "ok": true,
            "iteration": iteration_id,
            "path": path,
            "change_type": change_type,
            "diff": content,
        }),
        || Report::Raw(format!("{content}\n")),
    ))
}

/// `find_change_for_file/2`: an exact match on the change's path with the leading
/// slash stripped from both sides, so `--file` takes either form. `originalPath`
/// is deliberately not consulted (captured: a renamed file's old path matches
/// nothing).
fn find_change<'a>(changes: &'a [Value], file: &str) -> Option<&'a Value> {
    let target = file.trim_start_matches('/');

    changes
        .iter()
        .find(|change| change_path(change).trim_start_matches('/') == target)
}

/// `change_path/1`: the change's `item.path`, then `originalPath`, then `path`,
/// then `?`.
fn change_path(change: &Value) -> String {
    change
        .pointer("/item/path")
        .and_then(Value::as_str)
        .or_else(|| change.get("originalPath").and_then(Value::as_str))
        .or_else(|| change.get("path").and_then(Value::as_str))
        .unwrap_or("?")
        .to_owned()
}

/// `change_type/1`: the five names pass through when spelled, an integer maps
/// through the frozen table, and anything else is `change`.
fn change_type(change: &Value) -> String {
    match change.get("changeType") {
        Some(Value::String(name))
            if matches!(
                name.as_str(),
                "add" | "edit" | "delete" | "rename" | "directory"
            ) =>
        {
            name.clone()
        }
        Some(Value::Number(number)) => int_change_type(number.as_i64()).to_owned(),
        _ => "change".to_owned(),
    }
}

fn int_change_type(number: Option<i64>) -> &'static str {
    match number {
        Some(1) => "add",
        Some(2) => "edit",
        Some(4) => "delete",
        Some(8) => "rename",
        Some(16) => "directory",
        _ => "change",
    }
}

/// `get_in(iteration, ["targetRefCommit", "commitId"])` and its source twin; both
/// have to be present for either content mode (the captured guard).
fn commit_pair(iteration: &Value) -> Result<(String, String), AdoError> {
    let commit = |pointer: &str| {
        iteration
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };

    match (
        commit("/targetRefCommit/commitId"),
        commit("/sourceRefCommit/commitId"),
    ) {
        (Some(base), Some(target)) => Ok((base, target)),
        _ => Err(AdoError {
            code: ErrorCode::ApiError,
            status: None,
            message: "Iteration is missing sourceRefCommit or targetRefCommit".to_owned(),
            details: None,
        }),
    }
}

/// Which revision of a file a content fetch reads: the iteration's target commit
/// ("base", the old side) or its source commit ("target", the new side).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Side {
    Base,
    Target,
}

/// `fetch_or_empty/6`: a 404 is an empty revision only when the file is known to
/// be absent on that side — a new file's base, a deleted file's target; anything
/// else is `File not found in commit {commit}`.
fn fetch_revision(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    path: &str,
    commit: &str,
    change_type: &str,
    side: Side,
) -> Result<String, AdoError> {
    match fetch_item(context, project, repo_id, path, commit) {
        Ok(content) => Ok(content),
        Err(error) if error.status == Some(404) => {
            let absent_side = matches!(
                (change_type, side),
                ("add", Side::Base) | ("delete", Side::Target)
            );

            if absent_side {
                Ok(String::new())
            } else {
                Err(AdoError::not_found(format!(
                    "File not found in commit {commit}"
                )))
            }
        }
        Err(error) => Err(error),
    }
}

/// `GET …/items?path=…&versionType=commit&version=…`, read as text. The frozen
/// `get_raw/2` would also send `api-version` (D25); this build's `url_for` merges
/// it the same way, and the body is a file revision, not JSON.
fn fetch_item(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    path: &str,
    commit: &str,
) -> Result<String, AdoError> {
    let url = context.client()?.url_for(
        &items_path(project, repo_id),
        &[
            ("path".to_owned(), path.to_owned()),
            ("versionType".to_owned(), "commit".to_owned()),
            ("version".to_owned(), commit.to_owned()),
        ],
    );
    let mut body = context.client()?.get_raw(&url)?;

    read_raw_text(&mut body)
}

/// The body of a content fetch, streamed like every other raw read. A non-UTF-8
/// revision is lossily substituted where the frozen CLI emits its bytes: a diff is
/// text and this build's report layer carries `String` (§8).
fn read_raw_text(body: &mut RawBody) -> Result<String, AdoError> {
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = Vec::new();

    loop {
        let read = body.read_chunk(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// `render_unified/5`: the whole-repo diff between the iteration's two commits.
/// `file_count` is the *change list's* length, not the `/diffs/commits` count
/// (captured: the two differ), and the content is the per-file diffs joined with
/// a blank line, where a file whose revisions cannot be read is dropped rather
/// than failing the stream (the frozen `collect_diffs/5`).
fn render_unified(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    pr_id: i64,
    iteration_id: i64,
    file_count: usize,
) -> Result<Report, AdoError> {
    let iteration = fetch_iteration_data(
        context,
        &iterations_path(project, repo_id, pr_id),
        iteration_id,
    )?;
    let (base, target) = commit_pair(&iteration)?;
    let response = context.client()?.get(
        &diffs_path(project, repo_id),
        &[
            ("baseVersionType".to_owned(), "commit".to_owned()),
            ("baseVersion".to_owned(), base.clone()),
            ("targetVersionType".to_owned(), "commit".to_owned()),
            ("targetVersion".to_owned(), target.clone()),
        ],
    )?;
    let changes = response
        .get("changes")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| AdoError {
            code: ErrorCode::ApiError,
            status: None,
            message: "No changes found".to_owned(),
            details: None,
        })?;

    let mut blocks = Vec::new();

    for change in &changes {
        if let Some(block) =
            unified_change_block(context, project, repo_id, change, &base, &target)?
        {
            blocks.push(block);
        }
    }

    let content = blocks.join("\n");

    Ok(context.json_or_report(
        json!({
            "ok": true,
            "iteration": iteration_id,
            "mode": "unified",
            "file_count": file_count,
            "diff": content,
        }),
        || Report::Raw(format!("{content}\n\n")),
    ))
}

fn unified_change_block(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    change: &Value,
    base: &str,
    target: &str,
) -> Result<Option<String>, AdoError> {
    let Some(path) = change.pointer("/item/path").and_then(Value::as_str) else {
        return Ok(None);
    };
    let change_type = change.get("changeType");

    let Some(old) = raw_revision(
        context,
        project,
        repo_id,
        path,
        base,
        change_type,
        Side::Base,
    )?
    else {
        return Ok(None);
    };
    let Some(new) = raw_revision(
        context,
        project,
        repo_id,
        path,
        target,
        change_type,
        Side::Target,
    )?
    else {
        return Ok(None);
    };

    Ok(Some(unified_diff_text(path, &old, &new)))
}

/// `fetch_or_empty_raw/6`: the raw `changeType` (1/"add", 4/"delete") short-
/// circuits the side that cannot have content, and every other failure — including
/// a 404 on a file that should exist — drops the file from the stream, which is
/// what the frozen `collect_diffs`' `else -> nil` does.
fn raw_revision(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    path: &str,
    commit: &str,
    change_type: Option<&Value>,
    side: Side,
) -> Result<Option<String>, AdoError> {
    let absent = (side == Side::Base && change_type_is(change_type, 1, "add"))
        || (side == Side::Target && change_type_is(change_type, 4, "delete"));

    if absent {
        return Ok(Some(String::new()));
    }

    match fetch_item(context, project, repo_id, path, commit) {
        Ok(content) => Ok(Some(content)),
        Err(_) => Ok(None),
    }
}

fn change_type_is(change_type: Option<&Value>, number: i64, name: &str) -> bool {
    match change_type {
        Some(Value::Number(value)) => value.as_i64() == Some(number),
        Some(Value::String(value)) => value == name,
        _ => false,
    }
}

/// `format_unified_diff/5`: the git header for `path`, then one hunk covering the
/// whole file — `@@ -0,0 +1,n @@` for a new file, `@@ -1,n +0,0 @@` for a deleted
/// one, and no hunk at all when the two revisions are textually equal (all
/// captured). Content splits on `\n` with the trailing empty element the frozen
/// `String.split/2` keeps, so a file ending in a newline renders a final ` \n`
/// context line.
fn unified_diff_text(path: &str, old: &str, new: &str) -> String {
    let path = ensure_leading_slash(path);
    let mut text = format!("diff --git a{path} b{path}\n--- a{path}\n+++ b{path}\n");
    let old_lines = split_lines(old);
    let new_lines = split_lines(new);

    if old_lines.is_empty() && new_lines.is_empty() {
        return text;
    }

    if old_lines.is_empty() {
        text.push_str(&format!("@@ -0,0 +1,{} @@\n", new_lines.len()));
        push_diff_lines(&mut text, &new_lines, '+');
        return text;
    }

    if new_lines.is_empty() {
        text.push_str(&format!("@@ -1,{} +0,0 @@\n", old_lines.len()));
        push_diff_lines(&mut text, &old_lines, '-');
        return text;
    }

    let edits = line_diff(&old_lines, &new_lines);

    if edits.iter().all(|edit| matches!(edit, Edit::Equal(_))) {
        return text;
    }

    text.push_str(&format!(
        "@@ -1,{} +1,{} @@\n",
        old_lines.len(),
        new_lines.len()
    ));

    for edit in edits {
        match edit {
            Edit::Equal(line) => push_diff_line(&mut text, line, ' '),
            Edit::Delete(line) => push_diff_line(&mut text, line, '-'),
            Edit::Insert(line) => push_diff_line(&mut text, line, '+'),
        }
    }

    text
}

fn split_lines(content: &str) -> Vec<&str> {
    if content.is_empty() {
        Vec::new()
    } else {
        content.split('\n').collect()
    }
}

fn push_diff_lines(text: &mut String, lines: &[&str], prefix: char) {
    for line in lines {
        push_diff_line(text, line, prefix);
    }
}

fn push_diff_line(text: &mut String, line: &str, prefix: char) {
    text.push(prefix);
    text.push_str(line);
    text.push('\n');
}

fn ensure_leading_slash(path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

/// One line of the diff between the two revisions. The two sides' common prefix
/// and suffix are kept as context and the middle is diffed with a longest-common-
/// subsequence walk whose ties prefer a delete first, which is the order the
/// listener capture shows (`-` before `+` for a replaced line).
#[derive(Debug, PartialEq)]
enum Edit<'a> {
    Equal(&'a str),
    Delete(&'a str),
    Insert(&'a str),
}

/// Beyond this many cells the middle is rendered as every line deleted then every
/// line inserted: still a valid unified diff, and the guard keeps a pathological
/// pair of revisions from allocating an unbounded table. A file whose changes are
/// small keeps its exact diff — the prefix/suffix trim already leaves only the
/// changed region.
const MAX_DIFF_CELLS: usize = 1_000_000;

fn line_diff<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Edit<'a>> {
    let mut prefix = 0;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < old.len() - prefix
        && suffix < new.len() - prefix
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let middle_old = &old[prefix..old.len() - suffix];
    let middle_new = &new[prefix..new.len() - suffix];
    let mut edits = Vec::with_capacity(old.len() + new.len());

    edits.extend(old[..prefix].iter().map(|line| Edit::Equal(line)));

    if middle_old.len().saturating_mul(middle_new.len()) > MAX_DIFF_CELLS {
        edits.extend(middle_old.iter().map(|line| Edit::Delete(line)));
        edits.extend(middle_new.iter().map(|line| Edit::Insert(line)));
    } else {
        edits.extend(lcs_edits(middle_old, middle_new));
    }

    edits.extend(
        old[old.len() - suffix..]
            .iter()
            .map(|line| Edit::Equal(line)),
    );

    edits
}

fn lcs_edits<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Edit<'a>> {
    let rows = old.len() + 1;
    let columns = new.len() + 1;
    let mut lcs = vec![0u32; rows * columns];

    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            lcs[i * columns + j] = if old[i] == new[j] {
                lcs[(i + 1) * columns + j + 1] + 1
            } else {
                lcs[(i + 1) * columns + j].max(lcs[i * columns + j + 1])
            };
        }
    }

    let mut edits = Vec::new();
    let (mut i, mut j) = (0, 0);

    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            edits.push(Edit::Equal(old[i]));
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * columns + j] >= lcs[i * columns + j + 1] {
            edits.push(Edit::Delete(old[i]));
            i += 1;
        } else {
            edits.push(Edit::Insert(new[j]));
            j += 1;
        }
    }

    edits.extend(old[i..].iter().map(|line| Edit::Delete(line)));
    edits.extend(new[j..].iter().map(|line| Edit::Insert(line)));

    edits
}

/// The default view's `--json` envelope, which is the frozen `render_file_list`'s
/// own shape — `ok`, the iteration, the counts and the per-file objects — not this
/// build's `{ok, result}` (the `version`/`schema` `ok_named` precedent: a frozen
/// envelope this build mirrors rather than rewrites).
fn file_list_envelope(iteration_id: i64, changes: &[Value]) -> Value {
    json!({
        "ok": true,
        "iteration": iteration_id,
        "count": changes.len(),
        "total_additions": total_items(changes, "additions"),
        "total_deletions": total_items(changes, "deletions"),
        "changes": changes
            .iter()
            .map(|change| json!({
                "path": change_path(change),
                "change_type": change_type(change),
                "change_id": or_default(change.get("changeId").or_else(|| change.get("id")), Value::Null),
                "additions": item_count(change, "additions"),
                "deletions": item_count(change, "deletions"),
            }))
            .collect::<Vec<_>>(),
    })
}

/// The default view's human form: this build's one table style (spec §8; D37's
/// row covers the frozen fixed-width layout this replaces).
fn file_list_table(changes: &[Value]) -> Report {
    if changes.is_empty() {
        return Report::Text("No changes found.".to_owned());
    }

    Report::Table {
        headers: vec![
            "PATH".to_owned(),
            "TYPE".to_owned(),
            "ADDITIONS".to_owned(),
            "DELETIONS".to_owned(),
        ],
        rows: changes
            .iter()
            .map(|change| {
                vec![
                    change_path(change),
                    change_type(change),
                    cell(&item_count(change, "additions")),
                    cell(&item_count(change, "deletions")),
                ]
            })
            .collect(),
    }
}

fn item_count(change: &Value, key: &str) -> Value {
    or_default(change.pointer(&format!("/item/{key}")), json!(0))
}

fn total_items(changes: &[Value], key: &str) -> i64 {
    changes
        .iter()
        .map(|change| item_count(change, key).as_i64().unwrap_or(0))
        .sum()
}

/// Elixir's `||` over a JSON value: `null` and `false` fall through to the
/// default, every other value stands.
fn or_default(value: Option<&Value>, default: Value) -> Value {
    match value {
        Some(value) if !value.is_null() && *value != Value::Bool(false) => value.clone(),
        _ => default,
    }
}

fn cell(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn iterations_path(project: &str, repo_id: &str, pr_id: i64) -> String {
    format!(
        "{}/pullRequests/{pr_id}/iterations",
        repository_path(project, repo_id)
    )
}

fn changes_path(project: &str, repo_id: &str, pr_id: i64, iteration_id: i64) -> String {
    format!(
        "{}/{iteration_id}/changes",
        iterations_path(project, repo_id, pr_id)
    )
}

fn items_path(project: &str, repo_id: &str) -> String {
    format!("{}/items", repository_path(project, repo_id))
}

fn diffs_path(project: &str, repo_id: &str) -> String {
    format!("{}/diffs/commits", repository_path(project, repo_id))
}

/// `/…/git/repositories/{repo_id}` — the collection both the diff paths and the
/// items/diffs endpoints hang off. The module spells `pullRequests` with a capital
/// R on its diff paths (captured), unlike the `pullrequests` of the list paths.
fn repository_path(project: &str, repo_id: &str) -> String {
    format!(
        "/{}/_apis/git/repositories/{}",
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

    /// The listener capture's exact bytes: a replaced line renders `-` then `+`,
    /// the trailing empty line a file ending in `\n` splits into renders as a
    /// context line, and the hunk covers the whole file from line 1.
    #[test]
    fn unified_diff_text_matches_the_captured_edit_bytes() {
        assert_eq!(
            unified_diff_text(
                "/src/app.ex",
                "line one\nline two\nline three\n",
                "line one\nline two changed\nline three\nline four\n",
            ),
            "diff --git a/src/app.ex b/src/app.ex\n--- a/src/app.ex\n+++ b/src/app.ex\n@@ -1,4 +1,5 @@\n line one\n-line two\n+line two changed\n line three\n+line four\n \n"
        );
    }

    /// A path without a leading slash gains one, as `format_unified_diff/5`'s
    /// `ensure_leading_slash/1` does.
    #[test]
    fn unified_diff_text_prefixes_the_path_and_keeps_identical_content_hunkless() {
        assert_eq!(
            unified_diff_text("src/app.ex", "same\n", "same\n"),
            "diff --git a/src/app.ex b/src/app.ex\n--- a/src/app.ex\n+++ b/src/app.ex\n"
        );
        assert_eq!(
            unified_diff_text("/empty.txt", "", ""),
            "diff --git a/empty.txt b/empty.txt\n--- a/empty.txt\n+++ b/empty.txt\n"
        );
    }

    /// The captured new-file and deleted-file hunk headers: `@@ -0,0 +1,n @@` and
    /// `@@ -1,n +0,0 @@`, each with its whole body prefixed.
    #[test]
    fn unified_diff_text_renders_new_and_deleted_files() {
        let new_file = unified_diff_text("/docs/readme.md", "", "line one\nline two\n");

        assert_eq!(
            new_file,
            "diff --git a/docs/readme.md b/docs/readme.md\n--- a/docs/readme.md\n+++ b/docs/readme.md\n@@ -0,0 +1,3 @@\n+line one\n+line two\n+\n"
        );

        let deleted = unified_diff_text("/old/file.txt", "line one\nline two\n", "");

        assert_eq!(
            deleted,
            "diff --git a/old/file.txt b/old/file.txt\n--- a/old/file.txt\n+++ b/old/file.txt\n@@ -1,3 +0,0 @@\n-line one\n-line two\n-\n"
        );
    }

    /// An insertion inside a file keeps its surrounding lines as context, and a
    /// pure deletion inserts nothing.
    #[test]
    fn line_diff_keeps_context_and_prefers_deletes_on_a_tie() {
        assert_eq!(
            line_diff(&["a", "c"], &["a", "b", "c"]),
            vec![Edit::Equal("a"), Edit::Insert("b"), Edit::Equal("c")]
        );
        assert_eq!(
            line_diff(&["a", "b", "c"], &["a", "c"]),
            vec![Edit::Equal("a"), Edit::Delete("b"), Edit::Equal("c")]
        );
        assert_eq!(
            line_diff(&["a", "b"], &["a", "x"]),
            vec![Edit::Equal("a"), Edit::Delete("b"), Edit::Insert("x")],
            "a replacement deletes before it inserts (the captured order)"
        );
    }

    /// `change_type/1`'s frozen table on both spellings and the unknown fallback.
    #[test]
    fn change_type_maps_names_and_integers_and_falls_back() {
        for (raw, expected) in [
            (json!("add"), "add"),
            (json!("rename"), "rename"),
            (json!(1), "add"),
            (json!(2), "edit"),
            (json!(4), "delete"),
            (json!(8), "rename"),
            (json!(16), "directory"),
            (json!(32), "change"),
            (json!("oddity"), "change"),
            (json!(true), "change"),
            (Value::Null, "change"),
        ] {
            assert_eq!(
                change_type(&json!({"changeType": raw})),
                expected,
                "changeType {raw}"
            );
        }
        assert_eq!(change_type(&json!({})), "change");
    }

    /// `change_path/1`'s fallback chain, and the `find_change_for_file/2` match
    /// that strips one leading slash from either side.
    #[test]
    fn change_path_and_find_change_use_the_captured_fallbacks() {
        assert_eq!(
            change_path(&json!({"item": {"path": "/src/app.ex"}})),
            "/src/app.ex"
        );
        assert_eq!(
            change_path(&json!({"originalPath": "/renamed/old.ex"})),
            "/renamed/old.ex"
        );
        assert_eq!(change_path(&json!({"path": "/bare.ex"})), "/bare.ex");
        assert_eq!(change_path(&json!({})), "?");

        let changes = vec![
            json!({"item": {"path": "/src/app.ex"}}),
            json!({"item": {"path": "/renamed/new.ex"}, "originalPath": "/renamed/old.ex"}),
        ];

        assert!(find_change(&changes, "/src/app.ex").is_some());
        assert!(find_change(&changes, "src/app.ex").is_some());
        assert!(
            find_change(&changes, "/renamed/old.ex").is_none(),
            "a rename's old path is not matched (captured)"
        );
        assert!(find_change(&changes, "/src/nope.ex").is_none());
    }

    /// The default view's envelope is the frozen `render_file_list/3` shape,
    /// including the `change_id` fallback and the `|| 0` counts.
    #[test]
    fn file_list_envelope_carries_the_frozen_keys_and_totals() {
        let changes = vec![
            json!({"changeTrackingId": 1, "changeId": 1, "changeType": 2, "item": {"path": "/src/app.ex", "additions": 3, "deletions": 1}}),
            json!({"changeTrackingId": 2, "id": 22, "changeType": "add", "item": {"path": "/docs/readme.md"}}),
        ];

        assert_eq!(
            file_list_envelope(2, &changes),
            json!({
                "ok": true,
                "iteration": 2,
                "count": 2,
                "total_additions": 3,
                "total_deletions": 1,
                "changes": [
                    {"path": "/src/app.ex", "change_type": "edit", "change_id": 1, "additions": 3, "deletions": 1},
                    {"path": "/docs/readme.md", "change_type": "add", "change_id": 22, "additions": 0, "deletions": 0},
                ],
            })
        );
    }

    /// The empty change list's human form is this build's message; the table's
    /// columns are the frozen ones.
    #[test]
    fn file_list_table_names_the_captured_columns() {
        assert_eq!(
            file_list_table(&[]),
            Report::Text("No changes found.".to_owned())
        );

        let Report::Table { headers, rows } = file_list_table(&[json!({
            "changeType": 4,
            "item": {"path": "/old/file.txt", "additions": 0, "deletions": 9},
        })]) else {
            panic!("a non-empty change list is a table");
        };

        assert_eq!(headers, ["PATH", "TYPE", "ADDITIONS", "DELETIONS"]);
        assert_eq!(rows, vec![vec!["/old/file.txt", "delete", "0", "9"]]);
    }

    /// The diff paths spell `pullRequests` with a capital R and encode both
    /// segments, the captured spelling for this command.
    #[test]
    fn diff_paths_use_the_captured_spelling() {
        assert_eq!(
            iterations_path("Alpha", "Alpha.Core", 137),
            "/Alpha/_apis/git/repositories/Alpha.Core/pullRequests/137/iterations"
        );
        assert_eq!(
            changes_path("Alpha", "Alpha.Core", 137, 2),
            "/Alpha/_apis/git/repositories/Alpha.Core/pullRequests/137/iterations/2/changes"
        );
        assert_eq!(
            items_path("Alpha", "Alpha.Core"),
            "/Alpha/_apis/git/repositories/Alpha.Core/items"
        );
        assert_eq!(
            diffs_path("Alpha", "Alpha.Core"),
            "/Alpha/_apis/git/repositories/Alpha.Core/diffs/commits"
        );
        assert_eq!(
            iterations_path("Alpha Beta", "Core/One", 7),
            "/Alpha%20Beta/_apis/git/repositories/Core%2FOne/pullRequests/7/iterations"
        );
    }
}
