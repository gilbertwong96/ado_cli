defmodule AdoCli.CLI.PullRequests do
  @moduledoc """
  Commands for managing Azure DevOps Pull Requests.

    ado_cli prs list PROJECT REPO         [--status active|completed|abandoned] [--creator USER]
    ado_cli prs show PROJECT REPO PR_ID
    ado_cli prs create PROJECT REPO        --title TITLE --source BRANCH --target BRANCH [--description DESC]
    ado_cli prs complete PROJECT REPO PR_ID [--delete-source]
    ado_cli prs abandon PROJECT REPO PR_ID
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado_cli prs",
      doc: "Manage Azure DevOps pull requests.",
      subcommands: [
        list: [
          name: "ado_cli prs list",
          doc: "List pull requests in a repository.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [
            status: [
              type: :string,
              doc: "Filter by status (active, completed, abandoned, all)",
              doc_arg: "STATUS"
            ],
            creator: [
              type: :string,
              doc: "Filter by creator email or display name",
              doc_arg: "USER"
            ],
            top: [type: :integer, doc: "Maximum number to return", doc_arg: "N"]
          ],
          execute: &list_prs/1
        ],
        show: [
          name: "ado_cli prs show",
          doc: "Show details of a specific pull request.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Pull request ID"]
          ],
          execute: &show_pr/1
        ],
        create: [
          name: "ado_cli prs create",
          doc: "Create a new pull request.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [
            title: [type: :string, doc: "Pull request title", doc_arg: "TITLE"],
            description: [type: :string, doc: "Pull request description", doc_arg: "DESC"],
            source: [
              type: :string,
              doc: "Source branch name (e.g. refs/heads/feature)",
              doc_arg: "BRANCH"
            ],
            target: [
              type: :string,
              doc: "Target branch name (e.g. refs/heads/main)",
              doc_arg: "BRANCH"
            ],
            draft: [type: :boolean, default: false, doc: "Create as draft PR"]
          ],
          execute: &create_pr/1
        ],
        complete: [
          name: "ado_cli prs complete",
          doc: "Complete (merge) a pull request.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Pull request ID"]
          ],
          options: [
            delete_source: [
              type: :boolean,
              default: false,
              doc: "Delete source branch after merge"
            ],
            merge_strategy: [
              type: :string,
              doc: "Merge strategy (squash, rebase, noFastForward)",
              doc_arg: "STRATEGY"
            ]
          ],
          execute: &complete_pr/1
        ],
        abandon: [
          name: "ado_cli prs abandon",
          doc: "Abandon a pull request.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Pull request ID"]
          ],
          execute: &abandon_pr/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  # ── Read ──────────────────────────────────────────────────────────────

  @doc """
  Lists pull requests in a repository.

  Supports `--status` (active, completed, abandoned, all),
  `--creator`, and `--top`.
  """
  def list_prs(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id

    params =
      %{"searchCriteria.status" => parsed.options.status || "active"}
      |> put_if(parsed.options.creator, "searchCriteria.creatorId")
      |> put_if(parsed.options.top, "$top")

    result =
      Client.list(
        "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests",
        params
      )

    Helpers.handle_api_result(result, parsed, fn prs ->
      Helpers.json_or_format(prs, parsed, &print_prs_table/1)
    end)
  end

  @doc """
  Shows full details of a pull request.
  """
  def show_pr(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id
    pr_id = parsed.arguments.pr_id

    case Client.get(
           "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}"
         ) do
      {:ok, pr} ->
        Helpers.json_or_format(pr, parsed, &print_pr_detail/1)

      {:error, %{status: 404}} ->
        halt_error("Pull request ##{pr_id} not found in #{project}/#{repo_id}")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Write ─────────────────────────────────────────────────────────────

  @doc """
  Creates a new pull request.

  Requires `--title`, `--source` (source branch), and `--target` (target branch).
  Supports `--description` and `--draft`.
  """
  def create_pr(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id

    validate_pr_create_opts!(parsed.options)
    body = build_pr_body(parsed.options)

    case Client.post(
           "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests",
           body
         ) do
      {:ok, pr} ->
        writeln(success("Pull request ##{pr["pullRequestId"]} created: #{pr["title"]}"))
        writeln("  Status:    #{pr["status"]}")
        writeln("  Source:    #{pr["sourceRefName"]}")
        writeln("  Target:    #{pr["targetRefName"]}")
        writeln("  Created:   #{pr["creationDate"]}")
        writeln("  URL:       #{repo_pr_url(project, repo_id, pr["pullRequestId"])}")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc """
  Completes (merges) a pull request.

  Supports `--delete-source` to delete the source branch,
  and `--merge-strategy` (squash, rebase, noFastForward).
  """
  def complete_pr(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id
    pr_id = parsed.arguments.pr_id

    body = %{
      "status" => "completed",
      "deleteSourceBranch" => parsed.options.delete_source
    }

    body = put_if_key(merge_strategy(parsed.options.merge_strategy), body, "mergeStrategy")

    path =
      "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}"

    case Client.patch(path, body) do
      {:ok, pr} ->
        writeln(success("Pull request ##{pr["pullRequestId"]} completed (merged)."))
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Pull request ##{pr_id} not found")

      {:error, %{status: status, body: body}} ->
        halt_error(
          "Cannot complete PR ##{pr_id}: #{inspect(body["message"] || "HTTP #{status}")}"
        )

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc """
  Abandons a pull request without merging.
  """
  def abandon_pr(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id
    pr_id = parsed.arguments.pr_id

    body = %{"status" => "abandoned"}

    path =
      "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}"

    case Client.patch(path, body) do
      {:ok, pr} ->
        writeln(success("Pull request ##{pr["pullRequestId"]} abandoned."))
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Pull request ##{pr_id} not found")

      {:error, %{status: status, body: body}} ->
        halt_error("Cannot abandon PR ##{pr_id}: #{inspect(body["message"] || "HTTP #{status}")}")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Helpers ───────────────────────────────────────────────────────────

  defp put_if(map, nil, _key), do: map
  defp put_if(map, value, key), do: Map.put(map, key, value)

  defp put_if_key(nil, map, _key), do: map
  defp put_if_key(value, map, key), do: Map.put(map, key, value)

  defp validate_pr_create_opts!(opts) do
    unless opts.title, do: halt_error("--title is required")
    unless opts.source, do: halt_error("--source is required (source branch)")
    unless opts.target, do: halt_error("--target is required (target branch)")
  end

  defp build_pr_body(opts) do
    base = %{
      "title" => opts.title,
      "sourceRefName" => ensure_ref_prefix(opts.source),
      "targetRefName" => ensure_ref_prefix(opts.target),
      "isDraft" => opts.draft
    }

    put_if_key(opts.description, base, "description")
  end

  defp ensure_ref_prefix("refs/" <> _ = ref), do: ref
  defp ensure_ref_prefix(branch), do: "refs/heads/#{branch}"

  defp merge_strategy(nil), do: nil
  defp merge_strategy("squash"), do: "squashMerge"
  defp merge_strategy("rebase"), do: "rebaseMerge"
  defp merge_strategy("noFastForward"), do: "noFastForward"
  defp merge_strategy(other), do: other

  defp repo_pr_url(org \\ nil, project, repo_id, pr_id) do
    org = org || Application.get_env(:ado_cli, :azure_devops)[:org] || "{org}"
    base = Application.get_env(:ado_cli, :azure_devops)[:server] || "https://dev.azure.com"
    "#{base}/#{org}/#{project}/_git/#{repo_id}/pullrequest/#{pr_id}"
  end

  # ── Formatting ────────────────────────────────────────────────────────

  defp print_prs_table(prs) do
    if Enum.empty?(prs) do
      writeln("No pull requests found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 6)}  #{String.pad_trailing("Title", 50)}  Source -> Target       Status"
      )

      writeln(String.duplicate("─", 120))

      Enum.each(prs, fn pr ->
        id = to_string(pr["pullRequestId"] || "")
        title = pr["title"] || ""
        source = String.replace_prefix(pr["sourceRefName"] || "", "refs/heads/", "")
        target = String.replace_prefix(pr["targetRefName"] || "", "refs/heads/", "")
        status = pr["status"] || ""

        writeln(
          "#{String.pad_trailing(id, 6)}  #{String.pad_trailing(title, 50)}  #{source} -> #{String.pad_trailing(target, 15)} #{status}"
        )
      end)

      writeln("")
      writeln("#{length(prs)} pull request(s)")
    end
  end

  defp print_pr_detail(pr) do
    writeln("")
    writeln(success("Pull Request ##{pr["pullRequestId"]}"))
    writeln(String.duplicate("─", 60))
    writeln("  Title:       #{pr["title"]}")
    writeln("  Description: #{pr["description"] || "(none)"}")
    writeln("  Status:      #{pr["status"]}")
    writeln("  Source:      #{pr["sourceRefName"]}")
    writeln("  Target:      #{pr["targetRefName"]}")
    writeln("  Created By:  #{get_in(pr, ["createdBy", "displayName"]) || "?"}")
    writeln("  Created:     #{pr["creationDate"]}")

    if reviewers = pr["reviewers"] do
      writeln("  Reviewers:   #{Enum.map_join(reviewers, ", ", & &1["displayName"])}")
    end

    writeln("  URL:         #{pr["url"]}")
    writeln("")
  end
end
