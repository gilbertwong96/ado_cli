//! `ado security grant|revoke` — the whole of `lib/ado_cli/cli/security.ex`: the
//! typed-flag guard in front of a secret-reading mutation, the project-name → id
//! lookup with its UUID short-circuit, the caller-descriptor fetch with its two
//! refusals, and the `_apis/accesscontrolentries` write that toggles the Library
//! namespace's `ViewSecrets` bit for the calling user.
//!
//! Five captured shapes decide the code:
//!
//!   * the guard's absence is **loud on both sides** — exit 1, stderr, no request
//!     and no document even under `--json` (`sec-grant-guard-json`/`-human`,
//!     `sec-revoke-guard-json`), so this build answers [`AdoError::cancelled`]:
//!     D32's shape, where the oracle's own refusal already was;
//!   * the chain is the lookup (`GET /_apis/projects`, `api-version=7.1`) only
//!     when the argument is not a UUID, then
//!     `GET /_apis/connectionData?api-version=7.1-preview.1`, then
//!     `POST /_apis/accesscontrolentries/<namespace>?api-version=7.1` with
//!     `{"token":<resolved id>,"merge":<command is grant>,"accessControlEntries":
//!     [{"descriptor":…,"allow":8|0,"deny":0,"extendedInfo":{}}]}`;
//!   * both writes print their sentence **in both modes** (D33): the message
//!     envelope under `--json`, the same sentence in human mode;
//!   * the module's three `#{inspect(err)}` sites keep their step name and render
//!     the body's `message` member instead of Elixir's tuple syntax (D24's class);
//!   * the 400/401/403 branch is the module's own three-cause prose, with the raw
//!     upstream bytes where the oracle interpolates `inspect/2` of the decoded map.
//!
//! One D34 row is this area's own: a matching project entry **without an `id`**
//! crashes the frozen `%{"id" => id}` match and the rescue exits 0 with empty
//! streams (`sec-grant-proj-noid`); this build fails loudly after the one lookup.

use ado_core::envelope::ok_message;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::context::Context;
use crate::output::Report;

/// The Library security namespace GUID (Microsoft.Security's namespace reference).
const LIBRARY_NAMESPACE_ID: &str = "b7e84409-6553-448a-bbb2-af228e07cbeb";

/// The `ViewSecrets` bit (terraform-provider-azuredevops / Microsoft.Security).
const VIEW_SECRETS_BIT: i64 = 8;

/// The only permission the frozen module accepts, and the `--permission` default
/// when the option is absent.
pub const DEFAULT_PERMISSION: &str = "ViewSecrets";

/// The org-scoped project collection the name → id lookup reads.
const PROJECTS_PATH: &str = "/_apis/projects";

/// The caller-identity endpoint, carrying the preview version.
const CONNECTION_DATA_PATH: &str = "/_apis/connectionData";

/// The module's `@source_url`, named in the unsupported-permission refusal.
const SOURCE_URL: &str = "https://github.com/gilbertwong96/ado_cli";

/// `validate_inputs/3`'s first branch: the whole point of the command. The
/// oracle checks it first, before the project and the permission, and sends
/// nothing — captured on both commands and in both modes.
const SAFETY_FLAG_REFUSAL: &str = "Refusing to run without the safety flag. Re-run with --yes-this-mutates-secret-read to confirm.";

/// `fetch_caller_descriptor/0`'s two refusals, verbatim.
const MSA_REFUSAL: &str = "Your identity is a personal Microsoft account (MSA). The Azure DevOps Security API rejects \
     'msa.*' descriptors for permission grants, so this command cannot elevate the caller. \
     Use a work/school Entra ID (AAD) identity, or grant 'View library item secrets' on the \
     Library via the web UI.";

const MISSING_DESCRIPTOR: &str = "Could not determine caller identity descriptor. The /_apis/connectionData endpoint \
     did not return a subjectDescriptor. Re-run 'ado whoami' to verify auth is healthy.";

/// `ado security grant PROJECT_NAME_OR_ID [--permission P] --yes-this-mutates-secret-read`.
pub fn grant(
    context: &mut Context,
    project: &str,
    permission: &str,
    confirmed: bool,
) -> Result<Report, AdoError> {
    run(context, project, permission, confirmed, true)
}

