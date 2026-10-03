//! `ado banners show|set|delete` — the whole of `lib/ado_cli/cli/banners.ex`:
//! the organization-scoped `_apis/settings/entries/banners` entry, its two views
//! and its two writes.
//!
//! Four captured shapes decide the code:
//!
//!   * `show`'s `--json` document is the entry's **`value` member**, not the
//!     settings entry, and a missing entry is the same "no banner" state as an
//!     empty value — on stdout, exit **0** (the module's `halt_success("")`
//!     path), never an error;
//!   * `set`'s body is `{"value":{"message":…,"type":…,"level":…}}` with the
//!     module's two `Map.get` defaults (`info`, `projectCollection`), and a
//!     present empty value is sent as `""`;
//!   * both writes print their human line **even under `--json`** in the oracle
//!     (D33); this build answers the message envelope there and the same sentence
//!     in human mode;
//!   * `delete` carries the module's own 404 wording (`No banner to delete.`) on
//!     stderr with no envelope (D4), where every other failure is the classified
//!     envelope.
//!
//! `--message` resolves the `@<file>` and `-` forms its own help advertises, the
//! `connections --access-token` convention (Ruling 4(b); D44). The frozen CLI
//! sends both strings literally — captured — which is why the repair carries a
//! deviation row rather than parity.

use std::fs;
use std::io::Read;

use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::context::Context;
use crate::output::Report;

/// The organization-scoped settings entry; the client injects the organization.
/// The preview `api-version` the settings-entry surface requires: it answers
/// 400 `VssInvalidPreviewVersionException` for a plain `7.1` (D60).
const PREVIEW_API_VERSION: &str = "7.1-preview.1";

/// The preview pair the three calls below send; the client merges a caller's
/// `api-version` over its own default.
fn preview_params() -> Vec<(String, String)> {
    vec![("api-version".to_owned(), PREVIEW_API_VERSION.to_owned())]
}

/// The organization-wide banner entry. The settings surface is **scoped**: an
/// organization-wide value lives under the `host` scope, and `entries/banners` is
/// read as a scope *named* banners — which is why the API answers 400 `userId` for
/// it (the divergence D62).
const BANNERS_PATH: &str = "/_apis/settings/entries/host/banners";

/// The module's `Map.get(parsed.options, :type, "info")`.
const DEFAULT_TYPE: &str = "info";

/// The module's `Map.get(parsed.options, :level, "projectCollection")`.
const DEFAULT_LEVEL: &str = "projectCollection";

/// The module's two-branch sentence, shared by the empty value and the 404. The
/// two trailing blank lines are the capture's, ANSI-stripped: the module's
/// `display_banner/2` closes with `writeln("")` and the shell adds its
/// `halt_success("")` empty line.
const NO_BANNER: &str = "\nNo banner configured.\n\n\n";

/// `ado banners show`: `GET …/entries/banners`. A 404 is not an error — the
/// module answers its "no banner" sentence and exits 0 — so this build renders it
/// as the empty banner: the same sentence in human mode and the empty-value
/// envelope under `--json`.
pub fn show(context: &mut Context) -> Result<Report, AdoError> {
    let value = match context.client()?.get(BANNERS_PATH, &preview_params()) {
        Ok(entry) => banner_value(&entry),
        Err(error) if error.code == ErrorCode::NotFound => json!({}),
        Err(error) => return Err(error),
    };

    Ok(context.json_or_report(ok_value(value.clone()), || banner_view(&value)))
}

/// `ado banners set --message MSG [--type TYPE] [--level LEVEL]`:
/// `PUT …/entries/banners` with the module's `value` object, then its line.
pub fn set(
    context: &mut Context,
    message: &str,
    banner_type: Option<String>,
    level: Option<String>,
) -> Result<Report, AdoError> {
    let message = resolve_message(message)?;

    context.client()?.put(
        BANNERS_PATH,
        &set_body(&message, banner_type, level),
        &preview_params(),
    )?;

    Ok(success_line(context, format!("Banner set: \"{message}\"")))
}

