//! `ado users list|show|add|remove` — the entitlement half of
//! `lib/ado_cli/cli/users.ex`: the organization-scoped `_apis/userentitlements`
//! surface, the table and detail views, and the module's own 404 wording.
//!
//! `remove` asks the confirmation the module's own doc promises, with `--force`
//! as its bypass (Ruling A1): the frozen CLI declares no `--force` and never
//! prompts — captured, its DELETE goes out on `n` and on EOF — so the question
//! and the flag are this build's repair, following the Wave 2 prompt family
//! (D30/D31/D32: the question on stderr, EOF and `n` a refusal, `--force` no
//! question). The removal is the only irreversible org-wide mutation in the CLI;
//! that is why it gets a gate where its sibling `users add` does not.
//!
//! One claim in the module's own docs is prose, not an invocation, and is not
//! ported: its header advertises an `[--search SEARCH]` on `list` that the
//! module's option table never declares (the frozen parser rejects the flag).

use ado_core::client::encode_path_segment;
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The module's default when `--license` is absent.
const DEFAULT_LICENSE: &str = "express";

/// `ado users list`: `GET /_apis/userentitlements`, with the module's `$top`
/// when `--top` is given. No project argument: the path is organization-scoped.
/// The value array unwraps to the value envelope; the human path is the module's
/// three-column table.
pub fn list(context: &mut Context, top: Option<i64>) -> Result<Report, AdoError> {
    let params = top_params(top);
    let users = items(context.client()?.list(USERENTITLEMENTS, &params)?);

    Ok(
        context.json_or_report(ok_value(Value::Array(users.clone())), || {
            users_table(&users)
        }),
    )
}

