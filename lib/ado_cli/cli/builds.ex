defmodule AdoCli.CLI.Builds do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado pipelines builds",
      doc: "Manage Azure Pipelines classic builds.",
      subcommands: [
        list: [
          name: "ado pipelines builds list",
          doc: "List builds in a project.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            top: [type: :integer, doc: "Maximum number to return", doc_arg: "N"],
            definitions: [
              type: :string,
              doc: "Filter by definition IDs (comma-separated)",
              doc_arg: "IDS"
            ]
          ],
          execute: &list_builds/1
        ],
        show: [
          name: "ado pipelines builds show",
          doc: "Show details of a specific build.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            build_id: [type: :integer, doc: "Build ID"]
          ],
          execute: &show_build/1
        ],
        queue: [
          name: "ado pipelines builds queue",
          doc: "Queue a new build.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            definition: [
              type: :integer,
              required: true,
              doc: "Build definition ID",
              doc_arg: "ID"
            ],
            branch: [type: :string, doc: "Source branch (default: main)", doc_arg: "BRANCH"]
          ],
          execute: &queue_build/1
        ],
        cancel: [
          name: "ado pipelines builds cancel",
          doc: "Cancel a build.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            build_id: [type: :integer, doc: "Build ID"]
          ],
          execute: &cancel_build/1
        ],
        tags: [
          name: "ado pipelines builds tags",
          doc: "Manage build tags.",
          subcommands: [
            list: [
              name: "ado pipelines builds tags list",
              doc: "List tags on a build.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                build_id: [type: :integer, doc: "Build ID"]
              ],
              execute: &list_tags/1
            ],
            add: [
              name: "ado pipelines builds tags add",
              doc: "Add tags to a build.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                build_id: [type: :integer, doc: "Build ID"]
              ],
              options: [
                tags: [
                  type: :string,
                  required: true,
                  doc: "Tags (comma-separated)",
                  doc_arg: "TAGS"
                ]
              ],
              execute: &add_tags/1
            ]
          ]
        ],
        definitions: [
          name: "ado pipelines builds definitions",
          doc: "Manage classic build definitions.",
          subcommands: [
            list: [
              name: "ado pipelines builds definitions list",
              doc: "List classic build definitions.",
              arguments: [project: [type: :string, doc: "Project name or ID"]],
              execute: &list_definitions/1
            ]
          ]
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_builds(parsed) do
    project = parsed.arguments.project
    params = %{}

    params =
      if top = Map.get(parsed.options, :top), do: Map.put(params, "$top", top), else: params

    params =
      if defs = Map.get(parsed.options, :definitions),
        do: Map.put(params, "definitions", defs),
        else: params

    result = Client.list("/#{URI.encode(project)}/_apis/build/builds", params)

    Helpers.handle_api_result(result, parsed, fn builds ->
      Helpers.json_or_format(builds, parsed, &print_builds_table/1)
    end)
  end

  def show_build(parsed) do
    %{project: project, build_id: build_id} = parsed.arguments

    case Client.get("/#{URI.encode(project)}/_apis/build/builds/#{build_id}") do
      {:ok, build} -> Helpers.json_or_format(build, parsed, &print_build_detail/1)
      {:error, %{status: 404}} -> halt_error("Build ##{build_id} not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def queue_build(parsed) do
    project = parsed.arguments.project
    def_id = Map.fetch!(parsed.options, :definition)
    branch = Map.get(parsed.options, :branch, "main")
    body = %{"definition" => %{"id" => def_id}, "sourceBranch" => "refs/heads/#{branch}"}

    case Client.post("/#{URI.encode(project)}/_apis/build/builds", body) do
      {:ok, build} ->
        success("Build ##{build["id"]} queued.\n")
        writeln("  Status: #{build["status"]}")
        writeln("  URL:    #{build["_links"]["web"]["href"]}")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def cancel_build(parsed) do
    %{project: project, build_id: build_id} = parsed.arguments

    case Client.patch("/#{URI.encode(project)}/_apis/build/builds/#{build_id}", %{
           "status" => "cancelling"
         }) do
      {:ok, build} ->
        success("Build ##{build["id"]} cancelled.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def list_tags(parsed) do
    %{project: project, build_id: build_id} = parsed.arguments

    case Client.get("/#{URI.encode(project)}/_apis/build/builds/#{build_id}/tags") do
      {:ok, tags} ->
        if is_list(tags) && Enum.empty?(tags) do
          writeln("No tags.")
          halt_success("")
        else
          Helpers.json_or_format(tags, parsed, fn tags ->
            writeln("Tags: #{Enum.join(tags, ", ")}")
            halt_success("")
          end)
        end

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def add_tags(parsed) do
    %{project: project, build_id: build_id} = parsed.arguments
    tags_raw = Map.fetch!(parsed.options, :tags)
    tags = Enum.map(String.split(tags_raw, ","), &String.trim/1)

    case Client.put("/#{URI.encode(project)}/_apis/build/builds/#{build_id}/tags", tags) do
      {:ok, _} ->
        success("Tags added to build ##{build_id}.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def list_definitions(parsed) do
    project = parsed.arguments.project
    result = Client.list("/#{URI.encode(project)}/_apis/build/definitions")

    Helpers.handle_api_result(result, parsed, fn defs ->
      Helpers.json_or_format(defs, parsed, &print_definitions_table/1)
    end)
  end

  defp print_builds_table(builds) do
    if Enum.empty?(builds) do
      writeln("No builds found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 8)} #{String.pad_trailing("Definition", 30)} #{String.pad_trailing("Status", 14)} #{String.pad_trailing("Result", 14)} Branch"
      )

      writeln(String.duplicate("-", 100))

      Enum.each(builds, fn b ->
        writeln(
          "#{String.pad_trailing(to_string(b["id"]), 8)} #{String.pad_trailing(b["definition"]["name"] || "", 30)} #{String.pad_trailing(b["status"] || "", 14)} #{String.pad_trailing(b["result"] || "", 14)} #{b["sourceBranch"] || ""}"
        )
      end)

      writeln("")
      writeln("#{length(builds)} build(s)")
    end
  end

  defp print_build_detail(build) do
    writeln("")
    success("Build Details\n")
    writeln(String.duplicate("-", 60))
    writeln("  ID:         #{build["id"]}")
    writeln("  Definition: #{build["definition"]["name"]}")
    writeln("  Status:     #{build["status"]}")
    writeln("  Result:     #{build["result"]}")
    writeln("  Branch:     #{build["sourceBranch"]}")
    writeln("  Requested:  #{build["requestedFor"]["displayName"]}")
    writeln("  Queue:      #{build["queueTime"]}")
    if build["_links"]["web"], do: writeln("  Web:        #{build["_links"]["web"]["href"]}")
    writeln("")
  end

  defp print_definitions_table(defs) do
    if Enum.empty?(defs) do
      writeln("No classic build definitions found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 8)} #{String.pad_trailing("Name", 40)} #{String.pad_trailing("Queue", 10)}"
      )

      writeln(String.duplicate("-", 60))

      Enum.each(defs, fn d ->
        queue = d["queue"]["name"] || d["queueStatus"] || ""

        writeln(
          "#{String.pad_trailing(to_string(d["id"]), 8)} #{String.pad_trailing(d["name"] || "", 40)} #{String.pad_trailing(queue, 10)}"
        )
      end)

      writeln("")
      writeln("#{length(defs)} definition(s)")
    end
  end
end
