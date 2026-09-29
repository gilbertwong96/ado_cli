//! `ado teams list|show|create|update|delete` and `ado teams members list` — the
//! team half of `lib/ado_cli/cli/teams.ex`: the `_apis/teams` REST surface, the
//! table and detail views, and the module's own 404 wording. `delete` never
//! prompts (R1/R5): the frozen CLI sends its DELETE on `n` and on EOF.

use ado_core::client::encode_path_segment;
use ado_core::envelope::{ok_message, ok_value};
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Map, Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The module's guard when `update` carries no option at all.
const NO_OPTIONS: &str = "At least one of --name or --description is required.";

/// `ado teams list`: `GET /{project}/_apis/teams`, with the module's `$top` when
/// `--top` is given. The value array unwraps to the value envelope; the human
/// path is the module's three-column table.
pub fn list(context: &mut Context, project: &str, top: Option<i64>) -> Result<Report, AdoError> {
    let params = top_params(top);
    let teams = items(context.client()?.list(&teams_path(project), &params)?);

    Ok(
        context.json_or_report(ok_value(Value::Array(teams.clone())), || {
            teams_table(&teams)
        }),
    )
}

/// `ado teams show`: `GET …/teams/{team_id}`; a 404 takes the module's own
/// wording.
pub fn show(context: &mut Context, project: &str, team_id: &str) -> Result<Report, AdoError> {
    let path = team_path(project, team_id);

    match context.client()?.get(&path, &[]) {
        Ok(team) => {
            Ok(context.json_or_report(ok_value(team.clone()), || Report::Text(team_detail(&team))))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(not_found(error, team_id)),
        Err(error) => Err(error),
    }
}

/// `ado teams create`: `POST` the collection with the module's `{"name" => …}`
/// body, plus `description` only when given. The response names the created
/// team, so the human line carries its name and id.
pub fn create(
    context: &mut Context,
    project: &str,
    name: &str,
    description: Option<String>,
) -> Result<Report, AdoError> {
    let team =
        context
            .client()?
            .post(&teams_path(project), &create_body(name, description), &[])?;

    Ok(context.json_or_report(ok_value(team.clone()), || {
        Report::Text(format!(
            "Team '{}' created (ID: {}).",
            value_text(team.get("name")),
            value_text(team.get("id")),
        ))
    }))
}

/// `ado teams update`: `PATCH` the team with only the fields given; no field at
/// all is the module's guard, which precedes any request. A 404 takes the
/// module's own wording.
pub fn update(
    context: &mut Context,
    project: &str,
    team_id: &str,
    name: Option<String>,
    description: Option<String>,
) -> Result<Report, AdoError> {
    if name.is_none() && description.is_none() {
        return Err(AdoError::validation(NO_OPTIONS));
    }

    let path = team_path(project, team_id);

    match context
        .client()?
        .patch(&path, &update_body(name, description), &[])
    {
        Ok(team) => Ok(context.json_or_report(ok_value(team.clone()), || {
            Report::Text(format!("Team '{}' updated.", value_text(team.get("name"))))
        })),
        Err(error) if error.code == ErrorCode::NotFound => Err(not_found(error, team_id)),
        Err(error) => Err(error),
    }
}

/// `ado teams delete`: the module's plain `DELETE`, without a prompt (R1/R5).
pub fn delete(context: &mut Context, project: &str, team_id: &str) -> Result<Report, AdoError> {
    let path = team_path(project, team_id);

    match context.client()?.delete(&path, &[]) {
        Ok(()) => {
            let message = format!("Team '{team_id}' deleted.");

            Ok(context.json_or_report(ok_message(&message), || Report::Text(message)))
        }
        Err(error) if error.code == ErrorCode::NotFound => Err(not_found(error, team_id)),
        Err(error) => Err(error),
    }
}

/// `ado teams members list`: `GET …/teams/{team_id}/members`, the nested leaf one
/// level below `teams`'s own `show`. The value array unwraps to the value
/// envelope; the human path is the module's three-column table.
pub fn members_list(
    context: &mut Context,
    project: &str,
    team_id: &str,
) -> Result<Report, AdoError> {
    let members = items(
        context
            .client()?
            .list(&members_path(project, team_id), &[])?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(members.clone())), || {
            members_table(&members)
        }),
    )
}

