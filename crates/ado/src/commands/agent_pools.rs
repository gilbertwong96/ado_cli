//! `ado agent-pools list|show|queues list` — the whole of
//! `lib/ado_cli/cli/agent_pools.ex`: the organization-scoped
//! `_apis/distributedtask/pools` surface, the project-scoped `queues` under it,
//! and the three views.
//!
//! `show` is the module's two-request chain — the pool, then its agents. The
//! `--json` result merges both when the agents fetch succeeds and carries the
//! bare pool when it fails: the module ignores *any* agents failure and still
//! exits 0. The human detail is the module's `print_pool_detail/1` layout.
//!
//! The human view prints the pool's fields and, since Ruling B3, its agents
//! block (the D42 repair):
//!
//!   * the frozen module wraps the two payloads as an atom-keyed map
//!     (`%{pool: …, agents: …}`) while the formatter looks for the string keys
//!     `"pool"`/`"agents"`, so its four field reads are all `nil`; this build
//!     reads the merged JSON object, whose keys are strings, so the same layout
//!     carries the pool;
//!   * `print_agents_detail/1` prints only for a **list** `agents` member, and
//!     the wrapped value is the agents endpoint's whole body (a map), so the
//!     frozen CLI prints no agents at all. This build hands the human formatter
//!     the endpoint body's `value` list ([`human_view`]), so the block prints
//!     the agents the command's own doc promises; the `--json` envelope keeps
//!     the wrapped body untouched, which is what both sides carry and MATCH.

use ado_core::client::encode_path_segment;
use ado_core::envelope::ok_value;
use ado_core::error::{AdoError, ErrorCode};
use serde_json::{Value, json};

use crate::commands::items::items;
use crate::context::Context;
use crate::output::Report;

/// The organization-scoped collection both pool commands share; the client
/// injects the organization ahead of it.
const POOLS: &str = "/_apis/distributedtask/pools";

/// `ado agent-pools list`: `GET /_apis/distributedtask/pools`. The value array
/// unwraps to the value envelope; the human path is the module's four-column
/// table.
pub fn list(context: &mut Context) -> Result<Report, AdoError> {
    let pools = items(context.client()?.list(POOLS, &[])?);

    Ok(
        context.json_or_report(ok_value(Value::Array(pools.clone())), || {
            pools_table(&pools)
        }),
    )
}

/// `ado agent-pools show`: `GET …/pools/{pool_id}`, then `…/agents`. A 404 on the
/// pool takes the module's `Agent pool #<id> not found` wording ([`ErrorCode::NotFound`]'s
/// class); any agents failure falls back to the bare pool.
pub fn show(context: &mut Context, pool_id: i64) -> Result<Report, AdoError> {
    let pool = match context.client()?.get(&pool_path(pool_id), &[]) {
        Ok(pool) => pool,
        Err(error) if error.code == ErrorCode::NotFound => {
            return Err(AdoError {
                message: format!("Agent pool #{pool_id} not found"),
                ..error
            });
        }
        Err(error) => return Err(error),
    };

    let merged = match context.client()?.get(&agents_path(pool_id), &[]) {
        Ok(agents) => json!({"pool": pool, "agents": agents}),
        Err(_) => pool,
    };

    Ok(context.json_or_report(ok_value(merged.clone()), || {
        Report::Text(pool_detail(&human_view(&merged)))
    }))
}

/// `ado agent-pools queues list PROJECT [--pool POOL_ID]`:
/// `GET /{project}/_apis/distributedtask/queues`, `poolId` only when `--pool` is
/// given.
pub fn queues_list(
    context: &mut Context,
    project: &str,
    pool: Option<i64>,
) -> Result<Report, AdoError> {
    let queues = items(
        context
            .client()?
            .list(&queues_path(project), &pool_params(pool))?,
    );

    Ok(
        context.json_or_report(ok_value(Value::Array(queues.clone())), || {
            queues_table(&queues)
        }),
    )
}

