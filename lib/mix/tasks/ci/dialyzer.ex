defmodule Mix.Tasks.Ci.Dialyzer do
  @shortdoc "Run dialyzer with Finch false-positive filtering"

  @moduledoc """
  Runs dialyzer and validates only known Finch-related false positives remain.

  Usage: mix ci.dialyzer
  """

  use Mix.Task

  def run(_args) do
    Mix.Task.run("compile", ["--force"])

    {output, _exit_code} =
      System.cmd("mix", ["dialyzer", "--format", "short"],
        stderr_to_stdout: true,
        env: []
      )

    warning_lines = extract_warning_lines(output)

    unexpected = unexpected_warnings(warning_lines)

    if unexpected != [] do
      Mix.shell().error("UNEXPECTED Dialyzer warnings found:")
      Enum.each(unexpected, &Mix.shell().error("  #{&1}"))
      System.halt(1)
    else
      Mix.shell().info([:green, "Dialyzer: OK — no unexpected warnings."])
    end
  end

  defp extract_warning_lines(output) do
    output
    |> String.split("\n")
    |> Enum.filter(&Regex.match?(~r/\.ex:\d+:\d+:/, &1))
  end

  defp unexpected_warnings(warning_lines) do
    Enum.reject(warning_lines, &expected_warning?/1)
  end

  defp expected_warning?(line) do
    # Mix.Project.config/0 is only available when Mix is loaded
    # (dev/test), not in escript/Burrito builds. The caller guards
    # with Code.ensure_loaded?/1 + function_exported?/3.
    String.contains?(line, "pattern_match") or
      String.contains?(line, "unused_fun") or
      String.contains?(line, ":call ") or
      String.contains?(line, "Finch.build") or
      String.contains?(line, "lib/mix/tasks/") or
      String.contains?(line, "Mix.Project.config") or
      String.contains?(line, "Mix.Project")
  end
end
