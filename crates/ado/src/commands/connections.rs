//! `ado connections list|show|create|update|delete` — the whole of
//! `lib/ado_cli/cli/connections.ex`: the project-scoped
//! `_apis/serviceendpoint/endpoints` surface, its three writes' bodies, the two
//! views and the wave's fourth confirmation (`delete`, D30–D32).
//!
//! Three captured shapes decide the code:
//!
//!   * `create` takes `name`, `type` and `url` **positionally** — the
//!     moduledoc's `--name NAME --type TYPE --url URL` is not the parser's argv
//!     (the oracle refuses all three option spellings), while `update` keeps
//!     `--name`/`--description`/`--url` as options;
//!   * `--data` is **nested under `"data"`**, not merged into the body — the
//!     frozen help text's "merged into the request body" and "reserved keys"
//!     described a merge that does not happen (`connections_test.exs` pins the
//!     nesting: `decoded["data"]["subscriptionId"]`). Ruling B6 corrected this
//!     build's help line to the nesting; the frozen help still lies and Wave 4's
//!     docs rewrite owns that side (D55).
//!   * the success line names the **response's** `name` on `update` (a rename to
//!     `Renamed` answers `Service connection 'GitHub' updated.`) and the
//!     positional id on `delete`.
//!
//! `--type` reaches the wire as a query pair (`type=github`), not a client-side
//! filter, and `--type ''` sends the empty pair. The `--access-token` forms are
//! the module's `resolve_token/1`: a literal, `-` (stdin, trimmed) or `@path`
//! (trimmed); `''` is absent. Every write prints its human success line **even
//! under `--json`** in the oracle (D33); this build answers with the value
//! envelope for `create`/`update` and the message envelope for `delete`.

use std::fs;
use std::io::Read;

use ado_core::client::encode_path_segment;
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Map, Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The project-scoped collection every path here builds on; the module's
/// `"/#{URI.encode(project)}/_apis/serviceendpoint/endpoints"`.
const ENDPOINTS_PATH: &str = "/_apis/serviceendpoint/endpoints";

/// The helper's refusal wording (`Helpers.confirm_delete/2`'s
/// `halt_error("Aborted.")`); it is this build's §8 wording, printed on stderr.
const ABORTED: &str = "Aborted.";

/// `merge_data/2`'s bad-JSON wording, verbatim.
const DATA_INVALID: &str =
    "--data is not valid JSON. Pass an object, e.g. '{\"subscriptionId\":\"...\"}'";

/// `merge_data/2`'s non-object wording, verbatim.
const DATA_NOT_OBJECT: &str = "--data must be a JSON object, e.g. '{\"subscriptionId\":\"...\"}'";

/// `update_connection/1`'s empty-body guard, verbatim.
const UPDATE_GUARD: &str =
    "At least one of --name, --description, --url, --access-token, or --data is required.";

/// The scheme the module sends when `--scheme` is absent
/// (`Map.get(parsed.options, :scheme, "Token")`).
const DEFAULT_SCHEME: &str = "Token";

/// `create`'s options as clap parses them (`name`/`type`/`url` are positionals).
pub struct CreateOptions {
    pub description: Option<String>,
    pub scheme: Option<String>,
    pub access_token: Option<String>,
    pub data: Option<String>,
    pub ready: bool,
}

/// `update`'s options as clap parses them; an all-absent set is the module's
/// empty-body guard.
pub struct UpdateOptions {
    pub name: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub access_token: Option<String>,
    pub data: Option<String>,
}

/// `ado connections list PROJECT [--type TYPE]`:
/// `GET /{project}/_apis/serviceendpoint/endpoints`, `type` only when `--type` is
/// given (an empty value is a present option and reaches the wire as `type=`).
pub fn list(
    context: &mut Context,
    project: &str,
    conn_type: Option<String>,
) -> Result<Report, AdoError> {
    let connections = items(
        context
            .client()?
            .list(&endpoints_path(project), &list_params(conn_type))?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(connections.clone())), || {
            connections_table(&connections)
        }),
    )
}

