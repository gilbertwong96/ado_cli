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
      name: "ado projects",
      doc:
        "Manage Azure DevOps projects. A project is the top-level container for repos, pipelines, work items, and teams. Every Azure DevOps organization has at least one project.",
      subcommands: [
        list: [
          name: "ado projects list",
          doc:
            "List all projects in the organization. Output is a table (Name, ID, State, Visibility). Returns top 100 by default; use --top to change the page size. Pass --json for raw data.",
          options: [
            state: [
              type: :string,
              doc:
                "Project lifecycle state. Valid: wellFormed (default — healthy and usable), creating, deleting, new (just created, being initialized), all (every state including soft-deleted)",
              doc_arg: "STATE"
            ],
            top: [
              type: :integer,
              doc: "Maximum number of projects to return. Default 100, max 1000.",
              doc_arg: "N"
            ],
            skip: [
              type: :integer,
              doc:
                "Number of projects to skip (for pagination). Use with --top to page through results.",
              doc_arg: "N"
            ]
          ],
          execute: &list_projects/1
        ],
        show: [
          name: "ado projects show",
          doc:
            "Show details of a single project: ID, name, description, state, visibility, process template, and capabilities (with --capabilities). The argument accepts either the project name (e.g. 'MyApp') or the GUID.",
          arguments: [
            project_id: [
              type: :string,
              doc:
                "Project name (e.g. 'MyApp') or GUID. Both are accepted; names are case-insensitive."
            ]
          ],
          options: [
            capabilities: [
              type: :boolean,
              default: false,
              doc:
                "Include the project's capability map (whether version control, boards, pipelines, test plans are enabled). Adds ~30 lines of output."
            ]
          ],
          execute: &show_project/1
        ],
        create: [
          name: "ado projects create",
          doc:
            "Create a new project. The project becomes 'wellFormed' within 30-60 seconds; the CLI does not wait. Use the resulting name with other commands.",
          arguments: [
            name: [
              type: :string,
              doc:
                "Project name. Must be unique within the org, 3-64 chars, alphanumeric with hyphens (no spaces). Cannot be changed without deleting and recreating."
            ]
          ],
          options: [
            description: [
              type: :string,
              doc:
                "Project description shown in the project picker. Multi-word values do not need quoting.",
              doc_arg: "DESC"
            ],
            visibility: [
              type: :string,
              doc:
                "Who can see the project. Valid: private (default — only invited members), public (anyone on the internet can view, including non-Azure-DevOps users). Note: public projects require AAD and org-level enabling.",
              doc_arg: "VISIBILITY"
            ],
            process: [
              type: :string,
              doc:
                "Process template that defines work item types and states. Common values: 'Agile', 'Scrum', 'CMMI', 'Basic'. Default depends on the org.",
              doc_arg: "PROCESS"
            ],
            source_control: [
              type: :string,
              doc:
                "Initial source control type. Valid: 'Git' (default — modern, distributed), 'Tfvc' (legacy Team Foundation Version Control). Cannot be changed after creation.",
              doc_arg: "TYPE"
            ]
          ],
          execute: &create_project/1
        ],
        update: [
          name: "ado projects update",
          doc:
            "Update a project's name or description. The name change propagates to all URLs (old URLs redirect for a grace period).",
          arguments: [project_id: [type: :string, doc: "Project name or GUID"]],
          options: [
            name: [
              type: :string,
              doc: "New project name (must be unique, same constraints as create)",
              doc_arg: "NAME"
            ],
            description: [type: :string, doc: "New project description", doc_arg: "DESC"]
          ],
          execute: &update_project/1
        ],
        delete: [
          name: "ado projects delete",
          doc:
            "Permanently delete a project. This is IRREVERSIBLE: all repos, work items, pipelines, and history are erased. Use --force to skip the interactive confirmation (the CLI will still ask via the API). Plan for a 30-90 day soft-delete window if you change your mind.",
          arguments: [project_id: [type: :string, doc: "Project name or GUID"]],
          options: [
            force: [
              type: :boolean,
              default: false,
              doc:
                "Skip the interactive confirmation prompt (useful in scripts). The Azure DevOps API itself does not require a separate confirmation."
            ]
          ],
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
        success("Project '#{project["name"]}' created (ID: #{project["id"]}).\n")
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
        success("Project updated: #{project["name"]}\n")
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
      AdoCli.CLI.Helpers.confirm_delete("project", project_id)
    end

    case Client.delete("/_apis/projects/#{URI.encode(project_id)}") do
      :ok ->
        success("Project '#{project_id}' queued for deletion.\n")
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
    success("Project Details\n")
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
