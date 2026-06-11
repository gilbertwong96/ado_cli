defmodule AdoCli.CLI.WorkItems do
  @moduledoc """
  Commands for managing Azure DevOps Work Items.

    ado_cli workitems list PROJECT       [--type TYPE] [--state STATE] [--assigned-to USER]
    ado_cli workitems show ID            [--expand LEVEL]
    ado_cli workitems query PROJECT      --wiql WIQL
    ado_cli workitems create PROJECT      --type TYPE --title TITLE [--description DESC] [--assigned-to USER]
    ado_cli workitems update ID           --title TITLE [--state STATE] [--assigned-to USER]
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado_cli workitems",
      doc: "Manage Azure DevOps work items.",
      subcommands: [
        list: [
          name: "ado_cli workitems list",
          doc: "List work items in a project.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            type: [type: :string, doc: "Work item type (Bug, Task, User Story)", doc_arg: "TYPE"],
            assigned_to: [type: :string, doc: "Filter by assigned user", doc_arg: "USER"],
            state: [type: :string, doc: "Filter by state", doc_arg: "STATE"],
            top: [type: :integer, doc: "Maximum items to return", doc_arg: "N"]
          ],
          execute: &list_work_items/1
        ],
        show: [
          name: "ado_cli workitems show",
          doc: "Show details of a specific work item.",
          arguments: [id: [type: :integer, doc: "Work item ID"]],
          options: [
            expand: [type: :string, default: "all", doc: "Expand level", doc_arg: "LEVEL"]
          ],
          execute: &show_work_item/1
        ],
        query: [
          name: "ado_cli workitems query",
          doc: "Run a WIQL query against a project.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            wiql: [type: :string, doc: "WIQL query string", doc_arg: "WIQL"],
            top: [type: :integer, doc: "Maximum number of results", doc_arg: "N"]
          ],
          execute: &query_work_items/1
        ],
        create: [
          name: "ado_cli workitems create",
          doc: "Create a new work item.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            type: [
              type: :string,
              required: true,
              doc: "Work item type (Bug, Task, User Story, Epic, Issue)",
              doc_arg: "TYPE"
            ],
            title: [type: :string, required: true, doc: "Work item title", doc_arg: "TITLE"],
            description: [type: :string, doc: "Work item description", doc_arg: "DESC"],
            assigned_to: [type: :string, doc: "Assign to user", doc_arg: "USER"],
            state: [type: :string, doc: "Initial state", doc_arg: "STATE"],
            priority: [type: :integer, doc: "Priority (1-4)", doc_arg: "N"],
            tags: [type: :string, doc: "Comma-separated tags", doc_arg: "TAGS"]
          ],
          execute: &create_work_item/1
        ],
        update: [
          name: "ado_cli workitems update",
          doc: "Update an existing work item.",
          arguments: [id: [type: :integer, doc: "Work item ID"]],
          options: [
            title: [type: :string, doc: "New title", doc_arg: "TITLE"],
            description: [type: :string, doc: "New description", doc_arg: "DESC"],
            state: [type: :string, doc: "New state", doc_arg: "STATE"],
            assigned_to: [type: :string, doc: "Assign to user", doc_arg: "USER"],
            priority: [type: :integer, doc: "Priority (1-4)", doc_arg: "N"],
            tags: [type: :string, doc: "Comma-separated tags (replaces all)", doc_arg: "TAGS"]
          ],
          execute: &update_work_item/1
        ],
        comments: [
          name: "ado_cli workitems comments",
          doc: "Manage work item discussion comments.",
          subcommands: [
            list: [
              name: "ado_cli workitems comments list",
              doc: "List discussion comments on a work item.",
              arguments: [
                id: [type: :integer, doc: "Work item ID"]
              ],
              execute: &list_work_item_comments/1
            ],
            add: [
              name: "ado_cli workitems comments add",
              doc: "Add a discussion comment to a work item.",
              arguments: [
                id: [type: :integer, doc: "Work item ID"]
              ],
              options: [
                text: [type: :string, doc: "Comment text", required: true, doc_arg: "TEXT"]
              ],
              execute: &add_work_item_comment/1
            ],
            update: [
              name: "ado_cli workitems comments update",
              doc: "Update a discussion comment on a work item.",
              arguments: [
                id: [type: :integer, doc: "Work item ID"],
                comment_id: [type: :integer, doc: "Comment revision (from history)"]
              ],
              options: [
                text: [type: :string, doc: "New comment text", required: true, doc_arg: "TEXT"]
              ],
              execute: &update_work_item_comment/1
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
  Lists work items in a project using a WIQL query.

  Supports filtering by `--type`, `--assigned-to`, `--state`, and `--top`.
  """
  def list_work_items(parsed) do
    project = parsed.arguments.project

    filters =
      []
      |> add_wiql_filter(Map.get(parsed.options, :type), "System.WorkItemType")
      |> add_wiql_filter(Map.get(parsed.options, :assigned_to), "System.AssignedTo")
      |> add_wiql_filter(Map.get(parsed.options, :state), "System.State")

    wiql =
      ["ORDER BY [System.Id] DESC" | filters]
      |> then(&["WHERE [System.TeamProject] = '#{escape_wiql(project)}'" | &1])
      |> then(&["FROM WorkItems" | &1])
      |> then(
        &[
          "SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], [System.AssignedTo]"
          | &1
        ]
      )
      |> Enum.reverse()
      |> Enum.join(" ")

    run_wiql_query(project, wiql, parsed)
  end

  @doc """
  Shows full details of a work item by ID.

  Use `--expand` to control detail level (none, relations, fields, links, all).
  """
  def show_work_item(parsed) do
    id = parsed.arguments.id
    params = %{"$expand" => Map.get(parsed.options, :expand, "all")}

    case Client.get("/_apis/wit/workitems/#{id}", params) do
      {:ok, wi} -> Helpers.json_or_format(wi, parsed, &print_work_item_detail/1)
      {:error, %{status: 404}} -> halt_error("Work item ##{id} not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc """
  Runs a raw WIQL query against a project.

  Requires `--wiql` with the WIQL query string.
  """
  def query_work_items(parsed) do
    project = parsed.arguments.project
    wiql = Map.get(parsed.options, :wiql)
    unless wiql, do: halt_error("--wiql is required for the query command")
    run_wiql_query(project, wiql, parsed)
  end

  # ── Write ─────────────────────────────────────────────────────────────

  @doc """
  Creates a new work item using JSON-Patch format.

  Requires `--type` (Bug, Task, User Story, etc.) and `--title`.
  Supports `--description`, `--assigned-to`, `--state`, `--priority`, `--tags`.
  """
  def create_work_item(parsed) do
    project = parsed.arguments.project
    type = Map.get(parsed.options, :type)
    title = Map.get(parsed.options, :title)

    unless type, do: halt_error("--type is required (e.g. Bug, Task, User Story)")
    unless title, do: halt_error("--title is required")

    patch =
      build_json_patch([
        {"/fields/System.Title", title},
        {"/fields/System.Description", Map.get(parsed.options, :description)},
        {"/fields/System.AssignedTo", Map.get(parsed.options, :assigned_to)},
        {"/fields/System.State", Map.get(parsed.options, :state)},
        {"/fields/Microsoft.VSTS.Common.Priority", Map.get(parsed.options, :priority)},
        {"/fields/System.Tags", Map.get(parsed.options, :tags)}
      ])

    case Client.post("/#{URI.encode(project)}/_apis/wit/workitems/$#{URI.encode(type)}", patch) do
      {:ok, wi} ->
        writeln(success("Work item ##{wi["id"]} created: #{wi["fields"]["System.Title"]}"))
        writeln("  Type:  #{wi["fields"]["System.WorkItemType"]}")
        writeln("  State: #{wi["fields"]["System.State"]}")
        writeln("  URL:   #{wi["_links"]["html"]["href"]}")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc """
  Updates an existing work item's fields.

  Supports `--title`, `--description`, `--state`, `--assigned-to`,
  `--priority`, and `--tags`.
  """
  def update_work_item(parsed) do
    id = parsed.arguments.id

    patch =
      build_json_patch([
        {"/fields/System.Title", Map.get(parsed.options, :title)},
        {"/fields/System.Description", Map.get(parsed.options, :description)},
        {"/fields/System.State", Map.get(parsed.options, :state)},
        {"/fields/System.AssignedTo", Map.get(parsed.options, :assigned_to)},
        {"/fields/Microsoft.VSTS.Common.Priority", Map.get(parsed.options, :priority)},
        {"/fields/System.Tags", Map.get(parsed.options, :tags)}
      ])

    if patch == [] do
      halt_error(
        "At least one field to update is required (--title, --state, --assigned-to, etc.)"
      )
    end

    case Client.patch("/_apis/wit/workitems/#{id}", patch) do
      {:ok, wi} ->
        writeln(success("Work item ##{wi["id"]} updated."))
        writeln("  Title: #{wi["fields"]["System.Title"]}")
        writeln("  State: #{wi["fields"]["System.State"]}")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Work item ##{id} not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Helpers ───────────────────────────────────────────────────────────

  defp build_json_patch(fields) do
    fields
    |> Enum.reject(fn {_path, value} -> is_nil(value) end)
    |> Enum.map(fn {path, value} ->
      %{"op" => "add", "path" => path, "value" => format_field_value(value)}
    end)
  end

  defp format_field_value(value) when is_integer(value), do: value
  defp format_field_value(value), do: value

  defp run_wiql_query(project, wiql, parsed) do
    top = Map.get(parsed.options, :top)
    body = %{"query" => wiql}

    case Client.post("/#{URI.encode(project)}/_apis/wit/wiql", body) do
      {:ok, %{"workItems" => items}} ->
        items = if top, do: Enum.take(items, top), else: items

        if Enum.empty?(items) do
          writeln("No work items found.")
          halt_success("")
        else
          Helpers.json_or_format(items, parsed, &print_work_items_table/1)
        end

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp add_wiql_filter(parts, nil, _field), do: parts

  defp add_wiql_filter(parts, value, field),
    do: ["AND [#{field}] = '#{escape_wiql(value)}'" | parts]

  defp escape_wiql(str), do: String.replace(str, "'", "''")

  # ── Formatting ────────────────────────────────────────────────────────

  defp print_work_items_table(items) do
    writeln("")

    writeln(
      "#{String.pad_trailing("ID", 8)}  #{String.pad_trailing("Type", 18)}  #{String.pad_trailing("Title", 50)}  State"
    )

    writeln(String.duplicate("─", 110))

    Enum.each(items, fn item ->
      fields = item["fields"] || %{}

      writeln(
        "#{String.pad_trailing(to_string(item["id"] || ""), 8)}  #{String.pad_trailing(fields["System.WorkItemType"] || "", 18)}  #{String.pad_trailing(fields["System.Title"] || "", 50)}  #{fields["System.State"] || ""}"
      )
    end)

    writeln("")
    writeln("#{length(items)} work item(s)")
  end

  defp print_work_item_detail(wi) do
    fields = wi["fields"] || %{}
    writeln("")
    writeln(success("Work Item ##{wi["id"]}"))
    writeln(String.duplicate("─", 60))
    writeln("  Type:        #{fields["System.WorkItemType"] || "?"}")
    writeln("  Title:       #{fields["System.Title"] || "?"}")
    writeln("  State:       #{fields["System.State"] || "?"}")
    writeln("  Assigned To: #{display_name(fields["System.AssignedTo"], "(unassigned)")}")
    writeln("  Created By:  #{display_name(fields["System.CreatedBy"], "?")}")
    writeln("  Created:     #{fields["System.CreatedDate"] || "?"}")

    if fields["System.Description"],
      do: writeln("  Description: #{String.slice(fields["System.Description"], 0, 200)}...")

    writeln("  URL:         #{wi["url"]}")
    writeln("")
  end

  defp display_name(%{"displayName" => name}, _default), do: name
  defp display_name(nil, default), do: default
  defp display_name(_, default), do: default

  # ── Comments ────────────────────────────────────────────────────────

  @doc """
  Lists discussion comments (history) of a work item.
  """
  def list_work_item_comments(parsed) do
    id = parsed.arguments.id
    path = "/_apis/wit/workItems/#{id}/comments"

    case Client.get(path) do
      {:ok, %{"comments" => comments}} ->
        Helpers.json_or_format(comments, parsed, fn comments ->
          writeln("")
          Enum.each(comments, &print_comment/1)
        end)

      {:ok, _} ->
        writeln("No comments found.")

      {:error, reason} ->
        Helpers.handle_api_result({:error, reason}, parsed, nil)
    end

    halt_success("Done.")
  end

  @doc """
  Adds a discussion comment to a work item via System.History.
  """
  def add_work_item_comment(parsed) do
    id = parsed.arguments.id
    text = parsed.options.text

    patch = [
      %{"op" => "add", "path" => "/fields/System.History", "value" => text}
    ]

    path = "/_apis/wit/workitems/#{id}"

    case Client.patch(path, patch) do
      {:ok, _} ->
        success("Comment added to work item ##{id}.\n")

      {:error, reason} ->
        Helpers.handle_api_result({:error, reason}, parsed, nil)
    end

    halt_success("Done.")
  end

  @doc """
  Updates a discussion comment (adds a new history entry that corrects the previous).
  """
  def update_work_item_comment(parsed) do
    id = parsed.arguments.id
    text = parsed.options.text

    patch = [
      %{"op" => "add", "path" => "/fields/System.History", "value" => "[Edited] #{text}"}
    ]

    path = "/_apis/wit/workitems/#{id}"

    case Client.patch(path, patch) do
      {:ok, _} ->
        success("Comment updated on work item ##{id}.\n")

      {:error, reason} ->
        Helpers.handle_api_result({:error, reason}, parsed, nil)
    end

    halt_success("Done.")
  end

  defp print_comment(comment) do
    author = (comment["createdBy"] && comment["createdBy"]["displayName"]) || "unknown"
    date = comment["createdDate"] || ""
    text = comment["text"] || ""
    cid = comment["id"]
    writeln("  [#{cid}] #{author} (#{date})")
    writeln("  #{text}")
    writeln("")
  end
end
