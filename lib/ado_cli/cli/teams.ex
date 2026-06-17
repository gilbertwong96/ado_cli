defmodule AdoCli.CLI.Teams do
  @moduledoc """
  Commands for managing Azure DevOps teams.

    ado teams list PROJECT
    ado teams show PROJECT TEAM_ID
    ado teams create PROJECT --name NAME [--description DESC]
    ado teams update PROJECT TEAM_ID [--name NAME] [--description DESC]
    ado teams delete PROJECT TEAM_ID
    ado teams members list PROJECT TEAM_ID
  """

  @behaviour CliMate.CLI.Command
  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado teams",
      doc:
        "Manage Azure DevOps teams (groups of members with shared area paths and iterations). Teams are the unit for sprint planning and work item assignment.",
      subcommands: [
        list: [
          name: "ado teams list",
          doc:
            "List all teams in a project. Output is a table (ID, Name, Description). Use --top to limit.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [top: [type: :integer, doc: "Maximum number to return", doc_arg: "N"]],
          execute: &list_teams/1
        ],
        show: [
          name: "ado teams show",
          doc:
            "Show a single team: ID, name, description, identity URL, and project context. Accepts name or GUID.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            team_id: [type: :string, doc: "Team name or ID"]
          ],
          execute: &show_team/1
        ],
        create: [
          name: "ado teams create",
          doc:
            "Create a new team in a project. The team inherits the project default area path and iteration. Members are added separately with the members subcommand.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            name: [type: :string, required: true, doc: "Team name", doc_arg: "NAME"],
            description: [type: :string, doc: "Team description", doc_arg: "DESC"]
          ],
          execute: &create_team/1
        ],
        update: [
          name: "ado teams update",
          doc: "Update a team.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            team_id: [type: :string, doc: "Team name or ID"]
          ],
          options: [
            name: [type: :string, doc: "New team name", doc_arg: "NAME"],
            description: [type: :string, doc: "New description", doc_arg: "DESC"]
          ],
          execute: &update_team/1
        ],
        delete: [
          name: "ado teams delete",
          doc:
            "Delete a team. Members are not removed from the org; just the team container is deleted.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            team_id: [type: :string, doc: "Team name or ID"]
          ],
          execute: &delete_team/1
        ],
        members: [
          name: "ado teams members",
          doc:
            "Add/remove users to/from a team. Team membership is separate from project membership; users must be in the project first.",
          subcommands: [
            list: [
              name: "ado teams members list",
              doc:
                "List all members of a team. Output shows display name, unique name (email), and member ID.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                team_id: [type: :string, doc: "Team name or ID"]
              ],
              execute: &list_team_members/1
            ]
          ]
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_teams(parsed) do
    project = parsed.arguments.project
    params = %{}

    params =
      if top = Map.get(parsed.options, :top), do: Map.put(params, "$top", top), else: params

    result = Client.list("/#{URI.encode(project)}/_apis/teams", params)

    Helpers.handle_api_result(result, parsed, fn teams ->
      Helpers.json_or_format(teams, parsed, &print_teams_table/1)
    end)
  end

  def show_team(parsed) do
    project = parsed.arguments.project
    team_id = parsed.arguments.team_id

    case Client.get("/#{URI.encode(project)}/_apis/teams/#{URI.encode(team_id)}") do
      {:ok, team} -> Helpers.json_or_format(team, parsed, &print_team_detail/1)
      {:error, %{status: 404}} -> halt_error("Team '#{team_id}' not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def create_team(parsed) do
    project = parsed.arguments.project
    name = Map.fetch!(parsed.options, :name)
    body = %{"name" => name}

    body =
      if desc = Map.get(parsed.options, :description),
        do: Map.put(body, "description", desc),
        else: body

    case Client.post("/#{URI.encode(project)}/_apis/teams", body) do
      {:ok, team} ->
        success("Team '#{team["name"]}' created (ID: #{team["id"]}).\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def update_team(parsed) do
    project = parsed.arguments.project
    team_id = parsed.arguments.team_id
    body = %{}
    body = if name = Map.get(parsed.options, :name), do: Map.put(body, "name", name), else: body

    body =
      if desc = Map.get(parsed.options, :description),
        do: Map.put(body, "description", desc),
        else: body

    if body == %{}, do: halt_error("At least one of --name or --description is required.")

    case Client.patch("/#{URI.encode(project)}/_apis/teams/#{URI.encode(team_id)}", body) do
      {:ok, team} ->
        success("Team '#{team["name"]}' updated.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Team '#{team_id}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_team(parsed) do
    project = parsed.arguments.project
    team_id = parsed.arguments.team_id

    case Client.delete("/#{URI.encode(project)}/_apis/teams/#{URI.encode(team_id)}") do
      :ok ->
        success("Team '#{team_id}' deleted.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Team '#{team_id}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def list_team_members(parsed) do
    project = parsed.arguments.project
    team_id = parsed.arguments.team_id

    result = Client.list("/#{URI.encode(project)}/_apis/teams/#{URI.encode(team_id)}/members")

    Helpers.handle_api_result(result, parsed, fn members ->
      Helpers.json_or_format(members, parsed, &print_team_members_table/1)
    end)
  end

  defp print_teams_table(teams) do
    if Enum.empty?(teams) do
      writeln("No teams found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 40)}  #{String.pad_trailing("Name", 30)}  Description")
      writeln(String.duplicate("─", 90))

      Enum.each(teams, fn t ->
        writeln(
          "#{String.pad_trailing(t["id"] || "", 40)}  #{String.pad_trailing(t["name"] || "", 30)}  #{String.slice(t["description"] || "", 0, 30)}"
        )
      end)

      writeln("")
      writeln("#{length(teams)} team(s)")
    end
  end

  defp print_team_detail(team) do
    writeln("")
    success("Team Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:          #{team["id"]}")
    writeln("  Name:        #{team["name"]}")
    writeln("  Description: #{team["description"] || "(none)"}")
    writeln("  URL:         #{team["url"]}")
    writeln("")
  end

  defp print_team_members_table(members) do
    if Enum.empty?(members) do
      writeln("No members found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 40)}  #{String.pad_trailing("Display Name", 25)}  Unique Name"
      )

      writeln(String.duplicate("─", 100))

      Enum.each(members, fn m ->
        writeln(
          "#{String.pad_trailing(m["id"] || "", 40)}  #{String.pad_trailing(m["identity"]["displayName"] || "", 25)}  #{m["identity"]["uniqueName"] || ""}"
        )
      end)

      writeln("")
      writeln("#{length(members)} member(s)")
    end
  end
end
