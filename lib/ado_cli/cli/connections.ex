defmodule AdoCli.CLI.Connections do
  @moduledoc """
  Commands for managing Azure DevOps service connections.

    ado connections list PROJECT [--type TYPE]
    ado connections show PROJECT CONNECTION_ID
  """

  @behaviour CliMate.CLI.Command
  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado connections",
      doc:
        "Manage service connections (a.k.a. service endpoints). A service connection stores credentials for external services (Azure subscriptions, GitHub repos, Docker registries, Kubernetes clusters) so pipelines can access them without re-entering secrets.",
      subcommands: [
        list: [
          name: "ado connections list",
          doc:
            "List service connections in a project. Output is a table (ID, Name, Type). Use --type to filter to a specific kind (e.g. 'github', 'kubernetes', 'azure'). Pass --json for raw data.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            type: [
              type: :string,
              doc:
                "Filter by connection type. Common values: 'github', 'azure' (Azure subscription), 'kubernetes' (K8s service account), 'dockerregistry', 'bitbucket', 'git'. Pass the Azure DevOps type ID string exactly as shown in the web UI.",
              doc_arg: "TYPE"
            ]
          ],
          execute: &list_connections/1
        ],
        show: [
          name: "ado connections show",
          doc:
            "Show details of a service connection: ID, name, type, target URL, and ready state. Secrets (passwords, tokens) are NEVER returned by this command — even with --json.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            connection_id: [type: :string, doc: "Service connection ID (UUID from `list`)"]
          ],
          execute: &show_connection/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_connections(parsed) do
    project = parsed.arguments.project
    params = %{}

    params =
      if type = Map.get(parsed.options, :type), do: Map.put(params, "type", type), else: params

    result = Client.list("/#{URI.encode(project)}/_apis/serviceendpoint/endpoints", params)

    Helpers.handle_api_result(result, parsed, fn connections ->
      Helpers.json_or_format(connections, parsed, &print_connections_table/1)
    end)
  end

  def show_connection(parsed) do
    project = parsed.arguments.project
    conn_id = parsed.arguments.connection_id

    case Client.get(
           "/#{URI.encode(project)}/_apis/serviceendpoint/endpoints/#{URI.encode(conn_id)}"
         ) do
      {:ok, conn} -> Helpers.json_or_format(conn, parsed, &print_connection_detail/1)
      {:error, %{status: 404}} -> halt_error("Service connection '#{conn_id}' not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_connections_table(conns) do
    if Enum.empty?(conns) do
      writeln("No service connections found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 40)} #{String.pad_trailing("Name", 30)} Type")
      writeln(String.duplicate("─", 85))

      AdoCli.CLI.Helpers.print_id_name_type_table(conns)

      writeln("")
      writeln("#{length(conns)} connection(s)")
    end
  end

  defp print_connection_detail(conn) do
    writeln("")
    success("Service Connection Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:    #{conn["id"]}")
    writeln("  Name:  #{conn["name"]}")
    writeln("  Type:  #{conn["type"]}")
    writeln("  URL:   #{conn["url"]}")
    writeln("  Ready: #{conn["isReady"]}")
    writeln("")
  end
end