/// One pool below the collection.
fn pool_path(pool_id: i64) -> String {
    format!("{POOLS}/{pool_id}")
}

/// The pool's agents, a child of the pool it belongs to.
fn agents_path(pool_id: i64) -> String {
    format!("{POOLS}/{pool_id}/agents")
}

/// The queue collection of one project; the frozen module encodes the project
/// with `URI.encode/1` and this build escapes every segment strictly (D22).
fn queues_path(project: &str) -> String {
    format!(
        "/{}/_apis/distributedtask/queues",
        encode_path_segment(project)
    )
}

/// The module's `if pool = Map.get(parsed.options, :pool), do: %{"poolId" => pool}`.
fn pool_params(pool: Option<i64>) -> Vec<(String, String)> {
    pool.map(|pool| vec![("poolId".to_owned(), pool.to_string())])
        .unwrap_or_default()
}

/// The module's `print_pools_table/1` columns (ID, Name, Auto-provision, Type),
/// with the module's "No agent pools found." when empty.
fn pools_table(pools: &[Value]) -> Report {
    if pools.is_empty() {
        return Report::Text("No agent pools found.".to_owned());
    }

    let rows = pools
        .iter()
        .map(|pool| {
            vec![
                value_text(pool.get("id")),
                value_text(pool.get("name")),
                auto_provision(pool),
                pool_type(pool),
            ]
        })
        .collect();

    Report::Table {
        headers: vec![
            "ID".to_owned(),
            "Name".to_owned(),
            "Auto-provision".to_owned(),
            "Type".to_owned(),
        ],
        rows,
    }
}

/// The module's `p["autoProvision"] || false`: only nil and `false` fall back.
fn auto_provision(pool: &Value) -> String {
    match pool.get("autoProvision") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "false".to_owned(),
        Some(value) => value_text(Some(value)),
    }
}

/// The module's `p["poolType"] || "?"`.
fn pool_type(pool: &Value) -> String {
    match pool.get("poolType") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "?".to_owned(),
        Some(value) => value_text(Some(value)),
    }
}

/// The module's `print_queues_table/1` columns (ID, Name, Pool), with the
/// module's "No agent queues found." when empty.
fn queues_table(queues: &[Value]) -> Report {
    if queues.is_empty() {
        return Report::Text("No agent queues found.".to_owned());
    }

    let rows = queues
        .iter()
        .map(|queue| {
            vec![
                value_text(queue.get("id")),
                value_text(queue.get("name")),
                queue_pool_name(queue),
            ]
        })
        .collect();

    Report::Table {
        headers: vec!["ID".to_owned(), "Name".to_owned(), "Pool".to_owned()],
        rows,
    }
}

/// `q["pool"]["name"] || q["pool"]["id"] || ""` — a missing `pool` object reads
/// as empty, exactly as the module's `nil["name"]` does.
fn queue_pool_name(queue: &Value) -> String {
    let pool = queue.get("pool");

    match pool.and_then(|pool| pool.get("name")) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => {
            match pool.and_then(|pool| pool.get("id")) {
                None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
                Some(value) => value_text(Some(value)),
            }
        }
        Some(value) => value_text(Some(value)),
    }
}

/// The value handed to the human formatter: the agents member unwrapped from the
/// endpoint's body map to its `value` list, so `print_agents_detail/1`'s list
/// guard is reached and the block prints (Ruling B3 — D42's repair). Every other
/// shape is left alone: the bare-pool path (a failed agents fetch) and a body
/// without a `value` list keep the no-block detail.
fn human_view(data: &Value) -> Value {
    let Some(items) = data
        .get("agents")
        .and_then(|agents| agents.get("value"))
        .and_then(Value::as_array)
    else {
        return data.clone();
    };

    let mut view = data.clone();
    view["agents"] = Value::Array(items.clone());
    view
}

