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
        [--file-path PATH --line N [--end-line N]] [--thread-id TID] [--status STATUS] [--json]
    ado_cli prs comments update PROJECT REPO PR_ID THREAD_ID COMMENT_ID
        [--content TEXT|@FILE|-] [--status STATUS] [--resolved-by-me] [--dry-run] [--json]
    ado_cli prs comments delete PROJECT REPO PR_ID THREAD_ID [--comment-id ID] [--force] [--json]
    ado_cli prs comments resolve PROJECT REPO PR_ID THREAD_ID [--status STATUS] [--resolved-by-me] [--json]

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
  # credo:disable-for-next-line Credo.Check.Refactor.ABCSize
  def command do
    [
      name: "ado prs",
      doc:
        "Manage Azure DevOps pull requests (PRs). A PR is a request to merge code from one branch (source) into another (target), with required reviewers, policies, and discussion threads.",
      subcommands: [
        list: [
          name: "ado prs list",
          doc:
            "List pull requests in a repository. Output is a table (ID, Title, Status, Source, Target, Creator). Use --status to filter (default: active). Pass --json for raw data.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [
            status: [
              type: :string,
              doc:
                "PR status filter. Valid: active (open, default — includes drafts), completed (merged or closed), abandoned (closed without merging), all (every status). For active PRs only, omit this flag.",
              doc_arg: "STATUS"
            ],
            creator: [
              type: :string,
              doc:
                "Filter by creator's email or display name (substring match, case-insensitive). Use the exact email for a single user.",
              doc_arg: "USER"
            ],
            top: [
              type: :integer,
              doc: "Maximum number of PRs to return. Default 50, max 1000.",
              doc_arg: "N"
            ]
          ],
          execute: &list_prs/1
        ],
        show: [
          name: "ado prs show",
          doc:
            "Show full details of a single pull request: title, description, source/target branches, creator, status, reviewers, labels, policies, merge status, and links. Pass --json for the raw API response (best for scripting).",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Numeric pull request ID"]
          ],
          execute: &show_pr/1
        ],
        create: [
          name: "ado prs create",
          doc:
            "Create a new pull request. The source and target branches must exist; the source must be different from the target. Returns the new PR ID and web URL.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [
            title: [
              type: :string,
              doc:
                "PR title (required). Shown in the PR list and as the merge commit subject (depending on merge strategy). Multi-word values do not need quoting.",
              doc_arg: "TITLE"
            ],
            description: [
              type: :string,
              doc:
                "PR description (markdown supported). Shown in the PR overview. Multi-word values do not need quoting.",
              doc_arg: "DESC"
            ],
            source: [
              type: :string,
              doc:
                "Source branch as a full ref (e.g. 'refs/heads/feature/my-branch'). Use the short name ('my-branch') — 'refs/heads/' is added automatically.",
              doc_arg: "BRANCH"
            ],
            target: [
              type: :string,
              doc:
                "Target branch as a full ref (e.g. 'refs/heads/main') or short name ('main'). Default: the repo's default branch (usually 'main' or 'master').",
              doc_arg: "BRANCH"
            ],
            draft: [
              type: :boolean,
              default: false,
              doc:
                "Create as a draft PR. Drafts are visible in lists but cannot be completed (merged) until you click 'Ready for review' in the UI."
            ]
          ],
          execute: &create_pr/1
        ],
        complete: [
          name: "ado prs complete",
          doc:
            "Complete (merge) a pull request. Fails if any required policies haven't passed (builds, required reviewers, branch policies). The merge is non-atomic: the API may return success but the actual merge can take seconds to minutes.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Numeric PR ID"]
          ],
          options: [
            delete_source: [
              type: :boolean,
              default: false,
              doc:
                "Delete the source branch after the merge succeeds. Useful for keeping the repo clean; if the merge fails, the branch is not deleted."
            ],
            merge_strategy: [
              type: :string,
              doc:
                "Merge strategy. Valid: 'squash' (combine all commits into one on target, default for most repos), 'rebase' (replay commits without merge), 'noFastForward' (preserve all commits with a merge commit). The strategy must be enabled in the repo's branch policies.",
              doc_arg: "STRATEGY"
            ]
          ],
          execute: &complete_pr/1
        ],
        approve: [
          name: "ado prs approve",
          doc:
            "Approve a pull request (records a +10 vote on your behalf). If you're not already a reviewer, the API auto-adds you as one. The approval counts toward branch policies that require N approvals.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Numeric PR ID"]
          ],
          execute: &approve_pr/1
        ],
        vote: [
          name: "ado prs vote",
          doc:
            "Record a vote on a pull request with a specific value. Use `ado prs approve` as a shortcut for +10. To change or remove your vote, simply vote again with the new value.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Numeric PR ID"]
          ],
          options: [
            vote: [
              type: :integer,
              required: true,
              doc:
                "Vote value. Valid: 10 (approve), 5 (approve with suggestions, still allows merge), 0 (reset/withdraw your vote), -5 (wait for author, blocks merge), -10 (reject, blocks merge).",
              doc_arg: "VOTE"
            ]
          ],
          execute: &vote_pr/1
        ],
        abandon: [
          name: "ado prs abandon",
          doc:
            "Abandon a pull request (close without merging). The PR stays in the list with status 'abandoned'; the source branch is preserved. The action is reversible in the web UI but not from the CLI.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Numeric PR ID"]
          ],
          execute: &abandon_pr/1
        ],
        diff: [
          name: "ado prs diff",
          doc:
            "Show the diff for a pull request in one of three modes. " <>
              "Default: table of changed files (path, change type, +/- counts) — fast, no file content fetched. " <>
              "--file PATH: full unified diff for one file (like `git diff <path>`). " <>
              "--unified: single concatenated diff stream for all files (pipe to `less`, `delta`, `code --diff`). " <>
              "--iteration N: inspect an earlier iteration (default: latest = N-1 for a non-draft PR).",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            pr_id: [type: :integer, doc: "Numeric PR ID"]
          ],
          options: [
            file: [
              type: :string,
              doc:
                "Show the full unified diff for a single path (relative to repo root, with or without leading slash). Must match a file in the default view's path column.",
              doc_arg: "PATH"
            ],
            iteration: [
              type: :integer,
              doc:
                "Iteration number to inspect (default: latest). Iteration 1 is the first push, 2 is the first 'push' after a review, etc. Useful for reviewing earlier versions after force-pushes.",
              doc_arg: "N"
            ],
            unified: [
              type: :boolean,
              default: false,
              doc:
                "Output a single concatenated unified diff stream for ALL changed files (like `git diff` on the whole PR). Pipe to a pager or syntax highlighter."
            ],
            json: [
              type: :boolean,
              default: false,
              doc:
                "Output the change list as a JSON envelope. Ignored in --file and --unified modes."
            ]
          ],
          execute: &diff_pr/1
        ],
        comments: [
          name: "ado prs comments",
          doc:
            "Manage pull request review comments. Subcommands: add (create thread or reply), list (view threads), update (edit content or status), delete (remove comment or close thread), resolve (mark thread as fixed). A 'thread' is the top-level comment; a 'comment' is a reply within a thread.",
          subcommands: [
            list: [
              name: "ado prs comments list",
              doc:
                "List review threads on a pull request. Default output is a compact table of thread headers (ID, status, file, line, author). Use --all to expand each thread with full comment content, file paths, and reply markers.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Numeric PR ID"]
              ],
              options: [
                all: [
                  type: :boolean,
                  default: false,
                  doc:
                    "Show full comment content, file paths, and reply markers for each thread (verbose mode). Default shows just thread headers."
                ]
              ],
              execute: &list_comments/1
            ],
            update: [
              name: "ado prs comments update",
              doc:
                "Update a comment or thread. Pass --content to edit a comment's text, " <>
                  "--status to change a thread's resolution state, or both. " <>
                  "--content supports @<file> and - (stdin) for multi-line input.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Numeric PR ID"],
                thread_id: [type: :integer, doc: "Thread ID (from `comments list`)"],
                comment_id: [
                  type: :integer,
                  doc: "Comment ID within the thread (from `comments list --all`)"
                ]
              ],
              options: [
                content: [
                  type: :string,
                  doc:
                    "New comment content. Multi-word values do NOT need quoting " <>
                      "— all subsequent args are joined until the next flag. " <>
                      "Use @<file> to read from a file or `-` to read from stdin. " <>
                      "Omit to update status only.",
                  doc_arg: "TEXT"
                ],
                status: [
                  type: :string,
                  doc:
                    "New thread status. Valid: active (default — open thread), fixed (resolved, hides from active view), wontFix (acknowledged but won't fix), closed (admin-closed), byDesign (working as intended). Omit to update content only.",
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
                      "as JSON, then exit. Makes no network calls. Useful for previewing the patch before applying."
                ],
                json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
              ],
              execute: &update_comment/1
            ],
            add: [
              name: "ado prs comments add",
              doc:
                "Add a review comment to a pull request. " <>
                  "By default creates a new thread; pass --thread-id to reply to an existing one. " <>
                  "For an inline code comment, also pass --file-path and --line.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Numeric PR ID"]
              ],
              options: [
                content: [
                  type: :string,
                  required: true,
                  doc:
                    "Comment text (markdown supported in the web UI). Multi-word values do NOT " <>
                      "need quoting — all subsequent args are joined until the " <>
                      "next flag. Use @<file> to read from a file or `-` to read " <>
                      "from stdin.",
                  doc_arg: "TEXT"
                ],
                file_path: [
                  type: :string,
                  doc:
                    "File path for an inline comment (e.g. 'src/foo.ex'). Omit for a general PR comment (not attached to a file).",
                  doc_arg: "PATH"
                ],
                line: [
                  type: :integer,
                  doc:
                    "Starting line number for an inline comment. Must be a line in the diff (right side for new files, left for deleted). Requires --file-path. Use --end-line to comment on a range of lines.",
                  doc_arg: "N"
                ],
                end_line: [
                  type: :integer,
                  doc:
                    "Ending line number for a multi-line (codeblock) comment. The comment will span from --line to --end-line inclusive. Requires --file-path and --line.",
                  doc_arg: "N"
                ],
                thread_id: [
                  type: :integer,
                  doc:
                    "Reply to an existing thread (the comment is added as a new reply). Without this flag, a NEW thread is created.",
                  doc_arg: "THREAD_ID"
                ],
                comment_id: [
                  type: :integer,
                  doc:
                    "Parent comment to reply to (requires --thread-id). " <>
                      "Use 0 to start a new top-level comment in the thread (default behavior if --thread-id is set but --comment-id is not).",
                  doc_arg: "COMMENT_ID"
                ],
                status: [
                  type: :string,
                  default: "active",
                  doc:
                    "Thread status when creating a new thread. Valid: active (default), fixed, wontFix, closed, byDesign.",
                  doc_arg: "STATUS"
                ],
                json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
              ],
              execute: &add_comment/1
            ],
            delete: [
              name: "ado prs comments delete",
              doc:
                "Delete a review comment or close a thread. " <>
                  "Pass --comment-id to delete a specific comment (HTTP DELETE). " <>
                  "Without --comment-id, the thread is closed (PATCH status=closed). " <>
                  "Use --force to skip the confirmation prompt.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Numeric PR ID"],
                thread_id: [type: :integer, doc: "Thread ID (from `comments list`)"]
              ],
              options: [
                comment_id: [
                  type: :integer,
                  doc:
                    "Comment ID within the thread to delete. Omit to delete the entire thread.",
                  doc_arg: "COMMENT_ID"
                ],
                force: [
                  type: :boolean,
                  default: false,
                  doc: "Skip confirmation prompt."
                ],
                json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
              ],
              execute: &delete_comment/1
            ],
            resolve: [
              name: "ado prs comments resolve",
              doc:
                "Resolve a review thread by setting its status. " <>
                  "This is a convenience wrapper around `comments update --status` " <>
                  "that does not require a comment ID. Default status is 'fixed'. " <>
                  "Use --resolved-by-me to attribute the resolution to yourself.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Numeric PR ID"],
                thread_id: [type: :integer, doc: "Thread ID (from `comments list`)"]
              ],
              options: [
                status: [
                  type: :string,
                  default: "fixed",
                  doc:
                    "Resolution status. Valid: fixed (resolved), wontFix (won't fix), closed (admin-closed), byDesign (working as intended), active (reopen).",
                  doc_arg: "STATUS"
                ],
                resolved_by_me: [
                  type: :boolean,
                  default: false,
                  doc:
                    "Attribute the resolution to the currently-authenticated user. " <>
                      "Makes an extra GET to /_apis/connectionData to look up your GUID."
                ],
                json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
              ],
              execute: &resolve_thread/1
            ]
          ]
        ],
        reviewers: [
          name: "ado prs reviewers",
          doc:
            "Manage pull request reviewers. Reviewers receive notifications, can vote (approve/reject/wait), and count toward branch policies that require N approvals.",
          subcommands: [
            list: [
              name: "ado prs reviewers list",
              doc:
                "List reviewers on a pull request. Output is a table (Display Name, Email, Vote, Status). Vote values: 10 (approved), 5 (approved w/ suggestions), -5 (waiting), -10 (rejected), 0 (no vote, or reset).",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Numeric PR ID"]
              ],
              options: [
                json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
              ],
              execute: &list_reviewers/1
            ],
            add: [
              name: "ado prs reviewers add",
              doc:
                "Add a reviewer to a pull request. The reviewer receives a notification email and shows up in the PR's reviewer list. The --reviewer value can be either a user GUID (most reliable) or an email address (resolved to a GUID by the API).",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Numeric PR ID"]
              ],
              options: [
                reviewer: [
                  type: :string,
                  required: true,
                  doc:
                    "Reviewer identifier. Accepts a user GUID (preferred — e.g. from `ado users show alice@example.com`) or an email address. GUIDs are case-insensitive.",
                  doc_arg: "USER"
                ],
                required: [
                  type: :boolean,
                  default: false,
                  doc:
                    "Mark as a required reviewer (default: optional). The PR cannot be completed until all required reviewers have voted (vote != 0)."
                ],
                json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
              ],
              execute: &add_reviewer/1
            ],
            remove: [
              name: "ado prs reviewers remove",
              doc:
                "Remove a reviewer from a pull request. The user's vote is discarded. Does NOT notify the user (unlike adding).",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                repo_id: [type: :string, doc: "Repository name or ID"],
                pr_id: [type: :integer, doc: "Numeric PR ID"]
              ],
              options: [
                reviewer: [
                  type: :string,
                  required: true,
                  doc:
                    "Reviewer identifier (GUID or email — see `add`). Use a GUID for unambiguous removal.",
                  doc_arg: "USER"
                ],
                json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
              ],
              execute: &remove_reviewer/1
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

            do_complete_patch(path, body, pr_id, parsed)
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

  # Extracted from complete_pr/1 to reduce nesting depth.
  defp do_complete_patch(path, body, pr_id, parsed) do
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
    case Client.get(changes_path(parsed, iteration_id)) do
      {:ok, %{"changeEntries" => changes}} -> {:ok, changes}
      {:ok, %{"value" => changes}} -> {:ok, changes}
      other -> other
    end
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
        do_render_file_diff(parsed, iteration_id, change, file, json?)
    end
  end

  defp do_render_file_diff(parsed, iteration_id, change, file, json?) do
    case fetch_iteration_data(parsed, iteration_id) do
      {:ok, iteration} ->
        case fetch_file_diff(parsed, iteration, file, change) do
          {:ok, content} ->
            emit_diff_or_json(
              json?,
              iteration_id,
              change_path(change),
              change_type(change),
              content
            )

            :ok

          {:error, reason} ->
            bail(reason, parsed)
        end

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  defp emit_diff_or_json(true, iteration_id, path, type, content) do
    IO.puts(
      JSON.encode!(%{
        ok: true,
        iteration: iteration_id,
        path: path,
        change_type: type,
        diff: content
      })
    )
  end

  defp emit_diff_or_json(false, _iteration_id, _path, _type, content) do
    IO.puts(content)
  end

  # --unified: emit the full diff between source and target commits.
  defp render_unified(parsed, iteration_id, changes, json?) do
    case fetch_iteration_data(parsed, iteration_id) do
      {:ok, iteration} ->
        case fetch_full_diff(parsed, iteration) do
          {:ok, content} ->
            if json? do
              IO.puts(
                JSON.encode!(%{
                  ok: true,
                  iteration: iteration_id,
                  mode: "unified",
                  file_count: length(changes),
                  diff: content
                })
              )
            else
              IO.puts(content)
              IO.puts("")
            end

            :ok

          {:error, reason} ->
            bail(reason, parsed)
        end

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  # ── diff helpers ────────────────────────────────────────────────
  defp fetch_iteration_data(parsed, iteration_id) do
    case Client.get(iterations_path(parsed)) do
      {:ok, %{"value" => iterations}} when is_list(iterations) ->
        case Enum.find(iterations, &(&1["id"] == iteration_id)) do
          nil -> {:error, "Iteration #{iteration_id} not found"}
          iteration -> {:ok, iteration}
        end

      {:error, _} = err ->
        err
    end
  end

  defp fetch_file_diff(parsed, iteration, _file, change) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    base = get_in(iteration, ["targetRefCommit", "commitId"])
    target = get_in(iteration, ["sourceRefCommit", "commitId"])
    ctype = change_type(change)
    # Use the API's canonical path (always has leading /) for item fetches.
    api_path = change_path(change)

    if !base || !target do
      {:error, "Iteration is missing sourceRefCommit or targetRefCommit"}
    else
      with {:ok, old_content} <- fetch_or_empty(project, repo_id, api_path, base, ctype, :del),
           {:ok, new_content} <- fetch_or_empty(project, repo_id, api_path, target, ctype, :ins) do
        {:ok, format_unified_diff(api_path, old_content, new_content, base, target)}
      end
    end
  end

  defp fetch_full_diff(parsed, iteration) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    base = get_in(iteration, ["targetRefCommit", "commitId"])
    target = get_in(iteration, ["sourceRefCommit", "commitId"])

    if !base || !target do
      {:error, "Iteration is missing sourceRefCommit or targetRefCommit"}
    else
      changes_path = "/#{project}/_apis/git/repositories/#{repo_id}/diffs/commits"

      params = %{
        "baseVersionType" => "commit",
        "baseVersion" => base,
        "targetVersionType" => "commit",
        "targetVersion" => target
      }

      case Client.get(changes_path, params) do
        {:ok, %{"changes" => changes}} when is_list(changes) ->
          diffs = collect_diffs(changes, project, repo_id, base, target)
          {:ok, diffs}

        _ ->
          {:error, "No changes found"}
      end
    end
  end

  defp collect_diffs(changes, project, repo_id, base, target) do
    changes
    |> Enum.map(fn ch ->
      path = get_in(ch, ["item", "path"])
      # changeType: 1=add, 2=edit, 4=delete
      ctype = ch["changeType"]

      with {:ok, old_content} <- fetch_or_empty_raw(project, repo_id, path, base, ctype, :del),
           {:ok, new_content} <- fetch_or_empty_raw(project, repo_id, path, target, ctype, :ins) do
        format_unified_diff(path, old_content, new_content, base, target)
      else
        _ -> nil
      end
    end)
    |> Enum.reject(&is_nil/1)
    |> Enum.join("\n")
  end

  defp fetch_file_content(project, repo_id, path, commit_id) do
    content_path = "/#{project}/_apis/git/repositories/#{repo_id}/items"

    params = %{
      "path" => path,
      "versionType" => "commit",
      "version" => commit_id
    }

    Client.get_raw(content_path, params)
  end

  # Fetches file content, falling back to empty string when the file
  # does not exist in that commit (404). The side parameter (:del for
  # base/old, :ins for target/new) controls which 404s are allowed:
  #   * "add" files — the file is new; base fetch returns 404 → use ""
  #   * "delete" files — the file was removed; target fetch returns 404 → use ""
  #   * "edit" files — the file must exist in both commits; 404 is an error
  defp fetch_or_empty(project, repo_id, path, commit_id, ctype, side) do
    case fetch_file_content(project, repo_id, path, commit_id) do
      {:ok, content} ->
        {:ok, content}

      {:error, %{status: 404}} ->
        if (ctype == "add" and side == :del) or (ctype == "delete" and side == :ins) do
          {:ok, ""}
        else
          {:error, "File not found in commit #{commit_id}"}
        end

      {:error, _} = err ->
        err
    end
  end

  # Same as fetch_or_empty/6 but uses raw changeType from the
  # /diffs/commits response (1=add, 4=delete, or string "add"/"delete").
  defp fetch_or_empty_raw(_project, _repo_id, _path, _commit_id, ctype, :del)
       when ctype in [1, "add"],
       do: {:ok, ""}

  defp fetch_or_empty_raw(_project, _repo_id, _path, _commit_id, ctype, :ins)
       when ctype in [4, "delete"],
       do: {:ok, ""}

  defp fetch_or_empty_raw(project, repo_id, path, commit_id, _ctype, _side) do
    case fetch_file_content(project, repo_id, path, commit_id) do
      {:ok, content} -> {:ok, content}
      {:error, _} = err -> err
    end
  end

  defp format_unified_diff(path, old_content, new_content, base_sha, target_sha) do
    # Ensure path has a leading "/" for consistency
    path = if String.starts_with?(path, "/"), do: path, else: "/#{path}"
    _short_base = String.slice(base_sha, 0, 7)
    _short_target = String.slice(target_sha, 0, 7)

    header = """
    diff --git a#{path} b#{path}
    --- a#{path}
    +++ b#{path}
    """

    # Split content into lines. Empty content (new file / deleted file)
    # is treated as an empty list, not [""], to produce correct hunk
    # headers (@@ -0,0 for new files, @@ -N,0 +0,0 for deleted files).
    old_lines =
      if old_content == "", do: [], else: String.split(old_content, "\n")

    new_lines =
      if new_content == "", do: [], else: String.split(new_content, "\n")

    changes = compute_hunk(old_lines, new_lines)
    "#{header}#{changes}"
  end

  # Simple line-by-line diff. Produces unified hunk output.
  # Handles empty lists for new files (old_lines=[]) and deleted
  # files (new_lines=[]) with correct hunk headers.
  defp compute_hunk([], []), do: ""

  defp compute_hunk([], new_lines) do
    n = length(new_lines)
    header = "@@ -0,0 +1,#{n} @@\n"
    body = format_diff_lines([{:ins, new_lines}])
    "#{header}#{body}"
  end

  defp compute_hunk(old_lines, []) do
    n = length(old_lines)
    header = "@@ -1,#{n} +0,0 @@\n"
    body = format_diff_lines([{:del, old_lines}])
    "#{header}#{body}"
  end

  defp compute_hunk(old_lines, new_lines) do
    diff = List.myers_difference(old_lines, new_lines)

    additions = count(diff, :ins)
    deletions = count(diff, :del)

    if additions == 0 and deletions == 0 do
      ""
    else
      header = "@@ -1,#{length(old_lines)} +1,#{length(new_lines)} @@\n"
      body = format_diff_lines(diff)
      "#{header}#{body}"
    end
  end

  defp count(list, kind), do: Enum.count(list, &match?({^kind, _}, &1))

  defp format_diff_lines(diff) do
    Enum.map_join(diff, fn
      {:eq, lines} ->
        Enum.map_join(lines, fn l -> " #{l}\n" end)

      {:ins, lines} ->
        Enum.map_join(lines, fn l -> "+#{l}\n" end)

      {:del, lines} ->
        Enum.map_join(lines, fn l -> "-#{l}\n" end)
    end)
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

  defp ensure_leading_slash("/" <> _ = path), do: path
  defp ensure_leading_slash(path), do: "/#{path}"

  defp change_type(change) do
    # Azure DevOps can return changeType as string ("add") or integer (1).
    # Normalize to integer first, then map to display string.
    case change["changeType"] do
      "add" -> "add"
      "edit" -> "edit"
      "delete" -> "delete"
      "rename" -> "rename"
      "directory" -> "directory"
      n when is_integer(n) -> int_change_type(n)
      _ -> "change"
    end
  end

  defp int_change_type(1), do: "add"
  defp int_change_type(2), do: "edit"
  defp int_change_type(4), do: "delete"
  defp int_change_type(8), do: "rename"
  defp int_change_type(16), do: "directory"
  defp int_change_type(_), do: "change"

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

  defp resolve_reviewer_id(_project, _repo_id, pr_id) do
    # Use the authenticated user's identity GUID as the reviewer ID.
    # The Azure DevOps PUT /reviewers/{id} endpoint auto-adds the user
    # as a reviewer with the given vote if they aren't one already.
    # Using createdBy.id as a fallback would cause
    # "You cannot record a vote for someone else" errors.
    case AdoCli.Auth.current_user_id() do
      {:ok, user_id} ->
        user_id

      {:error, reason} ->
        halt_error("Cannot determine authenticated user identity for PR ##{pr_id}: #{reason}")
    end
  end

  defp vote_label(10), do: "+10 (approved)"
  defp vote_label(5), do: "+5 (approved with suggestions)"
  defp vote_label(0), do: "0 (reset)"
  defp vote_label(-5), do: "-5 (waiting for author)"
  defp vote_label(-10), do: "-10 (rejected)"
  defp vote_label(n), do: "#{n}"

  # ── Reviewers ────────────────────────────────────────────────────────

  def list_reviewers(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id
    pr_id = parsed.arguments.pr_id

    path =
      "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}/reviewers"

    case Client.get(path) do
      {:ok, %{"value" => reviewers}} ->
        Helpers.json_or_format(reviewers, parsed, fn reviewers ->
          writeln("")

          if reviewers == [] do
            writeln("No reviewers.")
          else
            writeln(
              String.pad_trailing("Display Name", 30) <>
                " " <>
                String.pad_trailing("Email", 35) <>
                " " <>
                String.pad_trailing("Vote", 6) <> "Required"
            )

            writeln(String.duplicate("─", 85))

            Enum.each(reviewers, &print_reviewer_row/1)
          end

          writeln("")
        end)

        halt_success("Done.")

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  def add_reviewer(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id
    pr_id = parsed.arguments.pr_id
    reviewer = Map.fetch!(parsed.options, :reviewer)
    required? = Map.get(parsed.options, :required, false)

    reviewer_path =
      "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}/reviewers/#{URI.encode(reviewer)}"

    body = %{"id" => reviewer}
    body = if required?, do: Map.put(body, "isRequired", true), else: body

    case Client.put(reviewer_path, body) do
      {:ok, _} ->
        label = if required?, do: "(required)", else: "(optional)"
        success("Reviewer #{reviewer} added #{label}.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Reviewer not found: #{reviewer}. Use the user's GUID from Azure DevOps.")

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  def remove_reviewer(parsed) do
    project = parsed.arguments.project
    repo_id = parsed.arguments.repo_id
    pr_id = parsed.arguments.pr_id
    reviewer = Map.fetch!(parsed.options, :reviewer)

    reviewer_path =
      "/#{URI.encode(project)}/_apis/git/repositories/#{URI.encode(repo_id)}/pullrequests/#{pr_id}/reviewers/#{URI.encode(reviewer)}"

    case Client.delete(reviewer_path) do
      :ok ->
        success("Reviewer #{reviewer} removed.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Reviewer not found: #{reviewer}")

      {:error, reason} ->
        writeln("")
        writeln("xx  Remove failed: #{inspect(reason)}")
        halt_error("")
    end
  end

  defp print_reviewer_row(r) do
    name = String.pad_trailing(r["displayName"] || "?", 30)
    email = String.pad_trailing(r["uniqueName"] || "?", 35)
    vote = String.pad_trailing(to_string(r["vote"] || 0), 6)
    required = if r["isRequired"], do: "yes", else: "no"
    writeln("#{name} #{email} #{vote} #{required}")
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
        end_line = Map.get(parsed.options, :end_line)
        thread_id = Map.get(parsed.options, :thread_id)
        parent_comment_id = Map.get(parsed.options, :comment_id, 0) || 0

        cond do
          thread_id ->
            # Reply mode: POST a new comment to an existing thread
            do_reply_to_thread(parsed, thread_id, content, parent_comment_id, json?)

          file_path && line ->
            # New inline thread: POST a new thread with file/line context
            do_new_inline_thread(parsed, content, file_path, line, end_line, status, json?)

          true ->
            # New general thread: POST a new thread with no file context
            do_new_general_thread(parsed, content, status, json?)
        end

      {:error, message} ->
        halt_error(message)
    end
  end

  @doc """
  Delete a review comment or close an entire thread.

  With --comment-id: deletes a specific comment within a thread (HTTP DELETE).
  Without --comment-id: closes the entire thread (PATCH status=closed) since
  Azure DevOps does not support DELETE on threads.
  """
  def delete_comment(parsed) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id
    thread_id = parsed.arguments.thread_id
    comment_id = Map.get(parsed.options, :comment_id)
    force? = Map.get(parsed.options, :force, false)
    json? = Map.get(parsed.options, :json, false)

    base =
      "/#{project}/_apis/git/repositories/#{repo_id}/pullrequests/#{pr_id}/threads/#{thread_id}"

    {method, path, target_label} =
      if comment_id do
        {:delete, "#{base}/comments/#{comment_id}",
         "comment #{comment_id} in thread #{thread_id}"}
      else
        {:patch, base, "thread #{thread_id}"}
      end

    confirm_delete!(target_label, force?)

    result = do_delete_request(method, path)
    render_delete_result(result, target_label, json?, parsed)
  end

  defp confirm_delete!(_label, true), do: :ok

  defp confirm_delete!(label, false) do
    CliMate.CLI.write("Close #{label}? [y/N] ")

    case String.trim(IO.gets("")) do
      "y" -> :ok
      "Y" -> :ok
      _ -> halt_success("Cancelled.")
    end
  end

  defp do_delete_request(:delete, path), do: Client.delete(path)
  defp do_delete_request(:patch, path), do: Client.patch(path, %{"status" => "closed"})

  defp render_delete_result(result, target_label, json?, parsed) do
    case result do
      r when r == :ok or (is_tuple(r) and elem(r, 0) == :ok) ->
        if json? do
          IO.puts(JSON.encode!(%{ok: true, closed: target_label}))
        else
          success("Closed #{target_label}.")
        end

        halt_success("")

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  @doc """
  Resolve a review thread by setting its status.

  Convenience wrapper around the thread PATCH endpoint. Default status
  is "fixed". Use --resolved-by-me to attribute to the current user.
  """
  def resolve_thread(parsed) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id
    thread_id = parsed.arguments.thread_id
    status = Map.get(parsed.options, :status, "fixed")
    resolved_by_me = Map.get(parsed.options, :resolved_by_me, false)
    json? = Map.get(parsed.options, :json, false)

    path =
      "/#{project}/_apis/git/repositories/#{repo_id}/pullrequests/#{pr_id}/threads/#{thread_id}"

    body = build_resolve_body(status, resolved_by_me, pr_id)

    case Client.patch(path, body) do
      {:ok, _result} ->
        label = if resolved_by_me, do: " (by you)", else: ""

        if json? do
          IO.puts(JSON.encode!(%{ok: true, thread: thread_id, status: status}))
        else
          success("Thread #{thread_id} resolved as '#{status}'#{label}.")
        end

        halt_success("")

      {:error, reason} ->
        bail(reason, parsed)
    end
  end

  defp build_resolve_body(status, true, _pr_id) do
    case AdoCli.Auth.current_user_id() do
      {:ok, user_id} ->
        %{"status" => status, "resolvedBy" => %{"id" => user_id}}

      {:error, reason} ->
        halt_error("Cannot determine authenticated user identity: #{reason}")
    end
  end

  defp build_resolve_body(status, _resolved_by_me, _pr_id), do: %{"status" => status}

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

  defp do_new_inline_thread(parsed, content, file_path, line, end_line, status, json?) do
    project = URI.encode(parsed.arguments.project)
    repo_id = URI.encode(parsed.arguments.repo_id)
    pr_id = parsed.arguments.pr_id

    path = "/#{project}/_apis/git/repositories/#{repo_id}/pullrequests/#{pr_id}/threads"

    # Azure DevOps requires filePath with a leading /. Without it,
    # changeTrackingId is missing and the web UI shows "file no longer exists".
    canonical_path = ensure_leading_slash(file_path)

    # For a single-line comment: start and end are on the same line
    # (offset 1 to 2 covers the whole line). For a multi-line codeblock,
    # start is at line:offset 1 and end is at end_line:offset 1.
    end_line = end_line || line

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
        "filePath" => canonical_path,
        "rightFileStart" => %{"line" => line, "offset" => 1},
        "rightFileEnd" => %{"line" => end_line, "offset" => 1}
      }
    }

    case Client.post(path, body) do
      {:ok, result} ->
        range_label = if end_line && end_line != line, do: "#{line}-#{end_line}", else: "#{line}"
        render_add_result(result, "Comment added to #{canonical_path}:#{range_label}.", json?)

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