/// `ado security revoke PROJECT_NAME_OR_ID [--permission P] --yes-this-mutates-secret-read`.
pub fn revoke(
    context: &mut Context,
    project: &str,
    permission: &str,
    confirmed: bool,
) -> Result<Report, AdoError> {
    run(context, project, permission, confirmed, false)
}

/// The shared body of both leaves: the module's three guards, the chain, and the
/// sentence for the direction.
fn run(
    context: &mut Context,
    project: &str,
    permission: &str,
    confirmed: bool,
    set: bool,
) -> Result<Report, AdoError> {
    validate_inputs(project, permission, confirmed)?;

    let project_id = resolve_project_id(context, project)?;
    let descriptor = fetch_caller_descriptor(context)?;

    modify_library_bit(context, &project_id, &descriptor, set)?;

    let message = if set {
        granted_line(project, &project_id, permission)
    } else {
        revoked_line(project, &project_id, permission)
    };

    Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
}

/// `validate_inputs/3`'s `cond`, in its order: the guard beats a missing project,
/// which beats a foreign permission. The oracle writes each of the latter two to
/// stderr with no envelope; this build's error path answers the envelope under
/// `--json` (D4).
fn validate_inputs(project: &str, permission: &str, confirmed: bool) -> Result<(), AdoError> {
    if !confirmed {
        return Err(AdoError::cancelled(SAFETY_FLAG_REFUSAL));
    }

    if project.is_empty() {
        return Err(AdoError::validation("Project argument is required."));
    }

    if permission != DEFAULT_PERMISSION {
        return Err(AdoError::validation(format!(
            "Unsupported permission '{permission}'. Currently only 'ViewSecrets' is supported. \
             Patches welcome at {SOURCE_URL}."
        )));
    }

    Ok(())
}

/// `resolve_project_id/1`: a UUID argument is used directly, anything else reads
/// the project collection and finds the entry by name. The lookup's failure keeps
/// the module's step name; a list the argument is not in is `not_found`; a body
/// this build cannot read is a loud failure where the oracle inspects it.
fn resolve_project_id(context: &mut Context, project: &str) -> Result<Value, AdoError> {
    if is_uuid(project) {
        return Ok(json!(project));
    }

    let body = context
        .client()?
        .get(PROJECTS_PATH, &[version_pair("7.1")])
        .map_err(|error| step_failure("Failed to look up project ID", &error))?;

    let projects = body.get("value").and_then(Value::as_array).ok_or_else(|| {
        malformed("Failed to look up project ID: the response has no 'value' array.")
    })?;

    let entry = projects
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some(project))
        .ok_or_else(|| AdoError::not_found(format!("Project '{project}' not found.")))?;

    entry.get("id").cloned().ok_or_else(|| {
        malformed(format!(
            "Failed to look up project ID: the entry for '{project}' has no id."
        ))
    })
}

/// The frozen regex, `~r/^[0-9a-fA-F]{8}-…{12}$/`, whose `$` also matches before
/// one trailing newline (PCRE, verified against the oracle): `"<uuid>\n"` is
/// accepted and the argument is passed through verbatim.
fn is_uuid(project: &str) -> bool {
    let candidate = project.strip_suffix('\n').unwrap_or(project);
    let groups = candidate.split('-').collect::<Vec<_>>();
    let lengths = [8, 4, 4, 4, 12];

    groups.len() == lengths.len()
        && groups.iter().zip(lengths).all(|(group, length)| {
            group.len() == length && group.chars().all(|c| c.is_ascii_hexdigit())
        })
}