/// `ado banners delete`: `DELETE …/entries/banners`. A 404 keeps the module's own
/// wording (no envelope); every other failure is the classified envelope. There is
/// no confirmation — the frozen command prompts for nothing (captured).
pub fn delete(context: &mut Context) -> Result<Report, AdoError> {
    match context.client()?.delete(BANNERS_PATH, &preview_params()) {
        Ok(()) => Ok(success_line(context, "Banner removed.".to_owned())),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: "No banner to delete.".to_owned(),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// One success line: the message envelope under `--json`, the module's sentence in
/// human mode (the oracle prints the human line in both modes; D33).
fn success_line(context: &Context, message: String) -> Report {
    context.json_or_report(ok_message(&message), || Report::Text(message))
}

/// The module's `entry["value"] || %{}`: a missing, nil or false member is the
/// empty object.
fn banner_value(entry: &Value) -> Value {
    match entry.get("value") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => json!({}),
        Some(value) => value.clone(),
    }
}

/// The module's `if val == "" or val == %{}` branch pair.
fn banner_view(value: &Value) -> Report {
    let empty = match value {
        Value::String(text) => text.is_empty(),
        Value::Object(map) => map.is_empty(),
        _ => false,
    };

    Report::Text(if empty {
        NO_BANNER.to_owned()
    } else {
        format!(
            concat!(
                "\nCurrent banner:\n",
                "  Message: {}\n",
                "  Type:    {}\n",
                "  Level:   {}\n",
                // The capture's two blank lines: `display_banner/2`'s own
                // `writeln("")` plus the shell's `halt_success("")` artefact.
                "\n\n",
            ),
            field(value, "message", "(empty)"),
            field(value, "type", DEFAULT_TYPE),
            field(value, "level", DEFAULT_LEVEL),
        )
    })
}

/// The module's `val["message"] || "(empty)"` per field: a missing member reads as
/// the member's own default, and `null`/`false` are falsy like an absent key, while
/// `""` is a value.
fn field(value: &Value, name: &str, fallback: &str) -> String {
    match value.get(name) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => fallback.to_owned(),
        Some(field) => value_text(field),
    }
}

/// `set_banner/1`'s body, verbatim: the `value` wrapper and the three members.
fn set_body(message: &str, banner_type: Option<String>, level: Option<String>) -> Value {
    json!({
        "value": {
            "message": message,
            "type": banner_type.unwrap_or_else(|| DEFAULT_TYPE.to_owned()),
            "level": level.unwrap_or_else(|| DEFAULT_LEVEL.to_owned()),
        }
    })
}

/// The `connections --access-token` convention (`resolve_token/1`): a literal, `-`
/// for all of stdin or `@path` for a file, both trimmed. The frozen `banners set`
/// never reads either — it sends the literal string (D44); this is Ruling 4(b)'s
/// repair.
fn resolve_message(raw: &str) -> Result<String, AdoError> {
    if raw.is_empty() {
        return Ok(String::new());
    }

    if raw == "-" {
        let mut content = String::new();

        std::io::stdin()
            .read_to_string(&mut content)
            .map_err(|error| {
                AdoError::validation(format!("Cannot read the message from stdin: {error}"))
            })?;

        return Ok(content.trim().to_owned());
    }

    match raw.strip_prefix('@') {
        Some(path) => fs::read_to_string(path)
            .map(|content| content.trim().to_owned())
            .map_err(|error| {
                AdoError::validation(format!("Cannot read message file \"{path}\": {error}"))
            }),
        None => Ok(raw.to_owned()),
    }
}

