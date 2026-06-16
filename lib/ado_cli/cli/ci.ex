defmodule AdoCli.CLI.CI do
  @moduledoc """
  Commands for watching Azure DevOps pipelines in real-time.

    ado ci watch PROJECT BUILD_ID       [--org ORG] [--poll-interval MS]
    ado ci watch PROJECT --latest      [--org ORG] [--definition ID] [--branch REF]

  Streams live build status and per-line log output from the
  Azure DevOps Build API. Useful for monitoring a pipeline
  from the terminal without opening a browser.

  Implementation notes:

    * Build status is polled via `GET .../build/builds/{id}`
    * The build timeline (jobs/steps) is fetched via
      `GET .../build/builds/{id}/timeline`
    * Live log content is fetched incrementally via
      `GET .../build/builds/{id}/logs/{logId}?id={N}`
      where `N` is the last line we have already seen
    * The watch loop exits when the build reaches a terminal
      state (completed, failed, canceled) or when the user
      sends Ctrl+C
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CI.Watcher
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado ci",
      doc: "Watch Azure DevOps pipelines in real-time.",
      subcommands: [
        watch: [
          name: "ado ci watch",
          doc: "Stream live status and log output for an Azure DevOps build.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            build_id: [
              type: :integer,
              required: false,
              doc: "Build ID to watch (use --latest to pick the latest run)"
            ]
          ],
          options: [
            org: [
              type: :string,
              doc: "Organization (defaults to $ADO_ORG or config)",
              doc_arg: "ORG"
            ],
            latest: [
              type: :boolean,
              default: false,
              doc: "Watch the latest build for the project (or for --definition)"
            ],
            definition: [
              type: :integer,
              doc: "When using --latest, pick the latest build of this pipeline",
              doc_arg: "ID"
            ],
            branch: [
              type: :string,
              doc: "When using --latest, restrict to this branch (e.g. refs/heads/main)",
              doc_arg: "REF"
            ],
            "poll-interval": [
              type: :integer,
              default: 2000,
              doc: "How often to poll (milliseconds)",
              doc_arg: "MS"
            ]
          ],
          execute: &watch/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  @doc """
  Watches a build. Resolves the build ID (either from arguments
  or from --latest), then runs the streaming watch loop.
  """
  def watch(parsed) do
    project = parsed.arguments.project
    org = org_from(parsed)
    build_id = resolve_build_id(parsed, project, org)

    case build_id do
      {:ok, id} ->
        case Watcher.watch(id, project, org, poll_ms: poll_ms(parsed)) do
          :ok ->
            writeln("")
            writeln("✓ Build #{id} completed.")
            halt_success("")

          {:error, reason} ->
            writeln("")
            writeln("xx  Watch failed: #{inspect(reason)}")
            halt_error("")
        end

      {:error, reason} ->
        writeln("")
        writeln("xx  Could not resolve build ID: #{reason}")
        halt_error("")
    end
  end

  # ── helpers ──────────────────────────────────────────────────────────

  defp org_from(parsed) do
    case Map.get(parsed.options, :org) do
      nil -> System.get_env("ADO_ORG")
      org -> org
    end
  end

  defp poll_ms(parsed) do
    case Map.get(parsed.options, :"poll-interval") do
      nil -> 2000
      n when is_integer(n) and n >= 250 -> n
      _ -> 2000
    end
  end

  # If a build_id was passed, use it. Otherwise, with --latest, fetch
  # the most recent build (optionally filtered by definition/branch).
  #
  # Note: we use Map.get/2 instead of dot-access (`parsed.arguments.build_id`)
  # because CliMate omits absent optional arguments from the map entirely
  # rather than inserting them as `nil`. Dot access on a missing key
  # raises KeyError and crashes the watcher.
  defp resolve_build_id(parsed, project, org) do
    case Map.get(parsed.arguments, :build_id) do
      id when is_integer(id) and id > 0 ->
        {:ok, id}

      _ ->
        use_latest? = Map.get(parsed.options, :latest, false)

        if use_latest? do
          fetch_latest_build(project, org, parsed)
        else
          {:error,
           "no build ID given; pass BUILD_ID as the second positional argument or use --latest"}
        end
    end
  end

  defp fetch_latest_build(project, org, parsed) do
    params = %{"$top" => 1}

    params =
      if def_id = Map.get(parsed.options, :definition),
        do: Map.put(params, "definitions", def_id),
        else: params

    params =
      if branch = Map.get(parsed.options, :branch),
        do: Map.put(params, "branchName", branch),
        else: params

    case Client.get(build_path(project, "/_apis/build/builds", org), params) do
      {:ok, %{"value" => [%{"id" => id} | _]}} -> {:ok, id}
      {:ok, %{"value" => []}} -> {:error, "no builds found for this project/definition/branch"}
      {:error, %{status: status}} -> {:error, "API returned HTTP #{status}"}
      other -> {:error, "unexpected response: #{inspect(other)}"}
    end
  end

  # The Client module already injects the org, so we only need to pass
  # the project-scoped path. This helper exists to make the call
  # site readable.
  #
  # Azure DevOps build endpoints are scoped to a project:
  #   /{project}/_apis/build/builds
  #   /{project}/_apis/build/builds/{id}
  #   /{project}/_apis/build/builds/{id}/logs
  # The org gets injected by the Client module. The previous version
  # of this helper dropped both project and org, producing URLs like
  # /_apis/build/builds/{id} which the API rejected with
  # "VS800075: The project with id 'No project was specified.'".
  defp build_path(project, path, _org), do: "/#{project}#{path}"
end
