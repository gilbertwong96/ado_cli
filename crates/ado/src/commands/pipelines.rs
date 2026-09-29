//! `ado pipelines list|show|run|create|update|delete`, the `vars` variable
//! groups, the per-pipeline `variables`, the `secure_files` library and the
//! `pipelines-folders` node — the read and write paths of
//! `lib/ado_cli/cli/pipelines.ex` and `lib/ado_cli/cli/folders.ex`: the same REST
//! surface, the same bodies, and the same human layouts.
//!
//! Three deletes were re-verified against the mock with `n` on stdin (R5):
//! `pipelines delete` and `pipelines vars delete` proceed without a question and
//! have no `--force`; `pipelines secure_files delete` asks no question either,
//! but without `--force` it refuses — the oracle prints its guard and exits 0
//! having sent nothing, this build exits 1 with the same message on stderr
//! (R6, D32). `pipelines-folders delete` is a fourth: it was re-run against the
//! mock with `n` and on EOF, and both runs sent the DELETE (R5).

use std::collections::{BTreeMap, HashSet};
use std::fs;

use ado_core::client::encode_path_segment;
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Map, Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The org-scoped project collection `vars delete` resolves its `projectIds`
/// against.
const PROJECTS_PATH: &str = "/_apis/projects";

/// `secure_files delete`'s guard without `--force`: a message that replaces a
/// question (R6). The oracle prints it and exits 0 having sent nothing; this
/// build refuses with exit 1 (D32) and the same wording.
const SECURE_FILE_GUARD: &str =
    "This will permanently delete the Secure File. Pass --force to confirm.";

/// `Map.get(parsed.options, :branch, "main")`: the branch a run uses when
/// `--branch` is absent.
const DEFAULT_RUN_BRANCH: &str = "main";

/// `Map.get(parsed.options, :folder, "/")`: the folder a created pipeline lands
/// in when `--folder` is absent.
const DEFAULT_PIPELINE_FOLDER: &str = "/";

/// `list_pipelines/1`: `GET /{project}/_apis/pipelines` with the module's `$top`
/// and `folder` filters. Under `--json` the body is the value envelope — a bare
/// array under `result`, the kind the module's `Helpers.json_or_format` picks
/// (W1-R12).
pub fn list(
    context: &mut Context,
    project: &str,
    top: Option<i64>,
    folder: Option<String>,
) -> Result<Report, AdoError> {
    let path = format!("/{}/_apis/pipelines", encode_path_segment(project));
    let pipelines = items(context.client()?.list(&path, &list_params(top, folder))?);

    Ok(
        context.json_or_report(ok_value(Value::Array(pipelines.clone())), || {
            pipelines_table(&pipelines)
        }),
    )
}

