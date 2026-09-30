//! `ado branch-policies list|show|create|update|delete` — the whole of
//! `lib/ado_cli/cli/branch_policies.ex`, under
//! `/{project}/_apis/policy/configurations`. The schema calls this node
//! `ado repos policies`; `branch-policies` is the runnable spelling (R3/D18).
//!
//! A policy is a `type` object plus a `settings` object whose shape belongs to
//! the type (a build definition id, a reviewer count, …), so `update` reads the
//! policy first and copies both back verbatim, changing only the flags the caller
//! named. `delete` never prompts (R1/R5): the frozen CLI sends its DELETE on `n`
//! and on EOF.

use ado_core::client::encode_path_segment;
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// `create`'s `Map.get(parsed.options, :blocking, true)`: the option table
/// declares `default: true`, so an absent `--blocking` is a blocking policy.
const DEFAULT_BLOCKING: bool = true;

/// `ado branch-policies list`: `GET /{project}/_apis/policy/configurations` with
/// the module's `repositoryId` and optional `branch`. `Client.list` unwraps the
/// `value` array and the value envelope carries it.
pub fn list(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    branch: Option<String>,
) -> Result<Report, AdoError> {
    let policies = items(
        context
            .client()?
            .list(&configurations_path(project), &list_params(repo_id, branch))?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(policies.clone())), || {
            policies_table(&policies)
        }),
    )
}

