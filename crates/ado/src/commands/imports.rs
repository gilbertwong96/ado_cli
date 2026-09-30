//! `ado imports list|show|create` — the whole of `lib/ado_cli/cli/imports.ex`:
//! the project-scoped `_apis/git/importRequests` surface, the `$top` pair, the
//! create body and the two views.
//!
//! Four captured shapes decide the code:
//!
//!   * `--top` is the module's `if t = Map.get(parsed.options, :top)`, so `0` is
//!     a present option and sends `$top=0`;
//!   * `create`'s `gitSource` carries `url` plus `user`/`password` **whenever the
//!     option is present** — `""` is truthy in Elixir, so `--user ''` sends
//!     `"user":""` where an absent option sends nothing;
//!   * `create` prints its human block **even under `--json`** in the oracle
//!     (D33); this build answers the value envelope there and the same block in
//!     human mode;
//!   * `show` carries the module's own 404 wording (`Import '<id>' not found`) on
//!     stderr with no envelope (D4), where `list`'s and `create`'s errors are the
//!     classified envelope.
//!
//! The frozen table's fixed 38/12 padding is its own rendering; this build's
//! `Report::Table` is §8 surface (the `extensions`/`connections` precedent).

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Map, Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The project-scoped collection every path here builds on.
const IMPORT_REQUESTS_PATH: &str = "/_apis/git/importRequests";

/// `ado imports list PROJECT [--top N]`: `GET …/git/importRequests`, with the
/// module's `$top` pair only when the option is present.
pub fn list(context: &mut Context, project: &str, top: Option<i64>) -> Result<Report, AdoError> {
    let imports = items(
        context
            .client()?
            .list(&collection_path(project), &top_params(top))?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(imports.clone())), || {
            imports_table(&imports)
        }),
    )
}