/// `show_pipeline/1`: `GET /{project}/_apis/pipelines/{pipeline_id}`, with no params
/// beyond the version. The module answers a 404 with its own message.
pub fn show(context: &mut Context, project: &str, pipeline_id: i64) -> Result<Report, AdoError> {
    let path = pipeline_path(project, pipeline_id);
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(pipeline) => {
            Ok(context.json_or_report(ok_value(pipeline.clone()), || pipeline_detail(&pipeline)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pipeline #{pipeline_id} not found in project '{project}'"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `run_pipeline/1`: `POST /{project}/_apis/pipelines/{id}/runs` with the
/// module's resources body and, when given, the parsed run variables. The frozen
/// CLI prints its run summary even under `--json`; this build emits the created
/// run as the value envelope (D33).
pub fn run(
    context: &mut Context,
    project: &str,
    pipeline_id: i64,
    branch: Option<String>,
    variables: Option<String>,
) -> Result<Report, AdoError> {
    let body = run_body(branch.as_deref(), variables.as_deref());
    let path = format!(
        "/{}/_apis/pipelines/{pipeline_id}/runs",
        encode_path_segment(project)
    );
    let run = context.client()?.post(&path, &body, &[])?;

    Ok(context.json_or_report(ok_value(run.clone()), || run_summary(&run)))
}

/// The module's `run_pipeline/1` body: the self repository's ref always
/// (`refs/heads/<branch>`, defaulting to `main`), and `variables` — each pair as
/// its own `{"value": …}` object — only when `--variables` is present.
fn run_body(branch: Option<&str>, variables: Option<&str>) -> Value {
    let mut body = json!({
        "resources": {
            "repositories": {
                "self": {
                    "refName": format!(
                        "refs/heads/{}",
                        branch.unwrap_or(DEFAULT_RUN_BRANCH)
                    ),
                },
            },
        },
    });

    if let Some(variables) = variables {
        body["variables"] = Value::Object(parse_run_variables(variables));
    }

    body
}

/// The module's `parse_variables/1`: comma-separated `KEY=VALUE` pairs, each
/// trimmed, a pair without `=` dropped, and each value wrapped as
/// `{"value": …}` — the module's `%{"value" => value}`, which never marks a run
/// variable secret.
fn parse_run_variables(variables: &str) -> Map<String, Value> {
    variables
        .split(',')
        .map(str::trim)
        .filter_map(|pair| {
            pair.split_once('=')
                .map(|(key, value)| (key.to_owned(), json!({"value": value})))
        })
        .collect()
}

/// The module's run summary, minus the colour: the four labelled lines, each
/// read with `#{}`'s empty-for-nil semantics.
fn run_summary(run: &Value) -> Report {
    Report::Text(format!(
        "Pipeline run triggered!\n\n  Run ID:   {}\n  State:    {}\n  Pipeline: {}\n  URL:      {}",
        interpolated(run.get("id")),
        interpolated(run.get("state")),
        interpolated(run.pointer("/pipeline/name")),
        interpolated(run.pointer("/_links/web/href")),
    ))
}

/// `create_pipeline/1`: `POST /{project}/_apis/pipelines` with the module's YAML
/// body — the repository as both `id` and `name` — and the module's success line.
/// Under `--json` the created pipeline is the value envelope (D33).
pub fn create(
    context: &mut Context,
    project: &str,
    name: &str,
    repo: &str,
    yaml_path: &str,
    folder: Option<String>,
) -> Result<Report, AdoError> {
    let body = create_body(name, repo, yaml_path, folder.as_deref());
    let path = format!("/{}/_apis/pipelines", encode_path_segment(project));
    let pipeline = context.client()?.post(&path, &body, &[])?;

    Ok(context.json_or_report(ok_value(pipeline.clone()), || {
        Report::Text(format!(
            "Pipeline '{}' created (ID: {}).",
            field(&pipeline, "name"),
            id_cell(&pipeline),
        ))
    }))
}

/// The module's `create_pipeline/1` body, captured: `folder` is always sent
/// (defaulting to `/`), and the repository is both its id and its name.
fn create_body(name: &str, repo: &str, yaml_path: &str, folder: Option<&str>) -> Value {
    json!({
        "name": name,
        "folder": folder.unwrap_or(DEFAULT_PIPELINE_FOLDER),
        "configuration": {
            "type": "yaml",
            "path": yaml_path,
            "repository": {"id": repo, "name": repo, "type": "azureReposGit"},
        },
    })
}

/// `update_pipeline/1`: `PATCH /{project}/_apis/pipelines/{id}` with only the
/// options given, after the module's own guard — no `--name` and no `--path` is a
/// `validation_error` with no request (captured). Under `--json` the updated
/// pipeline is the value envelope (D33); the module answers a 404 with the
/// pipeline's id alone.
pub fn update(
    context: &mut Context,
    project: &str,
    pipeline_id: i64,
    name: Option<String>,
    path: Option<String>,
) -> Result<Report, AdoError> {
    if name.is_none() && path.is_none() {
        return Err(AdoError::validation(
            "At least one of --name or --path is required.",
        ));
    }

    let mut body = Map::new();

    if let Some(name) = name {
        body.insert("name".to_owned(), json!(name));
    }
    if let Some(path) = path {
        body.insert("configuration".to_owned(), json!({"path": path}));
    }

    let request_path = pipeline_path(project, pipeline_id);

    match context
        .client()?
        .patch(&request_path, &Value::Object(body), &[])
    {
        Ok(pipeline) => Ok(context.json_or_report(ok_value(pipeline.clone()), || {
            Report::Text("Pipeline updated.".to_owned())
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pipeline #{pipeline_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `delete_pipeline/1`: `DELETE /{project}/_apis/pipelines/{id}`. The frozen
/// command has no `--force` and asks no confirmation at all (captured on `n` and
/// on EOF, R5); its success line is this build's message envelope (D33), and a
/// 404 carries the pipeline's id alone.
pub fn delete(context: &mut Context, project: &str, pipeline_id: i64) -> Result<Report, AdoError> {
    let path = pipeline_path(project, pipeline_id);

    match context.client()?.delete(&path, &[]) {
        Ok(()) => {
            let message = "Pipeline deleted.".to_owned();

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pipeline #{pipeline_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `list_var_groups/1`: `GET /{project}/_apis/distributedtask/variablegroups`
/// with the module's `$top`. Under `--json` the body is the value envelope — a
/// bare array under `result` (W1-R12).
pub fn vars_list(
    context: &mut Context,
    project: &str,
    top: Option<i64>,
) -> Result<Report, AdoError> {
    let params = var_list_params(top);
    let groups = items(
        context
            .client()?
            .list(&vars_collection_path(project), &params)?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(groups.clone())), || {
            vars_table(&groups)
        }),
    )
}

/// `show_var_group/1`: `GET /{project}/_apis/distributedtask/variablegroups/{id}`.
/// The module answers a 404 with its own message.
pub fn vars_show(context: &mut Context, project: &str, group_id: i64) -> Result<Report, AdoError> {
    let path = vars_group_path(project, group_id);
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(group) => Ok(context.json_or_report(ok_value(group.clone()), || vars_detail(&group))),
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Variable group #{group_id} not found in project '{project}'"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `create_var_group/1`: `POST /{project}/_apis/distributedtask/variablegroups`
/// with the module's body — the project reference by name, and `variables` only
/// when `--variables` is given. Under `--json` the created group is the value
/// envelope (D33).
pub fn vars_create(
    context: &mut Context,
    project: &str,
    name: &str,
    description: Option<String>,
    variables: Option<String>,
    secret: Option<String>,
) -> Result<Report, AdoError> {
    let body = var_group_body(
        project,
        name,
        description.as_deref(),
        variables.as_deref(),
        secret.as_deref(),
    );
    let group = context
        .client()?
        .post(&vars_collection_path(project), &body, &[])?;

    Ok(context.json_or_report(ok_value(group.clone()), || {
        Report::Text(format!(
            "Variable group '{}' created (ID: {}).",
            field(&group, "name"),
            id_cell(&group),
        ))
    }))
}

/// The module's `build_var_group_body/2`, captured: `type` is always `Vsts`, the
/// reference repeats the group's name for the project, `description` is added
/// only when given, and `variables` only when `--variables` is present.
fn var_group_body(
    project: &str,
    name: &str,
    description: Option<&str>,
    variables: Option<&str>,
    secret: Option<&str>,
) -> Value {
    let mut body = json!({
        "name": name,
        "type": "Vsts",
        "variableGroupProjectReferences": [
            {"name": name, "projectReference": {"name": project}},
        ],
    });
    let fields = body.as_object_mut().expect("the body is an object");

    if let Some(description) = description {
        fields.insert("description".to_owned(), json!(description));
    }
    if let Some(variables) = variables {
        fields.insert(
            "variables".to_owned(),
            Value::Object(parse_var_group_variables(variables, secret)),
        );
    }

    body
}

/// `update_var_group/1`: fetch the existing group, merge the options into it,
/// then `PUT` the merged body. The merge is the module's, captured: the passed
/// `--name`/`--description` win, `variableGroupProjectReferences` stays the
/// existing array, and `--variables` is merged over the existing variables (a
/// key the option does not name keeps the shape the API returned).
pub fn vars_update(
    context: &mut Context,
    project: &str,
    group_id: i64,
    name: Option<String>,
    description: Option<String>,
    variables: Option<String>,
    secret: Option<String>,
) -> Result<Report, AdoError> {
    let path = vars_group_path(project, group_id);
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(existing) => {
            let body = merge_var_group_body(
                &existing,
                name.as_deref(),
                description.as_deref(),
                variables.as_deref(),
                secret.as_deref(),
            );

            match context.client()?.put(&path, &body, &[]) {
                Ok(group) => Ok(context.json_or_report(ok_value(group.clone()), || {
                    Report::Text(format!(
                        "Variable group '{}' updated.",
                        field(&group, "name")
                    ))
                })),
                Err(error) => Err(error),
            }
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Variable group #{group_id} not found in project '{project}'"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// The module's `merge_var_group_body/2`: the option wins, else the existing
/// value for `name`/`description`, `type` falls back to `Vsts` and the
/// references to an empty array, and the variable map is `Map.merge(existing,
/// parsed)` when `--variables` is present and the existing map otherwise — sent
/// only when it is not empty.
fn merge_var_group_body(
    existing: &Value,
    name: Option<&str>,
    description: Option<&str>,
    variables: Option<&str>,
    secret: Option<&str>,
) -> Value {
    let name = name
        .map(|name| json!(name))
        .unwrap_or_else(|| existing.get("name").cloned().unwrap_or(Value::Null));
    let description = description
        .map(|description| json!(description))
        .or_else(|| existing.get("description").cloned());
    let mut body = json!({
        "name": name,
        "type": existing
            .get("type")
            .filter(|value| truthy(value))
            .cloned()
            .unwrap_or_else(|| json!("Vsts")),
        "variableGroupProjectReferences": existing
            .get("variableGroupProjectReferences")
            .filter(|value| truthy(value))
            .cloned()
            .unwrap_or_else(|| json!([])),
    });

    if let Some(description) = description.filter(truthy) {
        body["description"] = description;
    }

    let merged = match variables {
        Some(variables) => {
            let mut merged = existing
                .get("variables")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();

            for (key, value) in parse_var_group_variables(variables, secret) {
                merged.insert(key, value);
            }

            Value::Object(merged)
        }
        None => existing
            .get("variables")
            .cloned()
            .unwrap_or_else(|| json!({})),
    };

    if merged
        .as_object()
        .is_none_or(|variables| !variables.is_empty())
    {
        body["variables"] = merged;
    }

    body
}

/// `delete_var_group/1`: resolve the project name to an id, then
/// `DELETE /{project}/_apis/distributedtask/variablegroups/{id}?projectIds=<id>`.
/// The lookup is the module's: a miss or a failed `GET /_apis/projects` leaves
/// `projectIds` off the delete, which still proceeds (both captured). No prompt
/// stands in front of either request (R5).
pub fn vars_delete(
    context: &mut Context,
    project: &str,
    group_id: i64,
) -> Result<Report, AdoError> {
    let params = resolved_project_ids(context, project);
    let path = vars_group_path(project, group_id);

    match context.client()?.delete(&path, &params) {
        Ok(()) => {
            let message = format!("Variable group #{group_id} deleted.");

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Variable group #{group_id} not found in project '{project}'"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `list_pipeline_vars/1`: `GET /{project}/_apis/pipelines/{pipeline_id}`, then the
/// pipeline's `configuration.variables` as one record per variable
/// (`key`, `value`, `isSecret`). A pipeline without a variables map reads as
/// none; the module's 404 message carries the pipeline's id alone.
///
/// The records are key-sorted because both sides enumerate a small map in term
/// order (serde's `BTreeMap` here, Erlang's flatmap there); a pipeline with more
/// than 32 variables is the recorded carry.
pub fn variables_list(
    context: &mut Context,
    project: &str,
    pipeline_id: i64,
) -> Result<Report, AdoError> {
    let path = pipeline_path(project, pipeline_id);

    match context.client()?.get(&path, &[]) {
        Ok(pipeline) => {
            let variables = pipeline_variables(&pipeline);

            Ok(
                context.json_or_report(ok_value(Value::Array(variables.clone())), || {
                    variables_table(&variables)
                }),
            )
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pipeline #{pipeline_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `create_pipeline_var/1`: fetch the pipeline, put the new variable into
/// `configuration.variables`, and `PATCH` the whole pipeline back. The module's
/// success line is this build's message envelope (D33).
pub fn variables_create(
    context: &mut Context,
    project: &str,
    pipeline_id: i64,
    key: &str,
    value: &str,
    secret: bool,
) -> Result<Report, AdoError> {
    let path = pipeline_path(project, pipeline_id);
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(pipeline) => {
            let mut variables = pipeline_variables_map(&pipeline);
            variables.insert(key.to_owned(), json!({"value": value, "isSecret": secret}));

            let body = pipeline_with_variables(&pipeline, variables);

            match context.client()?.patch(&path, &body, &[]) {
                Ok(_) => {
                    let message = format!("Variable '{key}' added.");

                    Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
                }
                Err(error) => Err(error),
            }
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pipeline #{pipeline_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `delete_pipeline_var/1`: fetch the pipeline, drop the named variable from
/// `configuration.variables`, and `PATCH` the whole pipeline back — the key is
/// removed, the map stays (captured: an emptied map is still sent as `{}`).
/// No prompt stands in front of either request; the captured `n` run sends both
/// (R5). The module's success line is this build's message envelope (D33).
pub fn variables_delete(
    context: &mut Context,
    project: &str,
    pipeline_id: i64,
    key: &str,
) -> Result<Report, AdoError> {
    let path = pipeline_path(project, pipeline_id);
    let response = context.client()?.get(&path, &[]);

    match response {
        Ok(pipeline) => {
            let mut variables = pipeline_variables_map(&pipeline);
            variables.remove(key);

            let body = pipeline_with_variables(&pipeline, variables);

            match context.client()?.patch(&path, &body, &[]) {
                Ok(_) => {
                    let message = format!("Variable '{key}' removed.");

                    Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
                }
                Err(error) => Err(error),
            }
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Pipeline #{pipeline_id} not found"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `secure_files_list/1`: `GET /{project}/_apis/distributedtask/securefiles`
/// with the module's `$top`. Under `--json` the body is the value envelope — a
/// bare array under `result` (W1-R12).
pub fn secure_files_list(
    context: &mut Context,
    project: &str,
    top: Option<i64>,
) -> Result<Report, AdoError> {
    let params = top
        .map(|top| vec![("$top".to_owned(), top.to_string())])
        .unwrap_or_default();
    let files = items(
        context
            .client()?
            .list(&secure_files_path(project), &params)?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(files.clone())), || {
            secure_files_table(&files)
        }),
    )
}

/// `secure_files_show/1`: `GET …/securefiles/{id}`. The module answers a 404 with
/// its own message.
pub fn secure_files_show(
    context: &mut Context,
    project: &str,
    secure_file_id: &str,
) -> Result<Report, AdoError> {
    let path = secure_file_path(project, secure_file_id);

    match context.client()?.get(&path, &[]) {
        Ok(file) => {
            Ok(context.json_or_report(ok_value(file.clone()), || secure_file_detail(&file)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Secure file {secure_file_id} not found in project '{project}'"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `secure_files_upload/1`: read the local file, optionally replace a file of the
/// same name, then `POST` the bytes. The body is **not JSON** — the frozen
/// `Client.post_binary/3` sends the raw bytes with `application/octet-stream`
/// and the name as a query parameter (captured, headers included). A failed
/// replace reports and sends no upload; a 409 becomes the module's
/// replace-or-rename message. The success line is this build's message envelope
/// (D33), carrying the replace line first when one happened.
pub fn secure_files_upload(
    context: &mut Context,
    project: &str,
    name: &str,
    file: &str,
    allow_exists: bool,
) -> Result<Report, AdoError> {
    let bytes =
        fs::read(file).map_err(|_| AdoError::validation(format!("File not found: {file}")))?;
    let mut replaced = None;

    if allow_exists && let Some(existing_id) = find_secure_file_id_by_name(context, project, name) {
        context
            .client()?
            .delete(&secure_file_path(project, &existing_id), &[])?;

        replaced = Some(existing_id);
    }

    let params = vec![("name".to_owned(), name.to_owned())];

    match context
        .client()?
        .post_binary(&secure_files_path(project), &bytes, &params)
    {
        Ok(response) => {
            let uploaded = format!(
                "Secure file '{}' uploaded (ID: {}, {} bytes).",
                field(&response, "name"),
                field(&response, "id"),
                bytes.len(),
            );
            let message = match replaced {
                Some(existing_id) => {
                    format!("Deleted existing '{name}' (id: {existing_id}).\n{uploaded}")
                }
                None => uploaded,
            };

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::Conflict => Err(AdoError {
            message: upload_conflict_message(name, &error),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `secure_files_delete/1`: `DELETE …/securefiles/{id}` once `--force` is given.
/// The guard is a message, not a question (R6): the oracle prints it and exits 0
/// having sent nothing, while this build refuses with exit 1 — D32's rule that a
/// refusal is never a success — and the same wording on stderr.
pub fn secure_files_delete(
    context: &mut Context,
    project: &str,
    secure_file_id: &str,
    force: bool,
) -> Result<Report, AdoError> {
    if !force {
        return Err(AdoError::cancelled(SECURE_FILE_GUARD));
    }

    match context
        .client()?
        .delete(&secure_file_path(project, secure_file_id), &[])
    {
        Ok(()) => {
            let message = format!("Secure file {secure_file_id} deleted.");

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(AdoError {
            message: format!("Secure file {secure_file_id} not found in project '{project}'"),
            ..error
        }),
        Err(error) => Err(error),
    }
}

/// `list_folders/1`: `GET /{project}/_apis/pipelines` with the module's `folder`
/// (always, `--path` or `/`) and `path` (only with `--path`) params, grouped by
/// each pipeline's own `folder` field. The oracle prints its table even under
/// `--json` (captured); this build emits the raw pipeline list as the value
/// envelope, which is what the module's own doc promises: "Pass --json for raw
/// pipeline data".
pub fn folders_list(
    context: &mut Context,
    project: &str,
    path: Option<String>,
) -> Result<Report, AdoError> {
    let pipelines = items(context.client()?.list(
        &pipelines_collection_path(project),
        &folder_params(path.as_deref()),
    )?);

    Ok(
        context.json_or_report(ok_value(Value::Array(pipelines.clone())), || {
            folders_table(&pipelines)
        }),
    )
}

/// `create_folder/1`: `POST …/pipelines/folders` with the path alone. The
/// created folder is the value envelope under `--json` (D33); the human line is
/// the module's.
pub fn folders_create(
    context: &mut Context,
    project: &str,
    path: &str,
) -> Result<Report, AdoError> {
    let folder = context.client()?.post(
        &folders_collection_path(project),
        &json!({"path": path}),
        &[],
    )?;

    Ok(context.json_or_report(ok_value(folder.clone()), || {
        Report::Text(format!("Folder '{path}' created."))
    }))
}

/// `delete_folder/1`: `DELETE …/pipelines/folders/{path}`. No question and no
/// `--force` (captured on `n` and on EOF, R5); the success line is this build's
/// message envelope (D33).
pub fn folders_delete(
    context: &mut Context,
    project: &str,
    path: &str,
) -> Result<Report, AdoError> {
    context.client()?.delete(&folder_path(project, path), &[])?;

    let message = format!("Folder '{path}' deleted.");

    Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
}

/// `find_secure_file_id_by_name/2`: look the name up through the server-side
/// `namePattern` filter and take the first exact match's `id`. A failed lookup
/// and a match without a usable id both read as not found (captured: the oracle
/// proceeds to the upload when the lookup fails; a match it cannot read an id
/// from makes it exit 0 silently).
fn find_secure_file_id_by_name(context: &mut Context, project: &str, name: &str) -> Option<String> {
    let params = vec![("namePattern".to_owned(), name.to_owned())];
    let listing = context
        .client()
        .ok()?
        .list(&secure_files_path(project), &params)
        .ok()?;

    items(listing)
        .iter()
        .find(|file| file.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|file| file.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The module's `list_pipeline_vars/1` mapping: each entry of
/// `configuration.variables` as `{"key", "value", "isSecret"}`, where an absent
/// value or `isSecret` is `null` — the module reads `v["value"]`/`v["isSecret"]`,
/// which is `nil` for a nil variable.
fn pipeline_variables(pipeline: &Value) -> Vec<Value> {
    pipeline_variables_map(pipeline)
        .into_iter()
        .map(|(key, variable)| {
            json!({
                "key": key,
                "value": variable.get("value").cloned().unwrap_or(Value::Null),
                "isSecret": variable.get("isSecret").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

/// `get_in(pipeline, ["configuration", "variables"]) || %{}`, read as a map: a
/// missing `configuration`, a missing `variables`, and a shape that is not an
/// object all read as none.
fn pipeline_variables_map(pipeline: &Value) -> Map<String, Value> {
    pipeline
        .pointer("/configuration/variables")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// `put_in(pipeline, ["configuration", "variables"], variables)`: the whole
/// pipeline object with exactly that nested key set. The module's `put_in`
/// creates the `configuration` map when the pipeline has none; this build does
/// the same (the oracle exits 1 silently there — the captured carry).
fn pipeline_with_variables(pipeline: &Value, variables: Map<String, Value>) -> Value {
    let mut body = match pipeline {
        Value::Object(fields) => fields.clone(),
        _ => Map::new(),
    };
    let mut configuration = body
        .get("configuration")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    configuration.insert("variables".to_owned(), Value::Object(variables));
    body.insert("configuration".to_owned(), Value::Object(configuration));

    Value::Object(body)
}

/// The `Helpers.extract_error_message/1` half of the upload's 409 message: the
/// API body's `message`, or the module's `"name conflict"` fallback.
fn upload_conflict_message(name: &str, error: &AdoError) -> String {
    let body_message = error
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
        .unwrap_or_else(|| "name conflict".to_owned());

    format!(
        "A secure file named '{name}' already exists. Re-run with --allow-exists to replace it, \
         or use a different name. ({body_message})"
    )
}

/// The module's `print_pipeline_vars_table/1`: Key, Value, Secret, with its own
/// message for an empty list. A secret shows only its label.
fn variables_table(variables: &[Value]) -> Report {
    if variables.is_empty() {
        return Report::Text("No pipeline variables defined.".to_owned());
    }

    let rows = variables
        .iter()
        .map(|variable| {
            vec![
                field(variable, "key"),
                interpolated(variable.get("value")),
                match variable.get("isSecret") {
                    Some(value) if truthy(value) => "yes".to_owned(),
                    _ => "no".to_owned(),
                },
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["Key".to_owned(), "Value".to_owned(), "Secret".to_owned()],
        rows,
    }
}

/// The module's `print_secure_files_table/1`: ID, Name, Size, Modified, with its
/// own message for an empty list; a missing size or timestamp is `?`.
fn secure_files_table(files: &[Value]) -> Report {
    if files.is_empty() {
        return Report::Text("No secure files found.".to_owned());
    }

    let rows = files
        .iter()
        .map(|file| {
            vec![
                field(file, "id"),
                field(file, "name"),
                match file.get("contentLength") {
                    None | Some(Value::Null) => "?".to_owned(),
                    Some(size) => text(size),
                },
                match file.get("modifiedOn") {
                    Some(value) if truthy(value) => text(value),
                    _ => "?".to_owned(),
                },
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Name".to_owned(),
            "Size".to_owned(),
            "Modified".to_owned(),
        ],
        rows,
    }
}

/// The module's `print_secure_file_detail/1`, minus the colour: the labels and
/// the `by …` fallback are the formatter's.
fn secure_file_detail(file: &Value) -> Report {
    let mut detail = String::from("\n");

    detail.push_str("Secure File Details\n");
    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:        {}\n", field(file, "id")));
    detail.push_str(&format!("  Name:      {}\n", field(file, "name")));
    detail.push_str(&format!(
        "  Size:      {} bytes\n",
        interpolated(file.get("contentLength"))
    ));
    detail.push_str(&format!(
        "  Created:   {} by {}\n",
        interpolated(file.get("createdOn")),
        display_name(file, "createdBy")
    ));
    detail.push_str(&format!(
        "  Modified:  {} by {}\n",
        interpolated(file.get("modifiedOn")),
        display_name(file, "modifiedBy")
    ));
    detail.push('\n');

    Report::Text(detail)
}

/// `get_in(file, [key, "displayName"]) || "?"`: a missing actor, one without a
/// display name, and a `false` display name all read as `?`.
fn display_name(file: &Value, key: &str) -> String {
    file.get(key)
        .and_then(|actor| actor.get("displayName"))
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_owned()
}

/// The module's `delete_var_group/1` project-id lookup: `GET /_apis/projects`,
/// the first entry whose `name` equals the argument, and its `id` as the
/// `projectIds` pair. A lookup that fails and a project the list does not name
/// both leave the query without the pair.
fn resolved_project_ids(context: &mut Context, project: &str) -> Vec<(String, String)> {
    let lookup = match context.client() {
        Ok(client) => client.list(PROJECTS_PATH, &[]),
        Err(_) => return Vec::new(),
    };

    match lookup {
        Ok(value) => items(value)
            .iter()
            .find(|candidate| candidate.get("name").and_then(Value::as_str) == Some(project))
            .and_then(|found| found.get("id"))
            .and_then(Value::as_str)
            .map(|id| vec![("projectIds".to_owned(), id.to_owned())])
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// `/{project}/_apis/pipelines/{pipeline_id}`, the per-definition path the reads,
/// the mutations and the variables paths all share.
fn pipeline_path(project: &str, pipeline_id: i64) -> String {
    format!(
        "/{}/_apis/pipelines/{pipeline_id}",
        encode_path_segment(project)
    )
}

/// The variable-group collection path both `vars` paths build on.
fn vars_collection_path(project: &str) -> String {
    format!(
        "/{}/_apis/distributedtask/variablegroups",
        encode_path_segment(project)
    )
}

fn vars_group_path(project: &str, group_id: i64) -> String {
    format!("{}/{group_id}", vars_collection_path(project))
}

/// `/{project}/_apis/distributedtask/securefiles`, the collection the four
/// secure-file paths build on.
/// The module's `list_folders/1` params: `folder` always, defaulting to the root
/// as the oracle sends it (`path || "/"`), and `path` only when `--path` was
/// given. Captured: `folder=%2F` alone, or both pairs carrying the same value.
fn folder_params(path: Option<&str>) -> Vec<(String, String)> {
    match path {
        Some(path) => vec![
            ("folder".to_owned(), path.to_owned()),
            ("path".to_owned(), path.to_owned()),
        ],
        None => vec![("folder".to_owned(), "/".to_owned())],
    }
}

fn folders_collection_path(project: &str) -> String {
    format!("{}/folders", pipelines_collection_path(project))
}

fn folder_path(project: &str, path: &str) -> String {
    format!(
        "{}/{}",
        folders_collection_path(project),
        encode_folder_path(path)
    )
}

/// A folder path's `/` separators are the hierarchy the API addresses, so they
/// stay separators — as they do through the oracle's `URI.encode/1` — while each
/// segment carries `encode_path_segment`'s RFC 3986 set, so a segment cannot end
/// the path (D22: the oracle's `URI.encode/1` kept `?` and turned it into a
/// query, captured).
fn encode_folder_path(path: &str) -> String {
    path.split('/')
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// The module's `list_folders/1` grouping and table: folders sorted by their raw
/// value (the oracle's term order, where a missing folder — nil — precedes every
/// binary), each with its pipeline count, the missing folder displayed as `/`.
fn folders_table(pipelines: &[Value]) -> Report {
    if pipelines.is_empty() {
        return Report::Text("No folders found.".to_owned());
    }

    let mut counts: BTreeMap<Option<String>, usize> = BTreeMap::new();

    for pipeline in pipelines {
        let folder = pipeline
            .get("folder")
            .and_then(Value::as_str)
            .map(str::to_owned);
        *counts.entry(folder).or_insert(0) += 1;
    }

    let rows = counts
        .into_iter()
        .map(|(folder, count)| vec![folder.unwrap_or_else(|| "/".to_owned()), count.to_string()])
        .collect();

    Report::Table {
        headers: vec!["Folder".to_owned(), "Pipelines".to_owned()],
        rows,
    }
}

fn pipelines_collection_path(project: &str) -> String {
    format!("/{}/_apis/pipelines", encode_path_segment(project))
}

fn secure_files_path(project: &str) -> String {
    format!(
        "/{}/_apis/distributedtask/securefiles",
        encode_path_segment(project)
    )
}

fn secure_file_path(project: &str, secure_file_id: &str) -> String {
    format!(
        "{}/{}",
        secure_files_path(project),
        encode_path_segment(secure_file_id)
    )
}

/// The module's `list_var_groups/1` params: `$top` only when the option is
/// present (present means sent, `--top 0` included).
fn var_list_params(top: Option<i64>) -> Vec<(String, String)> {
    top.map(|top| vec![("$top".to_owned(), top.to_string())])
        .unwrap_or_default()
}

/// The module's `print_var_groups_table/1`: ID, Name, Description (the
/// formatter's 38-character slice) and the variable count, with its own message
/// for an empty list.
fn vars_table(groups: &[Value]) -> Report {
    if groups.is_empty() {
        return Report::Text("No variable groups found.".to_owned());
    }

    let rows = groups
        .iter()
        .map(|group| {
            vec![
                id_cell(group),
                field(group, "name"),
                description_cell(group),
                variables_count(group).to_string(),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Name".to_owned(),
            "Description".to_owned(),
            "Variables".to_owned(),
        ],
        rows,
    }
}

/// `String.slice(g["description"] || "", 0, 38)`: a missing or false
/// description is empty, and anything longer is cut to 38 characters.
fn description_cell(group: &Value) -> String {
    group
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(38)
        .collect()
}

/// `map_size(g["variables"])`: how many variables the group carries. The module
/// raises on a group without a variable map; this build reads that as none.
fn variables_count(group: &Value) -> usize {
    group
        .get("variables")
        .and_then(Value::as_object)
        .map_or(0, Map::len)
}

/// The module's `print_var_group_detail/1`, minus the colour: the four labelled
/// lines (description falling back to `(none)`), then one variable name per line
/// — a secret shows only its marker, never a value.
fn vars_detail(group: &Value) -> Report {
    let mut detail = String::from("\n");

    detail.push_str("Variable Group Details\n");
    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:          {}\n", id_cell(group)));
    detail.push_str(&format!("  Name:        {}\n", field(group, "name")));
    detail.push_str(&format!("  Description: {}\n", description_or_none(group)));
    detail.push_str(&format!("  Type:        {}\n", field(group, "type")));

    if let Some(variables) = group.get("variables").filter(|value| truthy(value)) {
        detail.push('\n');
        detail.push_str("  Variables:\n");

        for (key, variable) in variables.as_object().into_iter().flatten() {
            let secret = match variable.get("isSecret") {
                Some(value) if truthy(value) => " [secret]",
                _ => "",
            };

            detail.push_str(&format!("    {key}{secret}\n"));
        }
    }

    detail.push('\n');

    Report::Text(detail)
}

/// `group["description"] || "(none)"`.
fn description_or_none(group: &Value) -> String {
    match group.get("description") {
        Some(value) if truthy(value) => interpolated(Some(value)),
        _ => "(none)".to_owned(),
    }
}

/// The module's `parse_var_group_variables/2`: comma-separated `KEY=VALUE` pairs,
/// each trimmed, with a pair without `=` dropped; every kept pair becomes
/// `{"value": …, "isSecret": …}`, secret exactly when its key is in the trimmed
/// `--secret` list.
fn parse_var_group_variables(variables: &str, secret: Option<&str>) -> Map<String, Value> {
    let secret_keys = parse_secret_keys(secret);

    variables
        .split(',')
        .map(str::trim)
        .filter_map(|pair| {
            pair.split_once('=').map(|(key, value)| {
                (
                    key.to_owned(),
                    json!({"value": value, "isSecret": secret_keys.contains(key)}),
                )
            })
        })
        .collect()
}

/// The module's `parse_secret_keys/1`: a nil or empty option is no secrets, and
/// anything else is its trimmed comma-separated keys.
fn parse_secret_keys(secret: Option<&str>) -> HashSet<&str> {
    match secret {
        None | Some("") => HashSet::new(),
        Some(keys) => keys.split(',').map(str::trim).collect(),
    }
}

/// `#{}` interpolation of a decoded JSON value: a missing key and `nil` are
/// empty, a string is itself, anything else its JSON text.
fn interpolated(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// The module's `list_pipelines/1` params: `$top` then `folder`, each added only
/// when the option is present. `if value` is truthy for `0` and `""`, so an
/// explicit zero top and an explicit empty folder both reach the wire.
fn list_params(top: Option<i64>, folder: Option<String>) -> Vec<(String, String)> {
    let mut params = Vec::new();

    if let Some(top) = top {
        params.push(("$top".to_owned(), top.to_string()));
    }
    if let Some(folder) = folder {
        params.push(("folder".to_owned(), folder));
    }

    params
}

/// The module's `print_pipelines_table/1`: the formatter's columns are ID, Name and
/// Folder, and its empty answer is the module's own message.
fn pipelines_table(pipelines: &[Value]) -> Report {
    if pipelines.is_empty() {
        return Report::Text("No pipelines found.".to_owned());
    }

    let rows = pipelines
        .iter()
        .map(|pipeline| {
            vec![
                id_cell(pipeline),
                field(pipeline, "name"),
                folder_cell(pipeline),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Name".to_owned(), "Folder".to_owned()],
        rows,
    }
}

/// The module's `print_pipeline_detail/1`, minus the colour: the labels and
/// fallbacks are the formatter's, and the optional Type/Path/Web lines follow its
/// truthy guards.
fn pipeline_detail(pipeline: &Value) -> Report {
    let mut detail = String::from("\n");

    detail.push_str("Pipeline Details\n");
    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:     {}\n", id_cell(pipeline)));
    detail.push_str(&format!("  Name:   {}\n", field(pipeline, "name")));
    detail.push_str(&format!("  Folder: {}\n", folder_cell(pipeline)));
    detail.push_str(&format!("  URL:    {}\n", field(pipeline, "url")));

    if let Some(configuration) = pipeline.get("configuration").filter(|value| truthy(value)) {
        detail.push_str(&format!(
            "  Type:   {}\n",
            field_or(configuration, "type", "?")
        ));

        if let Some(path) = configuration.get("path").filter(|value| truthy(value)) {
            detail.push_str(&format!("  Path:   {}\n", text(path)));
        }
    }

    if let Some(web) = pipeline
        .get("_links")
        .and_then(|links| links.get("web"))
        .filter(|value| truthy(value))
    {
        detail.push_str(&format!("  Web:    {}\n", field(web, "href")));
    }

    detail.push('\n');

    Report::Text(detail)
}

/// `to_string(p["id"] || "")`: a numeric id reads as its text, a string id as
/// itself, and a missing, null or false id is empty.
fn id_cell(pipeline: &Value) -> String {
    match pipeline.get("id") {
        Some(Value::String(id)) => id.clone(),
        Some(Value::Null) | Some(Value::Bool(false)) | None => String::new(),
        Some(id) => id.to_string(),
    }
}

/// `p["folder"] || "/"`: `nil` and `false` fall back to the root marker, while an
/// explicit empty folder is truthy in Elixir and stays empty.
fn folder_cell(pipeline: &Value) -> String {
    match pipeline.get("folder") {
        Some(value) if truthy(value) => text(value),
        _ => "/".to_owned(),
    }
}

/// The `||` fallback of the module's `configuration["type"] || "?"`: only `nil` and
/// `false` fall through, so an empty type stays empty.
fn field_or(value: &Value, key: &str, fallback: &str) -> String {
    match value.get(key) {
        Some(value) if truthy(value) => text(value),
        _ => fallback.to_owned(),
    }
}

/// Elixir's truthiness for a decoded JSON value: only `null` and `false` are falsy.
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

/// A JSON value as the text `#{}` would interpolate: a string as itself, anything
/// else by its JSON text.
fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `value["key"] || ""`: a missing or non-string field is empty.
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
    fn list_params_keeps_present_values_in_the_modules_order() {
        assert_eq!(
            list_params(None, None),
            Vec::new(),
            "an absent options map sends no param beyond api-version"
        );
        assert_eq!(
            list_params(Some(5), Some("MyTeam/Frontend".to_owned())),
            vec![
                ("$top".to_owned(), "5".to_owned()),
                ("folder".to_owned(), "MyTeam/Frontend".to_owned()),
            ],
            "the module's own order: $top, then folder"
        );
        assert_eq!(
            list_params(Some(0), Some(String::new())),
            vec![
                ("$top".to_owned(), "0".to_owned()),
                ("folder".to_owned(), String::new()),
            ],
            "present means sent: an explicit zero top and an empty folder"
        );
        assert_eq!(
            list_params(Some(-1), None),
            vec![("$top".to_owned(), "-1".to_owned())],
            "a negative top is sent, as the oracle sends it"
        );
    }

    #[test]
    fn pipelines_table_uses_the_module_columns() {
        let pipelines = vec![
            json!({"id": 12, "name": "Alpha CI", "folder": "\\MyTeam\\Frontend"}),
            json!({"id": 7, "name": "Alpha Nightly", "folder": "\\"}),
        ];

        assert_eq!(
            pipelines_table(&pipelines),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "Folder".to_owned()],
                rows: vec![
                    vec![
                        "12".to_owned(),
                        "Alpha CI".to_owned(),
                        "\\MyTeam\\Frontend".to_owned(),
                    ],
                    vec!["7".to_owned(), "Alpha Nightly".to_owned(), "\\".to_owned()],
                ],
            },
            "the formatter's cell values, with the folder as the API sends it"
        );
    }

    #[test]
    fn pipelines_table_reads_missing_fields_as_the_module_does() {
        let pipelines = vec![json!({"id": "7", "name": null}), json!({"folder": ""})];

        assert_eq!(
            pipelines_table(&pipelines),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "Folder".to_owned()],
                rows: vec![
                    vec!["7".to_owned(), String::new(), "/".to_owned()],
                    vec![String::new(), String::new(), String::new()],
                ],
            },
            "a string id reads as itself; a missing folder is '/', an explicit empty one is not"
        );
    }

    #[test]
    fn pipelines_table_of_nothing_is_the_module_message() {
        assert_eq!(
            pipelines_table(&[]),
            Report::Text("No pipelines found.".to_owned())
        );
    }

    #[test]
    fn pipeline_detail_prints_the_labels_and_the_optional_lines() {
        let pipeline = json!({
            "_links": {
                "web": {"href": "https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=12"},
            },
            "configuration": {"path": "pipelines/ci.yml", "type": "yaml"},
            "folder": "\\MyTeam\\Frontend",
            "id": 12,
            "name": "Alpha CI",
            "url": "https://dev.azure.com/myorg/Alpha/_apis/pipelines/12?revision=4",
        });

        let Report::Text(detail) = pipeline_detail(&pipeline) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.starts_with("\nPipeline Details\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(&format!("{}\n", "─".repeat(60))),
            "detail: {detail}"
        );
        assert!(detail.contains("  ID:     12\n"), "detail: {detail}");
        assert!(detail.contains("  Name:   Alpha CI\n"), "detail: {detail}");
        assert!(
            detail.contains("  Folder: \\MyTeam\\Frontend\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(
                "  URL:    https://dev.azure.com/myorg/Alpha/_apis/pipelines/12?revision=4\n"
            ),
            "detail: {detail}"
        );
        assert!(detail.contains("  Type:   yaml\n"), "detail: {detail}");
        assert!(
            detail.contains("  Path:   pipelines/ci.yml\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(
                "  Web:    https://dev.azure.com/myorg/Alpha/_build/definition?definitionId=12\n"
            ),
            "detail: {detail}"
        );
        assert!(detail.ends_with("definitionId=12\n\n"), "detail: {detail}");
    }

    /// The module's guards are truthy-based: a missing or null `configuration`
    /// prints no Type/Path line, a `configuration` without a type prints `?`, and
    /// the Web line needs a truthy `_links.web`.
    #[test]
    fn pipeline_detail_guards_the_optional_lines_like_the_module() {
        for pipeline in [json!({}), json!({"configuration": null})] {
            let Report::Text(detail) = pipeline_detail(&pipeline) else {
                panic!("the detail is a text report");
            };
            assert!(!detail.contains("  Type:"), "detail: {detail}");
            assert!(!detail.contains("  Path:"), "detail: {detail}");
            assert!(!detail.contains("  Web:"), "detail: {detail}");
        }

        let Report::Text(empty_configuration) = pipeline_detail(&json!({"configuration": {}}))
        else {
            panic!("the detail is a text report");
        };
        assert!(
            empty_configuration.contains("  Type:   ?\n"),
            "an empty configuration is truthy in Elixir: {empty_configuration}"
        );
        assert!(
            !empty_configuration.contains("  Path:"),
            "detail: {empty_configuration}"
        );

        let Report::Text(web) = pipeline_detail(&json!({"_links": {"web": {}}})) else {
            panic!("the detail is a text report");
        };
        assert!(
            web.contains("  Web:    \n"),
            "an empty web object is truthy, so the label prints empty: {web}"
        );

        for pipeline in [json!({"_links": {}}), json!({"_links": {"web": null}})] {
            let Report::Text(detail) = pipeline_detail(&pipeline) else {
                panic!("the detail is a text report");
            };
            assert!(!detail.contains("  Web:"), "detail: {detail}");
        }
    }

    /// `#{pipeline["id"]}` and `pipeline["folder"] || "/"` for values that are not
    /// the usual strings.
    #[test]
    fn pipeline_detail_keeps_an_explicit_empty_type_and_an_empty_folder() {
        let pipeline = json!({
            "configuration": {"type": ""},
            "folder": "",
        });

        let Report::Text(detail) = pipeline_detail(&pipeline) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.contains("  Type:   \n"),
            "an empty type is truthy, not the '?' fallback: {detail}"
        );
        assert!(
            detail.contains("  Folder: \n"),
            "an empty folder is truthy, not the '/' fallback: {detail}"
        );
    }

    #[test]
    fn id_cell_reads_numbers_strings_and_missing_ids() {
        assert_eq!(id_cell(&json!({"id": 12})), "12");
        assert_eq!(id_cell(&json!({"id": "12"})), "12");
        assert_eq!(id_cell(&json!({"id": null})), "");
        assert_eq!(id_cell(&json!({"id": false})), "");
        assert_eq!(id_cell(&json!({})), "");
    }

    #[test]
    fn folder_cell_falls_back_only_for_nil_and_false() {
        assert_eq!(
            folder_cell(&json!({"folder": "\\MyTeam\\Frontend"})),
            "\\MyTeam\\Frontend"
        );
        assert_eq!(folder_cell(&json!({"folder": "\\"})), "\\");
        assert_eq!(folder_cell(&json!({"folder": ""})), "");
        assert_eq!(folder_cell(&json!({"folder": null})), "/");
        assert_eq!(folder_cell(&json!({"folder": false})), "/");
        assert_eq!(folder_cell(&json!({})), "/");
    }

    // ── the write helpers ────────────────────────────────────────────────

    #[test]
    fn run_body_defaults_to_main_and_wraps_each_variable() {
        assert_eq!(
            run_body(None, None),
            json!({"resources": {"repositories": {"self": {"refName": "refs/heads/main"}}}}),
            "the frozen default branch, and no variables key when the option is absent"
        );
        assert_eq!(
            run_body(Some("feature/foo"), Some("ENV=staging,DEBUG=true")),
            json!({
                "resources": {"repositories": {"self": {"refName": "refs/heads/feature/foo"}}},
                "variables": {"ENV": {"value": "staging"}, "DEBUG": {"value": "true"}},
            }),
            "each pair is trimmed into its own value object"
        );
    }

    #[test]
    fn run_body_drops_a_pair_without_an_equals_sign() {
        assert_eq!(
            run_body(None, Some("NOEQUALS")),
            json!({
                "resources": {"repositories": {"self": {"refName": "refs/heads/main"}}},
                "variables": {},
            }),
            "captured: the pair is dropped and the key stays"
        );
        assert_eq!(
            run_body(None, Some("A=b=c")),
            json!({
                "resources": {"repositories": {"self": {"refName": "refs/heads/main"}}},
                "variables": {"A": {"value": "b=c"}},
            }),
            "String.split(parts: 2): everything after the first = is the value"
        );
    }

    #[test]
    fn create_body_defaults_the_folder_to_the_root() {
        assert_eq!(
            create_body("New CI", "Alpha.Core", "pipelines/new.yml", None)["folder"],
            json!("/"),
            "an absent --folder is the module's '/' default"
        );
        assert_eq!(
            create_body("New CI", "Alpha.Core", "pipelines/new.yml", Some(""))["folder"],
            json!(""),
            "a present empty --folder is a value, not the default"
        );
        assert_eq!(
            create_body("New CI", "Alpha.Core", "pipelines/new.yml", Some("MyTeam"))["configuration"]
                ["repository"],
            json!({"id": "Alpha.Core", "name": "Alpha.Core", "type": "azureReposGit"}),
            "the repository is both id and name"
        );
    }

    #[test]
    fn var_group_body_omits_variables_and_description_when_absent() {
        assert_eq!(
            var_group_body("Alpha", "new-group", None, None, None),
            json!({
                "name": "new-group",
                "type": "Vsts",
                "variableGroupProjectReferences": [
                    {"name": "new-group", "projectReference": {"name": "Alpha"}},
                ],
            }),
            "captured: no --variables and no --description is neither key"
        );
        assert_eq!(
            var_group_body("Alpha", "new-group", Some(""), Some(""), None),
            json!({
                "name": "new-group",
                "type": "Vsts",
                "variableGroupProjectReferences": [
                    {"name": "new-group", "projectReference": {"name": "Alpha"}},
                ],
                "description": "",
                "variables": {},
            }),
            "a present empty value is truthy in Elixir, so both keys ship"
        );
    }

    #[test]
    fn parse_var_group_variables_marks_only_the_named_secrets() {
        assert_eq!(
            parse_var_group_variables("DB_HOST=db,DB_PASS=x", Some("DB_PASS,API_KEY")),
            serde_json::Map::from_iter([
                (
                    "DB_HOST".to_owned(),
                    json!({"value": "db", "isSecret": false})
                ),
                (
                    "DB_PASS".to_owned(),
                    json!({"value": "x", "isSecret": true})
                ),
            ]),
            "the trimmed --secret list decides isSecret"
        );
        assert_eq!(
            parse_var_group_variables("A=1", Some("")),
            serde_json::Map::from_iter([(
                "A".to_owned(),
                json!({"value": "1", "isSecret": false})
            )]),
            "an empty --secret list marks nothing"
        );
    }

    #[test]
    fn merge_var_group_body_prefers_the_options_and_keeps_the_existing_references() {
        let existing = json!({
            "name": "prod-secrets",
            "description": "Production secrets",
            "type": "Vsts",
            "variableGroupProjectReferences": [{"name": "prod-secrets", "projectReference": {"name": "Alpha"}}],
            "variables": {"DB_PASS": {"isSecret": true}},
        });

        assert_eq!(
            merge_var_group_body(
                &existing,
                Some("renamed"),
                Some("new description"),
                Some("DB_PASS=hunter2"),
                Some("DB_PASS"),
            ),
            json!({
                "name": "renamed",
                "description": "new description",
                "type": "Vsts",
                "variableGroupProjectReferences": [{"name": "prod-secrets", "projectReference": {"name": "Alpha"}}],
                "variables": {"DB_PASS": {"value": "hunter2", "isSecret": true}},
            }),
            "the options win; a named variable replaces the existing one; the references stay"
        );
    }

    #[test]
    fn merge_var_group_body_keeps_every_option_unset_from_the_existing_group() {
        let existing = json!({
            "name": "prod-secrets",
            "description": "Production secrets",
            "type": "Vsts",
            "variables": {"DB_HOST": {"value": "db"}},
        });

        assert_eq!(
            merge_var_group_body(&existing, None, None, None, None),
            json!({
                "name": "prod-secrets",
                "description": "Production secrets",
                "type": "Vsts",
                "variableGroupProjectReferences": [],
                "variables": {"DB_HOST": {"value": "db"}},
            }),
            "captured: type and references fall back, the existing variables are echoed"
        );
    }

    #[test]
    fn merge_var_group_body_omits_an_empty_variable_map() {
        let existing = json!({"name": "g", "variables": {}});

        assert_eq!(
            merge_var_group_body(&existing, None, None, None, None),
            json!({"name": "g", "type": "Vsts", "variableGroupProjectReferences": []}),
            "captured: an empty merged map is no variables key"
        );
        assert_eq!(
            merge_var_group_body(&existing, None, None, Some("A=1"), None)["variables"],
            json!({"A": {"value": "1", "isSecret": false}}),
            "a pair adds the map back"
        );
    }

    #[test]
    fn merge_var_group_body_omits_a_missing_description() {
        let existing = json!({"name": "g", "type": "Vsts", "variables": {}});

        assert_eq!(
            merge_var_group_body(&existing, None, None, None, None),
            json!({"name": "g", "type": "Vsts", "variableGroupProjectReferences": []}),
            "no existing and no option description is no description key"
        );
        assert_eq!(
            merge_var_group_body(&existing, Some("g"), Some(""), None, None)["description"],
            json!(""),
            "a present empty --description ships"
        );
    }

    #[test]
    fn var_list_params_sends_a_present_zero() {
        assert_eq!(var_list_params(None), Vec::new());
        assert_eq!(
            var_list_params(Some(0)),
            vec![("$top".to_owned(), "0".to_owned())],
            "present means sent"
        );
    }

    #[test]
    fn vars_table_truncates_the_description_and_counts_variables() {
        let groups = vec![
            json!({
                "id": 5,
                "name": "prod-secrets",
                "description": "Production secrets for the deploy stage",
                "variables": {"A": {"value": 1}, "B": {"value": 2}},
            }),
            json!({"id": 6, "name": "ci-shared"}),
        ];

        assert_eq!(
            vars_table(&groups),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Name".to_owned(),
                    "Description".to_owned(),
                    "Variables".to_owned(),
                ],
                rows: vec![
                    vec![
                        "5".to_owned(),
                        "prod-secrets".to_owned(),
                        "Production secrets for the deploy stag".to_owned(),
                        "2".to_owned(),
                    ],
                    vec![
                        "6".to_owned(),
                        "ci-shared".to_owned(),
                        String::new(),
                        "0".to_owned(),
                    ],
                ],
            },
            "the formatter's 38-character slice and map_size/1 count"
        );
    }

    #[test]
    fn vars_table_of_nothing_is_the_module_message() {
        assert_eq!(
            vars_table(&[]),
            Report::Text("No variable groups found.".to_owned())
        );
    }

    #[test]
    fn vars_detail_marks_secrets_and_falls_back_on_missing_fields() {
        let Report::Text(detail) = vars_detail(&json!({
            "id": 5,
            "name": "prod-secrets",
            "variables": {"DB_HOST": {"value": "db"}, "DB_PASS": {"isSecret": true}},
        })) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.starts_with("\nVariable Group Details\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(&format!("{}\n", "─".repeat(60))),
            "detail: {detail}"
        );
        assert!(detail.contains("  ID:          5\n"), "detail: {detail}");
        assert!(
            detail.contains("  Name:        prod-secrets\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Description: (none)\n"),
            "a missing description falls back: {detail}"
        );
        assert!(detail.contains("  Type:        \n"), "detail: {detail}");
        assert!(detail.contains("\n  Variables:\n"), "detail: {detail}");
        assert!(detail.contains("    DB_HOST\n"), "detail: {detail}");
        assert!(
            detail.contains("    DB_PASS [secret]\n"),
            "a secret shows its marker only: {detail}"
        );
        assert!(
            !detail.contains("db"),
            "no variable value is printed: {detail}"
        );
        assert!(detail.ends_with("\n\n"), "detail: {detail:?}");
    }

    #[test]
    fn vars_detail_without_variables_prints_no_variables_block() {
        for group in [json!({}), json!({"variables": null})] {
            let Report::Text(detail) = vars_detail(&group) else {
                panic!("the detail is a text report");
            };
            assert!(!detail.contains("Variables:"), "detail: {detail}");
        }
    }

    #[test]
    fn interpolated_reads_a_missing_key_and_nil_as_empty() {
        assert_eq!(interpolated(None), "");
        assert_eq!(interpolated(Some(&Value::Null)), "");
        assert_eq!(interpolated(Some(&json!("main"))), "main");
        assert_eq!(interpolated(Some(&json!(99))), "99");
    }

    // ── the pipeline variables (Task 5) ──────────────────────────────────

    #[test]
    fn pipeline_path_encodes_the_project_segment() {
        assert_eq!(pipeline_path("Alpha", 12), "/Alpha/_apis/pipelines/12");
        assert_eq!(
            pipeline_path("My Project", 12),
            "/My%20Project/_apis/pipelines/12",
            "a name cannot change the URL's structure (D22)"
        );
    }

    /// The module's `Enum.map(vars, fn {k, v} -> %{"key" => k, "value" =>
    /// v["value"], "isSecret" => v["isSecret"]} end)`: a nil variable and a
    /// non-object one read as `null` fields rather than raising (the oracle
    /// exits 0 silently on the shapes Access cannot index).
    #[test]
    fn pipeline_variables_maps_each_variable_with_nulls_for_missing_fields() {
        let pipeline = json!({
            "configuration": {
                "variables": {
                    "DB_HOST": {"value": "db.example.com"},
                    "DB_PASS": {"isSecret": true},
                    "DEBUG": {"value": "false", "isSecret": false},
                    "NULL": null,
                    "TEXT": "x",
                },
            },
        });

        assert_eq!(
            pipeline_variables(&pipeline),
            vec![
                json!({"key": "DB_HOST", "value": "db.example.com", "isSecret": null}),
                json!({"key": "DB_PASS", "value": null, "isSecret": true}),
                json!({"key": "DEBUG", "value": "false", "isSecret": false}),
                json!({"key": "NULL", "value": null, "isSecret": null}),
                json!({"key": "TEXT", "value": null, "isSecret": null}),
            ],
            "key-sorted, one record per variable"
        );
    }

    #[test]
    fn pipeline_variables_reads_an_absent_or_non_object_map_as_empty() {
        for pipeline in [
            json!({}),
            json!({"configuration": null}),
            json!({"configuration": {"variables": null}}),
            json!({"configuration": {"variables": []}}),
            json!({"configuration": {"variables": "x"}}),
        ] {
            assert_eq!(
                pipeline_variables(&pipeline),
                Vec::<Value>::new(),
                "{pipeline} has no variable map"
            );
        }
    }

    /// `put_in(pipeline, ["configuration", "variables"], …)`: the whole object
    /// with exactly that key set. The module's own `put_in` creates a missing
    /// `configuration`; the oracle exits 1 silently there (the captured carry),
    /// and this build follows the `put_in` shape instead.
    #[test]
    fn pipeline_with_variables_sets_the_nested_key_and_creates_configuration() {
        let pipeline = json!({"id": 7, "configuration": {"type": "yaml"}});

        assert_eq!(
            pipeline_with_variables(
                &pipeline,
                Map::from_iter([("A".to_owned(), json!({"value": "1"}))])
            ),
            json!({"id": 7, "configuration": {"type": "yaml", "variables": {"A": {"value": "1"}}}}),
            "the existing configuration keys are kept"
        );

        assert_eq!(
            pipeline_with_variables(
                &json!({"id": 7}),
                Map::from_iter([("A".to_owned(), json!({"value": "1"}))])
            ),
            json!({"id": 7, "configuration": {"variables": {"A": {"value": "1"}}}}),
            "a missing configuration is created, as put_in/3 does"
        );

        assert_eq!(
            pipeline_with_variables(&json!(["not", "a", "map"]), Map::new()),
            json!({"configuration": {"variables": {}}}),
            "a non-object pipeline body degrades to the object put_in would build"
        );
    }

    #[test]
    fn variables_table_marks_secrets_and_reads_missing_values_as_empty() {
        let variables = vec![
            json!({"key": "DB_HOST", "value": "db.example.com", "isSecret": null}),
            json!({"key": "DB_PASS", "value": null, "isSecret": true}),
            json!({"key": "COUNT", "value": 3, "isSecret": 0}),
        ];

        assert_eq!(
            variables_table(&variables),
            Report::Table {
                headers: vec!["Key".to_owned(), "Value".to_owned(), "Secret".to_owned()],
                rows: vec![
                    vec![
                        "DB_HOST".to_owned(),
                        "db.example.com".to_owned(),
                        "no".to_owned()
                    ],
                    vec!["DB_PASS".to_owned(), String::new(), "yes".to_owned()],
                    vec!["COUNT".to_owned(), "3".to_owned(), "yes".to_owned()],
                ],
            },
            "a secret is a label, a nil value is empty, and 0 is truthy in Elixir"
        );
        assert_eq!(
            variables_table(&[]),
            Report::Text("No pipeline variables defined.".to_owned())
        );
    }

    // ── the secure files (Task 5) ────────────────────────────────────────

    #[test]
    fn secure_files_table_reads_missing_fields_as_question_marks() {
        let files = vec![
            json!({"id": "f47ac10b", "name": "prod-cert.pem", "contentLength": 2048, "modifiedOn": "2026-09-20T12:00:00.000Z"}),
            json!({"name": "legacy.pem"}),
        ];

        assert_eq!(
            secure_files_table(&files),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Name".to_owned(),
                    "Size".to_owned(),
                    "Modified".to_owned(),
                ],
                rows: vec![
                    vec![
                        "f47ac10b".to_owned(),
                        "prod-cert.pem".to_owned(),
                        "2048".to_owned(),
                        "2026-09-20T12:00:00.000Z".to_owned(),
                    ],
                    vec![
                        String::new(),
                        "legacy.pem".to_owned(),
                        "?".to_owned(),
                        "?".to_owned(),
                    ],
                ],
            },
            "captured: a missing id is empty, a missing size and timestamp are ?"
        );
        assert_eq!(
            secure_files_table(&[]),
            Report::Text("No secure files found.".to_owned())
        );
    }

    #[test]
    fn secure_file_detail_formats_the_block_with_the_actor_fallback() {
        let Report::Text(detail) = secure_file_detail(&json!({
            "id": "f47ac10b",
            "name": "prod-cert.pem",
            "contentLength": 2048,
            "createdOn": "2026-09-01T09:00:00.000Z",
            "modifiedOn": "2026-09-20T12:00:00.000Z",
            "createdBy": {"displayName": "Ada Lovelace"},
            "modifiedBy": {},
        })) else {
            panic!("the detail is a text report");
        };

        assert!(
            detail.starts_with("\nSecure File Details\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains(&format!("{}\n", "─".repeat(60))),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  ID:        f47ac10b\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Name:      prod-cert.pem\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Size:      2048 bytes\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Created:   2026-09-01T09:00:00.000Z by Ada Lovelace\n"),
            "detail: {detail}"
        );
        assert!(
            detail.contains("  Modified:  2026-09-20T12:00:00.000Z by ?\n"),
            "an actor without a display name falls back: {detail}"
        );
        assert!(detail.ends_with("\n\n"), "detail: {detail:?}");
    }

    #[test]
    fn upload_conflict_message_uses_the_api_message_or_falls_back() {
        let api_message = "A secure file with name 'cert.pem' already exists.";
        let conflict = AdoError {
            status: Some(409),
            details: Some(
                json!({"status": 409, "body": format!("{{\"message\":\"{api_message}\"}}")}),
            ),
            ..AdoError::from_status(409, "")
        };

        assert_eq!(
            upload_conflict_message("cert.pem", &conflict),
            format!(
                "A secure file named 'cert.pem' already exists. Re-run with --allow-exists to \
                 replace it, or use a different name. ({api_message})"
            )
        );

        for body in ["not json", "{\"message\": 42}", "{}"] {
            let unreadable = AdoError {
                details: Some(json!({"status": 409, "body": body})),
                ..AdoError::from_status(409, "")
            };

            assert!(
                upload_conflict_message("cert.pem", &unreadable).ends_with("(name conflict)"),
                "the module's fallback for {body:?}"
            );
        }
    }

    #[test]
    fn secure_file_paths_encode_the_segments() {
        assert_eq!(
            secure_files_path("Alpha"),
            "/Alpha/_apis/distributedtask/securefiles"
        );
        assert_eq!(
            secure_file_path("Alpha", "f47ac10b-58cc-4372-a567-0e02b2c3d479"),
            "/Alpha/_apis/distributedtask/securefiles/f47ac10b-58cc-4372-a567-0e02b2c3d479"
        );
        assert_eq!(
            secure_file_path("My Project", "a/b"),
            "/My%20Project/_apis/distributedtask/securefiles/a%2Fb",
            "D22: the id cannot change the URL's structure either"
        );
    }

    #[test]
    fn folder_params_send_folder_always_and_path_only_when_given() {
        assert_eq!(
            folder_params(None),
            vec![("folder".to_owned(), "/".to_owned())],
            "the captured no-path request carries the root as folder alone"
        );
        assert_eq!(
            folder_params(Some("MyTeam/Frontend")),
            vec![
                ("folder".to_owned(), "MyTeam/Frontend".to_owned()),
                ("path".to_owned(), "MyTeam/Frontend".to_owned()),
            ],
            "the captured path request carries both pairs with the same value"
        );
    }

    #[test]
    fn folder_path_encodes_each_segment_and_keeps_the_separators() {
        assert_eq!(
            folder_path("Alpha", "MyTeam/Frontend"),
            "/Alpha/_apis/pipelines/folders/MyTeam/Frontend",
            "the hierarchy the API addresses stays in the URL path (the oracle's URI.encode/1 keeps / too)"
        );
        assert_eq!(
            folder_path("My Project", "My Team/Front end"),
            "/My%20Project/_apis/pipelines/folders/My%20Team/Front%20end",
            "the oracle's own captured bytes for a space (D22 shares %20)"
        );
        assert_eq!(
            folder_path("Alpha", "a?b"),
            "/Alpha/_apis/pipelines/folders/a%3Fb",
            "D22: the oracle's URI.encode/1 kept ? and turned the rest into a query; here a segment cannot end the path"
        );
    }

    #[test]
    fn folders_table_sorts_by_raw_folder_and_displays_the_missing_one_as_root() {
        let pipelines = vec![
            json!({"id": 12, "folder": "\\"}),
            json!({"id": 15, "folder": "MyTeam/Frontend"}),
            json!({"id": 16, "folder": "MyTeam/Frontend"}),
            json!({"id": 17, "folder": null}),
        ];

        assert_eq!(
            folders_table(&pipelines),
            Report::Table {
                headers: vec!["Folder".to_owned(), "Pipelines".to_owned()],
                rows: vec![
                    vec!["/".to_owned(), "1".to_owned()],
                    vec!["MyTeam/Frontend".to_owned(), "2".to_owned()],
                    vec!["\\".to_owned(), "1".to_owned()],
                ],
            },
            "term order: the nil key first, then binaries byte-wise, with null displayed as /"
        );
    }

    #[test]
    fn folders_table_reads_a_folderless_pipeline_as_the_root() {
        let pipelines = vec![json!({"id": 1}), json!({"id": 2, "folder": null})];

        assert_eq!(
            folders_table(&pipelines),
            Report::Table {
                headers: vec!["Folder".to_owned(), "Pipelines".to_owned()],
                rows: vec![vec!["/".to_owned(), "2".to_owned()]],
            },
            "a missing folder and a null folder are the same nil key"
        );
    }

    #[test]
    fn folders_table_of_nothing_is_the_module_message() {
        assert_eq!(
            folders_table(&[]),
            Report::Text("No folders found.".to_owned())
        );
    }
}