/// The module's `print_pool_detail/1`: the `─` rule, the four labelled fields,
/// and the agents block `print_agents_detail/1` draws for a **list** `agents`
/// member. The pool is the merged result's `pool` member, or the whole result
/// when the agents fetch failed. The command calls this with [`human_view`]'s
/// result, so the agents member is the endpoint's `value` list and the block is
/// reached (Ruling B3).
fn pool_detail(data: &Value) -> String {
    let pool = data.get("pool").unwrap_or(data);
    let mut detail = String::from("\nAgent Pool Details\n\n");

    detail.push_str(&"─".repeat(60));
    detail.push('\n');
    detail.push_str(&format!(
        "  ID:            {}\n",
        value_text(pool.get("id"))
    ));
    detail.push_str(&format!(
        "  Name:          {}\n",
        value_text(pool.get("name"))
    ));
    detail.push_str(&format!(
        "  Type:          {}\n",
        value_text(pool.get("poolType"))
    ));
    detail.push_str(&format!(
        "  Auto-provision: {}\n",
        value_text(pool.get("autoProvision"))
    ));

    if let Some(agents) = data.get("agents").and_then(Value::as_array) {
        detail.push_str(&format!("\n  Agents ({}):\n", agents.len()));

        for agent in agents {
            detail.push_str(&format!(
                "    {:<30}  {}  {}\n",
                value_text(agent.get("name")),
                agent_status(agent),
                value_text(agent.get("version"))
            ));
        }
    }

    detail.push('\n');

    detail
}

