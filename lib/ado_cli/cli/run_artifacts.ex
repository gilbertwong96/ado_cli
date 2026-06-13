defmodule AdoCli.CLI.RunArtifacts do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado pipelines artifacts",
      doc: "Manage pipeline run artifacts (upload, list, download).",
      subcommands: [
        list: [
          name: "ado pipelines artifacts list",
          doc: "List artifacts for a pipeline run.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            pipeline_id: [type: :integer, doc: "Pipeline definition ID"],
            run_id: [type: :integer, doc: "Run ID"]
          ],
          execute: &list_artifacts/1
        ],
        download: [
          name: "ado pipelines artifacts download",
          doc: "Download an artifact.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            pipeline_id: [type: :integer, doc: "Pipeline definition ID"],
            run_id: [type: :integer, doc: "Run ID"],
            artifact_name: [type: :string, doc: "Artifact name"]
          ],
          options: [
            output: [type: :string, doc: "Output file path", doc_arg: "PATH"]
          ],
          execute: &download_artifact/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_artifacts(parsed) do
    %{project: project, pipeline_id: pid, run_id: run_id} = parsed.arguments
    path = "/#{URI.encode(project)}/_apis/pipelines/#{pid}/runs/#{run_id}/artifacts"
    result = Client.list(path)

    Helpers.handle_api_result(result, parsed, fn artifacts ->
      Helpers.json_or_format(artifacts, parsed, &print_artifacts_table/1)
    end)
  end

  def download_artifact(parsed) do
    %{project: project, pipeline_id: pid, run_id: run_id, artifact_name: name} = parsed.arguments
    artifact_path = "/#{URI.encode(project)}/_apis/pipelines/#{pid}/runs/#{run_id}/artifacts"

    case Client.list(artifact_path) do
      {:ok, artifacts} ->
        case Enum.find(artifacts, &(&1["name"] == name)) do
          nil -> halt_error("Artifact '#{name}' not found in run ##{run_id}")
          artifact -> download_and_save(artifact, parsed)
        end

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp download_and_save(artifact, parsed) do
    url = artifact["resource"]["downloadUrl"]
    name = artifact["name"]

    case Client.get_raw(url) do
      {:ok, body} ->
        output = Map.get(parsed.options, :output, name <> ".zip")
        File.write!(output, body)
        success("Downloaded #{byte_size(body)} bytes to #{output}\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_artifacts_table(artifacts) do
    if Enum.empty?(artifacts) do
      writeln("No artifacts found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("Name", 30)}  Size")
      writeln(String.duplicate("-", 50))

      Enum.each(artifacts, fn a ->
        resource = a["resource"] || %{}
        size = resource["size"] || "?"
        writeln("#{String.pad_trailing(a["name"] || "", 30)}  #{size}")
      end)

      writeln("")
      writeln("#{length(artifacts)} artifact(s)")
    end
  end
end
