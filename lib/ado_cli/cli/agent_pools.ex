defmodule AdoCli.CLI.AgentPools do
  @moduledoc """
  Commands for managing Azure DevOps agent pools and queues.

    ado agent-pools list
    ado agent-pools show POOL_ID
    ado agent-pools queues list [--pool POOL_ID]
  """

  @behaviour CliMate.CLI.Command
  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado agent-pools",
      doc:
        "Manage Azure DevOps agent pools and queues. Pools host agents; queues are project-scoped views into pools used for pipeline runs.",
      subcommands: [
        list: [
          name: "ado agent-pools list",
          doc:
            "List every agent pool in the organization. Output is a table by default (ID, Name, Auto-provision, Type); pass --json for a machine-readable array. Use this to discover pool IDs for use with `pipelines-builds queue --pool` or for agent management.",
          execute: &list_pools/1
        ],
        show: [
          name: "ado agent-pools show",
          doc:
            "Show details of an agent pool, including its agents and their status. The pool ID is an integer (not a name); use `ado agent-pools list` to look it up.",
          arguments: [pool_id: [type: :integer, doc: "Numeric agent pool ID"]],
          execute: &show_pool/1
        ],
        queues: [
          name: "ado agent-pools queues",
          doc:
            "Manage agent queues. A queue is a project-scoped alias for a pool — pipelines run on a queue, not a pool directly.",
          subcommands: [
            list: [
              name: "ado agent-pools queues list",
              doc:
                "List agent queues in a project. Output is a table (ID, Name, Pool); pass --json for raw data. Use --pool to filter by a specific pool's queues.",
              arguments: [project: [type: :string, doc: "Project name or ID"]],
              options: [
                pool: [type: :integer, doc: "Filter by numeric agent pool ID", doc_arg: "POOL_ID"]
              ],
              execute: &list_queues/1
            ]
          ]
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_pools(parsed) do
    result = Client.list("/_apis/distributedtask/pools")

    Helpers.handle_api_result(result, parsed, fn pools ->
      Helpers.json_or_format(pools, parsed, &print_pools_table/1)
    end)
  end

  def show_pool(parsed) do
    pool_id = parsed.arguments.pool_id

    case Client.get("/_apis/distributedtask/pools/#{pool_id}") do
      {:ok, pool} ->
        # Also fetch agents
        case Client.get("/_apis/distributedtask/pools/#{pool_id}/agents") do
          {:ok, agents} ->
            Helpers.json_or_format(%{pool: pool, agents: agents}, parsed, &print_pool_detail/1)

          _ ->
            Helpers.json_or_format(pool, parsed, &print_pool_detail/1)
        end

      {:error, %{status: 404}} ->
        halt_error("Agent pool ##{pool_id} not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def list_queues(parsed) do
    project = parsed.arguments.project
    params = %{}

    params =
      if pool = Map.get(parsed.options, :pool), do: Map.put(params, "poolId", pool), else: params

    result = Client.list("/#{URI.encode(project)}/_apis/distributedtask/queues", params)

    Helpers.handle_api_result(result, parsed, fn queues ->
      Helpers.json_or_format(queues, parsed, &print_queues_table/1)
    end)
  end

  defp print_pools_table(pools) do
    if Enum.empty?(pools) do
      writeln("No agent pools found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 6)}  #{String.pad_trailing("Name", 30)}  Auto-provision  Type"
      )

      writeln(String.duplicate("─", 70))

      Enum.each(pools, fn p ->
        provision_status = to_string(p["autoProvision"] || false)

        writeln(
          "#{String.pad_trailing(to_string(p["id"] || ""), 6)}  #{String.pad_trailing(p["name"] || "", 30)}  #{String.pad_trailing(provision_status, 14)}  #{p["poolType"] || "?"}"
        )
      end)

      writeln("")
      writeln("#{length(pools)} pool(s)")
    end
  end

  defp print_pool_detail(data) do
    pool = if is_map(data) and Map.has_key?(data, "pool"), do: data["pool"], else: data

    writeln("")
    success("Agent Pool Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:            #{pool["id"]}")
    writeln("  Name:          #{pool["name"]}")
    writeln("  Type:          #{pool["poolType"]}")
    writeln("  Auto-provision: #{pool["autoProvision"]}")
    print_agents_detail(data)
    writeln("")
  end

  defp print_agents_detail(%{"agents" => agents}) when is_list(agents) do
    writeln("")
    writeln("  Agents (#{length(agents)}):")

    Enum.each(agents, fn a ->
      status = a["status"] || "unknown"
      writeln("    #{String.pad_trailing(a["name"] || "", 30)}  #{status}  #{a["version"] || ""}")
    end)
  end

  defp print_agents_detail(_), do: :ok

  defp print_queues_table(queues) do
    if Enum.empty?(queues) do
      writeln("No agent queues found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 6)}  #{String.pad_trailing("Name", 30)}  Pool")
      writeln(String.duplicate("─", 60))

      Enum.each(queues, fn q ->
        pool_name = q["pool"]["name"] || q["pool"]["id"] || ""

        writeln(
          "#{String.pad_trailing(to_string(q["id"] || ""), 6)}  #{String.pad_trailing(q["name"] || "", 30)}  #{pool_name}"
        )
      end)

      writeln("")
      writeln("#{length(queues)} queue(s)")
    end
  end
end
