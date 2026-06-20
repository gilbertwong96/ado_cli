defmodule AdoCli.CLI.Connections do
  @moduledoc """
  Commands for managing Azure DevOps service connections.

    ado connections list PROJECT [--type TYPE]
    ado connections show PROJECT CONNECTION_ID
    ado connections create PROJECT --name NAME --type TYPE --url URL
                                   [--description DESC] [--scheme SCHEME]
                                   [--access-token TOKEN] [--data JSON] [--ready]
    ado connections update PROJECT CONNECTION_ID
                                   [--name NAME] [--description DESC] [--url URL]
                                   [--access-token TOKEN] [--data JSON]
    ado connections delete PROJECT CONNECTION_ID [--force]
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
        ],
        create: [
          name: "ado connections create",
          doc:
            "Create a new service connection. The minimum required fields are name, type, and url. Pass --access-token to set the credential for the most common schemes (Token, e.g. GitHub PATs). Use --data with a JSON string for type-specific fields (e.g. subscriptionId for Azure RM, clusterUrl for Kubernetes). Returns the created connection object: 'id' (UUID), 'name', 'type', 'url', 'isReady' (boolean). Secrets are never returned — even with --json.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            name: [
              type: :string,
              doc:
                "Service connection name. Must be unique within the project (1-256 chars). Visible in pipeline agent job UIs."
            ],
            type: [
              type: :string,
              doc:
                "Service connection type (e.g. 'github', 'azure', 'kubernetes', 'dockerregistry', 'git'). For Azure RM, the value is the long type ID — use --data to set the rest."
            ],
            url: [
              type: :string,
              doc:
                "Target URL the connection authenticates against (e.g. 'https://github.com' for GitHub PAT)."
            ]
          ],
          options: [
            description: [
              type: :string,
              doc: "Optional human-readable description shown in the service connection list.",
              doc_arg: "DESC"
            ],
            scheme: [
              type: :string,
              doc:
                "Authorization scheme. Common values: 'Token' (default — sets accessToken parameter), 'UsernamePassword', 'None', 'Certificate'. Ignored if --data supplies the entire authorization object.",
              doc_arg: "SCHEME"
            ],
            access_token: [
              type: :string,
              doc:
                "Credential value. Accepts three forms: a literal string, `-` to read from stdin (no shell history), or `@path/to/file` to read from a file (trailing newline is stripped). Recommended: `echo \"$MY_PAT\" | ado connections create ... --access-token -`",
              doc_arg: "TOKEN"
            ],
            data: [
              type: :string,
              doc:
                "JSON object with type-specific fields merged into the request body (e.g. '{\"subscriptionId\":\"...\",\"subscriptionName\":\"...\"}' for Azure RM). The top-level keys 'name', 'type', 'url', and 'authorization' are reserved; use the dedicated flags for those.",
              doc_arg: "JSON"
            ],
            ready: [
              type: :boolean,
              default: false,
              doc:
                "Validate the connection during create (sets isReady=true). The API will attempt to authenticate and may fail if the credentials are wrong. Default: false — create succeeds even if auth fails, you can authorize later."
            ]
          ],
          execute: &create_connection/1
        ],
        update: [
          name: "ado connections update",
          doc:
            "Update an existing service connection. Pass any of --name, --description, --url, --access-token, or --data; at least one is required. To rotate credentials, pass --access-token (or --data for UsernamePassword / Certificate). Returns the updated connection object: 'id' (UUID), 'name', 'type', 'url', 'isReady' (boolean).",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            connection_id: [type: :string, doc: "Service connection ID (UUID from `list`)"]
          ],
          options: [
            name: [type: :string, doc: "New service connection name", doc_arg: "NAME"],
            description: [
              type: :string,
              doc: "New service connection description",
              doc_arg: "DESC"
            ],
            url: [type: :string, doc: "New target URL", doc_arg: "URL"],
            access_token: [
              type: :string,
              doc:
                "Replace the credential. Same forms as create: literal value, `-` (stdin), or `@path` (from file).",
              doc_arg: "TOKEN"
            ],
            data: [
              type: :string,
              doc:
                "JSON object merged into the request body. Use this to update type-specific fields (e.g. subscriptionId, clusterUrl).",
              doc_arg: "JSON"
            ]
          ],
          execute: &update_connection/1
        ],
        delete: [
          name: "ado connections delete",
          doc:
            "Permanently delete a service connection. IRREVERSIBLE: any pipeline that references the connection will fail until the reference is updated. Use --force in scripts to skip the interactive confirmation prompt.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            connection_id: [type: :string, doc: "Service connection ID (UUID from `list`)"]
          ],
          options: [
            force: [
              type: :boolean,
              default: false,
              doc: "Skip the interactive confirmation prompt (use in scripts/CI)."
            ]
          ],
          execute: &delete_connection/1
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

  def create_connection(parsed) do
    project = parsed.arguments.project

    authorization =
      Map.put(
        %{"scheme" => Map.get(parsed.options, :scheme, "Token"), "parameters" => %{}},
        "parameters",
        put_if_present(%{}, resolve_token(Map.get(parsed.options, :access_token)), "accessToken")
      )

    body = %{
      "name" => parsed.arguments.name,
      "type" => parsed.arguments.type,
      "url" => parsed.arguments.url,
      "authorization" => authorization,
      "isReady" => Map.get(parsed.options, :ready, false),
      "serviceEndpointProjectReferences" => [
        %{
          "projectReference" => %{"name" => project},
          "name" => parsed.arguments.name
        }
      ]
    }

    body = put_if_present(body, Map.get(parsed.options, :description), "description")
    body = merge_data(body, Map.get(parsed.options, :data))

    case Client.post("/#{URI.encode(project)}/_apis/serviceendpoint/endpoints", body) do
      {:ok, conn} ->
        success("Service connection '#{conn["name"]}' created.\n")
        writeln("  ID:    #{conn["id"]}")
        writeln("  Type:  #{conn["type"]}")
        writeln("  URL:   #{conn["url"]}")
        writeln("  Ready: #{conn["isReady"]}")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Project '#{project}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def update_connection(parsed) do
    project = parsed.arguments.project
    conn_id = parsed.arguments.connection_id

    body = %{}
    body = put_if_present(body, Map.get(parsed.options, :name), "name")
    body = put_if_present(body, Map.get(parsed.options, :description), "description")
    body = put_if_present(body, Map.get(parsed.options, :url), "url")

    body =
      if token = resolve_token(Map.get(parsed.options, :access_token)) do
        base = Map.get(body, "authorization", %{"scheme" => "Token", "parameters" => %{}})
        params = Map.put(base["parameters"] || %{}, "accessToken", token)
        Map.put(body, "authorization", Map.put(base, "parameters", params))
      else
        body
      end

    body = merge_data(body, Map.get(parsed.options, :data))

    if body == %{} do
      halt_error(
        "At least one of --name, --description, --url, --access-token, or --data is required."
      )
    end

    case Client.put(
           "/#{URI.encode(project)}/_apis/serviceendpoint/endpoints/#{URI.encode(conn_id)}",
           body
         ) do
      {:ok, conn} ->
        success("Service connection '#{conn["name"]}' updated.\n")
        writeln("  ID:    #{conn["id"]}")
        writeln("  Type:  #{conn["type"]}")
        writeln("  URL:   #{conn["url"]}")
        writeln("  Ready: #{conn["isReady"]}")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Service connection '#{conn_id}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_connection(parsed) do
    project = parsed.arguments.project
    conn_id = parsed.arguments.connection_id

    unless Map.get(parsed.options, :force) do
      AdoCli.CLI.Helpers.confirm_delete("service connection", "#{project}/#{conn_id}")
    end

    case Client.delete(
           "/#{URI.encode(project)}/_apis/serviceendpoint/endpoints/#{URI.encode(conn_id)}"
         ) do
      :ok ->
        success("Service connection '#{conn_id}' deleted from '#{project}'.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Service connection '#{conn_id}' not found")

      {:error, _} = error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc false
  def resolve_token(nil), do: nil
  def resolve_token(""), do: nil
  def resolve_token("-"), do: read_stdin()
  def resolve_token("@" <> path), do: read_file_secret(path)
  def resolve_token(token), do: token

  defp read_stdin do
    String.trim(to_string(IO.read(:stdio, 1_073_741_824)))
  end

  defp read_file_secret(path) do
    case File.read(path) do
      {:ok, content} -> String.trim(content)
      {:error, reason} -> halt_error("Cannot read secret file \"#{path}\": #{reason}")
    end
  end

  defp put_if_present(map, nil, _key), do: map
  defp put_if_present(map, "", _key), do: map
  defp put_if_present(map, value, key), do: Map.put(map, key, value)

  defp merge_data(body, nil), do: body
  defp merge_data(body, ""), do: body

  defp merge_data(body, json) do
    example = ~s({"subscriptionId":"..."})
    invalid = ~s(--data is not valid JSON. Pass an object, e.g. '#{example}')
    wrong_type = ~s(--data must be a JSON object, e.g. '#{example}')

    case JSON.decode(json) do
      {:ok, parsed} when is_map(parsed) ->
        Map.put(body, "data", parsed)

      {:ok, _} ->
        halt_error(wrong_type)

      {:error, _} ->
        halt_error(invalid)
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