/// The collection path: `/{project}/_apis/teams`.
fn teams_path(project: &str) -> String {
    format!("/{}/_apis/teams", encode_path_segment(project))
}

/// One team below the collection; the id is a single segment, so an id that
/// carries a separator or an `@` cannot change the URL's structure (D22).
fn team_path(project: &str, team_id: &str) -> String {
    format!("{}/{}", teams_path(project), encode_path_segment(team_id))
}

/// The members collection of one team: `…/teams/{team_id}/members`.
fn members_path(project: &str, team_id: &str) -> String {
    format!("{}/members", team_path(project, team_id))
}

/// The module's `params = if top = Map.get(parsed.options, :top), do: …, else: %{}`.
fn top_params(top: Option<i64>) -> Vec<(String, String)> {
    top.map(|top| vec![("$top".to_owned(), top.to_string())])
        .unwrap_or_default()
}

/// The module's `%{"name" => name}` body, plus `"description"` only when given.
fn create_body(name: &str, description: Option<String>) -> Value {
    let mut body = json!({ "name": name });

    if let Some(description) = description {
        body.as_object_mut()
            .expect("the create body is an object")
            .insert("description".to_owned(), json!(description));
    }

    body
}

/// The module's update body: only the fields whose options were given, so an
/// absent option contributes no key at all.
fn update_body(name: Option<String>, description: Option<String>) -> Value {
    let mut body = Map::new();

    if let Some(name) = name {
        body.insert("name".to_owned(), json!(name));
    }
    if let Some(description) = description {
        body.insert("description".to_owned(), json!(description));
    }

    Value::Object(body)
}

/// `halt_error("Team '<id>' not found")` keeps the module's wording on the
/// classified error, so `--json` carries it in the envelope (D4's class).
fn not_found(error: AdoError, team_id: &str) -> AdoError {
    AdoError {
        message: format!("Team '{team_id}' not found"),
        ..error
    }
}