/// `a["status"] || "unknown"`.
fn agent_status(agent: &Value) -> String {
    match agent.get("status") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "unknown".to_owned(),
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
    fn the_params_carry_pool_id_only_when_given() {
        assert_eq!(pool_params(None), Vec::new());
        assert_eq!(
            pool_params(Some(1)),
            vec![("poolId".to_owned(), "1".to_owned())]
        );
    }

    #[test]
    fn the_paths_encode_the_project_and_name_the_pool_collection() {
        assert_eq!(pool_path(1), "/_apis/distributedtask/pools/1");
        assert_eq!(agents_path(1), "/_apis/distributedtask/pools/1/agents");
        assert_eq!(queues_path("Alpha"), "/Alpha/_apis/distributedtask/queues");
        assert_eq!(
            queues_path("Alpha/Beta"),
            "/Alpha%2FBeta/_apis/distributedtask/queues",
            "a slash cannot split the path (D22)"
        );
    }

    #[test]
    fn the_pool_table_uses_the_module_columns_and_its_empty_sentence() {
        assert_eq!(
            pools_table(&[]),
            Report::Text("No agent pools found.".to_owned())
        );

        assert_eq!(
            pools_table(&[
                json!({"id": 1, "name": "Default", "autoProvision": true, "poolType": "automation"})
            ]),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Name".to_owned(),
                    "Auto-provision".to_owned(),
                    "Type".to_owned()
                ],
                rows: vec![vec![
                    "1".to_owned(),
                    "Default".to_owned(),
                    "true".to_owned(),
                    "automation".to_owned()
                ]],
            }
        );
    }

    #[test]
    fn the_pool_table_falls_back_for_the_two_optional_columns() {
        assert_eq!(
            pools_table(&[json!({"id": 2, "name": "Hosted"})]),
            Report::Table {
                headers: vec![
                    "ID".to_owned(),
                    "Name".to_owned(),
                    "Auto-provision".to_owned(),
                    "Type".to_owned()
                ],
                rows: vec![vec![
                    "2".to_owned(),
                    "Hosted".to_owned(),
                    "false".to_owned(),
                    "?".to_owned()
                ]],
            },
            "the module's `p[\"autoProvision\"] || false` and `p[\"poolType\"] || \"?\"`"
        );
    }

    #[test]
    fn the_queue_table_reads_the_nested_pool_name() {
        assert_eq!(
            queues_table(&[]),
            Report::Text("No agent queues found.".to_owned())
        );

        assert_eq!(
            queues_table(&[
                json!({"id": 3, "name": "Alpha Pool Queue", "pool": {"id": 1, "name": "Default"}}),
                json!({"id": 4, "name": "Hosted Queue", "pool": {"id": 2}}),
                json!({"id": 5, "name": "Bare Queue"}),
            ]),
            Report::Table {
                headers: vec!["ID".to_owned(), "Name".to_owned(), "Pool".to_owned()],
                rows: vec![
                    vec![
                        "3".to_owned(),
                        "Alpha Pool Queue".to_owned(),
                        "Default".to_owned()
                    ],
                    vec!["4".to_owned(), "Hosted Queue".to_owned(), "2".to_owned()],
                    vec!["5".to_owned(), "Bare Queue".to_owned(), String::new()],
                ],
            },
            "the name falls back to the id, then to empty"
        );
    }

    #[test]
    fn the_agents_block_prints_only_for_a_list_member() {
        let detail = pool_detail(&json!({
            "pool": {"id": 1, "name": "Default", "poolType": "automation", "autoProvision": true},
            "agents": [
                {"name": "agent-1", "status": "online", "version": "3.230.0"},
                {"name": "agent-2", "status": "offline"},
            ],
        }));

        assert_eq!(
            detail,
            format!(
                concat!(
                    "\nAgent Pool Details\n\n",
                    "{}\n",
                    "  ID:            1\n",
                    "  Name:          Default\n",
                    "  Type:          automation\n",
                    "  Auto-provision: true\n",
                    "\n",
                    "  Agents (2):\n",
                    "    agent-1                         online  3.230.0\n",
                    "    agent-2                         offline  \n",
                    "\n",
                ),
                "─".repeat(60)
            )
        );
    }

    #[test]
    fn the_human_view_unwraps_the_agents_list() {
        let wrapped = json!({
            "pool": {"id": 1, "name": "Default"},
            "agents": {"count": 2, "value": [
                {"name": "agent-1", "status": "online", "version": "3.230.0"},
                {"name": "agent-2", "status": "offline"},
            ]},
        });
        let view = human_view(&wrapped);

        assert_eq!(view["pool"], wrapped["pool"], "the pool passes through");
        assert_eq!(
            view["agents"],
            json!([
                {"name": "agent-1", "status": "online", "version": "3.230.0"},
                {"name": "agent-2", "status": "offline"},
            ])
        );
        assert_eq!(
            wrapped["agents"]["value"],
            json!([
                {"name": "agent-1", "status": "online", "version": "3.230.0"},
                {"name": "agent-2", "status": "offline"},
            ]),
            "the wrapped input is untouched: the --json envelope keeps it"
        );
    }

    #[test]
    fn the_human_view_leaves_every_other_shape_alone() {
        for data in [
            json!({"id": 2, "name": "Hosted"}),
            json!({"pool": {"id": 2}, "agents": {"count": 0}}),
            json!({"pool": {"id": 2}, "agents": {"value": "not a list"}}),
        ] {
            assert_eq!(human_view(&data), data, "{data}");
        }
    }

    #[test]
    fn the_bare_pool_detail_keeps_the_fields_and_omits_the_agents_block() {
        let detail = pool_detail(&json!({"id": 2, "name": "Hosted"}));
        assert_eq!(
            detail,
            format!(
                concat!(
                    "\nAgent Pool Details\n\n",
                    "{}\n",
                    "  ID:            2\n",
                    "  Name:          Hosted\n",
                    "  Type:          \n",
                    "  Auto-provision: \n",
                    "\n",
                ),
                "─".repeat(60)
            ),
            "the wrapped map's absent members interpolate as the empty string"
        );
    }

    #[test]
    fn an_absent_status_reads_unknown() {
        assert_eq!(
            agent_status(&json!({"name": "agent-1"})),
            "unknown".to_owned()
        );
        assert_eq!(
            agent_status(&json!({"status": "offline"})),
            "offline".to_owned()
        );
    }
}
