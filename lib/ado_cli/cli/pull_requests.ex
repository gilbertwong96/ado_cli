defmodule AdoCli.CLI.PullRequests do
  @moduledoc """
  Commands for managing Azure DevOps Pull Requests.

    ado_cli prs list PROJECT REPO         [--status active|completed|abandoned] [--creator USER]
    ado_cli prs show PROJECT REPO PR_ID
    ado_cli prs diff PROJECT REPO PR_ID   [--file PATH] [--iteration N] [--unified] [--json]
    ado_cli prs create PROJECT REPO        --title TITLE --source BRANCH --target BRANCH [--description DESC]
    ado_cli prs complete PROJECT REPO PR_ID [--delete-source]
    ado_cli prs abandon PROJECT REPO PR_ID

  `prs diff` lists changed files by default (path, change type, +/- counts),
  or shows the full unified diff for a specific path with --file, or emits
  a single concatenated unified diff with --unified.
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado prs",
      doc: "Manage Azure DevOps pull requests.",
      subcommands: [
        list: [
          name: "ado prs list",
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
          name: "ado prs show",
          doc: "Show details of a specific pull request.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Pull request ID"]
          ],
          execute: &show_pr/1
        ],
        create: [
          name: "ado prs create",
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
          name: "ado prs complete",
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
        approve: [
          name: "ado prs approve",
          doc: "Approve a pull request (vote +10).",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Pull request ID"]
          ],
          execute: &approve_pr/1
        ],
        vote: [
          name: "ado prs vote",
          doc: "Vote on a pull request.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Pull request ID"]
          ],
          options: [
            vote: [
              type: :integer,
              required: true,
              doc:
                "Vote: 10 (approve), 5 (approve with suggestions), 0 (reset), -5 (wait), -10 (reject)",
              doc_arg: "VOTE"
            ]
          ],
          execute: &vote_pr/1
        ],
        abandon: [
          name: "ado prs abandon",
          doc: "Abandon a pull request.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Pull request ID"]
          ],
          execute: &abandon_pr/1
        ],
        diff: [
          name: "ado prs diff",
          doc:
            "Show the diff for a pull request. " <>
              "By default lists changed files (path, change type, +/- counts). " <>
              "Pass --file to see the full unified diff for one path. " <>
              "Pass --unified to emit a single unified diff stream (pipe to less/delta). " <>
              "Pass --iteration to inspect an earlier iteration (default: latest).",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Pull request ID"]
          ],
          options: [
            file: [
              type: :string,
              doc: "Show the full diff for a single path (relative to repo root)",
              doc_arg: "PATH"
            ],
            iteration: [
              type: :integer,
              doc: "Iteration number to inspect (default: latest)",
              doc_arg: "N"
            ],
            unified: [
              type: :boolean,
              default: false,
              doc: "Output a single unified diff stream for all files"
            ],
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &diff_pr/1
        ],
        comments: [
          name: "ado prs comments",
          doc: "Manage pull request review comments.",
          subcommands: [
            list: [
              name: "ado prs comments list",
              doc: "List review threads on a pull request.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Pull request ID"]
              ],
              execute: &list_comments/1
            ],
            update: [
              name: "ado prs comments update",
              doc: "Update a review comment.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Pull request ID"],
                thread_id: [type: :integer, doc: "Thread ID"],
                comment_id: [type: :integer, doc: "Comment ID"]
              ],
              options: [
                content: [
                  type: :string,
                  doc: "New comment content",
                  required: true,
                  doc_arg: "TEXT"
                ]
              ],
              execute: &update_comment/1
            ]
          ]
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
      %{"searchCriteria.status" => Map.get(parsed.options, :status, "active")}
      |> put_if(Map.get(parsed.options, :creator), "searchCriteria.creatorId")
      |> put_if(Map.get(parsed.options, :top), "$top")

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
        success("Pull request ##{pr["pullRequestId"]} created: #{pr["title"]}\n")
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
      "deleteSourceBranch" => Map.get(parsed.options, :delete_source, false)
    }

    body =
      put_if_key(merge_strategy(Map.get(parsed.options, :merge_strategy)), body, "mergeStrategy")

    path =
      "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}"

    case Client.patch(path, body) do
      {:ok, pr} ->
        success("Pull request ##{pr["pullRequestId"]} completed (merged).\n")
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
  Approves a pull request (vote = +10).
  """
  def approve_pr(parsed), do: vote_pr(%{parsed | options: Map.put(parsed.options, :vote, 10)})

  @doc """
  Votes on a pull request.

  Requires `--vote` with one of: 10 (approve), 5 (approve with suggestions),
  0 (reset), -5 (wait for author), -10 (reject).
  """
  def vote_pr(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id
    pr_id = parsed.arguments.pr_id
    vote_value = parsed.options.vote
    reviewer_id = resolve_reviewer_id(project, repo_id, pr_id)

    path =
      "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}/reviewers/#{reviewer_id}"

    case Client.put(path, %{"vote" => vote_value}) do
      {:ok, _result} ->
        label = vote_label(vote_value)
        success("Voted #{label} on PR ##{pr_id}.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Pull request ##{pr_id} not found")

      {:error, %{status: status, body: body}} ->
        halt_error("Cannot vote on PR ##{pr_id}: #{inspect(body["message"] || "HTTP #{status}")}")

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
        success("Pull request ##{pr["pullRequestId"]} abandoned.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Pull request ##{pr_id} not found")

      {:error, %{status: status, body: body}} ->
        halt_error("Cannot abandon PR ##{pr_id}: #{inspect(body["message"] || "HTTP #{status}")}")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def diff_pr(parsed) do
    json? = Map.get(parsed.options, :json, false) == true
    file = Map.get(parsed.options, :file)
    iteration = Map.get(parsed.options, :iteration)
    unified? = Map.get(parsed.options, :unified, false) == true

    cond do
      is_binary(file) and unified? ->
        halt_error("Pass either --file or --unified, not both.")

      true ->
        # All inner functions return `{:ok, _} | {:error, _}`.
        # `halt_error/1` is only called here, exactly once, so
        # the test-mode leak (which returns a 3-tuple) doesn't
        # reach a case clause and crash with CaseClauseError.
        # We halt(0) at the end of the success branch so the
        # process exits cleanly in production.
        with {:ok, iteration_id} <- resolve_iteration(parsed, iteration),
             {:ok, changes} <- fetch_changes(parsed, iteration_id),
             :ok <- render_diff(parsed, changes, iteration_id, file, unified?, json?) do
          halt(0)
        else
          {:error, msg} when is_binary(msg) -> halt_error(msg)
          {:error, reason} -> Helpers.handle_api_result({:error, reason}, parsed, nil)
        end
    end
  end

  # Resolves the iteration ID. If the user passed --iteration N,
  # use that. Otherwise, fetch the iteration list and take the
  # last one (latest). If the PR has no iterations (rare but
  # possible for empty PRs), return an `{:error, message}`.
  defp resolve_iteration(parsed, nil) do
    case Client.get(iterations_path(parsed)) do
      {:ok, %{"value" => []}} ->
        {:error, "PR ##{parsed.arguments.pr_id} has no iterations (nothing to diff)."}

      {:ok, %{"value" => iterations}} when is_list(iterations) ->
        latest = List.last(iterations)
        id = latest && (latest["id"] || latest["number"])

        if id do
          {:ok, id}
        else
          {:error, "Could not determine latest iteration ID"}
        end

      {:error, _} = err ->
        err
    end
  end

  defp resolve_iteration(_parsed, n) when is_integer(n) and n > 0, do: {:ok, n}

  defp fetch_changes(parsed, iteration_id) do
    Client.list(changes_path(parsed, iteration_id))
  end

  defp render_diff(parsed, changes, iteration_id, file, unified?, json?) do
    cond do
      is_binary(file) ->
        render_file_diff(parsed, iteration_id, changes, file, json?)

      unified? ->
        render_unified(parsed, iteration_id, changes, json?)

      true ->
        render_file_list(changes, iteration_id, json?)
    end
  end

  # Default view: list of files with metadata.
  defp render_file_list(changes, iteration_id, json?) do
    summary = summarize_changes(changes)

    if json? do
      IO.puts(
        JSON.encode!(%{
          ok: true,
          iteration: iteration_id,
          count: length(changes),
          total_additions: summary.additions,
          total_deletions: summary.deletions,
          changes: Enum.map(changes, &change_to_envelope/1)
        })
      )
    else
      writeln("")
      writeln("PR diff (iteration #{iteration_id})")
      writeln(
        "#{String.pad_trailing("PATH", 50)}  #{String.pad_trailing("TYPE", 10)}  ADDITIONS  DELETIONS"
      )

      writeln(String.duplicate("─", 90))

      Enum.each(changes, fn change ->
        path = change_path(change)
        type = change_type(change) |> String.pad_trailing(10)
        adds = change["changeTrackingId"] && get_in(change, ["item", "additions"]) || 0
        dels = change["changeTrackingId"] && get_in(change, ["item", "deletions"]) || 0

        writeln("#{String.pad_trailing(path, 50)}  #{type}  #{pad_num(adds, 10)}  #{pad_num(dels, 10)}")
      end)

      writeln("")
      writeln("#{length(changes)} file(s) changed, +#{summary.additions} -#{summary.deletions}")
    end

    :ok
  end

  # --file: fetch the full diff for one path.
  defp render_file_diff(parsed, iteration_id, changes, file, json?) do
    case find_change_for_file(changes, file) do
      nil ->
        {:error, "No change matches --file '#{file}'. Use 'ado prs diff' (no flags) to list files."}

      change ->
        change_id = change["changeId"] || change["id"]
        path = change_path(change)

        case fetch_change_content(parsed, iteration_id, change_id) do
          {:ok, content} ->
            if json? do
              IO.puts(
                JSON.encode!(%{
                  ok: true,
                  iteration: iteration_id,
                  path: path,
                  change_type: change_type(change),
                  diff: content
                })
              )
            else
              IO.puts(content)
            end

            :ok

          {:error, reason} ->
            Helpers.handle_api_result({:error, reason}, parsed, nil)
        end
    end
  end

  # --unified: emit all file diffs concatenated.
  defp render_unified(parsed, iteration_id, changes, json?) do
    # Fetch each change's content in sequence. If any fails,
    # surface the first error and stop.
    case fetch_all_change_contents(parsed, iteration_id, changes) do
      {:ok, fetched} ->
        if json? do
          IO.puts(
            JSON.encode!(%{
              ok: true,
              iteration: iteration_id,
              mode: "unified",
              file_count: length(fetched),
              note: "Unified diff content is printed below the envelope; pipe to delta/less for pretty viewing"
            })
          )

          IO.puts("")
        end

        Enum.each(fetched, fn {_path, content} ->
          IO.puts(content)
          IO.puts("")
        end)

        :ok

      {:error, reason} ->
        Helpers.handle_api_result({:error, reason}, parsed, nil)
    end
  end

  defp fetch_all_change_contents(parsed, iteration_id, changes) do
    Enum.reduce_while(changes, {:ok, []}, fn change, {:ok, acc} ->
      change_id = change["changeId"] || change["id"]

      case fetch_change_content(parsed, iteration_id, change_id) do
        {:ok, content} -> {:cont, {:ok, [{change_path(change), content} | acc]}}
        {:error, _} = err -> {:halt, err}
      end
    end)
    |> case do
      {:ok, list} -> {:ok, Enum.reverse(list)}
      other -> other
    end
  end

  # ── diff helpers ────────────────────────────────────────────────

  defp fetch_change_content(parsed, iteration_id, change_id) do
    Client.get_raw(single_change_path(parsed, iteration_id, change_id))
  end

  defp find_change_for_file(changes, target) do
    # Strip leading '/' from the target so users can pass either
    # 'src/foo.ex' or '/src/foo.ex'. The API returns paths with
    # a leading slash.
    target = String.trim_leading(target, "/")

    Enum.find(changes, fn change ->
      String.trim_leading(change_path(change), "/") == target
    end)
  end

  defp change_path(change) do
    get_in(change, ["item", "path"]) || change["originalPath"] || change["path"] || "?"
  end

  defp change_type(change) do
    case change["changeType"] do
      1 -> "add"
      2 -> "edit"
      4 -> "delete"
      8 -> "rename"
      16 -> "directory"
      _ -> "change"
    end
  end

  defp change_to_envelope(change) do
    %{
      path: change_path(change),
      change_type: change_type(change),
      change_id: change["changeId"] || change["id"],
      additions: get_in(change, ["item", "additions"]) || 0,
      deletions: get_in(change, ["item", "deletions"]) || 0
    }
  end

  defp summarize_changes(changes) do
    Enum.reduce(changes, %{additions: 0, deletions: 0}, fn change, acc ->
      %{
        additions: acc.additions + (get_in(change, ["item", "additions"]) || 0),
        deletions: acc.deletions + (get_in(change, ["item", "deletions"]) || 0)
      }
    end)
  end

  defp pad_num(n, width), do: n |> to_string() |> String.pad_leading(width)

  # ── diff paths ──────────────────────────────────────────────────

  defp iterations_path(parsed) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id

    "/#{project}/_apis/git/repositories/#{repo_id}/pullRequests/#{pr_id}/iterations"
  end

  defp changes_path(parsed, iteration_id) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id

    "/#{project}/_apis/git/repositories/#{repo_id}/pullRequests/#{pr_id}/iterations/#{iteration_id}/changes"
  end

  defp single_change_path(parsed, iteration_id, change_id) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id

    "/#{project}/_apis/git/repositories/#{repo_id}/pullRequests/#{pr_id}/iterations/#{iteration_id}/changes/#{change_id}"
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

  defp resolve_reviewer_id(project, repo_id, pr_id) do
    case Client.list(
           "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}/reviewers"
         ) do
      {:ok, reviewers} when is_list(reviewers) and reviewers != [] ->
        Enum.find_value(reviewers, & &1["id"]) ||
          halt_error("Cannot determine reviewer ID for PR ##{pr_id}")

      _ ->
        halt_error(
          "Cannot find reviewers for PR ##{pr_id}. Try opening the PR in the browser first."
        )
    end
  end

  defp vote_label(10), do: "+10 (approved)"
  defp vote_label(5), do: "+5 (approved with suggestions)"
  defp vote_label(0), do: "0 (reset)"
  defp vote_label(-5), do: "-5 (waiting for author)"
  defp vote_label(-10), do: "-10 (rejected)"
  defp vote_label(n), do: "#{n}"

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
    success("Pull Request ##{pr["pullRequestId"]}\n")
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

  # ── Comments ────────────────────────────────────────────────────────

  def list_comments(parsed) do
    path = comments_path(parsed)

    case Client.list(path) do
      {:ok, threads} ->
        Helpers.json_or_format(threads, parsed, fn threads ->
          writeln("")
          Enum.each(threads, &print_thread/1)
        end)

      {:error, reason} ->
        Helpers.handle_api_result({:error, reason}, parsed, nil)
    end

    halt_success("Done.")
  end

  defp comments_path(parsed) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id
    "/#{project}/_apis/git/repositories/#{repo_id}/pullRequests/#{pr_id}/threads"
  end

  defp print_thread(thread) do
    id = thread["id"]
    status = thread["status"] || "unknown"
    writeln("  Thread #{id} [#{status}]")

    case thread["comments"] do
      nil -> :ok
      comments -> Enum.each(comments, &print_comment/1)
    end

    writeln("")
  end

  defp print_comment(comment) do
    author = (comment["author"] && comment["author"]["displayName"]) || "unknown"
    content = comment["content"] || ""
    cid = comment["id"]
    writeln("    [#{cid}] #{author}: #{String.slice(content, 0, 80)}")
  end

  def update_comment(parsed) do
    path = comment_path(parsed)
    body = %{"content" => parsed.options.content}

    case Client.patch(path, body) do
      {:ok, _} ->
        success("Comment #{parsed.arguments.comment_id} updated.\n\n")

      {:error, reason} ->
        Helpers.handle_api_result({:error, reason}, parsed, nil)
    end

    halt_success("Done.")
  end

  defp comment_path(parsed) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id
    thread_id = parsed.arguments.thread_id
    comment_id = parsed.arguments.comment_id

    "/#{project}/_apis/git/repositories/#{repo_id}/pullRequests/#{pr_id}/threads/#{thread_id}/comments/#{comment_id}"
  end
end
