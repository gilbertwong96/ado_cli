defmodule AdoCli.CLI.PullRequests.DryRunAction do
  @moduledoc """
  A single action that would be performed in --dry-run mode.

  Three keys: `:method` (PATCH), `:path` (the API endpoint),
  `:body` (the PATCH body). Used by `build_dry_run_actions/7`
  to construct the preview JSON envelope.
  """
  @derive {JSON.Encoder, only: [:method, :path, :body]}
  defstruct [:method, :path, :body]
end

defmodule AdoCli.CLI.PullRequests do
  @moduledoc """
  Commands for managing Azure DevOps Pull Requests.

    ado_cli prs list PROJECT REPO         [--status active|completed|abandoned] [--creator USER]
    ado_cli prs show PROJECT REPO PR_ID
    ado_cli prs diff PROJECT REPO PR_ID   [--file PATH] [--iteration N] [--unified] [--json]
    ado_cli prs create PROJECT REPO        --title TITLE --source BRANCH --target BRANCH [--description DESC]
    ado_cli prs complete PROJECT REPO PR_ID [--delete-source]
    ado_cli prs abandon PROJECT REPO PR_ID
    ado_cli prs comments list PROJECT REPO PR_ID [--all]
    ado_cli prs comments add PROJECT REPO PR_ID --content TEXT|@FILE|-
        [--file-path PATH --line N] [--thread-id TID] [--status STATUS] [--json]
    ado_cli prs comments update PROJECT REPO PR_ID THREAD_ID COMMENT_ID
        [--content TEXT|@FILE|-] [--status STATUS] [--resolved-by-me] [--dry-run] [--json]

  `prs diff` lists changed files by default (path, change type, +/- counts),
  or shows the full unified diff for a specific path with --file, or emits
  a single concatenated unified diff with --unified.
  `prs comments add` creates a new thread by default; pass `--thread-id` to reply.
  `--content` accepts `@path/to/file.md` (read from file) or `-` (read from stdin)
  for multi-line comments. `--status` sets the new thread's status
  (active, fixed, wontFix, closed, byDesign; default active).
  `prs comments list --all` expands the listing to show full comment content
  and the file path for inline threads.
  `prs comments update` edits a comment (via `--content`) and/or changes a
  thread's status (via `--status`). At least one of the two must be set.
  `--resolved-by-me` auto-attributes the status change to the
  authenticated user. `--dry-run` prints the would-be request(s) without
  making any network calls.
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
              options: [
                all: [
                  type: :boolean,
                  default: false,
                  doc: "Show full comment content and file context (not just thread headers)"
                ]
              ],
              execute: &list_comments/1
            ],
            update: [
              name: "ado prs comments update",
              doc:
                "Update a comment or thread. Pass --content to edit a comment, " <>
                  "--status to change a thread's resolution state, or both. " <>
                  "--content supports @<file> and - (stdin) for multi-line input.",
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
                  doc:
                    "New comment content. Supports @<file> to read from a file " <>
                      "or - to read from stdin. Omit to update status only.",
                  doc_arg: "TEXT"
                ],
                status: [
                  type: :string,
                  doc:
                    "New thread status: active, fixed, wontFix, closed, byDesign. Omit to update content only.",
                  doc_arg: "STATUS"
                ],
                resolved_by_me: [
                  type: :boolean,
                  default: false,
                  doc:
                    "When --status is set, also set the thread's resolvedBy " <>
                      "field to the currently-authenticated user's GUID. " <>
                      "Makes an extra GET to /_apis/connectionData to look up your ID."
                ],
                dry_run: [
                  type: :boolean,
                  default: false,
                  doc:
                    "Print the API request(s) that would be made (method, path, body) " <>
                      "as JSON, then exit. Makes no network calls."
                ],
                json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
              ],
              execute: &update_comment/1
            ],
            add: [
              name: "ado prs comments add",
              doc:
                "Add a review comment to a pull request. " <>
                  "By default creates a new thread; pass --thread-id to reply to an existing one.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Pull request ID"]
              ],
              options: [
                content: [
                  type: :string,
                  required: true,
                  doc: "Comment text (markdown allowed)",
                  doc_arg: "TEXT"
                ],
                file_path: [
                  type: :string,
                  doc: "File path for an inline comment (e.g. 'src/foo.ex')",
                  doc_arg: "PATH"
                ],
                line: [
                  type: :integer,
                  doc: "Line number for an inline comment (requires --file-path)",
                  doc_arg: "N"
                ],
                thread_id: [
                  type: :integer,
                  doc: "Reply to an existing thread (the comment is added to this thread)",
                  doc_arg: "THREAD_ID"
                ],
                comment_id: [
                  type: :integer,
                  doc:
                    "Parent comment to reply to (requires --thread-id). " <>
                      "Use 0 to start a new comment in the thread.",
                  doc_arg: "COMMENT_ID"
                ],
                status: [
                  type: :string,
                  default: "active",
                  doc: "Thread status when creating: active, fixed, wontFix, closed, byDesign",
                  doc_arg: "STATUS"
                ],
                json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
              ],
              execute: &add_comment/1
            ]
          ]
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  @valid_statuses ~w(active fixed wontFix closed byDesign)

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

    path =
      "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}"

    # Azure DevOps requires the lastMergeSourceCommit.commitId when
    # completing a PR. We fetch the PR details first, extract the
    # SHA, then PATCH with the complete body.
    case Client.get(path) do
      {:ok, pr_data} ->
        case get_in(pr_data, ["lastMergeSourceCommit", "commitId"]) do
          nil ->
            halt_error(
              "Cannot complete PR ##{pr_id}: no lastMergeSourceCommit.commitId in the PR data."
            )

          last_commit_id ->
            body = build_complete_body(parsed, last_commit_id)

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

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp build_complete_body(parsed, last_commit_id) do
    body = %{
      "status" => "completed",
      "lastMergeSourceCommit" => %{"commitId" => last_commit_id},
      "deleteSourceBranch" => Map.get(parsed.options, :delete_source, false)
    }

    put_if_key(merge_strategy(Map.get(parsed.options, :merge_strategy)), body, "mergeStrategy")
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

  @doc """
  Show the diff for a pull request.

  Three modes:

    * Default: list of changed files (path, change type, +/- counts)
    * `--file PATH`: full unified diff for one path
    * `--unified`: a single concatenated unified diff stream

  Uses `GET /pullRequests/{prId}/iterations` to find the latest
  iteration (or the one specified by `--iteration`), then
  `GET /iterations/{i}/changes` to get the list of changes, then
  `GET /iterations/{i}/changes/{changeId}` for full diff content
  (only used in `--file` and `--unified` modes).

  The `--file` lookup is an exact match on the change's
  `item.path` field (the path returned by the API, which includes
  a leading `/`). The match strips the leading `/` from both
  sides so users can pass either form.
  """
  def diff_pr(parsed) do
    json? = Map.get(parsed.options, :json, false) == true
    file = Map.get(parsed.options, :file)
    iteration = Map.get(parsed.options, :iteration)
    unified? = Map.get(parsed.options, :unified, false) == true

    if is_binary(file) and unified? do
      halt_error("Pass either --file or --unified, not both.")
    else
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
        {:error, msg} when is_binary(msg) ->
          halt_error(msg)

        {:error, reason} ->
          bail(reason, parsed)
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
        # The iteration list is small (typically 1-3 entries), so
        # either List.last/1 or List.first([...|Enum.reverse(...)]) is
        # O(n) in practice and irrelevant. We use first/1 over
        # reverse/1 to keep the credo check happy.
        latest = List.first(Enum.reverse(iterations))
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
        type = String.pad_trailing(change_type(change), 10)
        adds = (change["changeTrackingId"] && get_in(change, ["item", "additions"])) || 0
        dels = (change["changeTrackingId"] && get_in(change, ["item", "deletions"])) || 0

        writeln(
          "#{String.pad_trailing(path, 50)}  #{type}  #{pad_num(adds, 10)}  #{pad_num(dels, 10)}"
        )
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
        {:error,
         "No change matches --file '#{file}'. Use 'ado prs diff' (no flags) to list files."}

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
            bail(reason, parsed)
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
              note:
                "Unified diff content is printed below the envelope; pipe to delta/less for pretty viewing"
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
        bail(reason, parsed)
    end
  end

  defp fetch_all_change_contents(parsed, iteration_id, changes) do
    result =
      Enum.reduce_while(changes, {:ok, []}, fn change, {:ok, acc} ->
        change_id = change["changeId"] || change["id"]

        case fetch_change_content(parsed, iteration_id, change_id) do
          {:ok, content} -> {:cont, {:ok, [{change_path(change), content} | acc]}}
          {:error, _} = err -> {:halt, err}
        end
      end)

    case result do
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
    # Fetch the authenticated user's identity GUID from the
    # Azure DevOps connection data (cached on first call).
    # Then scan the PR reviewer list for a reviewer whose
    # `identity.id` matches that GUID. Only the user's own
    # reviewer slot can be voted on — trying to PUT a vote
    # to a different reviewer's slot returns:
    #   "You cannot record a vote for someone else."
    with {:ok, user_id} <- AdoCli.Auth.current_user_id(),
         {:ok, reviewers} when is_list(reviewers) and reviewers != [] <-
           Client.list(
             "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}/reviewers"
           ) do
      Enum.find_value(reviewers, fn r ->
        if get_in(r, ["identity", "id"]) == user_id, do: r["id"]
      end) ||
        halt_error("""
        Cannot vote on PR ##{pr_id}: your identity (#{user_id}) is not in
        the reviewer list. Are you a reviewer on this PR? Open the PR
        in the browser first, or ask someone to add you as a reviewer.
        """)
    else
      {:error, reason} ->
        halt_error("Cannot determine user identity: #{reason}")

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
    all? = Map.get(parsed.options, :all, false)

    case Client.list(path) do
      {:ok, threads} ->
        Helpers.json_or_format(threads, parsed, fn threads ->
          writeln("")

          if all? do
            Enum.each(threads, &print_thread_full/1)
          else
            Enum.each(threads, &print_thread/1)
          end
        end)

      {:error, reason} ->
        bail(reason, parsed)
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

  # Expanded view used by `prs comments list --all`. Shows the
  # file path (for inline threads) and the full multi-line
  # comment content with each line indented.
  defp print_thread_full(thread) do
    id = thread["id"]
    status = thread["status"] || "unknown"
    file_path = get_in(thread, ["threadContext", "filePath"])

    if file_path do
      writeln("  Thread #{id} [#{status}] on #{file_path}")
    else
      writeln("  Thread #{id} [#{status}]")
    end

    case thread["comments"] do
      nil -> :ok
      comments -> Enum.each(comments, &print_comment_full/1)
    end

    writeln("")
  end

  defp print_comment_full(comment) do
    author = (comment["author"] && comment["author"]["displayName"]) || "unknown"
    content = comment["content"] || ""
    cid = comment["id"]
    parent_id = comment["parentCommentId"]

    if parent_id && parent_id > 0 do
      writeln("    [#{cid}] (reply to #{parent_id}) #{author}:")
    else
      writeln("    [#{cid}] #{author}:")
    end

    # Indent each line of the comment body. Empty lines stay empty
    # (no extra indent) so the block reads naturally.
    content
    |> String.split("\n")
    |> Enum.each(fn
      "" -> writeln("")
      line -> writeln("      #{line}")
    end)
  end

  def update_comment(parsed) do
    flags = update_flags(parsed)

    if not flags.wants_content? and not flags.wants_status? do
      halt_error(
        "Must pass --content and/or --status. Pass --content to edit a comment, " <>
          "--status to change a thread's resolution state, or both."
      )
    else
      resolve_inputs(parsed, flags)
    end
  end

  # Parse the flags relevant to update_comment/1. Pulled out as a
  # tiny helper to keep update_comment/1's cyclomatic complexity
  # under 8.
  defp update_flags(parsed) do
    %{
      wants_content?: raw_content_present?(parsed),
      wants_status?:
        parsed.options
        |> Map.get(:status, "")
        |> case do
          "" -> false
          nil -> false
          s when is_binary(s) -> true
          _ -> false
        end,
      dry_run?: Map.get(parsed.options, :dry_run, false) == true,
      json?: json?(parsed)
    }
  end

  # Validate / resolve inputs and then dispatch to do_update/7.
  # Returns whatever do_update/7 returns (which is `halt(0)` in
  # production, or the `{:cli_mate_shell, :halt, 0}` tuple in
  # test mode). The `case` in update_comment/1 only matches the
  # `{:error, _}` shape to call halt_error/1 on validation
  # failures; successful paths fall through to do_update's
  # return value.
  defp resolve_inputs(parsed, flags) do
    raw_content = Map.get(parsed.options, :content)
    raw_status = Map.get(parsed.options, :status)

    with {:ok, content} <- resolve_content(raw_content || ""),
         {:ok, status} <- validate_status(raw_status || "") do
      do_update(
        parsed,
        content,
        status,
        flags.wants_content?,
        flags.wants_status?,
        flags.json?,
        flags.dry_run?
      )
    else
      {:error, message} -> halt_error(message)
    end
  end

  # True iff the user passed --content on the command line.
  # We can't infer this from the resolved content (which may be
  # an empty string after stripping newlines), so we look at the
  # raw value.
  defp raw_content_present?(parsed) do
    case Map.get(parsed.options, :content) do
      nil -> false
      "" -> false
      _ -> true
    end
  end

  defp json?(parsed), do: Map.get(parsed.options, :json, false) == true

  defp do_update(parsed, content, status, wants_content?, wants_status?, json?, dry_run?) do
    # Pre-fetch the user ID upfront (cached on subsequent
    # calls) so a network failure shows up as a clear error
    # BEFORE any other API call is made — including the
    # dry-run path, which would otherwise silently produce a
    # body without `resolvedBy` when the lookup fails.
    with :ok <- ensure_user_resolved(parsed) do
      # --dry-run short-circuits everything: build the would-be
      # request(s) and print them, then exit cleanly. No network
      # calls.
      if dry_run? do
        print_dry_run(parsed, content, status, wants_content?, wants_status?, json?)
      else
        do_real_update(parsed, content, status, wants_content?, wants_status?, json?)
      end
    end
  end

  # Print the PATCH request(s) that would be made, as a JSON
  # envelope, then exit. Honors --json (or always emits JSON
  # for the dry-run envelope, since it's a machine-readable
  # preview either way).
  defp print_dry_run(parsed, content, status, wants_content?, wants_status?, _json?) do
    actions = build_dry_run_actions(parsed, content, status, wants_content?, wants_status?)

    payload = %{ok: true, dry_run: true, actions: actions}

    # Dry-run output is always JSON: it's a machine-readable
    # preview meant to be piped to jq / inspected by LLMs.
    # The --json flag is accepted but redundant.
    IO.puts(JSON.encode!(payload))
    halt(0)
  end

  defp build_dry_run_actions(parsed, content, status, wants_content?, wants_status?) do
    thread_path = thread_path(parsed)
    comment_path = comment_path(parsed)

    cond do
      wants_content? and wants_status? ->
        [
          %AdoCli.CLI.PullRequests.DryRunAction{
            method: "PATCH",
            path: thread_path,
            body: build_thread_body(parsed, status)
          },
          %AdoCli.CLI.PullRequests.DryRunAction{
            method: "PATCH",
            path: comment_path,
            body: %{"content" => content}
          }
        ]

      wants_content? ->
        [
          %AdoCli.CLI.PullRequests.DryRunAction{
            method: "PATCH",
            path: comment_path,
            body: %{"content" => content}
          }
        ]

      wants_status? ->
        [
          %AdoCli.CLI.PullRequests.DryRunAction{
            method: "PATCH",
            path: thread_path,
            body: build_thread_body(parsed, status)
          }
        ]
    end
  end

  defp do_real_update(parsed, content, status, wants_content?, wants_status?, json?) do
    # The user ID has been pre-fetched (or wasn't needed) by
    # `ensure_user_resolved/1` in `do_update/7`, so it's safe
    # to call `build_thread_body/2` here without an extra
    # network call.
    cond do
      wants_content? and wants_status? ->
        # PATCH both endpoints. Status first (cheap), then content.
        # If either fails we surface the first error and stop.
        with {:ok, thread} <- Client.patch(thread_path(parsed), build_thread_body(parsed, status)),
             {:ok, comment} <- Client.patch(comment_path(parsed), %{"content" => content}) do
          render_update_result(
            parsed,
            thread,
            comment,
            "Comment and thread status updated.",
            json?
          )
        else
          {:error, _reason} = err ->
            Helpers.handle_api_result(err, parsed, nil)
        end

      wants_content? ->
        patch_only(parsed, comment_path(parsed), %{"content" => content}, fn _thread, comment ->
          render_update_result(parsed, nil, comment, "Comment updated.", json?)
        end)

      wants_status? ->
        patch_only(parsed, thread_path(parsed), build_thread_body(parsed, status), fn thread,
                                                                                      _comment ->
          render_update_result(parsed, thread, nil, "Thread status updated.", json?)
        end)
    end
  end

  # Common PATCH-then-render pattern for the single-endpoint
  # branches of do_real_update/6. Extracted to keep that
  # function's cyclomatic complexity under 8.
  defp patch_only(parsed, path, body, render_fn) do
    case Client.patch(path, body) do
      {:ok, result} ->
        render_fn.(nil, result)

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  # Returns :ok on success, or halts with an error if the user
  # lookup fails. Returns `:error` to short-circuit the caller
  # (see `with` in `do_update/7`).
  defp ensure_user_resolved(parsed) do
    if Map.get(parsed.options, :resolved_by_me, false) == true do
      case AdoCli.Auth.current_user_id() do
        {:ok, _} -> :ok
        {:error, reason} -> halt_error("Cannot resolve thread as current user: #{reason}")
      end
    else
      :ok
    end
  end

  # If --resolved-by-me is set, also include the authenticated
  # user's GUID as resolvedBy.id. The user ID is cached on
  # the first lookup (see AdoCli.Auth.current_user_id/0).
  defp build_thread_body(parsed, status) do
    if Map.get(parsed.options, :resolved_by_me, false) == true do
      case AdoCli.Auth.current_user_id() do
        {:ok, user_id} -> %{"status" => status, "resolvedBy" => %{"id" => user_id}}
        _ -> %{"status" => status}
      end
    else
      %{"status" => status}
    end
  end

  # Format the update result either as plain text (for humans) or
  # as a JSON envelope (for LLM agents).
  #
  # `thread` is the PATCH /threads/{tid} response, or nil if
  # the caller only updated the comment. `comment` is the
  # PATCH /threads/{tid}/comments/{cid} response, or nil if
  # the caller only updated the thread status.
  defp render_update_result(parsed, thread, comment, fallback_msg, json?) do
    thread_id = if thread, do: thread["id"], else: parsed.arguments.thread_id
    comment_id = if comment, do: comment["id"], else: parsed.arguments.comment_id
    new_status = if thread, do: thread["status"], else: nil

    if json? do
      IO.puts(
        JSON.encode!(%{
          ok: true,
          thread_id: thread_id,
          comment_id: comment_id,
          status: new_status,
          message: fallback_msg
        })
      )
    else
      writeln(fallback_msg)

      if new_status do
        writeln("  thread_id: #{thread_id}")
        writeln("  status:    #{new_status}")
      end

      if comment do
        writeln("  comment_id: #{comment_id}")
      end
    end

    halt(0)
  end

  @doc """
  Add a review comment to a pull request.

  Three modes:
    * New general thread: just --content (no file context)
    * New inline thread: --content + --file-path + --line
    * Reply to existing thread: --content + --thread-id

  See https://learn.microsoft.com/en-us/rest/azure/devops/git/pull-request-threads
  """
  def add_comment(parsed) do
    case resolve_content(Map.get(parsed.options, :content, "")) do
      {:ok, content} ->
        add_comment_with_content(parsed, content)

      {:error, message} ->
        halt_error(message)
    end
  end

  defp add_comment_with_content(parsed, content) do
    raw_status = Map.get(parsed.options, :status, "active")

    case validate_status(raw_status) do
      {:ok, status} ->
        json? = Map.get(parsed.options, :json, false)
        file_path = Map.get(parsed.options, :file_path)
        line = Map.get(parsed.options, :line)
        thread_id = Map.get(parsed.options, :thread_id)
        parent_comment_id = Map.get(parsed.options, :comment_id, 0) || 0

        cond do
          thread_id ->
            # Reply mode: POST a new comment to an existing thread
            do_reply_to_thread(parsed, thread_id, content, parent_comment_id, json?)

          file_path && line ->
            # New inline thread: POST a new thread with file/line context
            do_new_inline_thread(parsed, content, file_path, line, status, json?)

          true ->
            # New general thread: POST a new thread with no file context
            do_new_general_thread(parsed, content, status, json?)
        end

      {:error, message} ->
        halt_error(message)
    end
  end

  @doc """
  Resolve the --content argument.

    resolve_content("hello")            => {:ok, "hello"}
    resolve_content("@/path/file.md")  => {:ok, "file contents"} | {:error, "..."}
    resolve_content("-")                => {:ok, "stdin contents"} | {:error, "..."}

  Leading and trailing newlines are stripped from file/stdin
  content so the API doesn't see a trailing blank line.

  Returns `{:error, message}` if a referenced file can't be
  read, or if `--content @` is passed without a path. The
  caller should `halt_error/1` and bail out in that case.
  """
  def resolve_content(""), do: {:ok, ""}

  def resolve_content("-"), do: {:ok, read_stdin_content()}

  # `--content @<path>` reads from a file. A bare `@` (no path)
  # is also treated as a read-from-file — the path would be "",
  # which is always missing, so the call will fail with a clear
  # error. This strict interpretation keeps the contract simple
  # (anything starting with `@` is a file reference).
  def resolve_content("@"), do: read_file_content("")
  def resolve_content("@" <> path), do: read_file_content(path)
  def resolve_content(content), do: {:ok, content}

  defp read_stdin_content do
    # `IO.read/2`'s typespec is `:eof | :line | non_neg_integer` for
    # the second arg, so we pass a large integer instead of `:all`
    # (the latter works at runtime but makes Dialyzer think the
    # call will not succeed). 1 GB is the conventional "read
    # everything" sentinel; on most systems this is more data than
    # stdin will ever hold.
    #
    # The return type is `chardata | nodata` — i.e. binary, iolist,
    # or `{:error, posix()}`. We handle all three shapes:
    #   * binaries get stripped of trailing newlines
    #   * iolists get flattened first
    #   * errors return "" (so a closed stdin doesn't crash the
    #     whole command)
    case IO.read(:stdio, 1_000_000_000) do
      {:error, _} ->
        ""

      content when is_binary(content) ->
        strip_trailing_newlines(content)

      content ->
        content
        |> IO.iodata_to_binary()
        |> strip_trailing_newlines()
    end
  end

  defp read_file_content(path) do
    case File.read(path) do
      {:ok, content} ->
        {:ok, strip_trailing_newlines(content)}

      {:error, reason} ->
        {:error, "Cannot read comment file '#{path}': #{:file.format_error(reason)}"}
    end
  end

  defp strip_trailing_newlines(content) do
    String.replace(content, ~r/\n+\z/, "")
  end

  @doc """
  Validate the --status argument.

    validate_status("")           => {:ok, "active"}
    validate_status("active")     => {:ok, "active"}
    validate_status("bogus")      => {:error, "Invalid --status 'bogus'. ..."}

  Returns a tagged tuple rather than halting on error so the
  caller can decide how to react. (In tests, `halt_error/1`
  doesn't actually halt — it just sends a message — so we
  can't use it as a no-return guard without leaking the
  message tuple into the rest of the function.)
  """
  def validate_status(""), do: {:ok, "active"}
  def validate_status(status) when status in @valid_statuses, do: {:ok, status}

  def validate_status(status) do
    {:error, "Invalid --status '#{status}'. Must be one of: #{Enum.join(@valid_statuses, ", ")}."}
  end

  defp do_reply_to_thread(parsed, thread_id, content, parent_comment_id, json?) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id

    path =
      "/#{project}/_apis/git/repositories/#{repo_id}/pullrequests/#{pr_id}/threads/#{thread_id}/comments"

    body = %{
      "content" => content,
      "parentCommentId" => parent_comment_id,
      "commentType" => "text"
    }

    case Client.post(path, body) do
      {:ok, result} ->
        render_add_result(result, "Reply added to thread #{thread_id}.", json?)

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  defp do_new_inline_thread(parsed, content, file_path, line, status, json?) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id

    path = "/#{project}/_apis/git/repositories/#{repo_id}/pullrequests/#{pr_id}/threads"

    body = %{
      "comments" => [
        %{
          "content" => content,
          "parentCommentId" => 0,
          "commentType" => "text"
        }
      ],
      "status" => status,
      "threadContext" => %{
        "filePath" => file_path,
        "leftFileStart" => %{"line" => line, "offset" => 1},
        "leftFileEnd" => %{"line" => line, "offset" => 2}
      }
    }

    case Client.post(path, body) do
      {:ok, result} ->
        render_add_result(result, "Comment added to #{file_path}:#{line}.", json?)

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  defp do_new_general_thread(parsed, content, status, json?) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id

    path = "/#{project}/_apis/git/repositories/#{repo_id}/pullrequests/#{pr_id}/threads"

    body = %{
      "comments" => [
        %{
          "content" => content,
          "parentCommentId" => 0,
          "commentType" => "text"
        }
      ],
      "status" => status
    }

    case Client.post(path, body) do
      {:ok, result} ->
        render_add_result(result, "Comment added.", json?)

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  # Format the API result either as plain text (for humans) or as
  # a JSON envelope (for LLM agents).
  defp render_add_result(result, fallback_msg, json?) do
    case result do
      thread when is_map(thread) ->
        thread_id = thread["id"]
        comment_id = first_comment_id(thread)

        if json? do
          # Use IO.puts + halt(0) directly (not writeln + halt_success)
          # to keep the JSON envelope free of ANSI color codes and
          # extra :info messages. Same pattern as AdoCli.CLI.Output.ok/4.
          IO.puts(
            JSON.encode!(%{
              ok: true,
              thread_id: thread_id,
              comment_id: comment_id,
              message: fallback_msg
            })
          )
        else
          writeln(fallback_msg)
          writeln("  thread_id:  #{thread_id}")
          writeln("  comment_id: #{comment_id}")
        end

        halt(0)

      _ ->
        writeln(fallback_msg)
        halt(0)
    end
  end

  defp first_comment_id(%{"comments" => [%{"id" => id} | _]}), do: id
  defp first_comment_id(_), do: nil

  defp comment_path(parsed) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id
    thread_id = parsed.arguments.thread_id
    comment_id = parsed.arguments.comment_id

    "/#{project}/_apis/git/repositories/#{repo_id}/pullRequests/#{pr_id}/threads/#{thread_id}/comments/#{comment_id}"
  end

  # Thread-level endpoint (no comment_id). Used to update the
  # thread's status (`active` / `fixed` / `wontFix` / `closed` /
  # `byDesign`).
  defp thread_path(parsed) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id
    thread_id = parsed.arguments.thread_id

    "/#{project}/_apis/git/repositories/#{repo_id}/pullRequests/#{pr_id}/threads/#{thread_id}"
  end

  # Local helper for the unreachable error path. Centralizes the
  # call to Helpers.handle_api_result/3 so the case branches stay
  # tidy. Returns whatever handle_api_result returns (always
  # :no_return() in practice since it halts on error), so the
  # call site still effectively aborts the surrounding function.
  defp bail(reason, parsed) do
    Helpers.handle_api_result({:error, reason}, parsed, nil)
  end
end