/// `fetch_caller_descriptor/0`: the `authenticatedUser.subjectDescriptor` of the
/// connection data, with the MSA and missing-descriptor refusals verbatim. Any
/// non-string member — including a number — reads as missing, as the frozen
/// `when is_binary(d) and d != ""` guard does.
fn fetch_caller_descriptor(context: &mut Context) -> Result<String, AdoError> {
    let body = context
        .client()?
        .get(CONNECTION_DATA_PATH, &[version_pair("7.1-preview.1")])
        .map_err(|error| step_failure("Failed to fetch caller descriptor", &error))?;

    match body.pointer("/authenticatedUser/subjectDescriptor") {
        Some(Value::String(descriptor)) if !descriptor.is_empty() => {
            if descriptor.starts_with("msa.") {
                Err(AdoError::validation(MSA_REFUSAL))
            } else {
                Ok(descriptor.clone())
            }
        }
        _ => Err(AdoError::validation(MISSING_DESCRIPTOR)),
    }
}

/// `modify_library_bit/4`: the ACL entry the frozen body carries, with the
/// module's two branch values (`allow = bit | 0`, `merge = set?`). The response
/// body is discarded; the module only reads `{:ok, _}`.
fn modify_library_bit(
    context: &mut Context,
    project_id: &Value,
    descriptor: &str,
    set: bool,
) -> Result<(), AdoError> {
    let allow = if set { VIEW_SECRETS_BIT } else { 0 };
    let body = json!({
        "token": project_id,
        "merge": set,
        "accessControlEntries": [{
            "descriptor": descriptor,
            "allow": allow,
            "deny": 0,
            "extendedInfo": {}
        }]
    });

    let path = format!("/_apis/accesscontrolentries/{LIBRARY_NAMESPACE_ID}");

    match context.client()?.post(&path, &body, &[version_pair("7.1")]) {
        Ok(_) => Ok(()),
        Err(error) => Err(acl_failure(set, error)),
    }
}

/// The write's error branch: the module's own three-cause message for the three
/// statuses it names, the step failure for every other one.
fn acl_failure(set: bool, error: AdoError) -> AdoError {
    if matches!(error.status, Some(400 | 401 | 403)) {
        let action = if set { "grant" } else { "revoke" };
        let status = error.status.expect("the status the branch is keyed on");

        return AdoError {
            message: rejection_message(action, status, &error),
            ..error
        };
    }

    step_failure("API error", &error)
}

/// The module's `Azure DevOps rejected the <action> (<status>)` message: the
/// three causes verbatim, then `Raw response:` and the upstream bytes sliced
/// like the oracle's `String.slice(0, 200)` — the oracle renders the *decoded*
/// map with `inspect/2` there (D24's class).
fn rejection_message(action: &str, status: u16, error: &AdoError) -> String {
    let preview = raw_body(error).chars().take(200).collect::<String>();

    format!(
        "Azure DevOps rejected the {action} ({status}). Common causes: (a) your token lacks the \
         'vso.security_manage' scope (browser OAuth tokens don't have it; use a PAT with \
         'Project and Team'); (b) the caller's MSA-descriptor format isn't accepted by Azure \
         DevOps for MSA-backed accounts; (c) you are not a Project Collection Administrator. \
         Raw response: {preview}"
    )
}

/// A module step's failure: the step's own name, then the API body's `message`
/// member where the frozen interpolates `#{inspect(err)}` — the
/// `upload_conflict_message` idiom (`pipelines.rs`), with the classified message
/// as the fallback when the body carries none.
fn step_failure(step: &str, error: &AdoError) -> AdoError {
    AdoError {
        message: format!("{step}: {}", api_message(error)),
        ..error.clone()
    }
}

/// The `message` member of the error's raw body, or the classified message when
/// the body is not JSON, not an object, or nameless.
fn api_message(error: &AdoError) -> String {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("body"))
        .and_then(Value::as_str)
        .and_then(|body| serde_json::from_str::<Value>(body).ok())
        .and_then(|body| {
            body.get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| error.message.clone())
}

