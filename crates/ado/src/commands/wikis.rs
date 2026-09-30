//! `ado wikis list|show|pages list|show|create|update` — the whole of
//! `lib/ado_cli/cli/wikis.ex`: the wiki surface under
//! `/{project}/_apis/wiki/wikis`, its pages, and the two page writes.
//!
//! `pages` nests one level deeper than the other areas (`wikis pages <leaf>`),
//! and its three option-taking leaves declare `--path` (and `--content` on the
//! writes) as required where the oracle's `Map.fetch!` turns a missing one into a
//! silent exit 0 (D34) — this build's clap makes it loud.
//!
//! The frozen `pages show` writes the page's content **before** its envelope, and
//! in human mode writes it twice (the unconditional `writeln/1` ahead of
//! `json_or_format/3`'s formatter). This build writes it once in human mode and
//! one document under `--json` (D40).

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// `ado wikis list`: `GET /{project}/_apis/wiki/wikis`; the value array unwraps
/// to the value envelope and the human path is the module's three-column table.
pub fn list(context: &mut Context, project: &str) -> Result<Report, AdoError> {
    let wikis = items(context.client()?.list(&wikis_path(project), &[])?);

    Ok(
        context.json_or_report(ok_value(Value::Array(wikis.clone())), || {
            wikis_table(&wikis)
        }),
    )
}

