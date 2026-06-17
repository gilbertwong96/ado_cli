defmodule AdoCli.CLI.TestCoverage do
  @moduledoc """
  Fetch code coverage data from Azure DevOps test runs.

    ado test-coverage show PROJECT BUILD_ID

  REST API reference:
    https://learn.microsoft.com/en-us/rest/azure/devops/test/code-coverage
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado test-coverage",
      doc:
        "Fetch code coverage data from Azure DevOps. Coverage is reported per build; show it by build ID. Output is a bar chart (module name + visual bar + percentage). Pass --json for raw data.",
      subcommands: [
        show: [
          name: "ado test-coverage show",
          doc:
            "Show code coverage summary for a build: per-module coverage bars, total lines/covered lines, and overall percentage. Requires a build ID (the YAML pipeline or classic build that produced the coverage data).",
          arguments: [
            project: [type: :string, doc: "Project name or ID where the build ran"],
            build_id: [type: :integer, doc: "Numeric build ID that has coverage data attached"]
          ],
          options: [
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &show_coverage/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  @doc """
  Fetches code coverage data for a specific build.

  Returns a summary with coverage percentages per configuration
  (e.g. lines, branches) and a visual bar chart.
  """
  def show_coverage(parsed) do
    project = parsed.arguments.project
    build_id = parsed.arguments.build_id
    path = "/#{project}/_apis/test/codecoverage"
    params = %{"buildId" => build_id}

    case Client.get(path, params) do
      {:ok, %{"coverageData" => data}} ->
        Helpers.json_or_format(data, parsed, fn covers ->
          writeln("")
          writeln("Code Coverage for Build ##{build_id}")
          writeln(String.duplicate("─", 70))
          Enum.each(covers, &print_coverage_stats/1)
          writeln("")
        end)

        halt_success("Done.")

      {:ok, _empty_or_no_data} ->
        show_no_coverage(build_id)

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp show_no_coverage(build_id) do
    writeln("")
    writeln("No coverage data for build ##{build_id}.")
    writeln("")
    halt_success("Done.")
  end

  defp print_coverage_stats(cov) do
    config = cov["coverageStats"] || []

    Enum.each(config, fn s ->
      label = String.pad_trailing(s["label"] || "?", 20)
      total = s["total"] || 0
      covered = s["covered"] || 0
      pct = if total > 0, do: Float.round(covered / total * 100, 1), else: 0.0
      bar = coverage_bar(pct)
      writeln("  #{label} #{String.pad_leading("#{pct}%", 8)} #{bar}")
    end)
  end

  defp coverage_bar(pct) do
    filled = round(pct / 5)
    empty = 20 - filled

    color =
      cond do
        pct >= 80 -> :green
        pct >= 50 -> :yellow
        true -> :red
      end

    bar = String.duplicate("█", filled) <> String.duplicate("░", empty)
    color_text(bar, color)
  end

  defp color_text(text, :green), do: "\e[32m#{text}\e[0m"
  defp color_text(text, :yellow), do: "\e[33m#{text}\e[0m"
  defp color_text(text, :red), do: "\e[31m#{text}\e[0m"
end
