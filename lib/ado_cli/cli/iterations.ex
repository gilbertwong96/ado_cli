defmodule AdoCli.CLI.Iterations do
  @moduledoc """
  Commands for managing Azure DevOps iterations (sprints).

    ado iterations list PROJECT TEAM
    ado iterations show PROJECT TEAM ITERATION_ID
    ado iterations create PROJECT TEAM --name NAME --start-date YYYY-MM-DD --finish-date YYYY-MM-DD
    ado iterations update PROJECT TEAM ITERATION_ID [--name NAME] [--start DATE] [--finish DATE]
    ado iterations delete PROJECT TEAM ITERATION_ID
  """

  @behaviour CliMate.CLI.Command
  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado iterations",
      doc:
        "Manage Azure DevOps iterations (sprints). Iterations are time-boxed containers for work items used in Scrum-like workflows. They belong to a specific team (a project can have multiple teams with different sprint cadences).",
      subcommands: [
        list: [
          name: "ado iterations list",
          doc:
            "List all iterations (sprints) for a team. Output is a table (ID, Name, Start, Finish). Use --current to show only the active sprint.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            team: [
              type: :string,
              doc:
                "Team name or ID. Iterations are team-scoped — each team can have different sprint cadences."
            ]
          ],
          options: [
            current: [
              type: :boolean,
              default: false,
              doc:
                "If true, only return the iteration that is currently in-progress (matches today's date)."
            ]
          ],
          execute: &list_iterations/1
        ],
        show: [
          name: "ado iterations show",
          doc:
            "Show details of a single iteration: ID, name, full path, start date, finish date.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            team: [type: :string, doc: "Team name or ID"],
            iteration_id: [type: :string, doc: "Iteration identifier (UUID)"]
          ],
          execute: &show_iteration/1
        ],
        create: [
          name: "ado iterations create",
          doc:
            "Create a new iteration (sprint) for a team. Without --start-date and --finish-date, the iteration has no time bounds (acts as a backlog bucket).",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            team: [type: :string, doc: "Team name or ID"]
          ],
          options: [
            name: [
              type: :string,
              required: true,
              doc: "Iteration name (e.g. 'Sprint 23', 'Q1 2026')",
              doc_arg: "NAME"
            ],
            start_date: [
              type: :string,
              doc: "Sprint start date in ISO 8601 (YYYY-MM-DD, e.g. '2026-01-15')",
              doc_arg: "DATE"
            ],
            finish_date: [
              type: :string,
              doc:
                "Sprint end date in ISO 8601 (YYYY-MM-DD, e.g. '2026-01-29'). Should be after start_date.",
              doc_arg: "DATE"
            ]
          ],
          execute: &create_iteration/1
        ],
        update: [
          name: "ado iterations update",
          doc:
            "Modify an existing iteration's name, start date, or finish date. Pass at least one option. Existing work-item assignments are preserved when the dates change.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            team: [type: :string, doc: "Team name or ID"],
            iteration_id: [type: :string, doc: "Iteration identifier (UUID)"]
          ],
          options: [
            name: [type: :string, doc: "New iteration name", doc_arg: "NAME"],
            start_date: [type: :string, doc: "New start date (YYYY-MM-DD)", doc_arg: "DATE"],
            finish_date: [type: :string, doc: "New finish date (YYYY-MM-DD)", doc_arg: "DATE"]
          ],
          execute: &update_iteration/1
        ],
        delete: [
          name: "ado iterations delete",
          doc:
            "Delete an iteration. Fails if there are work items still assigned to it; reassign them to a different iteration first (use `ado workitems update --iteration`).",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            team: [type: :string, doc: "Team name or ID"],
            iteration_id: [type: :string, doc: "Iteration identifier (UUID)"]
          ],
          execute: &delete_iteration/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_iterations(parsed) do
    project = parsed.arguments.project
    team = parsed.arguments.team
    current = Map.get(parsed.options, :current)

    path =
      if current,
        do:
          "/#{URI.encode(project)}/#{URI.encode(team)}/_apis/work/teamsettings/iterations?$timeframe=current",
        else: "/#{URI.encode(project)}/#{URI.encode(team)}/_apis/work/teamsettings/iterations"

    result = Client.list(path)

    Helpers.handle_api_result(result, parsed, fn iterations ->
      Helpers.json_or_format(iterations, parsed, &print_iterations_table/1)
    end)
  end

  def show_iteration(parsed) do
    project = parsed.arguments.project
    team = parsed.arguments.team
    iter_id = parsed.arguments.iteration_id

    case Client.get(
           "/#{URI.encode(project)}/#{URI.encode(team)}/_apis/work/teamsettings/iterations/#{URI.encode(iter_id)}"
         ) do
      {:ok, iter} -> Helpers.json_or_format(iter, parsed, &print_iteration_detail/1)
      {:error, %{status: 404}} -> halt_error("Iteration not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def create_iteration(parsed) do
    project = parsed.arguments.project
    team = parsed.arguments.team
    name = Map.fetch!(parsed.options, :name)

    body = %{"name" => name}

    body =
      if d = Map.get(parsed.options, :start_date),
        do: put_in(body, ["attributes", "startDate"], d),
        else: body

    body =
      if d = Map.get(parsed.options, :finish_date),
        do: put_in(body, ["attributes", "finishDate"], d),
        else: body

    case Client.post(
           "/#{URI.encode(project)}/#{URI.encode(team)}/_apis/work/teamsettings/iterations",
           body
         ) do
      {:ok, iter} ->
        success("Iteration '#{iter["name"]}' created.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def update_iteration(parsed) do
    project = parsed.arguments.project
    team = parsed.arguments.team
    iter_id = parsed.arguments.iteration_id
    body = %{}
    body = if name = Map.get(parsed.options, :name), do: Map.put(body, "name", name), else: body

    body =
      if d = Map.get(parsed.options, :start_date),
        do: put_in(body, ["attributes", "startDate"], d),
        else: body

    body =
      if d = Map.get(parsed.options, :finish_date),
        do: put_in(body, ["attributes", "finishDate"], d),
        else: body

    if body == %{}, do: halt_error("At least one option is required.")

    case Client.patch(
           "/#{URI.encode(project)}/#{URI.encode(team)}/_apis/work/teamsettings/iterations/#{URI.encode(iter_id)}",
           body
         ) do
      {:ok, iter} ->
        success("Iteration '#{iter["name"]}' updated.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Iteration not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_iteration(parsed) do
    project = parsed.arguments.project
    team = parsed.arguments.team
    iter_id = parsed.arguments.iteration_id

    case Client.delete(
           "/#{URI.encode(project)}/#{URI.encode(team)}/_apis/work/teamsettings/iterations/#{URI.encode(iter_id)}"
         ) do
      :ok ->
        success("Iteration deleted.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Iteration not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_iterations_table(iters) do
    if Enum.empty?(iters) do
      writeln("No iterations found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 40)} #{String.pad_trailing("Name", 25)} Start       Finish"
      )

      writeln(String.duplicate("─", 95))

      Enum.each(iters, fn i ->
        attrs = i["attributes"] || %{}

        writeln(
          "#{String.pad_trailing(i["id"] || "", 40)} #{String.pad_trailing(i["name"] || "", 25)} #{String.slice(attrs["startDate"] || "", 0, 11)} #{String.slice(attrs["finishDate"] || "", 0, 11)}"
        )
      end)

      writeln("")
      writeln("#{length(iters)} iteration(s)")
    end
  end

  defp print_iteration_detail(iter) do
    attrs = iter["attributes"] || %{}
    writeln("")
    success("Iteration Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:    #{iter["id"]}")
    writeln("  Name:  #{iter["name"]}")
    writeln("  Path:  #{iter["path"]}")
    writeln("  Start: #{attrs["startDate"]}")
    writeln("  Finish: #{attrs["finishDate"]}")
    writeln("")
  end
end
