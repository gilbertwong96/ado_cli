defmodule AdoCli.CLI.Areas do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado areas",
      doc:
        "Manage Azure DevOps area paths (classification nodes). Areas organize work items into a hierarchy (e.g. 'Project\\Team\\Feature') for filtering and reporting.",
      subcommands: [
        list: [
          name: "ado areas list",
          doc:
            "List area paths in a project as a tree (default: only top-level; use --depth for children). Output is a hierarchical tree by default; pass --json for the raw root node with nested children.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            depth: [
              type: :integer,
              doc: "Depth of children to retrieve (1 = top-level only, 2 = includes sub-areas)",
              doc_arg: "N"
            ]
          ],
          execute: &list_areas/1
        ],
        show: [
          name: "ado areas show",
          doc:
            "Show details of a single area path (ID, name, full path, structure type). Returns 404 if the path does not exist.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            area_path: [
              type: :string,
              doc:
                "Area path using backslashes (e.g. MyProject\\Area\\SubArea). Escape the backslash in shells or wrap in single quotes."
            ]
          ],
          execute: &show_area/1
        ],
        create: [
          name: "ado areas create",
          doc:
            "Create a new area path. Omit --parent to create at the project root, or pass --parent to nest under an existing area.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            name: [
              type: :string,
              required: true,
              doc: "Name for the new area path (no backslashes)",
              doc_arg: "NAME"
            ],
            parent: [
              type: :string,
              doc:
                "Parent area path to nest under (e.g. MyProject\\Team). Omit to create at the root.",
              doc_arg: "PATH"
            ]
          ],
          execute: &create_area/1
        ],
        update: [
          name: "ado areas update",
          doc:
            "Rename an existing area path. Only the leaf name is changed; the path prefix is preserved.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            area_path: [type: :string, doc: "Current area path (e.g. MyProject\\OldName)"]
          ],
          options: [
            name: [
              type: :string,
              required: true,
              doc: "New name (no backslashes)",
              doc_arg: "NAME"
            ]
          ],
          execute: &update_area/1
        ],
        delete: [
          name: "ado areas delete",
          doc:
            "Delete an area path. Fails if the area has child areas or work items still assigned to it; reassign or remove those first.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            area_path: [type: :string, doc: "Area path to delete (e.g. MyProject\\OldArea)"]
          ],
          execute: &delete_area/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_areas(parsed) do
    project = parsed.arguments.project
    params = if d = Map.get(parsed.options, :depth), do: %{"$depth" => d}, else: %{}
    path = "/#{URI.encode(project)}/_apis/wit/classificationNodes/areas"

    case Client.get(path, params) do
      {:ok, root} -> Helpers.json_or_format(root, parsed, &print_area_tree(&1, 0))
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def show_area(parsed) do
    %{project: project, area_path: area_path} = parsed.arguments
    path = "/#{URI.encode(project)}/_apis/wit/classificationNodes/areas/#{URI.encode(area_path)}"

    case Client.get(path) do
      {:ok, area} -> Helpers.json_or_format(area, parsed, &print_area_detail/1)
      {:error, %{status: 404}} -> halt_error("Area path '#{area_path}' not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def create_area(parsed) do
    project = parsed.arguments.project
    name = Map.fetch!(parsed.options, :name)
    parent = Map.get(parsed.options, :parent)

    base_path =
      if parent,
        do: "/#{URI.encode(project)}/_apis/wit/classificationNodes/areas/#{URI.encode(parent)}",
        else: "/#{URI.encode(project)}/_apis/wit/classificationNodes/areas"

    case Client.post(base_path, %{"name" => name}) do
      {:ok, area} ->
        success("Area '#{area["name"]}' created (ID: #{area["id"]}).\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def update_area(parsed) do
    %{project: project, area_path: area_path} = parsed.arguments
    new_name = Map.fetch!(parsed.options, :name)
    path = "/#{URI.encode(project)}/_apis/wit/classificationNodes/areas/#{URI.encode(area_path)}"

    case Client.patch(path, %{"name" => new_name}) do
      {:ok, area} ->
        success("Area renamed to '#{area["name"]}'.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_area(parsed) do
    %{project: project, area_path: area_path} = parsed.arguments
    path = "/#{URI.encode(project)}/_apis/wit/classificationNodes/areas/#{URI.encode(area_path)}"

    case Client.delete(path) do
      :ok ->
        success("Area '#{area_path}' deleted.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_area_tree(node, depth) do
    indent = String.duplicate("  ", depth)
    leaf = if is_list(node["children"]) && node["children"] != [], do: " ▾", else: ""
    writeln("#{indent}#{node["name"]}#{leaf}")

    if is_list(node["children"]) do
      Enum.each(node["children"], &print_area_tree(&1, depth + 1))
      if depth == 0, do: halt_success("")
    end
  end

  defp print_area_detail(area) do
    writeln("")
    success("Area Path Details\n")
    writeln(String.duplicate("-", 60))
    writeln("  ID:        #{area["id"]}")
    writeln("  Name:      #{area["name"]}")
    writeln("  Path:      #{area["path"]}")
    writeln("  Structure: #{area["structureType"] || "hierarchy"}")
    if area["url"], do: writeln("  URL:       #{area["url"]}")
    writeln("")
    halt_success("")
  end
end