/// The module's `print_teams_table/1` columns (ID, Name, Description), with the
/// module's "No teams found." when empty.
fn teams_table(teams: &[Value]) -> Report {
    if teams.is_empty() {
        return Report::Text("No teams found.".to_owned());
    }

    let rows = teams
        .iter()
        .map(|team| {
            vec![
                value_text(team.get("id")),
                value_text(team.get("name")),
                value_text(team.get("description")),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Name".to_owned(), "Description".to_owned()],
        rows,
    }
}

/// The module's `print_team_members_table/1` columns (ID, Display Name, Unique
/// Name), with the module's "No members found." when empty. The display name and
/// unique name are `identity`'s fields.
fn members_table(members: &[Value]) -> Report {
    if members.is_empty() {
        return Report::Text("No members found.".to_owned());
    }

    let rows = members
        .iter()
        .map(|member| {
            let identity = member.get("identity");

            vec![
                value_text(member.get("id")),
                value_text(identity.and_then(|identity| identity.get("displayName"))),
                value_text(identity.and_then(|identity| identity.get("uniqueName"))),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Display Name".to_owned(),
            "Unique Name".to_owned(),
        ],
        rows,
    }
}

/// The module's `print_team_detail/1`, with its `─` rule, `description || "(none)"`
/// and the URL line printed even when the field is absent (as the empty string).
fn team_detail(team: &Value) -> String {
    let mut detail = String::from("\nTeam Details\n\n");

    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!("  ID:          {}\n", value_text(team.get("id"))));
    detail.push_str(&format!(
        "  Name:        {}\n",
        value_text(team.get("name"))
    ));
    detail.push_str(&format!(
        "  Description: {}\n",
        description_text(team.get("description"))
    ));
    detail.push_str(&format!("  URL:         {}\n", value_text(team.get("url"))));

    detail
}

/// The module's `team["description"] || "(none)"`: `nil` falls back, and an empty
/// string is a value like any other (only `nil` and `false` are falsy).
fn description_text(description: Option<&Value>) -> String {
    match description {
        None | Some(Value::Null) => "(none)".to_owned(),
        Some(value) => value_text(Some(value)),
    }
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
    fn top_params_only_carry_a_given_top() {
        assert_eq!(top_params(None), Vec::new());
        assert_eq!(
            top_params(Some(5)),
            vec![("$top".to_owned(), "5".to_owned())]
        );
        assert_eq!(
            top_params(Some(0)),
            vec![("$top".to_owned(), "0".to_owned())],
            "zero is a value: the Elixir's `if top` is truthy for 0"
        );
    }

    #[test]
    fn a_names_only_create_body_has_no_description_key() {
        assert_eq!(create_body("Team", None), json!({"name": "Team"}));
        assert_eq!(
            create_body("Team", Some("A team".to_owned())),
            json!({"name": "Team", "description": "A team"})
        );
    }

    #[test]
    fn an_update_body_carries_only_the_given_fields() {
        assert_eq!(
            update_body(Some("Renamed".to_owned()), None),
            json!({"name": "Renamed"})
        );
        assert_eq!(
            update_body(None, Some("New desc".to_owned())),
            json!({"description": "New desc"})
        );
        assert_eq!(
            update_body(Some("Renamed".to_owned()), Some("New desc".to_owned())),
            json!({"name": "Renamed", "description": "New desc"})
        );
        assert_eq!(update_body(None, None), json!({}));
    }

    #[test]
    fn the_table_uses_the_module_columns_and_its_empty_sentence() {
        assert_eq!(teams_table(&[]), Report::Text("No teams found.".to_owned()));

        let teams =
            vec![json!({"id": "team-1", "name": "Alpha Team", "description": "The alpha team"})];

        assert_eq!(
            teams_table(&teams),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "Description".to_owned()],
                rows: vec![vec![
                    "team-1".to_owned(),
                    "Alpha Team".to_owned(),
                    "The alpha team".to_owned(),
                ]],
            }
        );
    }

    #[test]
    fn the_members_table_reads_the_identity_fields() {
        assert_eq!(
            members_table(&[]),
            Report::Text("No members found.".to_owned())
        );

        let members = vec![json!({
            "id": "member-1",
            "identity": {"displayName": "Ada Lovelace", "uniqueName": "ada@example.com"},
        })];

        assert_eq!(
            members_table(&members),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Display Name".to_owned(),
                    "Unique Name".to_owned()
                ],
                rows: vec![vec![
                    "member-1".to_owned(),
                    "Ada Lovelace".to_owned(),
                    "ada@example.com".to_owned(),
                ]],
            }
        );
    }

    #[test]
    fn the_detail_prints_the_frozen_labels_and_the_description_fallback() {
        let detail = team_detail(&json!({"id": "team-1", "name": "Alpha Team"}));

        assert_eq!(
            detail,
            format!(
                "\nTeam Details\n\n{}\n  ID:          team-1\n  Name:        Alpha Team\n  Description: (none)\n  URL:         \n",
                "─".repeat(60)
            ),
            "a missing description falls back; a missing url prints empty"
        );

        let detail = team_detail(&json!({"description": ""}));

        assert!(
            detail.contains("  Description: \n"),
            "an empty description is a value, not the fallback: {detail:?}"
        );
    }

    #[test]
    fn the_paths_encode_each_segment_strictly() {
        assert_eq!(teams_path("Alpha Beta"), "/Alpha%20Beta/_apis/teams");
        assert_eq!(team_path("Alpha", "team/1"), "/Alpha/_apis/teams/team%2F1");
        assert_eq!(
            team_path("Alpha", "ada@example.com"),
            "/Alpha/_apis/teams/ada%40example.com",
            "an @ is escaped, unlike the frozen URI.encode/1 (D22)"
        );
        assert_eq!(
            members_path("Alpha", "team-1"),
            "/Alpha/_apis/teams/team-1/members"
        );
    }
}
