defmodule AdoCli.CLI.TestResults do
  @moduledoc """
  Commands for managing Azure DevOps Test Results and Code Coverage.

    ado test-results list PROJECT              [--build-id ID] [--top N] [--min-last-updated DATE]
    ado test-results show PROJECT RUN_ID
    ado test-results publish PROJECT --name NAME --file PATH [--build-id ID]
    ado test-coverage show PROJECT BUILD_ID

  REST API reference:
    https://learn.microsoft.com/en-us/rest/azure/devops/test
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado test-results",
      doc: "Manage Azure DevOps test results and code coverage.",
      subcommands: [
        list: [
          name: "ado test-results list",
          doc: "List recent test runs in a project.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"]
          ],
          options: [
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"],
            top: [type: :integer, doc: "Max runs to return (default: 50)", doc_arg: "N"],
            "build-id": [type: :integer, doc: "Filter by build ID", doc_arg: "ID"],
            "min-last-updated": [
              type: :string,
              doc: "ISO date filter for last updated",
              doc_arg: "DATE"
            ]
          ],
          execute: &list_runs/1
        ],
        show: [
          name: "ado test-results show",
          doc: "Show a specific test run by ID.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            run_id: [type: :integer, doc: "Test run ID"]
          ],
          options: [
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &show_run/1
        ],
        publish: [
          name: "ado test-results publish",
          doc: "Publish test results from a file (Cobertura XML, JUnit, etc.) to Azure DevOps.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"]
          ],
          options: [
            name: [type: :string, required: true, doc: "Test run name", doc_arg: "NAME"],
            file: [
              type: :string,
              required: true,
              doc: "Path to test results file",
              doc_arg: "PATH"
            ],
            "build-id": [type: :integer, doc: "Associate with a build", doc_arg: "ID"],
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &publish_results/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  # ── list ────────────────────────────────────────────────────────────

  def list_runs(parsed) do
    project = parsed.arguments.project
    params = build_list_params(parsed)
    path = "/#{project}/_apis/test/runs"

    case Client.get(path, params) do
      {:ok, %{"value" => runs}} ->
        Helpers.json_or_format(runs, parsed, fn _ ->
          writeln("")

          writeln(
            String.pad_trailing("ID", 8) <>
              " " <>
              String.pad_trailing("Name", 40) <>
              " " <>
              String.pad_trailing("State", 12) <> " " <> "Total / Passed / Failed"
          )

          writeln(String.duplicate("─", 90))

          Enum.each(runs, &print_run_row/1)

          writeln("")
        end)

        halt_success("Done.")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp build_list_params(parsed) do
    params = %{}

    params =
      if top = Map.get(parsed.options, :top, nil) do
        Map.put(params, "$top", top)
      else
        params
      end

    params =
      if build_id = Map.get(parsed.options, :"build-id", nil) do
        Map.put(params, "buildIds", build_id)
      else
        params
      end

    params =
      if min_date = Map.get(parsed.options, :"min-last-updated", nil) do
        Map.put(params, "minLastUpdatedDate", min_date)
      else
        params
      end

    params
  end

  defp find_stat(stats, label) do
    stat = Enum.find(stats, &(&1["outcome"] == label || &1["state"] == label))
    stat && stat["count"]
  end

  defp print_run_row(run) do
    id = to_string(run["id"] || "")
    name = String.slice(run["name"] || "", 0, 39)
    state = run["state"] || "?"

    stats = run["runStatistics"] || []
    total = find_stat(stats, "TotalTests") || "?"
    passed = find_stat(stats, "Passed") || "0"
    failed = find_stat(stats, "Failed") || "0"

    writeln(
      "#{String.pad_trailing(id, 8)} #{String.pad_trailing(name, 40)} " <>
        "#{String.pad_trailing(state, 12)} #{total} / #{passed} / #{failed}"
    )
  end

  # ── show ────────────────────────────────────────────────────────────

  def show_run(parsed) do
    project = parsed.arguments.project
    run_id = parsed.arguments.run_id
    path = "/#{project}/_apis/test/runs/#{run_id}"

    case Client.get(path) do
      {:ok, run} ->
        Helpers.json_or_format(run, parsed, fn _ ->
          writeln("")
          writeln("Test Run ##{run["id"]}")
          writeln("  Name:        #{run["name"]}")
          writeln("  State:       #{run["state"]}")
          writeln("  Started:     #{run["startedDate"]}")
          writeln("  Completed:   #{run["completedDate"]}")

          stats = run["runStatistics"] || []
          writeln("  Results:")

          Enum.each(stats, fn s ->
            label = String.pad_trailing(s["outcome"] || s["state"] || "?", 20)
            writeln("    #{label} #{s["count"]}")
          end)

          if build = run["build"] do
            writeln("  Build:       #{build["id"]}")
          end

          writeln("")
        end)

        halt_success("Done.")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── publish ─────────────────────────────────────────────────────────

  def publish_results(parsed) do
    project = parsed.arguments.project
    name = Map.fetch!(parsed.options, :name)
    file_path = Map.fetch!(parsed.options, :file)

    with {:ok, content} <- read_file(file_path),
         {:ok, run} <- create_test_run(project, name, parsed),
         :ok <- attach_file(project, run["id"], file_path, content) do
      json? = Map.get(parsed.options, :json, false)

      if json? do
        writeln(JSON.encode!(%{ok: true, run: %{id: run["id"], name: name}}))
      else
        writeln("")
        writeln("✓ Test run ##{run["id"]} created: #{name}")
        writeln("  File #{file_path} attached.")
        writeln("")
        writeln("  View run: https://dev.azure.com/_test/runs?runId=#{run["id"]}")
        writeln("")
      end

      halt_success("Done.")
    else
      {:error, :enoent} ->
        halt_error("File not found: #{file_path}")

      {:error, reason} when is_binary(reason) ->
        halt_error(reason)

      {:error, reason} ->
        writeln("")
        writeln("xx  Publish failed: #{inspect(reason)}")
        halt_error("")
    end
  end

  defp read_file(path) do
    case File.read(path) do
      {:ok, content} -> {:ok, content}
      {:error, :enoent} -> {:error, :enoent}
      {:error, reason} -> {:error, "#{inspect(reason)}"}
    end
  end

  defp create_test_run(project, name, parsed) do
    body = %{
      "name" => name,
      "isAutomated" => true,
      "state" => "InProgress"
    }

    body =
      if build_id = Map.get(parsed.options, :"build-id", nil) do
        Map.put(body, "build", %{"id" => build_id})
      else
        body
      end

    path = "/#{project}/_apis/test/runs"
    Client.post(path, body)
  end

  defp attach_file(project, run_id, file_path, _content) do
    # Determine attachment type from file extension
    ext =
      file_path
      |> String.downcase()
      |> Path.extname()

    _attachment_type =
      case ext do
        ".xml" -> "CodeCoverage"
        ".cobertura" -> "CodeCoverage"
        ".trx" -> "TmiTestRunSummaryResult"
        _ -> "GeneralAttachment"
      end

    # Mark the run as completed
    _ = Client.patch("/#{project}/_apis/test/runs/#{run_id}", %{"state" => "Completed"})

    # Upload the file. Azure DevOps Test attachments API uses a
    # multipart form POST to /_apis/test/runs/{id}/attachments.
    # We use a simple binary POST with the file content as the
    # raw body and a query parameter for the filename.
    query =
      URI.encode_query(%{
        "api-version" => "7.1-preview.1",
        "fileName" => Path.basename(file_path)
      })

    path = "/#{project}/_apis/test/runs/#{run_id}/attachments?#{query}"

    case Client.post(path, File.read!(file_path), %{"Content-Type" => "application/octet-stream"}) do
      {:ok, _} -> :ok
      {:error, reason} -> {:error, reason}
    end
  end
end