/// Elixir's `#{term}` interpolation for the JSON scalars these fields carry.
fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Value {
        json!({"id": "banners", "value": {
            "message": "Scheduled maintenance tonight",
            "type": "warning",
            "level": "projectCollection"
        }})
    }

    #[test]
    fn the_entrys_value_member_is_the_document() {
        assert_eq!(banner_value(&entry()), entry()["value"]);
        assert_eq!(banner_value(&json!({"id": "banners"})), json!({}));
        assert_eq!(banner_value(&json!({"value": null})), json!({}));
        assert_eq!(
            banner_value(&json!({"value": false})),
            json!({}),
            "the module's `|| %{{}}` catches false as well as nil"
        );
        assert_eq!(banner_value(&json!({"value": ""})), json!(""));
    }

    #[test]
    fn the_view_is_the_modules_three_labelled_fields() {
        assert_eq!(
            banner_view(&entry()["value"]),
            Report::Text(
                concat!(
                    "\nCurrent banner:\n",
                    "  Message: Scheduled maintenance tonight\n",
                    "  Type:    warning\n",
                    "  Level:   projectCollection\n",
                    "\n\n"
                )
                .to_owned()
            )
        );
    }

    #[test]
    fn the_view_falls_back_to_the_modules_defaults() {
        assert_eq!(
            banner_view(&json!({"message": "Heads up"})),
            Report::Text(
                concat!(
                    "\nCurrent banner:\n",
                    "  Message: Heads up\n",
                    "  Type:    info\n",
                    "  Level:   projectCollection\n",
                    "\n\n"
                )
                .to_owned()
            )
        );
        assert_eq!(
            banner_view(&json!({"message": "", "type": null})),
            Report::Text(
                concat!(
                    "\nCurrent banner:\n",
                    "  Message: \n",
                    "  Type:    info\n",
                    "  Level:   projectCollection\n",
                    "\n\n"
                )
                .to_owned()
            ),
            "nil is falsy and takes the default, while \"\" is truthy and prints as itself"
        );
        assert_eq!(
            banner_view(&json!({"type": "warning"})),
            Report::Text(
                concat!(
                    "\nCurrent banner:\n",
                    "  Message: (empty)\n",
                    "  Type:    warning\n",
                    "  Level:   projectCollection\n",
                    "\n\n"
                )
                .to_owned()
            ),
            "only a missing message takes the placeholder, and only a non-empty object reaches it"
        );
    }

    #[test]
    fn the_view_prints_the_sentence_for_both_empty_spellings() {
        assert_eq!(banner_view(&json!({})), Report::Text(NO_BANNER.to_owned()));
        assert_eq!(banner_view(&json!("")), Report::Text(NO_BANNER.to_owned()));
    }

    #[test]
    fn the_set_body_is_the_modules_value_object() {
        assert_eq!(
            set_body("Maintenance tonight", None, None),
            json!({"value": {
                "message": "Maintenance tonight",
                "type": "info",
                "level": "projectCollection"
            }})
        );
        assert_eq!(
            set_body(
                "Heads up",
                Some("warning".to_owned()),
                Some("project".to_owned())
            ),
            json!({"value": {
                "message": "Heads up",
                "type": "warning",
                "level": "project"
            }})
        );
        assert_eq!(
            set_body("Heads up", Some(String::new()), Some(String::new()))["value"]["type"],
            json!(""),
            "a present empty option is not the absent default"
        );
    }

    #[test]
    fn resolve_message_speaks_the_three_forms() {
        assert_eq!(resolve_message("").expect("empty"), "");
        assert_eq!(resolve_message("a literal").expect("literal"), "a literal");
    }

    #[test]
    fn resolve_message_reads_an_at_file_trimmed() {
        let path = std::env::temp_dir().join(format!(
            "ado-banner-message-{}-{}.txt",
            std::process::id(),
            line!()
        ));
        std::fs::write(&path, "  From a file.\n\n").expect("write the message file");

        let resolved = resolve_message(&format!("@{}", path.display())).expect("the file");

        std::fs::remove_file(&path).expect("remove the message file");

        assert_eq!(
            resolved, "From a file.",
            "the connections convention trims both ends"
        );
    }

    #[test]
    fn resolve_message_reports_a_missing_file_with_its_prefix() {
        let error = resolve_message("@nope.txt").expect_err("missing file");

        assert_eq!(error.code, ErrorCode::ValidationError);
        assert!(
            error
                .message
                .starts_with("Cannot read message file \"nope.txt\": "),
            "the connections-convention wording, naming this command's noun: {}",
            error.message
        );
    }
}
