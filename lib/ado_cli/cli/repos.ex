defmodule AdoCli.CLI.Repos do
  @moduledoc """
  Commands for managing Azure DevOps Git repositories.

    ado_cli repos list PROJECT
    ado_cli repos show PROJECT REPO
    ado_cli repos create PROJECT NAME       [--default-branch BRANCH]
    ado_cli repos delete PROJECT REPO        [--force]
    ado_cli repos branches PROJECT REPO
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado repos",
      doc: "Manage Azure DevOps Git repositories.",
      subcommands: [
        list: [
          name: "ado repos list",
          doc: "List repositories in a project.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            include_links: [type: :boolean, default: false, doc: "Include reference links"]
          ],
          execute: &list_repos/1
        ],
        show: [
          name: "ado repos show",
          doc: "Show details of a specific repository.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          execute: &show_repo/1
        ],
        create: [
          name: "ado repos create",
          doc: "Create a new Git repository.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            name: [type: :string, doc: "Repository name"]
          ],
          options: [
            default_branch: [
              type: :string,
              doc: "Default branch name (default: main)",
              doc_arg: "BRANCH"
            ]
          ],
          execute: &create_repo/1
        ],
        delete: [
          name: "ado repos delete",
          doc: "Delete a repository.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [force: [type: :boolean, default: false, doc: "Skip confirmation"]],
          execute: &delete_repo/1
        ],
        branches: [
          name: "ado repos branches",
          doc: "List branches in a repository.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [
            filter: [type: :string, doc: "Filter branches by name pattern", doc_arg: "PATTERN"]
          ],
          execute: &list_branches/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  # ── Read ──────────────────────────────────────────────────────────────

  @doc """
  Lists repositories in a project.

  Use `--include-links` to include reference links in the output.
  """
  def list_repos(parsed) do
    project = parsed.arguments.project
    params = if(Map.get(parsed.options, :include_links), do: %{"includeLinks" => true}, else: %{})
    result = Client.list("/#{URI.encode(project)}/_apis/git/repositories", params)

    Helpers.handle_api_result(result, parsed, fn repos ->
      Helpers.json_or_format(repos, parsed, &print_repos_table/1)
    end)
  end

  @doc """
  Shows details of a specific repository.
  """
  def show_repo(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id

    case Client.get("/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}") do
      {:ok, repo} ->
        Helpers.json_or_format(repo, parsed, &print_repo_detail/1)

      {:error, %{status: 404}} ->
        halt_error("Repository '#{repo_id}' not found in project '#{project}'")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc """
  Lists branches in a repository.

  Use `--filter` to filter branch names by a pattern.
  """
  def list_branches(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id
    params = %{"filter" => Map.get(parsed.options, :filter, "heads/")}

    result =
      Client.list(
        "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/refs",
        params
      )

    Helpers.handle_api_result(result, parsed, fn refs ->
      branches = Enum.filter(refs, &String.starts_with?(&1["name"] || "", "refs/heads/"))
      Helpers.json_or_format(branches, parsed, &print_branches_table/1)
    end)
  end

  # ── Write ─────────────────────────────────────────────────────────────

  @doc """
  Creates a new Git repository in a project.

  Use `--default-branch` to set a custom default branch name.
  """
  def create_repo(parsed) do
    project = parsed.arguments.project

    # Resolve project name to ID for the request body
    project_id =
      case Client.list("/_apis/projects") do
        {:ok, projects} ->
          found = Enum.find(projects, &(&1["name"] == project))
          if found, do: found["id"], else: project

        _ ->
          project
      end

    body = %{
      "name" => parsed.arguments.name,
      "project" => %{"id" => project_id}
    }

    body =
      if Map.get(parsed.options, :default_branch),
        do:
          Map.put(body, "defaultBranch", "refs/heads/#{Map.get(parsed.options, :default_branch)}"),
        else: body

    case Client.post("/#{URI.encode(project)}/_apis/git/repositories", body) do
      {:ok, repo} ->
        success("Repository '#{repo["name"]}' created.\n")
        writeln("  ID:             #{repo["id"]}")
        writeln("  Default Branch: #{get_in(repo, ["defaultBranch"]) || "refs/heads/main"}")
        writeln("  SSH URL:        #{repo["sshUrl"]}")
        writeln("  Web URL:        #{repo["webUrl"]}")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc """
  Deletes a repository.

  Requires `--force` to skip the confirmation prompt.
  """
  def delete_repo(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id

    unless Map.get(parsed.options, :force) do
      confirm_delete("repository", "#{project}/#{repo_id}")
    end

    case Client.delete("/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}") do
      :ok ->
        success("Repository '#{repo_id}' deleted from '#{project}'.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Repository '#{repo_id}' not found in project '#{project}'")

      {:error, _} = error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Helpers ───────────────────────────────────────────────────────────

  defp confirm_delete(kind, id) do
    write("Delete #{kind} '#{id}'? This cannot be undone. [y/N] ")

    if String.downcase(String.trim(IO.gets(""))) == "y" do
      :ok
    else
      halt_error("Aborted.")
    end
  end

  # ── Formatting ────────────────────────────────────────────────────────

  defp print_repos_table(repos) do
    if Enum.empty?(repos) do
      writeln("No repositories found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 40)}  #{String.pad_trailing("Name", 30)}  Default Branch"
      )

      writeln(String.duplicate("─", 100))

      Enum.each(repos, fn r ->
        branch = get_in(r, ["defaultBranch"]) || "(none)"
        branch = String.replace_prefix(branch, "refs/heads/", "")

        writeln(
          "#{String.pad_trailing(r["id"] || "", 40)}  #{String.pad_trailing(r["name"] || "", 30)}  #{branch}"
        )
      end)

      writeln("")
      writeln("#{length(repos)} repository(ies)")
    end
  end

  defp print_repo_detail(repo) do
    writeln("")
    success("Repository Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:             #{repo["id"]}")
    writeln("  Name:           #{repo["name"]}")
    writeln("  Default Branch: #{get_in(repo, ["defaultBranch"]) || "(none)"}")
    writeln("  Size:           #{repo["size"] || 0} bytes")
    writeln("  SSH URL:        #{repo["sshUrl"]}")
    writeln("  Web URL:        #{repo["webUrl"]}")

    if project = repo["project"],
      do: writeln("  Project:        #{project["name"]} (#{project["id"]})")

    writeln("")
  end

  defp print_branches_table(branches) do
    if Enum.empty?(branches) do
      writeln("No branches found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("Name", 50)}  Object ID")
      writeln(String.duplicate("─", 100))

      Enum.each(branches, fn b ->
        name = String.replace_prefix(b["name"] || "", "refs/heads/", "")
        obj = b["objectId"] || ""
        writeln("#{String.pad_trailing(name, 50)}  #{obj}")
      end)

      writeln("")
      writeln("#{length(branches)} branch(es)")
    end
  end
end