/// `ado wikis show`: `GET …/wikis/{wiki_id}`; a 404 takes the module's
/// `Wiki '<id>' not found` wording (D4's class).
pub fn show(context: &mut Context, project: &str, wiki_id: &str) -> Result<Report, AdoError> {
    let path = wiki_path(project, wiki_id);

    match context.client()?.get(&path, &[]) {
        Ok(wiki) => {
            Ok(context.json_or_report(ok_value(wiki.clone()), || Report::Text(wiki_detail(&wiki))))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Wiki '{wiki_id}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado wikis pages list`: `GET …/wikis/{wiki_id}/pages` with the module's two
/// params — `path` (the option, defaulting to `/`) and `recursionLevel=OneLevel`.
/// The body is an object with `subPages`, so the value envelope carries it whole
/// (`json_or_format`, not the list variant).
pub fn pages_list(
    context: &mut Context,
    project: &str,
    wiki_id: &str,
    path: Option<String>,
) -> Result<Report, AdoError> {
    let params = list_pages_params(path.as_deref().unwrap_or(DEFAULT_PAGE_PATH));
    let body = context
        .client()?
        .get(&pages_path(project, wiki_id), &params)?;

    Ok(context.json_or_report(ok_value(body.clone()), || pages_table(&body)))
}

/// `ado wikis pages show`: `GET …/pages` with `path` and `includeContent=true`.
/// The human path is the page's content once; the JSON path is the page's value
/// envelope as the command's only document (D40 — the frozen path writes the
/// content line in front of the envelope, even under `--json`). A 404 takes the
/// module's `Page '<path>' not found` wording.
pub fn pages_show(
    context: &mut Context,
    project: &str,
    wiki_id: &str,
    path: &str,
) -> Result<Report, AdoError> {
    let params = content_params(path);

    match context
        .client()?
        .get(&pages_path(project, wiki_id), &params)
    {
        Ok(page) => Ok(context.json_or_report(ok_value(page.clone()), || {
            Report::Text(content_text(page.get("content")))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Page '{path}' not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `ado wikis pages create`: `PUT …/pages` with the module's `{"content": …}`
/// body, its `Created via ado CLI` comment and no concurrency guard. The success
/// line names the **response's** path, as the capture shows.
pub fn pages_create(
    context: &mut Context,
    project: &str,
    wiki_id: &str,
    path: &str,
    content: &str,
) -> Result<Report, AdoError> {
    let params = write_params(path, "Created via ado CLI");
    let page = context.client()?.put(
        &pages_path(project, wiki_id),
        &content_body(content),
        &params,
    )?;

    Ok(context.json_or_report(ok_value(page.clone()), || {
        Report::Text(format!("Page '{}' created.", value_text(page.get("path"))))
    }))
}

/// `ado wikis pages update`: read the page for its `eTag` (asking for the content,
/// as the capture shows), then `PUT` with `If-Match: <eTag>` when the read
/// carried one — the module's optimistic-concurrency guard — and the
/// `Updated via ado CLI` comment.
pub fn pages_update(
    context: &mut Context,
    project: &str,
    wiki_id: &str,
    path: &str,
    content: &str,
) -> Result<Report, AdoError> {
    let existing = context
        .client()?
        .get(&pages_path(project, wiki_id), &content_params(path))?;
    let etag = existing
        .get("eTag")
        .and_then(Value::as_str)
        .filter(|etag| !etag.is_empty())
        .map(str::to_owned);
    let headers: Vec<(&str, &str)> = match etag.as_deref() {
        Some(etag) => vec![("If-Match", etag)],
        None => Vec::new(),
    };

    let page = context.client()?.put_with_headers(
        &pages_path(project, wiki_id),
        &content_body(content),
        &write_params(path, "Updated via ado CLI"),
        &headers,
    )?;

    Ok(context.json_or_report(ok_value(page.clone()), || {
        Report::Text(format!("Page '{}' updated.", value_text(page.get("path"))))
    }))
}

/// The module's default when `--path` is absent on `pages list`.
const DEFAULT_PAGE_PATH: &str = "/";

/// The collection path: `/{project}/_apis/wiki/wikis`.
fn wikis_path(project: &str) -> String {
    format!("/{}/_apis/wiki/wikis", encode_path_segment(project))
}

/// One wiki below the collection; the id is a single segment (D22).
fn wiki_path(project: &str, wiki_id: &str) -> String {
    format!("{}/{}", wikis_path(project), encode_path_segment(wiki_id))
}

/// A wiki's page collection; the page path travels as a query pair, so it never
/// touches the URL's structure.
fn pages_path(project: &str, wiki_id: &str) -> String {
    format!("{}/pages", wiki_path(project, wiki_id))
}

/// The module's `list_pages/1` params: the option or `/`, then `OneLevel`.
fn list_pages_params(path: &str) -> Vec<(String, String)> {
    vec![
        ("path".to_owned(), path.to_owned()),
        ("recursionLevel".to_owned(), "OneLevel".to_owned()),
    ]
}

/// The module's read params for `show`/`update`: the path and `includeContent`.
fn content_params(path: &str) -> Vec<(String, String)> {
    vec![
        ("path".to_owned(), path.to_owned()),
        ("includeContent".to_owned(), "true".to_owned()),
    ]
}

/// The module's write params: the path and its per-command comment.
fn write_params(path: &str, comment: &str) -> Vec<(String, String)> {
    vec![
        ("path".to_owned(), path.to_owned()),
        ("comment".to_owned(), comment.to_owned()),
    ]
}

/// The module's one-key write body.
fn content_body(content: &str) -> Value {
    json!({ "content": content })
}

/// The module's `print_wikis_table/1`: ID, Name, Type, with its "No wikis found."
/// when empty.
fn wikis_table(wikis: &[Value]) -> Report {
    if wikis.is_empty() {
        return Report::Text("No wikis found.".to_owned());
    }

    let rows = wikis
        .iter()
        .map(|wiki| {
            vec![
                value_text(wiki.get("id")),
                value_text(wiki.get("name")),
                value_text(wiki.get("type")),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Name".to_owned(), "Type".to_owned()],
        rows,
    }
}

/// The module's `print_wiki_detail/1`, whose URL is `remoteUrl || url`.
fn wiki_detail(wiki: &Value) -> String {
    let mut detail = String::from("\nWiki Details\n\n");

    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:   {}\n", value_text(wiki.get("id"))));
    detail.push_str(&format!("  Name: {}\n", value_text(wiki.get("name"))));
    detail.push_str(&format!("  Type: {}\n", value_text(wiki.get("type"))));
    detail.push_str(&format!("  URL:  {}\n", wiki_url(wiki)));
    detail.push('\n');

    detail
}

/// `wiki["remoteUrl"] || wiki["url"]`.
fn wiki_url(wiki: &Value) -> String {
    match wiki.get("remoteUrl") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => value_text(wiki.get("url")),
        Some(value) => value_text(Some(value)),
    }
}

/// The module's `print_pages_table/1`, whose page list is the body itself when it
/// is a list and `subPages || value || []` otherwise.
fn pages_table(body: &Value) -> Report {
    let pages = pages_of(body);

    if pages.is_empty() {
        return Report::Text("No pages found.".to_owned());
    }

    Report::Table {
        headers: vec!["Path".to_owned()],
        rows: pages.iter().map(|page| vec![page_path(page)]).collect(),
    }
}

/// The module's `pages = if is_list(data), do: data, else: data["subPages"] || data["value"] || []`.
fn pages_of(body: &Value) -> Vec<Value> {
    match body {
        Value::Array(pages) => pages.clone(),
        Value::Object(object) => object
            .get("subPages")
            .or_else(|| object.get("value"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// `p["path"] || p["pagePath"] || "/"`.
fn page_path(page: &Value) -> String {
    match page.get("path") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => match page.get("pagePath") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => DEFAULT_PAGE_PATH.to_owned(),
            Some(value) => value_text(Some(value)),
        },
        Some(value) => value_text(Some(value)),
    }
}

/// The module's `page["content"] || ""` — the single content the human path
/// writes (D40).
fn content_text(content: Option<&Value>) -> String {
    match content {
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(value) => value_text(Some(value)),
    }
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
    use serde_json::json;

    #[test]
    fn the_paths_encode_each_segment_strictly() {
        assert_eq!(wikis_path("Alpha"), "/Alpha/_apis/wiki/wikis");
        assert_eq!(
            wiki_path("Alpha", "a+b"),
            "/Alpha/_apis/wiki/wikis/a%2Bb",
            "the frozen URI.encode leaves + raw (D22)"
        );
        assert_eq!(
            pages_path("Alpha", "wiki-1"),
            "/Alpha/_apis/wiki/wikis/wiki-1/pages"
        );
    }

    #[test]
    fn the_page_list_reads_the_body_either_way() {
        assert_eq!(
            pages_of(&json!([{"path": "/One"}, {}])),
            vec![json!({"path": "/One"}), json!({})],
            "a list body is the page list"
        );
        assert_eq!(
            pages_of(&json!({"subPages": [{"path": "/Two"}]})),
            vec![json!({"path": "/Two"})]
        );
        assert_eq!(
            pages_of(&json!({"value": [{"path": "/Three"}]})),
            vec![json!({"path": "/Three"})],
            "subPages wins when both keys exist"
        );
        assert_eq!(pages_of(&json!({})), Vec::<Value>::new());
    }

    #[test]
    fn a_page_without_a_path_falls_back_to_pagepath_then_root() {
        assert_eq!(page_path(&json!({"path": "/Home"})), "/Home");
        assert_eq!(page_path(&json!({"pagePath": "/Legacy"})), "/Legacy");
        assert_eq!(page_path(&json!({})), "/");
        assert_eq!(
            page_path(&json!({"path": null, "pagePath": "/Legacy"})),
            "/Legacy"
        );
    }

    #[test]
    fn the_params_are_the_modules_pairs() {
        assert_eq!(
            list_pages_params("/Design"),
            vec![
                ("path".to_owned(), "/Design".to_owned()),
                ("recursionLevel".to_owned(), "OneLevel".to_owned()),
            ]
        );
        assert_eq!(
            content_params("/Home"),
            vec![
                ("path".to_owned(), "/Home".to_owned()),
                ("includeContent".to_owned(), "true".to_owned()),
            ]
        );
        assert_eq!(
            write_params("/Home", "Updated via ado CLI"),
            vec![
                ("path".to_owned(), "/Home".to_owned()),
                ("comment".to_owned(), "Updated via ado CLI".to_owned()),
            ]
        );
    }

    #[test]
    fn the_detail_prefers_the_remote_url_and_the_tables_keep_their_columns() {
        let detail = wiki_detail(&json!({
            "id": "wiki-1",
            "name": "Alpha Wiki",
            "type": "codeWiki",
            "remoteUrl": "remote",
            "url": "api",
        }));

        assert!(detail.contains("  URL:  remote\n"), "detail: {detail:?}");
        assert_eq!(
            wikis_table(&[json!({"id": "wiki-1", "name": "Alpha Wiki", "type": "codeWiki"})]),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "Type".to_owned()],
                rows: vec![vec![
                    "wiki-1".to_owned(),
                    "Alpha Wiki".to_owned(),
                    "codeWiki".to_owned()
                ]],
            }
        );
        assert_eq!(wikis_table(&[]), Report::Text("No wikis found.".to_owned()));
    }

    #[test]
    fn the_content_falls_back_to_the_empty_string() {
        assert_eq!(content_text(Some(&json!("# Home"))), "# Home");
        assert_eq!(content_text(None), "");
        assert_eq!(content_text(Some(&Value::Bool(false))), "");
    }
}
