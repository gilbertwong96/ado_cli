//! `ado extensions list|show|install|uninstall|enable|disable` — the whole of
//! `lib/ado_cli/cli/extensions.ex`: the organization-scoped
//! `_apis/extensionmanagement/installedextensions` surface, the client-side
//! `--search` filter, the four writes' bodies and the two views.
//!
//! The surface lives on Azure's `extmgmt` hub, so a cloud request is addressed to
//! `{org}.extmgmt.visualstudio.com` — the frozen CLI sent it to the org host and
//! every command answered 404 against a live organization
//! (`w4-live-org-findings.md` F4, the divergence D57). The surface is also
//! preview-only: the API answers 400 `VssInvalidPreviewVersionException` for
//! `api-version=7.1`, so F4's second half is still open.
//!
//! Four captured shapes decide the code:
//!
//!   * `--search` filters **client-side** on `extensionName` alone, as a
//!     case-insensitive substring (`--search mspremier`, the publisher, answers
//!     the empty list; an empty `--search` is a present option and keeps every
//!     row, because `""` is truthy in Elixir);
//!   * `install` posts exactly `{"publisherId":…,"extensionName":…}` and
//!     `enable`/`disable` PATCH the response path with
//!     `{"publisherId":…,"extensionName":…,"installState":{"flags":"none"|"disabled"}}`;
//!   * every write prints its human line **even under `--json`** in the oracle
//!     (D33); this build answers the message envelope under `--json` and the same
//!     sentence in human mode;
//!   * `show` and `uninstall` carry the module's own 404 wording
//!     (`Extension '<id>' not found`) on stderr with no envelope (D4), where
//!     `enable`/`disable` have no wording of their own — their errors are the
//!     classified envelope.
//!
//! The frozen table's fixed 45/12 padding and its `String.slice(version, 0, 10)`
//! are its own rendering; this build's `Report::Table` is §8 surface and keeps
//! the full version (the `branch-policies`/`iterations` precedent).
//!
//! One malformed-body guard is deliberate: the module's `e["installState"]
//! ["flags"]` raises when `installState` is missing, and this build reads that as
//! `enabled` (the defensive style `test_coverage`'s `coverageData` read records;
//! no capture covers it).

use ado_core::client::{Hub, encode_path_segment};
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The organization-scoped collection every path here builds on; the client
/// injects the organization ahead of it.
/// The preview `api-version` this surface requires: it is preview-only and
/// answers 400 `VssInvalidPreviewVersionException` for a plain `7.1`
/// (`w4-live-org-findings.md` F4's second half).
const PREVIEW_API_VERSION: &str = "7.1-preview.1";

/// The preview pair each call below sends; the client merges a caller's
/// `api-version` over its own default.
fn preview_params() -> Vec<(String, String)> {
    vec![("api-version".to_owned(), PREVIEW_API_VERSION.to_owned())]
}

const EXTENSIONS_PATH: &str = "/_apis/extensionmanagement/installedextensions";

/// `ado extensions list [--search SEARCH]`: `GET …/installedextensions`. The
/// value array unwraps to the value envelope after the module's client-side
/// filter; the human path is the module's three columns.
pub fn list(context: &mut Context, search: Option<String>) -> Result<Report, AdoError> {
    let extensions = items(
        context
            .client()?
            .hub(Hub::Extensions)
            .list(EXTENSIONS_PATH, &preview_params())?,
    );
    let filtered = filter_by_search(extensions, search.as_deref());

    Ok(
        context.json_or_report(ok_value(Value::Array(filtered.clone())), || {
            extensions_table(&filtered)
        }),
    )
}