/// The upstream bytes `AdoError::from_status` carries, or empty for a transport
/// failure (which has no body).
fn raw_body(error: &AdoError) -> String {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("body"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// A response this build cannot read: a 2xx whose shape is not the frozen one.
/// `api_error` because the server's answer is at fault, never the user's input.
fn malformed(message: impl Into<String>) -> AdoError {
    AdoError {
        code: ErrorCode::ApiError,
        status: None,
        message: message.into(),
        details: None,
    }
}

/// `"Granted '#{permission}' on Library namespace for project '#{project}'
/// (#{project_id}) …"` — the argument project and the **resolved** id, with
/// `#{id}`'s interpolation rules (`nil` reads empty).
fn granted_line(project: &str, project_id: &Value, permission: &str) -> String {
    format!(
        "Granted '{permission}' on Library namespace for project '{project}' ({}) to the \
         calling user. You can now download Secure Files without elevation. Revoke later with \
         'ado security revoke --project {project} --permission {permission} \
         --yes-this-mutates-secret-read'.",
        interpolate(project_id)
    )
}

/// `"Revoked '#{permission}' … (#{project_id}) for the calling user."`
fn revoked_line(project: &str, project_id: &Value, permission: &str) -> String {
    format!(
        "Revoked '{permission}' on Library namespace for project '{project}' ({}) for the \
         calling user.",
        interpolate(project_id)
    )
}

/// Elixir's `#{term}` interpolation for the JSON scalars an id can carry: `nil`
/// is empty, `true`/`false` their literals, a number its digits, a string itself.
fn interpolate(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

fn version_pair(version: &str) -> (String, String) {
    ("api-version".to_owned(), version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uuid_regex_is_the_frozens() {
        assert!(is_uuid("11111111-2222-3333-4444-555555555555"));
        assert!(is_uuid("11111111-2222-3333-4444-55555555555A"));
        assert!(is_uuid("abcdef01-2345-6789-abcd-ef0123456789"));
        assert!(
            is_uuid("11111111-2222-3333-4444-555555555555\n"),
            "PCRE's `$` matches before one trailing newline (verified against the oracle)"
        );

        assert!(!is_uuid(&"-".repeat(36)));
        assert!(!is_uuid("11111111-2222-3333-4444-55555555555"));
        assert!(!is_uuid("11111111-2222-3333-4444-555555555555\n\n"));
        assert!(!is_uuid("11111111-2222-3333-4444-555555555555\r\n"));
        assert!(!is_uuid(" 11111111-2222-3333-4444-555555555555"));
        assert!(!is_uuid("11111111-2222-3333-4444-55555555555g"));
        assert!(!is_uuid(""));
    }

    #[test]
    fn the_guard_is_the_first_check_and_the_only_refusal_of_its_kind() {
        let refusal = validate_inputs("", "Other", false).expect_err("the guard");

        assert_eq!(refusal.code, ErrorCode::Cancelled, "D32's refusal shape");
        assert_eq!(refusal.message, SAFETY_FLAG_REFUSAL);

        assert_eq!(
            validate_inputs("", DEFAULT_PERMISSION, true)
                .expect_err("the project")
                .message,
            "Project argument is required."
        );
        assert_eq!(
            validate_inputs("Alpha", "Other", true)
                .expect_err("the permission")
                .message,
            format!(
                "Unsupported permission 'Other'. Currently only 'ViewSecrets' is supported. \
                 Patches welcome at {SOURCE_URL}."
            )
        );
        assert!(validate_inputs("Alpha", DEFAULT_PERMISSION, true).is_ok());
    }

    #[test]
    fn the_two_sentences_interpolate_the_argument_and_the_resolved_id() {
        assert_eq!(
            granted_line(
                "Alpha",
                &json!("6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c"),
                "ViewSecrets"
            ),
            "Granted 'ViewSecrets' on Library namespace for project 'Alpha' \
             (6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c) to the calling user. You can now download \
             Secure Files without elevation. Revoke later with 'ado security revoke --project \
             Alpha --permission ViewSecrets --yes-this-mutates-secret-read'."
        );
        assert_eq!(
            revoked_line(
                "Alpha",
                &json!("6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c"),
                "ViewSecrets"
            ),
            "Revoked 'ViewSecrets' on Library namespace for project 'Alpha' \
             (6a1f8f6e-2b8d-4b9e-9d2a-1c3f5e7a9b0c) for the calling user."
        );
    }

    #[test]
    fn interpolate_follows_elixirs_sharp() {
        assert_eq!(interpolate(&json!(null)), "");
        assert_eq!(interpolate(&json!("text")), "text");
        assert_eq!(interpolate(&json!(true)), "true");
        assert_eq!(interpolate(&json!(42)), "42");
        assert_eq!(interpolate(&json!(1.5)), "1.5");
    }

    #[test]
    fn the_step_failure_prefers_the_bodys_message_member() {
        let status_error = AdoError::from_status(500, "{\"message\":\"TF400813: down.\"}");

        assert_eq!(
            step_failure("Failed to look up project ID", &status_error).message,
            "Failed to look up project ID: TF400813: down."
        );
    }

    #[test]
    fn the_step_failure_falls_back_to_the_classified_message() {
        for body in ["not json", "{\"message\": 42}", "{}", "[]"] {
            let error = AdoError::from_status(503, body);

            assert_eq!(
                step_failure("Failed to fetch caller descriptor", &error).message,
                format!("Failed to fetch caller descriptor: {}", error.message),
                "the classifier's own text for {body:?}"
            );
        }

        let transport = AdoError {
            code: ErrorCode::NetworkError,
            status: None,
            message: "Connection refused. Is the server reachable?".to_owned(),
            details: None,
        };

        assert_eq!(
            step_failure("API error", &transport).message,
            "API error: Connection refused. Is the server reachable?"
        );
    }

    #[test]
    fn the_rejection_message_carries_the_three_causes_and_the_raw_body() {
        let error = AdoError::from_status(403, "{\"message\":\"TF400813: denied.\"}");
        let message = rejection_message("grant", 403, &error);

        assert!(
            message.starts_with(
                "Azure DevOps rejected the grant (403). Common causes: (a) your token lacks \
                 the 'vso.security_manage' scope (browser OAuth tokens don't have it; use a PAT \
                 with 'Project and Team'); (b) the caller's MSA-descriptor format isn't accepted \
                 by Azure DevOps for MSA-backed accounts; (c) you are not a Project Collection \
                 Administrator. Raw response: "
            ),
            "{message}"
        );
        assert!(
            message.ends_with("Raw response: {\"message\":\"TF400813: denied.\"}"),
            "{message}"
        );

        assert!(
            rejection_message("revoke", 401, &error)
                .starts_with("Azure DevOps rejected the revoke (401). Common causes:")
        );
    }

    #[test]
    fn the_rejection_preview_is_sliced_to_two_hundred_characters() {
        let body = "x".repeat(300);
        let error = AdoError::from_status(403, &body);

        let message = rejection_message("grant", 403, &error);
        let preview = message
            .rsplit_once("Raw response: ")
            .expect("the preview marker")
            .1;

        assert_eq!(
            preview.chars().count(),
            200,
            "the module's String.slice(0, 200)"
        );
    }

    #[test]
    fn the_acl_failure_is_the_module_prose_only_for_its_three_statuses() {
        for status in [400, 401, 403] {
            let error = AdoError::from_status(status, "{\"message\":\"nope\"}");
            let failure = acl_failure(true, error);

            assert!(
                failure
                    .message
                    .starts_with(&format!("Azure DevOps rejected the grant ({status}).")),
                "{status}"
            );
            assert_eq!(
                failure.status,
                Some(status),
                "the classified envelope keeps it"
            );
        }

        let error = AdoError::from_status(500, "{\"message\":\"TF400813: down.\"}");
        let failure = acl_failure(false, error);

        assert_eq!(failure.message, "API error: TF400813: down.");
    }

    #[test]
    fn a_malformed_body_is_an_api_error_with_no_status() {
        let error = malformed("Failed to look up project ID: the response has no 'value' array.");

        assert_eq!(error.code, ErrorCode::ApiError);
        assert_eq!(error.status, None);
        assert_eq!(error.details, None);
    }

    #[test]
    fn the_version_pairs_are_the_captured_ones() {
        assert_eq!(
            version_pair("7.1"),
            ("api-version".to_owned(), "7.1".to_owned())
        );
        assert_eq!(
            version_pair("7.1-preview.1"),
            ("api-version".to_owned(), "7.1-preview.1".to_owned())
        );
    }
}
