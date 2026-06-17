defmodule AdoCli.CLI.Imports do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado imports",
      doc:
        "Manage Git repository imports (e.g. GitHub → Azure DevOps migration). Creates a new Azure DevOps repo and populates it with the git history, branches, and tags from a source repository. The new repo is a one-time copy, not a mirror.",
      subcommands: [
        list: [
          name: "ado imports list",
          doc:
            "List recent import requests in a project. Output is a table (ID, Status, Source URL). Use --top to limit. Pass --json for raw data.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            top: [
              type: :integer,
              doc: "Maximum number of imports to return. Default 50.",
              doc_arg: "N"
            ]
          ],
          execute: &list_imports/1
        ],
        show: [
          name: "ado imports show",
          doc:
            "Show the current status of an import request (queued, inProgress, completed, failed, etc.) and any error message. Imports for large repos can take hours.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            import_id: [type: :string, doc: "Import request ID (UUID, returned by `create`)"]
          ],
          execute: &show_import/1
        ],
        create: [
          name: "ado imports create",
          doc:
            "Import a Git repository into Azure DevOps. The new repo is created in the target project; the source repo is read once (branches, tags, and history are copied). For private source repos, pass --user and --password/PAT.",
          arguments: [
            project: [
              type: :string,
              doc: "Target project name or ID where the new repo will live"
            ],
            repo_name: [
              type: :string,
              doc: "Name for the new repository (must not already exist in the project)"
            ]
          ],
          options: [
            url: [
              type: :string,
              required: true,
              doc:
                "Source git URL ending in .git. Supports https:// (with optional basic auth) and git://. SSH URLs are not supported. Example: https://github.com/owner/repo.git",
              doc_arg: "URL"
            ],
            user: [
              type: :string,
              doc:
                "Username for source-repo authentication (only for private repos). For GitHub, use any non-empty string with a PAT as the password.",
              doc_arg: "USER"
            ],
            password: [
              type: :string,
              doc:
                "Password or Personal Access Token for the source repo. For GitHub, generate a PAT at https://github.com/settings/tokens with `repo` scope. For Azure DevOps, use a PAT with `vso.code` scope.",
              doc_arg: "PASS"
            ]
          ],
          execute: &create_import/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_imports(parsed) do
    project = parsed.arguments.project
    params = if t = Map.get(parsed.options, :top), do: %{"$top" => t}, else: %{}
    result = Client.list("/#{URI.encode(project)}/_apis/git/importRequests", params)

    Helpers.handle_api_result(result, parsed, fn imports ->
      Helpers.json_or_format(imports, parsed, &print_imports_table/1)
    end)
  end

  def show_import(parsed) do
    %{project: project, import_id: import_id} = parsed.arguments
    path = "/#{URI.encode(project)}/_apis/git/importRequests/#{URI.encode(import_id)}"

    case Client.get(path) do
      {:ok, imp} -> Helpers.json_or_format(imp, parsed, &print_import_detail/1)
      {:error, %{status: 404}} -> halt_error("Import '#{import_id}' not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def create_import(parsed) do
    %{project: project, repo_name: repo_name} = parsed.arguments
    url = Map.fetch!(parsed.options, :url)
    user = Map.get(parsed.options, :user)
    password = Map.get(parsed.options, :password)

    git_source = %{"url" => url}
    git_source = if user, do: Map.put(git_source, "user", user), else: git_source
    git_source = if password, do: Map.put(git_source, "password", password), else: git_source

    body = %{
      "parameters" => %{
        "gitSource" => git_source,
        "deleteServiceEndpointAfterImportIsDone" => false
      }
    }

    path =
      "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_name)}/importRequests"

    case Client.post(path, body) do
      {:ok, imp} ->
        writeln("")
        success("Import request created.\n")
        writeln("  ID:        #{imp["id"]}")
        writeln("  Status:    #{imp["status"]}")
        writeln("  URL:       #{imp["url"] || ""}")
        writeln("")
        writeln("Check status with:")
        writeln("  ado imports show #{project} #{imp["id"]}")
        writeln("")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_imports_table(imports) do
    if Enum.empty?(imports) do
      writeln("No imports found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 38)} #{String.pad_trailing("Status", 12)} Source")
      writeln(String.duplicate("-", 90))

      Enum.each(imports, fn i ->
        src =
          (i["parameters"] && i["parameters"]["gitSource"] && i["parameters"]["gitSource"]["url"]) ||
            ""

        writeln(
          "#{String.pad_trailing(i["id"] || "", 38)} #{String.pad_trailing(i["status"] || "", 12)} #{src}"
        )
      end)

      writeln("")
      writeln("#{length(imports)} import(s)")
    end
  end

  defp print_import_detail(imp) do
    writeln("")
    success("Import Status\n")
    writeln(String.duplicate("-", 60))
    writeln("  ID:     #{imp["id"]}")
    writeln("  Status: #{imp["status"]}")
    writeln("  Source: #{(imp["parameters"] || %{})["gitSource"]["url"] || ""}")

    if imp["detailedStatus"],
      do:
        writeln(
          "  Detail: #{imp["detailedStatus"]["errorMessage"] || imp["detailedStatus"]["allStepsSucceeded"]}"
        )

    writeln("  URL:    #{imp["url"] || "(none)"}")
    writeln("")
  end
end