/// `ado extensions show EXTENSION_ID`: `GET …/installedextensions/{id}`. A 404
/// takes the module's own wording ([`ErrorCode::NotFound`]'s class), which the
/// frozen CLI writes to stderr with no envelope even under `--json` (D4).
pub fn show(context: &mut Context, extension_id: &str) -> Result<Report, AdoError> {
    match context
        .client()?
        .hub(Hub::Extensions)
        .get(&extension_path(extension_id), &preview_params())
    {
        Ok(extension) => Ok(context.json_or_report(ok_value(extension.clone()), || {
            Report::Text(extension_detail(&extension))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Extension '{extension_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado extensions install --publisher P --name N`: `POST …/installedextensions`
/// with the module's two-field body, then its success line.
pub fn install(context: &mut Context, publisher: &str, name: &str) -> Result<Report, AdoError> {
    context.client()?.hub(Hub::Extensions).post(
        EXTENSIONS_PATH,
        &install_body(publisher, name),
        &preview_params(),
    )?;

    Ok(success_line(
        context,
        format!("Extension '{}' installed.", dotted_id(publisher, name)),
    ))
}

/// `ado extensions uninstall --publisher P --name N`:
/// `DELETE …/installedextensions/{P}.{N}`. A 404 keeps the module's own wording
/// (no envelope); every other failure is the classified envelope.
pub fn uninstall(context: &mut Context, publisher: &str, name: &str) -> Result<Report, AdoError> {
    let id = dotted_id(publisher, name);

    match context
        .client()?
        .hub(Hub::Extensions)
        .delete(&dotted_path(publisher, name), &preview_params())
    {
        Ok(()) => Ok(success_line(
            context,
            format!("Extension '{id}' uninstalled."),
        )),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Extension '{id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado extensions enable --publisher P --name N`:
/// `PATCH …/installedextensions/{P}.{N}` with the `"flags":"none"` body.
pub fn enable(context: &mut Context, publisher: &str, name: &str) -> Result<Report, AdoError> {
    patch_install_state(context, publisher, name, "none", "enabled")
}

/// `ado extensions disable --publisher P --name N`: the same PATCH with
/// `"flags":"disabled"`.
pub fn disable(context: &mut Context, publisher: &str, name: &str) -> Result<Report, AdoError> {
    patch_install_state(context, publisher, name, "disabled", "disabled")
}

/// The shared half of `enable`/`disable`: the module's `installState` body, then
/// the labelled success line.
fn patch_install_state(
    context: &mut Context,
    publisher: &str,
    name: &str,
    flags: &str,
    verb: &str,
) -> Result<Report, AdoError> {
    context.client()?.hub(Hub::Extensions).patch(
        &dotted_path(publisher, name),
        &install_state_body(publisher, name, flags),
        &preview_params(),
    )?;

    Ok(success_line(
        context,
        format!("Extension '{}' {verb}.", dotted_id(publisher, name)),
    ))
}

/// `install_extension/1`'s body, verbatim.
fn install_body(publisher: &str, name: &str) -> Value {
    json!({"publisherId": publisher, "extensionName": name})
}

/// `enable_extension/1`/`disable_extension/1`'s body, verbatim.
fn install_state_body(publisher: &str, name: &str, flags: &str) -> Value {
    json!({
        "publisherId": publisher,
        "extensionName": name,
        "installState": {"flags": flags},
    })
}

/// One success line: the message envelope under `--json`, the module's sentence
/// in human mode (the oracle prints the human line in both modes; D33).
fn success_line(context: &Context, message: String) -> Report {
    context.json_or_report(ok_message(&message), || Report::Text(message))
}

/// The module's `#{publisherId}.#{extensionName}` id every write names.
fn dotted_id(publisher: &str, name: &str) -> String {
    format!("{publisher}.{name}")
}

/// The extension collection below the organization; the frozen module calls
/// `URI.encode/1` on the id and this build escapes the segment strictly (D22).
fn extension_path(extension_id: &str) -> String {
    format!("{EXTENSIONS_PATH}/{}", encode_path_segment(extension_id))
}

/// The dotted id as one path segment: the frozen module interpolates it raw, so
/// a `/` becomes a path separator there; this build escapes it (D22).
fn dotted_path(publisher: &str, name: &str) -> String {
    format!(
        "{EXTENSIONS_PATH}/{}",
        encode_path_segment(&dotted_id(publisher, name))
    )
}

/// `list_extensions/1`'s `Enum.filter/2`: only when `--search` is given, a
/// case-insensitive substring of `extensionName` (`""` matches everything, and
/// a missing name reads as `""`, the module's `|| ""`).
fn filter_by_search(extensions: Vec<Value>, search: Option<&str>) -> Vec<Value> {
    let Some(search) = search else {
        return extensions;
    };
    let needle = search.to_lowercase();

    extensions
        .into_iter()
        .filter(|extension| {
            extension_name(extension)
                .to_lowercase()
                .contains(needle.as_str())
        })
        .collect()
}

/// The module's `e["extensionName"] || ""`.
fn extension_name(extension: &Value) -> &str {
    extension
        .get("extensionName")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// `print_extensions_table/1`'s columns (Publisher.Name, Version, State), with
/// the module's "No extensions found." when empty.
fn extensions_table(extensions: &[Value]) -> Report {
    if extensions.is_empty() {
        return Report::Text("No extensions found.".to_owned());
    }

    let rows = extensions
        .iter()
        .map(|extension| {
            vec![
                format!(
                    "{}.{}",
                    value_text(extension.get("publisherId")),
                    extension_name(extension),
                ),
                value_text(extension.get("version")),
                state_text(extension),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "Publisher.Name".to_owned(),
            "Version".to_owned(),
            "State".to_owned(),
        ],
        rows,
    }
}

/// The module's `if e["installState"]["flags"] == "disabled", do: "disabled",
/// else: "enabled"`; a missing `installState` reads `enabled` where the frozen
/// access would raise.
fn state_text(extension: &Value) -> String {
    let disabled = extension
        .get("installState")
        .and_then(|state| state.get("flags"))
        .and_then(Value::as_str)
        == Some("disabled");

    if disabled {
        "disabled".to_owned()
    } else {
        "enabled".to_owned()
    }
}

/// The module's `print_extension_detail/1`: the `─` rule and the four labelled
/// fields. The blank line after the green header is the module's
/// `success("Extension Details\n")` reset artefact, which the capture shows as a
/// line of its own (the `connections` detail has the same shape).
fn extension_detail(extension: &Value) -> String {
    format!(
        concat!(
            "\nExtension Details\n\n",
            "{}\n",
            "  Publisher: {}\n",
            "  Name:      {}\n",
            "  Version:   {}\n",
            "  State:     {}\n",
            "\n",
        ),
        "─".repeat(60),
        value_text(extension.get("publisherId")),
        extension_name(extension),
        value_text(extension.get("version")),
        state_text(extension),
    )
}

/// Elixir's `#{term}` interpolation for the JSON scalars these fields carry.
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

    fn extensions() -> Vec<Value> {
        json!([
            {"extensionId": "mspremier-bqc", "extensionName": "BuildQualityChecks",
             "publisherId": "mspremier", "version": "3.1.20240912.7",
             "installState": {"flags": "none"}},
            {"extensionName": "octopus-deploy", "publisherId": "octopus",
             "version": "1.2.3", "installState": {"flags": "disabled"}},
            {"extensionName": "SonarQube", "publisherId": "sonarsource",
             "version": "2.0.0", "installState": {"flags": "none"}}
        ])
        .as_array()
        .expect("an array")
        .clone()
    }

    #[test]
    fn the_show_path_encodes_the_id_as_one_segment() {
        assert_eq!(
            extension_path("mspremier.BuildQualityChecks"),
            "/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks"
        );
        assert_eq!(
            extension_path("pub/name"),
            "/_apis/extensionmanagement/installedextensions/pub%2Fname",
            "the frozen URI.encode/1 leaves the slash raw (D22)"
        );
        assert_eq!(
            extension_path("pub name"),
            "/_apis/extensionmanagement/installedextensions/pub%20name",
            "the one spelling both sides share"
        );
    }

    #[test]
    fn the_dotted_path_escapes_the_whole_id_as_one_segment() {
        assert_eq!(
            dotted_path("mspremier", "BuildQualityChecks"),
            "/_apis/extensionmanagement/installedextensions/mspremier.BuildQualityChecks"
        );
        assert_eq!(
            dotted_path("pub", "a/b"),
            "/_apis/extensionmanagement/installedextensions/pub.a%2Fb",
            "the frozen module interpolates the id raw, so the slash splits the path there (D22)"
        );
        assert_eq!(
            dotted_path("pub name", "x y"),
            "/_apis/extensionmanagement/installedextensions/pub%20name.x%20y",
            "the frozen module cannot send a space at all (Finch refuses the target)"
        );
    }

    #[test]
    fn the_search_filters_the_extension_name_case_insensitively() {
        assert_eq!(
            filter_by_search(extensions(), Some("build"))
                .iter()
                .map(extension_name)
                .collect::<Vec<_>>(),
            ["BuildQualityChecks"]
        );
        assert_eq!(
            filter_by_search(extensions(), Some("OCTOPUS"))
                .iter()
                .map(extension_name)
                .collect::<Vec<_>>(),
            ["octopus-deploy"]
        );
    }

    #[test]
    fn the_search_reads_only_the_name_and_an_empty_value_keeps_everything() {
        assert_eq!(
            filter_by_search(extensions(), Some("mspremier")),
            Vec::<Value>::new(),
            "the publisher is not searched"
        );
        assert_eq!(
            filter_by_search(extensions(), Some("")).len(),
            3,
            "a present empty option matches everything, as the module's `contains?` does"
        );
        assert_eq!(
            filter_by_search(extensions(), None).len(),
            3,
            "no option filters nothing"
        );
    }

    #[test]
    fn the_search_reads_a_missing_name_as_the_empty_string() {
        let nameless = vec![json!({"publisherId": "pub"})];

        assert_eq!(filter_by_search(nameless.clone(), Some("")), nameless);
        assert_eq!(filter_by_search(nameless, Some("pub")), Vec::<Value>::new());
    }

    #[test]
    fn the_empty_list_keeps_the_modules_sentence() {
        assert_eq!(
            extensions_table(&[]),
            Report::Text("No extensions found.".to_owned())
        );
    }

    #[test]
    fn the_table_carries_the_modules_columns_and_the_full_version() {
        let Report::Table { headers, rows } = extensions_table(&extensions()) else {
            panic!("a non-empty list is a table");
        };

        assert_eq!(headers, ["Publisher.Name", "Version", "State"]);
        assert_eq!(
            rows[0],
            ["mspremier.BuildQualityChecks", "3.1.20240912.7", "enabled"]
        );
        assert_eq!(rows[1], ["octopus.octopus-deploy", "1.2.3", "disabled"]);
    }

    #[test]
    fn the_state_reads_disabled_only_for_the_disabled_flag() {
        assert_eq!(
            state_text(&json!({"installState": {"flags": "disabled"}})),
            "disabled"
        );
        assert_eq!(
            state_text(&json!({"installState": {"flags": "none"}})),
            "enabled"
        );
        assert_eq!(state_text(&json!({"installState": {}})), "enabled");
        assert_eq!(
            state_text(&json!({})),
            "enabled",
            "the frozen access raises on a missing installState; this build reads enabled"
        );
    }

    #[test]
    fn the_detail_is_the_modules_layout() {
        assert_eq!(
            extension_detail(&extensions()[1]),
            format!(
                concat!(
                    "\nExtension Details\n\n",
                    "{}\n",
                    "  Publisher: octopus\n",
                    "  Name:      octopus-deploy\n",
                    "  Version:   1.2.3\n",
                    "  State:     disabled\n",
                    "\n",
                ),
                "─".repeat(60)
            )
        );
    }

    #[test]
    fn the_install_body_is_the_modules_two_fields() {
        assert_eq!(
            install_body("mspremier", "BuildQualityChecks"),
            json!({"publisherId": "mspremier", "extensionName": "BuildQualityChecks"})
        );
    }

    #[test]
    fn the_patch_body_carries_the_install_state_flags() {
        assert_eq!(
            install_state_body("mspremier", "BuildQualityChecks", "none"),
            json!({
                "publisherId": "mspremier",
                "extensionName": "BuildQualityChecks",
                "installState": {"flags": "none"},
            })
        );
        assert_eq!(
            install_state_body("mspremier", "BuildQualityChecks", "disabled")["installState"]["flags"],
            json!("disabled")
        );
    }
}
