defmodule AdoCli.CLI.Projects do
  @moduledoc """
  Commands for managing Azure DevOps projects.

    ado_cli projects list                [--state STATE] [--top N]
    ado_cli projects show ID             [--capabilities]
    ado_cli projects create NAME         [--description DESC] [--visibility public|private]
    ado_cli projects update ID           [--name NAME] [--description DESC]
    ado_cli projects delete ID           [--force]
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado_cli projects",
      doc: "Manage Azure DevOps projects.",
      subcommands: [
        list: [
          name: "ado_cli projects list",
          doc: "List all projects in the organization.",
          options: [
            state: [type: :string, doc: "Filter by project state", doc_arg: "STATE"],
            top: [type: :integer, doc: "Maximum number of projects to return", doc_arg: "N"],
            skip: [type: :integer, doc: "Number of projects to skip", doc_arg: "N"]
          ],
          execute: &list_projects/1
        ],
        show: [
          name: "ado_cli projects show",
          doc: "Show details of a specific project.",
          arguments: [project_id: [type: :string, doc: "Project name or ID"]],
          options: [capabilities: [type: :boolean, default: false, doc: "Include capabilities"]],
          execute: &show_project/1
        ],
        create: [
          name: "ado_cli projects create",
          doc: "Create a new project.",
          arguments: [name: [type: :string, doc: "Project name"]],
          options: [
            description: [type: :string, doc: "Project description", doc_arg: "DESC"],
            visibility: [
              type: :string,
              doc: "Visibility (private or public)",
              doc_arg: "VISIBILITY"
            ],
            process: [type: :string, doc: "Process template name", doc_arg: "PROCESS"],
            source_control: [
              type: :string,
              doc: "Source control type (Git or Tfvc)",
              doc_arg: "TYPE"
            ]
          ],
          execute: &create_project/1
        ],
        update: [
          name: "ado_cli projects update",
          doc: "Update an existing project.",
          arguments: [project_id: [type: :string, doc: "Project name or ID"]],
          options: [
            name: [type: :string, doc: "New project name", doc_arg: "NAME"],
            description: [type: :string, doc: "New project description", doc_arg: "DESC"]
          ],
          execute: &update_project/1
        ],
        delete: [
          name: "ado_cli projects delete",
          doc: "Delete a project.",
          arguments: [project_id: [type: :string, doc: "Project name or ID"]],
          options: [force: [type: :boolean, default: false, doc: "Skip confirmation"]],
          execute: &delete_project/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  # ── Read ──────────────────────────────────────────────────────────────

  @doc """
  Lists all projects in the organization.

  Supports filtering by `--state`, pagination via `--top` and `--skip`.
  """
  def list_projects(parsed) do
    params =
      build_params(parsed.options, [:state, :top, :skip], %{
        "stateFilter" => :state,
        "$top" => :top,
        "$skip" => :skip
      })

    result = Client.list("/_apis/projects", params)

    Helpers.handle_api_result(result, parsed, fn projects ->
      Helpers.json_or_format(projects, parsed, &print_projects_table/1)
    end)
  end

  @doc """
  Shows details of a specific project.

  Use `--capabilities` to include process and version control capabilities.
  """
  def show_project(parsed) do
    project_id = parsed.arguments.project_id

    params =
      if(Map.get(parsed.options, :capabilities), do: %{"includeCapabilities" => true}, else: %{})

    case Client.get("/_apis/projects/#{URI.encode(project_id)}", params) do
      {:ok, project} -> Helpers.json_or_format(project, parsed, &print_project_detail/1)
      {:error, %{status: 404}} -> halt_error("Project '#{project_id}' not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Write ─────────────────────────────────────────────────────────────

  @doc """
  Creates a new project.

  Supports `--description`, `--visibility` (private/public),
  `--process` (scrum/agile/basic/cmmi), and `--source-control` (Git/Tfvc).
  """
  def create_project(parsed) do
    body = %{
      "name" => parsed.arguments.name,
      "capabilities" => %{
        "versioncontrol" => %{
          "sourceControlType" => Map.get(parsed.options, :source_control, "Git")
        },
        "processTemplate" => %{
          "templateTypeId" => process_template_id(Map.get(parsed.options, :process))
        }
      }
    }

    body = put_if_key(Map.get(parsed.options, :description), body, "description")
    body = put_if_key(Map.get(parsed.options, :visibility), body, "visibility")

    case Client.post("/_apis/projects", body) do
      {:ok, project} ->
        writeln(success("Project '#{project["name"]}' created (ID: #{project["id"]})."))
        writeln("  Status: #{project["status"]}")
        writeln("  URL:    #{project["url"]}")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc """
  Updates an existing project's name and/or description.

  Requires at least one of `--name` or `--description`.
  """
  def update_project(parsed) do
    project_id = parsed.arguments.project_id
    body = %{}
    body = put_if_key(Map.get(parsed.options, :name), body, "name")
    body = put_if_key(Map.get(parsed.options, :description), body, "description")

    if body == %{} do
      halt_error("At least one of --name or --description is required.")
    end

    case Client.patch("/_apis/projects/#{URI.encode(project_id)}", body) do
      {:ok, project} ->
        writeln(success("Project updated: #{project["name"]}"))
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Project '#{project_id}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc """
  Deletes a project.

  Requires `--force` to skip the confirmation prompt.
  """
  def delete_project(parsed) do
    project_id = parsed.arguments.project_id

    unless Map.get(parsed.options, :force) do
      confirm_delete("project", project_id)
    end

    case Client.delete("/_apis/projects/#{URI.encode(project_id)}") do
      :ok ->
        writeln(success("Project '#{project_id}' queued for deletion."))
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Project '#{project_id}' not found")

      {:error, _} = error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Helpers ───────────────────────────────────────────────────────────

  defp build_params(options, keys, mappings) do
    Enum.reduce(keys, %{}, fn key, acc ->
      value = Map.get(options, key)
      param_key = Map.get(mappings, key, key)
      if value, do: Map.put(acc, param_key, value), else: acc
    end)
  end

  defp put_if_key(nil, map, _key), do: map
  defp put_if_key(value, map, key), do: Map.put(map, key, value)

  defp process_template_id(nil), do: "6b724908-ef14-45cf-84f8-768b5384da45"
  defp process_template_id("scrum"), do: "6b724908-ef14-45cf-84f8-768b5384da45"
  defp process_template_id("agile"), do: "adcc42ab-9882-485e-a3ed-7678f01f66bc"
  defp process_template_id("basic"), do: "b8a3a935-7e91-48b8-a94c-606d37c3e9f2"
  defp process_template_id("cmmi"), do: "27450541-8e31-4150-9947-dc59f998fc01"
  defp process_template_id(unknown), do: unknown

  defp confirm_delete(kind, id) do
    write("Delete #{kind} '#{id}'? This cannot be undone. [y/N] ")

    if String.downcase(String.trim(IO.gets(""))) == "y" do
      :ok
    else
      halt_error("Aborted.")
    end
  end

  # ── Formatting ────────────────────────────────────────────────────────

  defp print_projects_table(projects) do
    if Enum.empty?(projects) do
      writeln("No projects found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 40)}  #{String.pad_trailing("Name", 30)}  State")
      writeln(String.duplicate("─", 90))

      Enum.each(projects, fn p ->
        writeln(
          "#{String.pad_trailing(p["id"] || "", 40)}  #{String.pad_trailing(p["name"] || "", 30)}  #{p["state"] || ""}"
        )
      end)

      writeln("")
      writeln("#{length(projects)} project(s)")
    end
  end

  defp print_project_detail(project) do
    writeln("")
    writeln(success("Project Details"))
    writeln(String.duplicate("─", 60))
    writeln("  ID:          #{project["id"]}")
    writeln("  Name:        #{project["name"]}")
    writeln("  Description: #{project["description"] || "(none)"}")
    writeln("  State:       #{project["state"]}")
    writeln("  Visibility:  #{project["visibility"]}")
    writeln("  URL:         #{project["url"]}")

    if default_team = project["defaultTeam"] do
      writeln("  Default Team: #{default_team["name"]} (#{default_team["id"]})")
    end

    writeln("")
  end
end