/// `ado connections show PROJECT CONNECTION_ID`: `GET …/endpoints/{id}`. A 404 is
/// the module's own wording on stderr, no envelope even under `--json` (D4).
pub fn show(context: &mut Context, project: &str, conn_id: &str) -> Result<Report, AdoError> {
    match context.client()?.get(&endpoint_path(project, conn_id), &[]) {
        Ok(connection) => Ok(context.json_or_report(ok_value(connection.clone()), || {
            Report::Text(connection_detail(&connection))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Service connection '{conn_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado connections create PROJECT NAME TYPE URL [options]`:
/// `POST …/endpoints` with the module's body. The token is resolved and the data
/// parsed **before** the request, in that order, so a bad token file reports
/// before bad `--data`. A 404 is `Project '<project>' not found` (no envelope).
pub fn create(
    context: &mut Context,
    project: &str,
    name: &str,
    conn_type: &str,
    url: &str,
    options: CreateOptions,
) -> Result<Report, AdoError> {
    let authorization = authorization(
        options.scheme.as_deref(),
        resolve_token(options.access_token)?,
    );
    let mut body = Map::new();

    body.insert("name".to_owned(), json!(name));
    body.insert("type".to_owned(), json!(conn_type));
    body.insert("url".to_owned(), json!(url));
    body.insert("authorization".to_owned(), authorization);
    body.insert("isReady".to_owned(), json!(options.ready));
    body.insert(
        "serviceEndpointProjectReferences".to_owned(),
        json!([{"projectReference": {"name": project}, "name": name}]),
    );
    put_if_present(&mut body, options.description, "description");
    merge_data(&mut body, options.data)?;

    match context
        .client()?
        .post(&endpoints_path(project), &Value::Object(body), &[])
    {
        Ok(connection) => Ok(context.json_or_report(ok_value(connection.clone()), || {
            Report::Text(created_line(&connection))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Project '{project}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado connections update PROJECT CONNECTION_ID [options]`: `PUT …/endpoints/{id}`
/// with only the options given. An empty body is the module's guard, on stderr,
/// before any request; a 404 keeps the module's wording (no envelope).
pub fn update(
    context: &mut Context,
    project: &str,
    conn_id: &str,
    options: UpdateOptions,
) -> Result<Report, AdoError> {
    let mut body = Map::new();

    put_if_present(&mut body, options.name, "name");
    put_if_present(&mut body, options.description, "description");
    put_if_present(&mut body, options.url, "url");

    // The module reads a (never present) existing authorization object and falls
    // back to the Token scheme; `merge_data/2` runs after this, so no other path
    // can have set one first.
    if let Some(token) = resolve_token(options.access_token)? {
        body.insert(
            "authorization".to_owned(),
            authorization(Some(DEFAULT_SCHEME), Some(token)),
        );
    }

    merge_data(&mut body, options.data)?;

    if body.is_empty() {
        return Err(AdoError::validation(UPDATE_GUARD));
    }

    match context
        .client()?
        .put(&endpoint_path(project, conn_id), &Value::Object(body), &[])
    {
        Ok(connection) => Ok(context.json_or_report(ok_value(connection.clone()), || {
            Report::Text(updated_line(&connection))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Service connection '{conn_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado connections delete PROJECT CONNECTION_ID [--force]`: the wave's fourth
/// confirmation — asked before any credential is resolved or request is built —
/// then `DELETE …/endpoints/{id}`. `--force` skips the question; a "no" or EOF
/// returns the refusal, which exits 1 and sends nothing (D30/D32).
pub fn delete(
    context: &mut Context,
    project: &str,
    conn_id: &str,
    force: bool,
) -> Result<Report, AdoError> {
    if !force && !context.confirm(&delete_question(project, conn_id)) {
        return Err(AdoError::cancelled(ABORTED));
    }

    match context
        .client()?
        .delete(&endpoint_path(project, conn_id), &[])
    {
        Ok(()) => {
            let message = format!("Service connection '{conn_id}' deleted from '{project}'.");

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Service connection '{conn_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `Helpers.confirm_delete("service connection", "{project}/{conn_id}")`'s
/// question, verbatim: the command owns it, so the seam hard-codes no single
/// question.
fn delete_question(project: &str, conn_id: &str) -> String {
    format!("Delete service connection '{project}/{conn_id}'? This cannot be undone. [y/N] ")
}

/// The endpoints collection of one project; the frozen module encodes the project
/// with `URI.encode/1` and this build escapes every segment strictly (D22).
fn endpoints_path(project: &str) -> String {
    format!("/{}{ENDPOINTS_PATH}", encode_path_segment(project))
}

/// One endpoint below the collection, encoded the same way.
fn endpoint_path(project: &str, conn_id: &str) -> String {
    format!(
        "{}/{}",
        endpoints_path(project),
        encode_path_segment(conn_id)
    )
}

/// The module's `if type = Map.get(parsed.options, :type), do: %{"type" => type}`:
/// an empty value is truthy, so it is sent as the empty pair.
fn list_params(conn_type: Option<String>) -> Vec<(String, String)> {
    conn_type
        .map(|conn_type| vec![("type".to_owned(), conn_type)])
        .unwrap_or_default()
}

/// `create`'s authorization object: the scheme option or the module's default,
/// with `accessToken` the only parameter written.
fn authorization(scheme: Option<&str>, token: Option<String>) -> Value {
    let mut parameters = Map::new();

    if let Some(token) = token {
        parameters.insert("accessToken".to_owned(), json!(token));
    }

    json!({
        "scheme": scheme.unwrap_or(DEFAULT_SCHEME),
        "parameters": Value::Object(parameters),
    })
}

/// The module's `put_if_present/3`: `nil` and `""` are absent.
fn put_if_present(body: &mut Map<String, Value>, value: Option<String>, key: &str) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        body.insert(key.to_owned(), json!(value));
    }
}

/// `merge_data/2`: absent or empty is nothing, a JSON object nests under `data`,
/// anything else is one of the module's two loud errors.
fn merge_data(body: &mut Map<String, Value>, data: Option<String>) -> Result<(), AdoError> {
    let Some(text) = data.filter(|text| !text.is_empty()) else {
        return Ok(());
    };

    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(parsed)) => {
            body.insert("data".to_owned(), Value::Object(parsed));

            Ok(())
        }
        Ok(_) => Err(AdoError::validation(DATA_NOT_OBJECT)),
        Err(_) => Err(AdoError::validation(DATA_INVALID)),
    }
}

/// The module's `resolve_token/1`: `nil`/`""` is no token, `-` reads all of stdin
/// (trimmed), `@path` a file (trimmed), anything else is the literal.
fn resolve_token(raw: Option<String>) -> Result<Option<String>, AdoError> {
    match raw.as_deref() {
        None | Some("") => Ok(None),
        Some("-") => {
            let mut content = String::new();

            std::io::stdin()
                .read_to_string(&mut content)
                .map_err(|error| {
                    AdoError::validation(format!("Cannot read the token from stdin: {error}"))
                })?;

            Ok(Some(content.trim().to_owned()))
        }
        Some(raw) => match raw.strip_prefix('@') {
            Some(path) => fs::read_to_string(path)
                .map(|content| Some(content.trim().to_owned()))
                .map_err(|error| {
                    AdoError::validation(format!("Cannot read secret file \"{path}\": {error}"))
                }),
            None => Ok(Some(raw.to_owned())),
        },
    }
}

/// The module's `print_connections_table/1` columns (ID, Name, Type), with the
/// module's "No service connections found." when empty. The oracle's fixed
/// 40/30 padding is §8 surface; this build's table style is the wave's (D37).
fn connections_table(connections: &[Value]) -> Report {
    if connections.is_empty() {
        return Report::Text("No service connections found.".to_owned());
    }

    let rows = connections
        .iter()
        .map(|connection| {
            vec![
                value_text(connection.get("id")),
                value_text(connection.get("name")),
                value_text(connection.get("type")),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Name".to_owned(), "Type".to_owned()],
        rows,
    }
}

/// The module's `print_connection_detail/1`: the `─` rule and the five labelled
/// fields.
fn connection_detail(connection: &Value) -> String {
    format!(
        concat!(
            "\nService Connection Details\n\n",
            "{}\n",
            "  ID:    {}\n",
            "  Name:  {}\n",
            "  Type:  {}\n",
            "  URL:   {}\n",
            "  Ready: {}\n",
        ),
        "─".repeat(60),
        value_text(connection.get("id")),
        value_text(connection.get("name")),
        value_text(connection.get("type")),
        value_text(connection.get("url")),
        value_text(connection.get("isReady")),
    )
}

/// The module's `success("… created.\n")` plus its four `writeln/1` lines; the
/// blank line is the shell's own newline after the message (captured).
fn created_line(connection: &Value) -> String {
    format!(
        concat!(
            "Service connection '{}' created.\n",
            "\n",
            "  ID:    {}\n",
            "  Type:  {}\n",
            "  URL:   {}\n",
            "  Ready: {}",
        ),
        value_text(connection.get("name")),
        value_text(connection.get("id")),
        value_text(connection.get("type")),
        value_text(connection.get("url")),
        value_text(connection.get("isReady")),
    )
}

/// The same four lines for `update`, whose message names the **response's** name
/// (captured: a rename answers with the connection's current name).
fn updated_line(connection: &Value) -> String {
    format!(
        concat!(
            "Service connection '{}' updated.\n",
            "\n",
            "  ID:    {}\n",
            "  Type:  {}\n",
            "  URL:   {}\n",
            "  Ready: {}",
        ),
        value_text(connection.get("name")),
        value_text(connection.get("id")),
        value_text(connection.get("type")),
        value_text(connection.get("url")),
        value_text(connection.get("isReady")),
    )
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
    fn the_paths_encode_the_project_and_the_id() {
        assert_eq!(
            endpoints_path("Alpha"),
            "/Alpha/_apis/serviceendpoint/endpoints"
        );
        assert_eq!(
            endpoint_path("Alpha", "c1"),
            "/Alpha/_apis/serviceendpoint/endpoints/c1"
        );
        assert_eq!(
            endpoints_path("Alpha/Beta"),
            "/Alpha%2FBeta/_apis/serviceendpoint/endpoints",
            "a slash cannot split the path (D22)"
        );
        assert_eq!(
            endpoint_path("Alpha", "c 1"),
            "/Alpha/_apis/serviceendpoint/endpoints/c%201"
        );
    }

    #[test]
    fn the_type_param_is_sent_only_when_given_even_when_empty() {
        assert_eq!(list_params(None), Vec::new());
        assert_eq!(
            list_params(Some("github".to_owned())),
            vec![("type".to_owned(), "github".to_owned())]
        );
        assert_eq!(
            list_params(Some(String::new())),
            vec![("type".to_owned(), String::new())],
            "the module's `if type = …` is truthy for `\"\"`"
        );
    }

    #[test]
    fn the_authorization_carries_the_token_only_when_present() {
        assert_eq!(
            authorization(None, None),
            json!({"scheme": "Token", "parameters": {}})
        );
        assert_eq!(
            authorization(Some("UsernamePassword"), Some("u:p".to_owned())),
            json!({"scheme": "UsernamePassword", "parameters": {"accessToken": "u:p"}})
        );
        assert_eq!(
            authorization(Some(""), None),
            json!({"scheme": "", "parameters": {}}),
            "a present-but-empty scheme is sent verbatim"
        );
    }

    #[test]
    fn put_if_present_drops_nil_and_empty() {
        let mut body = Map::new();

        put_if_present(&mut body, None, "description");
        put_if_present(&mut body, Some(String::new()), "description");
        assert!(body.is_empty(), "nil and \"\" are absent");

        put_if_present(&mut body, Some("desc".to_owned()), "description");
        assert_eq!(body.get("description"), Some(&json!("desc")));
    }

    #[test]
    fn merge_data_nests_the_object_and_rejects_the_rest() {
        let mut body = Map::new();

        merge_data(&mut body, None).expect("absent is nothing");
        merge_data(&mut body, Some(String::new())).expect("empty is nothing");
        assert!(body.is_empty());

        merge_data(
            &mut body,
            Some(r#"{"name":"Other","subscriptionId":"s1"}"#.to_owned()),
        )
        .expect("an object nests");
        assert_eq!(
            body.get("data"),
            Some(&json!({"name": "Other", "subscriptionId": "s1"})),
            "the reserved key nests; it does not overwrite the body's own name"
        );

        assert_eq!(
            merge_data(&mut body, Some("notjson".to_owned()))
                .expect_err("bad JSON")
                .message,
            DATA_INVALID
        );
        assert_eq!(
            merge_data(&mut body, Some("[1]".to_owned()))
                .expect_err("not an object")
                .message,
            DATA_NOT_OBJECT
        );
    }

    #[test]
    fn resolve_token_speaks_the_modules_three_forms() {
        assert_eq!(resolve_token(None).expect("none"), None);
        assert_eq!(resolve_token(Some(String::new())).expect("empty"), None);
        assert_eq!(
            resolve_token(Some("ghp_xxx".to_owned())).expect("literal"),
            Some("ghp_xxx".to_owned())
        );
    }

    #[test]
    fn resolve_token_reports_a_missing_file_with_the_module_prefix() {
        let error = resolve_token(Some("@nope.txt".to_owned())).expect_err("missing file");

        assert_eq!(error.code, ErrorCode::ValidationError);
        assert!(
            error
                .message
                .starts_with("Cannot read secret file \"nope.txt\": "),
            "the module's prefix, the io error's tail (§8): {}",
            error.message
        );
    }

    #[test]
    fn the_delete_question_is_the_helpers_verbatim() {
        assert_eq!(
            delete_question("Alpha", "c1"),
            "Delete service connection 'Alpha/c1'? This cannot be undone. [y/N] "
        );
    }

    #[test]
    fn the_table_uses_the_module_columns_and_its_empty_sentence() {
        assert_eq!(
            connections_table(&[]),
            Report::Text("No service connections found.".to_owned())
        );

        assert_eq!(
            connections_table(&[
                json!({"id": "c1", "name": "GitHub", "type": "github"}),
                json!({"id": "c2", "name": "K8s"}),
            ]),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "Type".to_owned()],
                rows: vec![
                    vec!["c1".to_owned(), "GitHub".to_owned(), "github".to_owned()],
                    vec!["c2".to_owned(), "K8s".to_owned(), String::new()],
                ],
            }
        );
    }

    #[test]
    fn the_detail_is_the_modules_layout() {
        assert_eq!(
            connection_detail(&json!({
                "id": "c1",
                "name": "GitHub",
                "type": "github",
                "url": "https://github.com",
                "isReady": true,
            })),
            format!(
                concat!(
                    "\nService Connection Details\n\n",
                    "{}\n",
                    "  ID:    c1\n",
                    "  Name:  GitHub\n",
                    "  Type:  github\n",
                    "  URL:   https://github.com\n",
                    "  Ready: true\n",
                ),
                "─".repeat(60)
            )
        );
    }

    #[test]
    fn the_success_lines_name_the_response_and_the_empty_fields_print_empty() {
        assert_eq!(
            created_line(&json!({
                "id": "new-id",
                "name": "GitHub",
                "type": "github",
                "url": "https://github.com",
                "isReady": true,
            })),
            "Service connection 'GitHub' created.\n\n  ID:    new-id\n  Type:  github\n  URL:   https://github.com\n  Ready: true"
        );
        assert_eq!(
            updated_line(&json!({"name": "GitHub", "id": "c1", "type": "github"})),
            "Service connection 'GitHub' updated.\n\n  ID:    c1\n  Type:  github\n  URL:   \n  Ready: ",
            "an absent field interpolates as the empty string"
        );
    }
}
