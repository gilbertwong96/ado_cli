//! `ado pipelines-artifacts list|download` — the read paths of
//! `lib/ado_cli/cli/run_artifacts.ex`: the same REST surface, the same human
//! layout, and a byte download whose source URL is the one deliberate deviation
//! (D25: an absolute `resource.downloadUrl` is requested verbatim).
//!
//! `download` produces bytes, not an envelope: it streams the artifact to
//! `--output` (default `./<artifact-name>.zip`) and reports the module's
//! `Downloaded <n> bytes to <path>` line. The body goes straight to a sibling temp
//! file in 64 KiB chunks — no size cap — and only a fully received body is renamed
//! onto the target. `--output` is always a file path — the frozen `--output -`
//! writes a file literally named `-`, and this wave does not invent a stdout
//! destination.

use std::fs;
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

use ado_core::client::{RawBody, encode_path_segment};
use ado_core::envelope::ok_value;
use ado_core::error::AdoError;
use serde_json::Value;

use crate::context::Context;
use crate::output::Report;

/// `list_artifacts/1`: `GET /{project}/_apis/pipelines/{pipeline_id}/runs/{run_id}/artifacts`
/// with no params beyond the version. Under `--json` the body is the value
/// envelope — a bare array under `result`, the kind the module's
/// `Helpers.json_or_format` picks (W1-R12).
pub fn list(
    context: &mut Context,
    project: &str,
    pipeline_id: i64,
    run_id: i64,
) -> Result<Report, AdoError> {
    let artifacts = artifact_list(context, project, pipeline_id, run_id)?;

    Ok(
        context.json_or_report(ok_value(Value::Array(artifacts.clone())), || {
            artifacts_table(&artifacts)
        }),
    )
}

/// `download_artifact/1`: list the run's artifacts, pick one by its exact name,
/// `GET` its `resource.downloadUrl`, and stream the bytes into `--output` (default
/// `./<artifact-name>.zip`). The success line is the module's, with the number of
/// bytes actually written; there is no envelope, under `--json` or otherwise.
///
/// D25: an **absolute** `downloadUrl` is requested verbatim, with no `api-version`
/// added — the URL carries its own signed query, and the frozen client's habit of
/// prepending its base turns Azure's absolute URLs into a dead path. A **relative**
/// `downloadUrl` is resolved against the client's base, org-injected with the
/// version merged in, which is the frozen behaviour for that shape.
pub fn download(
    context: &mut Context,
    project: &str,
    pipeline_id: i64,
    run_id: i64,
    artifact_name: &str,
    output: Option<String>,
) -> Result<Report, AdoError> {
    let artifacts = artifact_list(context, project, pipeline_id, run_id)?;
    let artifact = find_artifact(&artifacts, artifact_name).ok_or_else(|| {
        AdoError::not_found(format!(
            "Artifact '{artifact_name}' not found in run #{run_id}"
        ))
    })?;

    let download_url = download_url(artifact, artifact_name)?;
    let client = context.client()?;
    let url = if is_absolute(&download_url) {
        download_url
    } else {
        client.url_for(&download_url, &[])
    };
    let body = client.get_raw(&url)?;

    let target = output.unwrap_or_else(|| format!("{artifact_name}.zip"));
    let written = write_artifact(&target, body)?;

    Ok(Report::Text(format!(
        "Downloaded {written} bytes to {target}"
    )))
}

/// The artifacts collection both paths start from: the module's
/// `Client.list("/#{URI.encode(project)}/_apis/pipelines/#{pid}/runs/#{run_id}/artifacts")`.
fn artifact_list(
    context: &mut Context,
    project: &str,
    pipeline_id: i64,
    run_id: i64,
) -> Result<Vec<Value>, AdoError> {
    let path = format!(
        "/{}/_apis/pipelines/{pipeline_id}/runs/{run_id}/artifacts",
        encode_path_segment(project)
    );

    Ok(items(context.client()?.list(&path, &[])?))
}

/// `Enum.find(artifacts, &(&1["name"] == name))`: exact, case-sensitive string
/// equality, so a differently-cased name is a different artifact.
fn find_artifact<'a>(artifacts: &'a [Value], name: &str) -> Option<&'a Value> {
    artifacts
        .iter()
        .find(|artifact| artifact.get("name").and_then(Value::as_str) == Some(name))
}

