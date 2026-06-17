defmodule AdoCli.CLI.Folders do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado pipelines folders",
      doc:
        "Manage pipeline folders. Folders organize pipelines in the web UI (like directories) and help with permissions and discoverability. They are purely organizational — they don't change pipeline behavior.",
      subcommands: [
        list: [
          name: "ado pipelines folders list",
          doc:
            "List pipeline folders in a project with the count of pipelines in each. Use --path to scope to a subtree. Output is a table (Folder, Pipelines). Pass --json for raw pipeline data.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            path: [
              type: :string,
              doc: "Subtree to list (e.g. 'MyTeam/Frontend'). Omit to list the whole project.",
              doc_arg: "PATH"
            ]
          ],
          execute: &list_folders/1
        ],
        create: [
          name: "ado pipelines folders create",
          doc:
            "Create a new pipeline folder. Use forward slashes for nesting (e.g. 'MyTeam/Frontend'). Parent folders are created automatically. Idempotent: returns success if the folder already exists.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            path: [
              type: :string,
              required: true,
              doc: "Folder path (forward-slash separated; nested paths are auto-created)",
              doc_arg: "PATH"
            ]
          ],
          execute: &create_folder/1
        ],
        delete: [
          name: "ado pipelines folders delete",
          doc:
            "Delete a folder AND all pipelines within it. This is a hard delete — pipelines inside are removed (not moved to root). Refuses if the folder doesn't exist or if it contains builds/runs that the API considers blocking.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            path: [
              type: :string,
              required: true,
              doc: "Folder path to delete (must match exactly as shown by `list`)",
              doc_arg: "PATH"
            ]
          ],
          execute: &delete_folder/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_folders(parsed) do
    project = parsed.arguments.project
    params = if path = Map.get(parsed.options, :path), do: %{"path" => path}, else: %{}

    result =
      Client.list(
        "/#{URI.encode(project)}/_apis/pipelines",
        Map.merge(params, %{"folder" => path || "/"})
      )

    Helpers.handle_api_result(result, parsed, fn pipelines ->
      groups = Enum.group_by(pipelines, & &1["folder"])
      folders = Map.keys(groups)

      if Enum.empty?(folders) do
        writeln("No folders found.")
      else
        writeln("")
        writeln("#{String.pad_trailing("Folder", 40)}  Pipelines")
        writeln(String.duplicate("-", 60))

        Enum.each(Enum.sort(folders), fn f ->
          count = length(Map.get(groups, f, []))
          writeln("#{String.pad_trailing(f || "/", 40)}  #{count}")
        end)

        writeln("")
        writeln("#{length(folders)} folder(s)")
      end

      halt_success("")
    end)
  end

  def create_folder(parsed) do
    project = parsed.arguments.project
    path = Map.fetch!(parsed.options, :path)
    body = %{"path" => path}

    case Client.post("/#{URI.encode(project)}/_apis/pipelines/folders", body) do
      {:ok, _} ->
        success("Folder '#{path}' created.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_folder(parsed) do
    project = parsed.arguments.project
    path = Map.fetch!(parsed.options, :path)

    case Client.delete("/#{URI.encode(project)}/_apis/pipelines/folders/#{URI.encode(path)}") do
      :ok ->
        success("Folder '#{path}' deleted.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end
end