/// `ado users show`: `GET …/userentitlements/{user_id}`; a 404 takes the module's
/// own wording.
pub fn show(context: &mut Context, user_id: &str) -> Result<Report, AdoError> {
    let path = user_path(user_id);

    match context.client()?.get(&path, &[]) {
        Ok(user) => {
            Ok(context.json_or_report(ok_value(user.clone()), || Report::Text(user_detail(&user))))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("User '{user_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado users add`: `POST /_apis/userentitlements` with the module's two-key
/// body, `--license` defaulting to `express`. The human line reads the response's
/// `user.principalName`.
pub fn add(
    context: &mut Context,
    email: &str,
    license: Option<String>,
) -> Result<Report, AdoError> {
    let license = license.as_deref().unwrap_or(DEFAULT_LICENSE);
    let user = context
        .client()?
        .post(USERENTITLEMENTS, &add_body(email, license), &[])?;

    Ok(context.json_or_report(ok_value(user.clone()), || {
        Report::Text(format!(
            "User '{}' added.",
            value_text(user.get("user").and_then(|user| user.get("principalName")))
        ))
    }))
}

/// `ado users remove`: the confirmation (unless `--force`) and then the module's
/// `DELETE`. A 404 takes the module's own wording.
pub fn remove(context: &mut Context, user_id: &str, force: bool) -> Result<Report, AdoError> {
    if !force && !context.confirm(&remove_question(user_id)) {
        return Err(AdoError::cancelled(ABORTED));
    }

    let path = user_path(user_id);

    match context.client()?.delete(&path, &[]) {
        Ok(()) => {
            let message = format!("User '{user_id}' removed.");

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("User '{user_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `Helpers.confirm_delete/2`'s refusal wording; it is this build's §8 wording,
/// printed on stderr.
const ABORTED: &str = "Aborted.";

/// The collection path; the client injects the organization ahead of it.
const USERENTITLEMENTS: &str = "/_apis/userentitlements";

/// `remove`'s question, from the docstring's own sentence ("Remove a user from
/// the organization entirely… The user is immediately blocked") in the prompt
/// family's `…? This cannot be undone. [y/N] ` shape. The frozen CLI asks
/// nothing; the question is this build's §8 wording.
fn remove_question(user_id: &str) -> String {
    format!("Remove user '{user_id}' from the organization entirely? This cannot be undone. [y/N] ")
}

/// One entitlement below the collection; the id is a single segment, so an email
/// address's `@` is escaped rather than left to the URL's own parsing (D22).
fn user_path(user_id: &str) -> String {
    format!("{USERENTITLEMENTS}/{}", encode_path_segment(user_id))
}

/// The module's `params = if top = Map.get(parsed.options, :top), do: …, else: %{}`.
fn top_params(top: Option<i64>) -> Vec<(String, String)> {
    top.map(|top| vec![("$top".to_owned(), top.to_string())])
        .unwrap_or_default()
}

/// The module's `%{"accessLevel" => %{"accountLicenseType" => license}, "user" => …}`
/// body: `license` is the option or the module's `express` default.
fn add_body(email: &str, license: &str) -> Value {
    json!({
        "accessLevel": {"accountLicenseType": license},
        "user": {"principalName": email, "subjectKind": "user"},
    })
}

/// The module's `print_users_table/1` columns (ID, Email, License), with the
/// module's "No users found." when empty.
fn users_table(users: &[Value]) -> Report {
    if users.is_empty() {
        return Report::Text("No users found.".to_owned());
    }

    let rows = users
        .iter()
        .map(|user| {
            vec![
                value_text(user.get("id")),
                value_text(user.get("user").and_then(|user| user.get("principalName"))),
                value_text(
                    user.get("accessLevel")
                        .and_then(|access| access.get("accountLicenseType")),
                ),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Email".to_owned(), "License".to_owned()],
        rows,
    }
}

/// The module's `print_user_detail/1`, with its `─` rule and its five labelled
/// fields.
fn user_detail(user: &Value) -> String {
    let profile = user.get("user");
    let access = user.get("accessLevel");
    let mut detail = String::from("\nUser Details\n\n");

    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:      {}\n", value_text(user.get("id"))));
    detail.push_str(&format!(
        "  Email:   {}\n",
        value_text(profile.and_then(|profile| profile.get("principalName")))
    ));
    detail.push_str(&format!(
        "  Name:    {}\n",
        value_text(profile.and_then(|profile| profile.get("displayName")))
    ));
    detail.push_str(&format!(
        "  License: {}\n",
        value_text(access.and_then(|access| access.get("accountLicenseType")))
    ));
    detail.push_str(&format!(
        "  Status:  {}\n",
        value_text(access.and_then(|access| access.get("status")))
    ));

    detail.push('\n');

    detail
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

    #[test]
    fn the_remove_question_is_the_docstrings_sentence() {
        assert_eq!(
            remove_question("user-1"),
            "Remove user 'user-1' from the organization entirely? This cannot be undone. [y/N] "
        );
    }

    #[test]
    fn top_params_only_carry_a_given_top() {
        assert_eq!(top_params(None), Vec::new());
        assert_eq!(
            top_params(Some(5)),
            vec![("$top".to_owned(), "5".to_owned())]
        );
    }

    #[test]
    fn the_add_body_is_the_captured_two_key_shape() {
        assert_eq!(
            add_body("ada@example.com", "express"),
            json!({
                "accessLevel": {"accountLicenseType": "express"},
                "user": {"principalName": "ada@example.com", "subjectKind": "user"},
            })
        );
        assert_eq!(
            add_body("ada@example.com", "stakeholder")["accessLevel"]["accountLicenseType"],
            json!("stakeholder"),
            "the given license replaces the default"
        );
    }

    #[test]
    fn the_table_uses_the_module_columns_and_its_empty_sentence() {
        assert_eq!(users_table(&[]), Report::Text("No users found.".to_owned()));

        let users = vec![json!({
            "id": "user-1",
            "user": {"principalName": "ada@example.com"},
            "accessLevel": {"accountLicenseType": "express"},
        })];

        assert_eq!(
            users_table(&users),
            Report::Table {
                headers: vec!["ID".to_owned(), "Email".to_owned(), "License".to_owned()],
                rows: vec![vec![
                    "user-1".to_owned(),
                    "ada@example.com".to_owned(),
                    "express".to_owned(),
                ]],
            }
        );
    }

    #[test]
    fn the_detail_prints_the_frozen_labels() {
        let detail = user_detail(&json!({
            "id": "user-1",
            "user": {"principalName": "ada@example.com", "displayName": "Ada Lovelace"},
            "accessLevel": {"accountLicenseType": "express", "status": "active"},
        }));

        assert_eq!(
            detail,
            format!(
                "\nUser Details\n\n{}\n  ID:      user-1\n  Email:   ada@example.com\n  Name:    Ada Lovelace\n  License: express\n  Status:  active\n\n",
                "─".repeat(60)
            )
        );
    }

    #[test]
    fn a_missing_field_interpolates_as_the_empty_string() {
        let detail = user_detail(&json!({"id": "user-1"}));

        assert_eq!(
            detail,
            format!(
                "\nUser Details\n\n{}\n  ID:      user-1\n  Email:   \n  Name:    \n  License: \n  Status:  \n\n",
                "─".repeat(60)
            )
        );
    }

    #[test]
    fn the_paths_encode_each_segment_strictly() {
        assert_eq!(user_path("user-1"), "/_apis/userentitlements/user-1");
        assert_eq!(
            user_path("ada@example.com"),
            "/_apis/userentitlements/ada%40example.com",
            "an @ is escaped, unlike the frozen URI.encode/1 (D22)"
        );
        assert_eq!(
            user_path("a/b"),
            "/_apis/userentitlements/a%2Fb",
            "a slash cannot split the path"
        );
    }
}
