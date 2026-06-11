defmodule AdoCli.CLI.Pipelines do
  @moduledoc """
  Commands for managing Azure DevOps Pipelines.

    ado_cli pipelines list PROJECT            [--top N] [--folder PATH]
    ado_cli pipelines show PROJECT ID
    ado_cli pipelines run PROJECT ID           [--branch BRANCH] [--variables KEY=VALUE,...]
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado pipelines",
      doc: "Manage Azure DevOps pipelines.",
      subcommands: [
        list: [
          name: "ado pipelines list",
          doc: "List pipelines in a project.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            top: [type: :integer, doc: "Maximum number to return", doc_arg: "N"],
            folder: [type: :string, doc: "Filter by folder path", doc_arg: "PATH"]
          ],
          execute: &list_pipelines/1
        ],
        show: [
          name: "ado pipelines show",
          doc: "Show details of a specific pipeline.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            pipeline_id: [type: :integer, doc: "Pipeline definition ID"]
          ],
          execute: &show_pipeline/1
        ],
        run: [
          name: "ado pipelines run",
          doc: "Trigger a pipeline run.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            pipeline_id: [type: :integer, doc: "Pipeline definition ID"]
          ],
          options: [
            branch: [
              type: :string,
              doc: "Branch to run on (default: default branch)",
              doc_arg: "BRANCH"
            ],
            variables: [type: :string, doc: "Pipeline variables (KEY=VALUE,...)", doc_arg: "VARS"]
          ],
          execute: &run_pipeline/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  # ── Read ──────────────────────────────────────────────────────────────

  @doc """
  Lists pipelines in a project.

  Supports `--top` for pagination and `--folder` for path filtering.
  """
  def list_pipelines(parsed) do
    project = parsed.arguments.project

    params =
      %{}
      |> put_if(Map.get(parsed.options, :top), "$top")
      |> put_if(Map.get(parsed.options, :folder), "folder")

    result = Client.list("/#{URI.encode(project)}/_apis/pipelines", params)

    Helpers.handle_api_result(result, parsed, fn pipelines ->
      Helpers.json_or_format(pipelines, parsed, &print_pipelines_table/1)
    end)
  end

  @doc """
  Shows details of a specific pipeline definition.
  """
  def show_pipeline(parsed) do
    project = parsed.arguments.project
    pipeline_id = parsed.arguments.pipeline_id

    case Client.get("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}") do
      {:ok, pipeline} ->
        Helpers.json_or_format(pipeline, parsed, &print_pipeline_detail/1)

      {:error, %{status: 404}} ->
        halt_error("Pipeline ##{pipeline_id} not found in project '#{project}'")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Write ─────────────────────────────────────────────────────────────

  @doc """
  Triggers a pipeline run.

  Supports `--branch` to run on a specific branch,
  and `--variables KEY=VALUE,...` for pipeline variables.
  """
  def run_pipeline(parsed) do
    project = parsed.arguments.project
    pipeline_id = parsed.arguments.pipeline_id

    body = %{
      "resources" => %{
        "repositories" => %{
          "self" => %{"refName" => "refs/heads/#{Map.get(parsed.options, :branch, "main")}"}
        }
      }
    }

    body =
      if vars = Map.get(parsed.options, :variables) do
        vars = parse_variables(vars)
        Map.put(body, "variables", vars)
      else
        body
      end

    case Client.post("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}/runs", body) do
      {:ok, run} ->
        writeln(success("Pipeline run triggered!"))
        writeln("  Run ID:   #{run["id"]}")
        writeln("  State:    #{run["state"]}")
        writeln("  Pipeline: #{run["pipeline"]["name"]}")
        writeln("  URL:      #{run["_links"]["web"]["href"]}")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Helpers ───────────────────────────────────────────────────────────

  defp put_if(map, nil, _key), do: map
  defp put_if(map, value, key), do: Map.put(map, key, value)

  defp parse_variables(vars_string) do
    vars_string
    |> String.split(",")
    |> Enum.map(&String.trim/1)
    |> Enum.reduce(%{}, fn pair, acc ->
      case String.split(pair, "=", parts: 2) do
        [key, value] -> Map.put(acc, key, %{"value" => value})
        _ -> acc
      end
    end)
  end

  # ── Formatting ────────────────────────────────────────────────────────

  defp print_pipelines_table(pipelines) do
    if Enum.empty?(pipelines) do
      writeln("No pipelines found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 6)}  #{String.pad_trailing("Name", 40)}  Folder")
      writeln(String.duplicate("─", 80))

      Enum.each(pipelines, fn p ->
        writeln(
          "#{String.pad_trailing(to_string(p["id"] || ""), 6)}  #{String.pad_trailing(p["name"] || "", 40)}  #{p["folder"] || "/"}"
        )
      end)

      writeln("")
      writeln("#{length(pipelines)} pipeline(s)")
    end
  end

  defp print_pipeline_detail(pipeline) do
    writeln("")
    writeln(success("Pipeline Details"))
    writeln(String.duplicate("─", 60))
    writeln("  ID:     #{pipeline["id"]}")
    writeln("  Name:   #{pipeline["name"]}")
    writeln("  Folder: #{pipeline["folder"] || "/"}")
    writeln("  URL:    #{pipeline["url"]}")

    if configuration = pipeline["configuration"] do
      writeln("  Type:   #{configuration["type"] || "?"}")
      if path = configuration["path"], do: writeln("  Path:   #{path}")
    end

    if pipeline["_links"]["web"], do: writeln("  Web:    #{pipeline["_links"]["web"]["href"]}")
    writeln("")
  end
end