/// `ado branch-policies show`: `GET …/configurations/{policy_id}`; a 404 takes
/// the module's own wording.
pub fn show(context: &mut Context, project: &str, policy_id: i64) -> Result<Report, AdoError> {
    let path = policy_path(project, policy_id);

    match context.client()?.get(&path, &[]) {
        Ok(policy) => Ok(context.json_or_report(ok_value(policy.clone()), || {
            Report::Text(policy_detail(&policy))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Policy #{policy_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado branch-policies create`: `POST` the collection with the module's body —
/// the type id, the scope built from the positionals, and `isEnabled: true`.
/// The response's id is what the module's success line names.
pub fn create(
    context: &mut Context,
    project: &str,
    repo_id: &str,
    type_id: &str,
    branch: &str,
    blocking: Option<bool>,
) -> Result<Report, AdoError> {
    let body = create_body(
        type_id,
        repo_id,
        branch,
        blocking.unwrap_or(DEFAULT_BLOCKING),
    );
    let policy = context
        .client()?
        .post(&configurations_path(project), &body, &[])?;

    Ok(context.json_or_report(ok_value(policy.clone()), || {
        Report::Text(format!("Policy #{} created.", value_text(policy.get("id"))))
    }))
}

/// `ado branch-policies update`: read the policy, then `PUT` it back with the
/// module's four keys — the whole `type` object and the type-specific `settings`
/// preserved from the read, and each flag replaced only when the caller named it
/// (`Map.get(parsed.options, :blocking, existing["isBlocking"])`). A 404 on the
/// read takes the module's own wording.
pub fn update(
    context: &mut Context,
    project: &str,
    policy_id: i64,
    blocking: Option<bool>,
    enabled: Option<bool>,
) -> Result<Report, AdoError> {
    let path = policy_path(project, policy_id);

    let existing = match context.client()?.get(&path, &[]) {
        Ok(existing) => existing,
        Err(error) if error.code == ErrorCode::NotFound => {
            return Err(AdoError {
                message: format!("Policy #{policy_id} not found"),
                ..error
            });
        }
        Err(error) => return Err(error),
    };

    let policy = context
        .client()?
        .put(&path, &update_body(&existing, blocking, enabled), &[])?;

    Ok(context.json_or_report(ok_value(policy.clone()), || {
        Report::Text(format!("Policy #{policy_id} updated."))
    }))
}

/// `ado branch-policies delete`: the module's plain `DELETE`, without a prompt
/// (R1/R5). A status error keeps the module's classified envelope — the frozen
/// `Client.delete/2` passes its response body on undecoded, so that envelope
/// matches this build's raw-body one (D24's other paths do not).
pub fn delete(context: &mut Context, project: &str, policy_id: i64) -> Result<Report, AdoError> {
    context
        .client()?
        .delete(&policy_path(project, policy_id), &[])?;

    let message = format!("Policy #{policy_id} deleted.");

    Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
}

/// The collection path: `/{project}/_apis/policy/configurations`. The frozen
/// module glues `?repositoryId=…` onto this path and then `build_url/2` appends
/// `?api-version=7.1`, so its `repositoryId` value swallows the version (D25's
/// third site); this build sends both as pairs.
fn configurations_path(project: &str) -> String {
    format!(
        "/{}/_apis/policy/configurations",
        encode_path_segment(project)
    )
}

/// One policy below the collection; the id is an integer, so it cannot change
/// the URL's structure.
fn policy_path(project: &str, policy_id: i64) -> String {
    format!("{}/{}", configurations_path(project), policy_id)
}

/// The module's `params = %{}` plus `branch` when `--branch` is given, and the
/// repositoryId the frozen module glues into the path (D25's third site).
fn list_params(repo_id: &str, branch: Option<String>) -> Vec<(String, String)> {
    let mut params = vec![("repositoryId".to_owned(), repo_id.to_owned())];

    if let Some(branch) = branch {
        params.push(("branch".to_owned(), branch));
    }

    params
}

/// The module's create body: `type.id` alone (the display name is the API's to
/// fill in), `isEnabled` unconditionally true, and a one-entry `Exact` scope.
fn create_body(type_id: &str, repo_id: &str, branch: &str, blocking: bool) -> Value {
    json!({
        "type": {"id": type_id},
        "isBlocking": blocking,
        "isEnabled": true,
        "settings": {
            "scope": [{
                "repositoryId": repo_id,
                "refName": branch,
                "matchKind": "Exact"
            }]
        }
    })
}

/// The module's update body: every key is present, an absent flag option keeps
/// the existing value, and an absent key in the read stays JSON `null` — never a
/// default this build invented.
fn update_body(existing: &Value, blocking: Option<bool>, enabled: Option<bool>) -> Value {
    json!({
        "type": existing.get("type").cloned().unwrap_or(Value::Null),
        "isBlocking": flag_or_existing(blocking, existing, "isBlocking"),
        "isEnabled": flag_or_existing(enabled, existing, "isEnabled"),
        "settings": existing.get("settings").cloned().unwrap_or(Value::Null),
    })
}

fn flag_or_existing(flag: Option<bool>, existing: &Value, key: &str) -> Value {
    flag.map(Value::Bool)
        .unwrap_or_else(|| existing.get(key).cloned().unwrap_or(Value::Null))
}

/// The module's `print_policies_table/1` columns (ID, Type, Branch, Blocking,
/// Enabled), with the module's "No policies found." when empty. The frozen
/// table's padding and `String.slice(type, 0, 34)` are its own rendering; this
/// build's `Report::Table` is §8 surface.
fn policies_table(policies: &[Value]) -> Report {
    if policies.is_empty() {
        return Report::Text("No policies found.".to_owned());
    }

    let rows = policies
        .iter()
        .map(|policy| {
            vec![
                value_text(policy.get("id")),
                type_text(policy),
                scope_text(policy, "refName"),
                value_text(policy.get("isBlocking")),
                value_text(policy.get("isEnabled")),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Type".to_owned(),
            "Branch".to_owned(),
            "Blocking".to_owned(),
            "Enabled".to_owned(),
        ],
        rows,
    }
}

/// The module's `print_policy_detail/1`, minus the colour: its rule of 60 ASCII
/// hyphens (unlike `teams`' box-drawing `─`), the `(none)` fallbacks on branch and
/// repository, and the two fields that interpolate an absent value as the empty
/// string.
fn policy_detail(policy: &Value) -> String {
    let mut detail = String::from("\nPolicy Details\n\n");

    detail.push_str(&"-".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:        {}\n", value_text(policy.get("id"))));
    detail.push_str(&format!("  Type:      {}\n", type_text(policy)));
    detail.push_str(&format!("  Branch:    {}\n", scope_text(policy, "refName")));
    detail.push_str(&format!(
        "  Repository:{}\n",
        scope_text(policy, "repositoryId")
    ));
    detail.push_str(&format!(
        "  Blocking:  {}\n",
        value_text(policy.get("isBlocking"))
    ));
    detail.push_str(&format!(
        "  Enabled:   {}\n",
        value_text(policy.get("isEnabled"))
    ));
    detail.push_str(&format!(
        "  Created:   {}\n",
        value_text(policy.get("createdDate"))
    ));

    detail.push('\n');

    detail
}

/// `policy["type"]["displayName"] || policy["type"]["id"]`, where an absent type
/// interpolates as the empty string (the module gives this line no fallback).
fn type_text(policy: &Value) -> String {
    let Some(type_object) = policy.get("type").filter(|value| truthy(value)) else {
        return String::new();
    };

    match type_object.get("displayName").filter(|value| truthy(value)) {
        Some(display_name) => value_text(Some(display_name)),
        None => value_text(type_object.get("id")),
    }
}

/// The first scope entry's `refName`/`repositoryId` with the module's `(none)`
/// fallback: `List.first(settings["scope"])[key] || "(none)"`.
fn scope_text(policy: &Value, key: &str) -> String {
    let scope = policy
        .get("settings")
        .and_then(|settings| settings.get("scope"))
        .and_then(Value::as_array)
        .and_then(|scope| scope.first());

    match scope
        .and_then(|scope| scope.get(key))
        .filter(|value| truthy(value))
    {
        Some(value) => value_text(Some(value)),
        None => "(none)".to_owned(),
    }
}

/// Elixir's `||`: only `nil` and `false` fall through, so an empty string is a
/// value like any other.
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
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
    fn list_params_carry_the_repository_id_and_an_optional_branch() {
        assert_eq!(
            list_params("Alpha.Core", None),
            vec![("repositoryId".to_owned(), "Alpha.Core".to_owned())],
            "the frozen module glues this into the path; here it is a pair (D25)"
        );
        assert_eq!(
            list_params("Alpha.Core", Some("main".to_owned())),
            vec![
                ("repositoryId".to_owned(), "Alpha.Core".to_owned()),
                ("branch".to_owned(), "main".to_owned()),
            ]
        );
        assert_eq!(
            list_params("Alpha.Core", Some(String::new())),
            vec![
                ("repositoryId".to_owned(), "Alpha.Core".to_owned()),
                ("branch".to_owned(), String::new()),
            ],
            "an explicitly empty --branch is the absent option's opposite"
        );
    }

    #[test]
    fn create_body_is_the_captured_type_specific_shape() {
        assert_eq!(
            create_body(
                "fa4e907d-c16b-4a4c-9dfa-4906e5d171dd",
                "Alpha.Core",
                "refs/heads/main",
                true
            ),
            json!({
                "type": {"id": "fa4e907d-c16b-4a4c-9dfa-4906e5d171dd"},
                "isBlocking": true,
                "isEnabled": true,
                "settings": {
                    "scope": [{
                        "repositoryId": "Alpha.Core",
                        "refName": "refs/heads/main",
                        "matchKind": "Exact"
                    }]
                }
            }),
            "the type is an id-only object and the scope one Exact entry"
        );
    }

    #[test]
    fn update_body_replaces_only_the_named_flags() {
        let existing = json!({
            "type": {"id": "t", "displayName": "Build"},
            "isBlocking": false,
            "isEnabled": true,
            "settings": {"scope": [{"refName": "refs/heads/main", "repositoryId": "Alpha.Core"}]}
        });

        assert_eq!(
            update_body(&existing, None, None),
            json!({
                "type": {"id": "t", "displayName": "Build"},
                "isBlocking": false,
                "isEnabled": true,
                "settings": {"scope": [{"refName": "refs/heads/main", "repositoryId": "Alpha.Core"}]}
            }),
            "no options leaves the read's values in place"
        );
        assert_eq!(
            update_body(&existing, Some(true), Some(false))["isBlocking"],
            json!(true)
        );
        assert_eq!(
            update_body(&existing, Some(true), Some(false))["isEnabled"],
            json!(false)
        );
    }

    #[test]
    fn update_body_keeps_an_absent_key_null() {
        let existing = json!({"id": 54, "settings": {"scope": [], "reviewerCount": 1}});

        assert_eq!(
            update_body(&existing, None, None),
            json!({
                "type": null,
                "isBlocking": null,
                "isEnabled": null,
                "settings": {"scope": [], "reviewerCount": 1}
            }),
            "the captured bare-policy body: an absent read field is null"
        );
    }

    #[test]
    fn detail_uses_the_module_labels_and_fallbacks() {
        let policy = json!({
            "id": 42,
            "type": {"id": "t", "displayName": "Build"},
            "isBlocking": false,
            "isEnabled": true,
            "createdDate": "2026-09-01T10:00:00.000Z",
            "settings": {"scope": [{"repositoryId": "Alpha.Core", "refName": "refs/heads/main"}]}
        });

        let detail = policy_detail(&policy);

        assert_eq!(
            detail,
            concat!(
                "\nPolicy Details\n\n",
                "------------------------------------------------------------\n",
                "  ID:        42\n",
                "  Type:      Build\n",
                "  Branch:    refs/heads/main\n",
                "  Repository:Alpha.Core\n",
                "  Blocking:  false\n",
                "  Enabled:   true\n",
                "  Created:   2026-09-01T10:00:00.000Z\n",
                "\n",
            )
        );
    }

    #[test]
    fn detail_without_a_type_or_scope_prints_the_empty_and_none_forms() {
        let detail = policy_detail(&json!({"id": 52}));

        assert!(
            detail.contains("  Type:      \n"),
            "an absent type is the empty string, not (none): {detail}"
        );
        assert!(
            detail.contains("  Branch:    (none)\n"),
            "an absent scope is (none): {detail}"
        );
        assert!(
            detail.contains("  Repository:(none)\n"),
            "an absent scope is (none): {detail}"
        );
        assert!(
            detail.contains("  Created:   \n"),
            "an absent createdDate is the empty string: {detail}"
        );
    }

    #[test]
    fn type_text_falls_through_a_missing_display_name() {
        assert_eq!(
            type_text(&json!({"type": {"id": "t"}})),
            "t",
            "the display name falls back to the id"
        );
        assert_eq!(
            type_text(&json!({"type": {"id": "t", "displayName": "Build"}})),
            "Build"
        );
        assert_eq!(type_text(&json!({"id": 52})), "");
        assert_eq!(type_text(&json!({"type": null})), "");
    }

    #[test]
    fn scope_text_keeps_an_empty_ref_name() {
        assert_eq!(
            scope_text(
                &json!({"settings": {"scope": [{"refName": ""}]}}),
                "refName"
            ),
            "",
            "Elixir's `||` falls through only nil and false"
        );
        assert_eq!(
            scope_text(&json!({"settings": {"scope": []}}), "refName"),
            "(none)"
        );
        assert_eq!(scope_text(&json!({"id": 1}), "refName"), "(none)");
    }

    #[test]
    fn table_of_nothing_is_the_module_message() {
        assert_eq!(
            policies_table(&[]),
            Report::Text("No policies found.".to_owned())
        );
    }

    #[test]
    fn table_rows_carry_the_module_fields() {
        assert_eq!(
            policies_table(&[json!({
                "id": 42,
                "type": {"id": "t", "displayName": "Build"},
                "isBlocking": false,
                "isEnabled": true,
                "settings": {"scope": [{"refName": "refs/heads/main"}]}
            })]),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Type".to_owned(),
                    "Branch".to_owned(),
                    "Blocking".to_owned(),
                    "Enabled".to_owned()
                ],
                rows: vec![vec![
                    "42".to_owned(),
                    "Build".to_owned(),
                    "refs/heads/main".to_owned(),
                    "false".to_owned(),
                    "true".to_owned()
                ]],
            }
        );
    }
}
