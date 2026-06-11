defmodule AdoCli.CLI.Releases do
  @moduledoc """
  Commands for managing Azure DevOps Releases.

    ado_cli releases list PROJECT
    ado_cli releases show PROJECT RELEASE_ID
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado_cli releases",
      doc: "Manage Azure DevOps releases.",
      subcommands: [
        list: [
          name: "ado_cli releases list",
          doc: "List releases in a project.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            top: [type: :integer, doc: "Maximum number of releases to return", doc_arg: "N"],
            definition_id: [type: :integer, doc: "Filter by release definition ID", doc_arg: "ID"],
            status: [
              type: :string,
              doc: "Filter by status (active, abandoned, draft, undefined)",
              doc_arg: "STATUS"
            ]
          ],
          execute: &list_releases/1
        ],
        show: [
          name: "ado_cli releases show",
          doc: "Show details of a specific release.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            release_id: [type: :integer, doc: "Release ID"]
          ],
          execute: &show_release/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  # ── Handlers ──────────────────────────────────────────────────────────

  @doc """
  Lists releases in a project.

  Supports `--top`, `--definition-id`, and `--status` filters.
  """
  def list_releases(parsed) do
    project = parsed.arguments.project

    params =
      %{}
      |> put_if(Map.get(parsed.options, :top), "$top")
      |> put_if(Map.get(parsed.options, :definition_id), "definitionId")
      |> put_if(Map.get(parsed.options, :status), "statusFilter")

    result = Client.list("/#{URI.encode(project)}/_apis/release/releases", params)

    Helpers.handle_api_result(result, parsed, fn releases ->
      Helpers.json_or_format(releases, parsed, &print_releases_table/1)
    end)
  end

  @doc """
  Shows details of a specific release including environment statuses.
  """
  def show_release(parsed) do
    project = parsed.arguments.project
    release_id = parsed.arguments.release_id

    case Client.get("/#{URI.encode(project)}/_apis/release/releases/#{release_id}") do
      {:ok, release} ->
        Helpers.json_or_format(release, parsed, &print_release_detail/1)

      {:error, %{status: 404}} ->
        halt_error("Release ##{release_id} not found in project '#{project}'")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Helpers ───────────────────────────────────────────────────────────

  defp put_if(map, nil, _key), do: map
  defp put_if(map, value, key), do: Map.put(map, key, value)

  # ── Formatting ────────────────────────────────────────────────────────

  defp print_releases_table(releases) do
    if Enum.empty?(releases) do
      writeln("No releases found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 6)}  #{String.pad_trailing("Name", 45)}  Status     Created"
      )

      writeln(String.duplicate("─", 110))

      Enum.each(releases, fn r ->
        id = to_string(r["id"] || "")
        name = r["name"] || ""
        status = r["status"] || "unknown"
        created = truncate_date(get_in(r, ["createdOn"]))

        writeln(
          "#{String.pad_trailing(id, 6)}  #{String.pad_trailing(name, 45)}  #{String.pad_trailing(status, 10)} #{created}"
        )
      end)

      writeln("")
      writeln("#{length(releases)} release(s)")
    end
  end

  defp print_release_detail(release) do
    writeln("")
    writeln(success("Release Details"))
    writeln(String.duplicate("─", 60))
    writeln("  ID:          #{release["id"]}")
    writeln("  Name:        #{release["name"]}")
    writeln("  Status:      #{release["status"]}")

    if definition = release["releaseDefinition"] do
      writeln("  Definition:  #{definition["name"] || definition["id"]}")
    end

    writeln("  Created On:  #{release["createdOn"]}")

    if created_by = release["createdBy"] do
      writeln("  Created By:  #{created_by["displayName"]}")
    end

    writeln("  URL:         #{release["url"]}")

    if environments = release["environments"] do
      writeln("")
      writeln("  Environments:")

      Enum.each(environments, fn env ->
        env_name = env["name"] || env["definitionEnvironmentId"]
        env_status = env["status"] || "unknown"
        writeln("    - #{env_name}: #{env_status}")
      end)
    end

    writeln("")
  end

  defp truncate_date(nil), do: ""

  defp truncate_date(date) when is_binary(date) and byte_size(date) > 10,
    do: binary_part(date, 0, 10)

  defp truncate_date(date), do: date
end