/// The module's `url = artifact["resource"]["downloadUrl"]`. The module hands a
/// missing or null value to `Client.get_raw/2` and crashes; Rust rejects it as
/// command-level input instead (D4's presentation of the same exit-1 outcome).
fn download_url(artifact: &Value, name: &str) -> Result<String, AdoError> {
    artifact
        .get("resource")
        .and_then(|resource| resource.get("downloadUrl"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AdoError::validation(format!("Artifact '{name}' has no download URL")))
}

/// A scheme and host mean the URL is used verbatim; everything else is a path the
/// client resolves against its base (D25).
fn is_absolute(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// The module's `File.write!/2` raises; Rust answers with the usual command-level
/// error presentation, the way `completion --write-to-file` does (D4).
fn write_error(path: &str, error: &io::Error) -> AdoError {
    AdoError::validation(format!("Could not write the artifact to {path}: {error}"))
}

/// Stream the open body into a sibling temp file and rename it onto `target` only
/// once the whole body arrived. That keeps both invariants: no partial artifact
/// ever appears at the target, and a pre-existing target survives a failed
/// download byte for byte — the module reads the whole body before its
/// `File.write!/2`, so it never leaves a partial file and never destroys an
/// existing one either. A failure while removing the temp file never masks the
/// original error.
fn write_artifact(target: &str, mut body: RawBody) -> Result<u64, AdoError> {
    let temp = temp_path(target);
    let mut file = fs::File::create(&temp).map_err(|error| write_error(target, &error))?;

    let written = match copy_body(&mut body, &mut file, target) {
        Ok(written) => written,
        Err(error) => {
            drop(file);
            let _ = fs::remove_file(&temp);

            return Err(error);
        }
    };

    drop(file);
    fs::rename(&temp, target).map_err(|error| {
        let _ = fs::remove_file(&temp);
        write_error(target, &error)
    })?;

    Ok(written)
}

/// The temp name sits beside the target, so the successful rename is a
/// same-directory replace of the target's bytes. The pid and per-process counter
/// infix keeps it unique, so a pre-existing `{target}.tmp` is never opened — a
/// failed download cannot truncate or delete it. The `.tmp` suffix stays, so the
/// temp still reads as a temporary download in the target's directory.
fn temp_path(target: &str) -> String {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{target}.{}.{unique}.tmp", std::process::id())
}

/// The copy loop, so a reader failure and a writer failure classify differently:
/// a dropped connection is the §6.2 transport class, while an unwritable target is
/// command-level input. A fixed 64 KiB buffer keeps the copy at one allocation and
/// the body itself has no size cap.
fn copy_body(body: &mut RawBody, file: &mut fs::File, target: &str) -> Result<u64, AdoError> {
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;

    loop {
        let read = body.read_chunk(&mut buffer)?;

        if read == 0 {
            return Ok(written);
        }

        file.write_all(&buffer[..read])
            .map_err(|error| write_error(target, &error))?;
        written += read as u64;
    }
}

/// The module's `print_artifacts_table/1`: Name and Size.
fn artifacts_table(artifacts: &[Value]) -> Report {
    if artifacts.is_empty() {
        return Report::Text("No artifacts found.".to_owned());
    }

    let rows = artifacts
        .iter()
        .map(|artifact| vec![or_empty(artifact, "name"), size_cell(artifact)])
        .collect();

    Report::Table {
        headers: vec!["Name".to_owned(), "Size".to_owned()],
        rows,
    }
}

/// `a["resource"]["size"] || "?"` where `resource = a["resource"] || %{}`. The
/// Elixir `||` is truthy-based, so only a missing, null or false size falls back
/// to `?`, while an explicit `0` prints `0` and an explicit `""` prints empty.
fn size_cell(artifact: &Value) -> String {
    artifact
        .get("resource")
        .and_then(|resource| resource.get("size"))
        .filter(|value| truthy(value))
        .map(text)
        .unwrap_or_else(|| "?".to_owned())
}

/// `value["key"] || ""`: only a missing, null or false field is empty.
fn or_empty(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(value) if truthy(value) => text(value),
        _ => String::new(),
    }
}

/// Elixir's truthiness for a decoded JSON value: only `null` and `false` are falsy.
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

/// A JSON value as `#{}` would interpolate it: `nil` as empty, a string as itself,
/// anything else by its JSON text.
fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The Elixir's `Client.list/2` unwraps a `value` key and passes anything else
/// through — including `null`, which the oracle renders as `"result":null` — while
/// this envelope always carries an array, so a non-array body becomes its single
/// element.
fn items(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        other => vec![other],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn temp_path_is_a_unique_tmp_sibling() {
        use std::path::Path;

        let first = temp_path("out.zip");
        let second = temp_path("out.zip");

        assert!(
            first.ends_with(".tmp"),
            "the temp reads as a temporary download: {first}"
        );
        assert!(
            first.starts_with("out.zip."),
            "the temp is derived from the target: {first}"
        );
        assert_ne!(
            first, "out.zip.tmp",
            "a pre-existing `{{target}}.tmp` is never reused: {first}"
        );
        assert_ne!(first, second, "each download gets its own temp name");
        assert_eq!(
            Path::new(&temp_path("/tmp/dir/out.zip")).parent(),
            Some(Path::new("/tmp/dir")),
            "the temp stays in the target's directory, so the rename is same-filesystem"
        );
    }

    #[test]
    fn artifacts_table_uses_the_module_columns_and_the_size_fallback() {
        let artifacts = vec![
            json!({"name": "drop", "resource": {"size": 2048}}),
            json!({"name": "TestResults"}),
        ];

        assert_eq!(
            artifacts_table(&artifacts),
            Report::Table {
                headers: vec!["Name".to_owned(), "Size".to_owned()],
                rows: vec![
                    vec!["drop".to_owned(), "2048".to_owned()],
                    vec!["TestResults".to_owned(), "?".to_owned()],
                ],
            },
            "the module's Size cell is resource.size, `?` when the artifact carries none"
        );
    }

    #[test]
    fn artifacts_table_reads_missing_names_like_the_module() {
        let artifacts = vec![json!({"name": null}), json!({"name": false})];

        assert_eq!(
            artifacts_table(&artifacts),
            Report::Table {
                headers: vec!["Name".to_owned(), "Size".to_owned()],
                rows: vec![
                    vec![String::new(), "?".to_owned()],
                    vec![String::new(), "?".to_owned()],
                ],
            },
            "`a[\"name\"] || \"\"`: only a missing, null or false name is empty"
        );
    }

    #[test]
    fn size_cell_falls_back_only_for_a_missing_null_or_false_size() {
        assert_eq!(size_cell(&json!({"resource": {"size": 2048}})), "2048");
        assert_eq!(size_cell(&json!({"resource": {"size": "2048"}})), "2048");
        assert_eq!(size_cell(&json!({"resource": {"size": 0}})), "0");
        assert_eq!(
            size_cell(&json!({"resource": {"size": ""}})),
            "",
            "an explicit empty size is truthy in Elixir and stays empty"
        );
        assert_eq!(size_cell(&json!({"resource": {"size": null}})), "?");
        assert_eq!(size_cell(&json!({"resource": {"size": false}})), "?");
        assert_eq!(size_cell(&json!({"resource": {}})), "?");
        assert_eq!(size_cell(&json!({"resource": null})), "?");
        assert_eq!(size_cell(&json!({})), "?");
    }

    #[test]
    fn artifacts_table_of_nothing_is_the_module_message() {
        assert_eq!(
            artifacts_table(&[]),
            Report::Text("No artifacts found.".to_owned())
        );
    }

    #[test]
    fn find_artifact_is_exact_and_case_sensitive() {
        let artifacts = vec![json!({"name": "drop"}), json!({"name": "TestResults"})];

        assert_eq!(
            find_artifact(&artifacts, "drop"),
            Some(&json!({"name": "drop"}))
        );
        assert_eq!(find_artifact(&artifacts, "DROP"), None);
        assert_eq!(find_artifact(&artifacts, "Test"), None);
        assert_eq!(find_artifact(&[json!({"name": 1})], "1"), None);
    }

    #[test]
    fn download_url_rejects_a_missing_or_non_string_url() {
        assert_eq!(
            download_url(
                &json!({"resource": {"downloadUrl": "/blob/drop.zip"}}),
                "drop"
            ),
            Ok("/blob/drop.zip".to_owned())
        );
        for artifact in [
            json!({}),
            json!({"resource": null}),
            json!({"resource": {}}),
            json!({"resource": {"downloadUrl": null}}),
            json!({"resource": {"downloadUrl": false}}),
        ] {
            let error = download_url(&artifact, "drop").expect_err("no url");
            assert_eq!(error.code, ado_core::error::ErrorCode::ValidationError);
            assert_eq!(error.message, "Artifact 'drop' has no download URL");
        }
    }

    #[test]
    fn is_absolute_recognizes_a_scheme_and_host() {
        assert!(is_absolute("https://dev.azure.com/myorg/blob"));
        assert!(is_absolute("http://127.0.0.1:8080/blob"));
        assert!(!is_absolute("/blob/drop.zip"));
        assert!(!is_absolute("blob/drop.zip"));
        assert!(!is_absolute(""));
    }

    #[test]
    fn items_wraps_a_non_array_body_for_the_human_path() {
        assert_eq!(
            items(json!([1, 2])),
            vec![json!(1), json!(2)],
            "the value array passes through"
        );
        assert_eq!(
            items(json!(null)),
            vec![json!(null)],
            "a non-array body becomes its single element"
        );
    }
}