/// `ado imports show PROJECT IMPORT_ID`: `GET …/importRequests/{id}`. A 404 takes
/// the module's own wording ([`ErrorCode::NotFound`]'s class), which the frozen
/// CLI writes to stderr with no envelope even under `--json` (D4).
pub fn show(context: &mut Context, project: &str, import_id: &str) -> Result<Report, AdoError> {
    match context.client()?.get(&import_path(project, import_id), &[]) {
        Ok(import) => Ok(context.json_or_report(ok_value(import.clone()), || {
            Report::Text(import_detail(&import))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Import '{import_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado imports create PROJECT REPO_NAME --url URL [--user U] [--password P]`:
/// `POST …/git/repositories/{repo}/importRequests` with the module's two-level
/// body, then its success block. Every failure is the classified envelope — the
/// module has no wording of its own here.
pub fn create(
    context: &mut Context,
    project: &str,
    repo_name: &str,
    url: &str,
    user: Option<String>,
    password: Option<String>,
) -> Result<Report, AdoError> {
    let import = context.client()?.post(
        &repository_path(project, repo_name),
        &create_body(url, user.as_deref(), password.as_deref()),
        &[],
    )?;

    Ok(context.json_or_report(ok_value(import.clone()), || {
        Report::Text(created_block(project, &import))
    }))
}

/// The module's `"/#{URI.encode(project)}/_apis/git/importRequests"`; the frozen
/// `URI.encode/1` leaves a `/` raw and this build escapes every segment strictly
/// (D22).
fn collection_path(project: &str) -> String {
    format!("/{}{IMPORT_REQUESTS_PATH}", encode_path_segment(project))
}

/// One import request below the collection, encoded the same way.
fn import_path(project: &str, import_id: &str) -> String {
    format!(
        "{}/{}",
        collection_path(project),
        encode_path_segment(import_id)
    )
}

/// The create target: the new repository's own `importRequests` collection.
fn repository_path(project: &str, repo_name: &str) -> String {
    format!(
        "/{}/_apis/git/repositories/{}/importRequests",
        encode_path_segment(project),
        encode_path_segment(repo_name)
    )
}

/// The module's `if t = Map.get(parsed.options, :top), do: %{"$top" => t}`:
/// absent is no pair, and `0` is present.
fn top_params(top: Option<i64>) -> Vec<(String, String)> {
    top.map(|top| vec![("$top".to_owned(), top.to_string())])
        .unwrap_or_default()
}

/// `create_import/1`'s body, verbatim: the `parameters` wrapper, the
/// `deleteServiceEndpointAfterImportIsDone` flag and the `gitSource` object.
fn create_body(url: &str, user: Option<&str>, password: Option<&str>) -> Value {
    let mut git_source = Map::new();

    git_source.insert("url".to_owned(), json!(url));

    // The module's `if user, do: Map.put(...)`: an absent option is `nil` and
    // falsy, a present empty one is `""` and truthy.
    if let Some(user) = user {
        git_source.insert("user".to_owned(), json!(user));
    }

    if let Some(password) = password {
        git_source.insert("password".to_owned(), json!(password));
    }

    json!({
        "parameters": {
            "gitSource": Value::Object(git_source),
            "deleteServiceEndpointAfterImportIsDone": false,
        }
    })
}

/// `print_imports_table/1`'s columns (ID, Status, Source), with the module's
/// "No imports found." when empty.
fn imports_table(imports: &[Value]) -> Report {
    if imports.is_empty() {
        return Report::Text("No imports found.".to_owned());
    }

    let rows = imports
        .iter()
        .map(|import| {
            vec![
                value_text(import.get("id")),
                value_text(import.get("status")),
                source_url(import),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Status".to_owned(), "Source".to_owned()],
        rows,
    }
}

/// `create_import/1`'s success block: the response's three fields and the
/// follow-up line naming the project and the new id.
fn created_block(project: &str, import: &Value) -> String {
    let id = value_text(import.get("id"));

    format!(
        concat!(
            "\nImport request created.\n",
            "  ID:        {}\n",
            "  Status:    {}\n",
            "  URL:       {}\n",
            "\n",
            "Check status with:\n",
            "  ado imports show {} {}\n",
        ),
        id,
        value_text(import.get("status")),
        value_text(import.get("url")),
        project,
        id,
    )
}

/// `print_import_detail/1`: the module's rule and labelled fields. The blank line
/// after the green header is the module's `success("Import Status\n")` reset
/// artefact, which the capture shows as a line of its own.
fn import_detail(import: &Value) -> String {
    let mut detail = format!(
        concat!(
            "\nImport Status\n\n",
            "{}\n",
            "  ID:     {}\n",
            "  Status: {}\n",
            "  Source: {}\n",
        ),
        "-".repeat(60),
        value_text(import.get("id")),
        value_text(import.get("status")),
        source_url(import),
    );

    // The module's `if imp["detailedStatus"]`: the line is absent when the member
    // is (not null — absent), and its value is `errorMessage || allStepsSucceeded`.
    if let Some(status) = import.get("detailedStatus") {
        detail.push_str(&format!("  Detail: {}\n", detail_value(status)));
    }

    detail.push_str(&format!(
        "  URL:    {}\n",
        import
            .get("url")
            .map(value_text_owned)
            .unwrap_or_else(|| "(none)".to_owned()),
    ));

    detail
}

/// The module's `detailedStatus["errorMessage"] || detailedStatus["allStepsSucceeded"]`:
/// a nil `errorMessage` falls through to the boolean, which prints `true`/`false`.
fn detail_value(status: &Value) -> String {
    let error_message = status.get("errorMessage");

    match error_message {
        Some(Value::Null) | None => value_text(status.get("allStepsSucceeded")),
        Some(message) => value_text_owned(message),
    }
}

/// The module's `(i["parameters"] && i["parameters"]["gitSource"] && …["url"]) || ""`.
fn source_url(import: &Value) -> String {
    import
        .get("parameters")
        .and_then(|parameters| parameters.get("gitSource"))
        .and_then(|git_source| git_source.get("url"))
        .map(value_text_owned)
        .unwrap_or_default()
}

/// Elixir's `#{term}` interpolation for the JSON scalars these fields carry.
fn value_text(value: Option<&Value>) -> String {
    value.map(value_text_owned).unwrap_or_default()
}

fn value_text_owned(value: &Value) -> String {
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

    fn imports() -> Vec<Value> {
        json!([
            {"id": "imp-1", "status": "completed",
             "parameters": {"gitSource": {"url": "https://github.com/owner/repo.git"}},
             "detailedStatus": {"allStepsSucceeded": true, "errorMessage": null}},
            {"id": "imp-2", "status": "failed"},
            {"id": "imp-3", "status": "queued",
             "parameters": {"gitSource": {}}}
        ])
        .as_array()
        .expect("an array")
        .clone()
    }

    #[test]
    fn the_collection_path_escapes_the_project_as_one_segment() {
        assert_eq!(collection_path("Alpha"), "/Alpha/_apis/git/importRequests");
        assert_eq!(
            collection_path("a/b"),
            "/a%2Fb/_apis/git/importRequests",
            "the frozen URI.encode/1 leaves the slash raw (D22)"
        );
        assert_eq!(
            collection_path("Alpha Beta"),
            "/Alpha%20Beta/_apis/git/importRequests",
            "the one spelling both sides share"
        );
    }

    #[test]
    fn the_import_and_repository_paths_escape_their_segments() {
        assert_eq!(
            import_path("Alpha", "imp 1"),
            "/Alpha/_apis/git/importRequests/imp%201"
        );
        assert_eq!(
            repository_path("Alpha", "a/b"),
            "/Alpha/_apis/git/repositories/a%2Fb/importRequests",
            "the frozen module interpolates the repo name raw (D22)"
        );
    }

    #[test]
    fn the_top_pair_is_present_exactly_when_the_option_is() {
        assert_eq!(
            top_params(Some(1)),
            vec![("$top".to_owned(), "1".to_owned())]
        );
        assert_eq!(
            top_params(Some(0)),
            vec![("$top".to_owned(), "0".to_owned())],
            "zero is truthy in the module's `if t = Map.get(...)`"
        );
        assert_eq!(top_params(None), Vec::<(String, String)>::new());
    }

    #[test]
    fn the_create_body_carries_the_two_levels_and_the_flag() {
        assert_eq!(
            create_body("https://github.com/owner/repo.git", None, None),
            json!({"parameters": {
                "gitSource": {"url": "https://github.com/owner/repo.git"},
                "deleteServiceEndpointAfterImportIsDone": false
            }})
        );
        assert_eq!(
            create_body("u", Some("octocat"), Some("ghp_x"))["parameters"]["gitSource"],
            json!({"url": "u", "user": "octocat", "password": "ghp_x"})
        );
    }

    #[test]
    fn the_create_body_keeps_a_present_empty_credential() {
        assert_eq!(
            create_body("u", Some(""), Some(""))["parameters"]["gitSource"],
            json!({"url": "u", "user": "", "password": ""}),
            "`if user` is truthy for \"\" in Elixir (captured: imp-create-empty-user-pass)"
        );
    }

    #[test]
    fn the_table_carries_the_modules_columns_and_the_empty_sentence() {
        assert_eq!(
            imports_table(&[]),
            Report::Text("No imports found.".to_owned())
        );

        let Report::Table { headers, rows } = imports_table(&imports()) else {
            panic!("a non-empty list is a table");
        };

        assert_eq!(headers, ["ID", "Status", "Source"]);
        assert_eq!(
            rows[0],
            ["imp-1", "completed", "https://github.com/owner/repo.git"]
        );
        assert_eq!(rows[1], ["imp-2", "failed", ""]);
        assert_eq!(rows[2], ["imp-3", "queued", ""]);
    }

    #[test]
    fn the_source_reads_the_nested_url_and_falls_back_to_empty() {
        assert_eq!(source_url(&json!({})), "");
        assert_eq!(source_url(&json!({"parameters": {}})), "");
        assert_eq!(source_url(&json!({"parameters": {"gitSource": {}}})), "");
        assert_eq!(
            source_url(&json!({"parameters": {"gitSource": {"url": "u"}}})),
            "u"
        );
    }

    #[test]
    fn the_detail_is_the_modules_layout_with_its_blank_line() {
        assert_eq!(
            import_detail(&json!({
                "id": "imp-1", "status": "inProgress",
                "url": "https://example.test/imp-1",
                "parameters": {"gitSource": {"url": "https://github.com/owner/repo.git"}},
                "detailedStatus": {"allStepsSucceeded": false, "errorMessage": null}
            })),
            format!(
                concat!(
                    "\nImport Status\n\n",
                    "{}\n",
                    "  ID:     imp-1\n",
                    "  Status: inProgress\n",
                    "  Source: https://github.com/owner/repo.git\n",
                    "  Detail: false\n",
                    "  URL:    https://example.test/imp-1\n"
                ),
                "-".repeat(60)
            )
        );
    }

    #[test]
    fn the_detail_omits_the_line_without_a_detailed_status() {
        let detail = import_detail(&json!({"id": "imp-2", "status": "queued"}));

        assert!(!detail.contains("Detail:"), "{detail}");
        assert!(detail.contains("  URL:    (none)\n"), "{detail}");
    }

    #[test]
    fn the_detail_prefers_a_real_error_message() {
        assert_eq!(
            detail_value(&json!({"errorMessage": "TF401019: failed", "allStepsSucceeded": false})),
            "TF401019: failed"
        );
        assert_eq!(
            detail_value(&json!({"errorMessage": null, "allStepsSucceeded": true})),
            "true"
        );
        assert_eq!(detail_value(&json!({"allStepsSucceeded": true})), "true");
    }

    #[test]
    fn the_created_block_names_the_project_and_the_new_id() {
        assert_eq!(
            created_block(
                "Alpha",
                &json!({"id": "imp-new", "status": "queued", "url": "https://example.test/imp-new"})
            ),
            concat!(
                "\nImport request created.\n",
                "  ID:        imp-new\n",
                "  Status:    queued\n",
                "  URL:       https://example.test/imp-new\n",
                "\n",
                "Check status with:\n",
                "  ado imports show Alpha imp-new\n"
            )
        );
    }
}
