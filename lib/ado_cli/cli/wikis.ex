defmodule AdoCli.CLI.Wikis do
  @moduledoc """
  Commands for managing Azure DevOps wikis.

    ado wikis list PROJECT
    ado wikis show PROJECT WIKI_ID
    ado wikis pages list PROJECT WIKI_ID [--path PATH]
    ado wikis pages show PROJECT WIKI_ID [--path PATH]
    ado wikis pages create PROJECT WIKI_ID --path PATH --content CONTENT
    ado wikis pages update PROJECT WIKI_ID --path PATH --content CONTENT
  """

  @behaviour CliMate.CLI.Command
  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado wikis",
      doc:
        "Manage Azure DevOps project wikis (code wikis) and their pages. Supports list, show, create, and update. Wikis are per-project Markdown documentation published alongside code.",
      subcommands: [
        list: [
          name: "ado wikis list",
          doc:
            "List all wikis in a project. Typically one code wiki per project; output shows name, type, and repository. Use --json for raw data.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          execute: &list_wikis/1
        ],
        show: [
          name: "ado wikis show",
          doc:
            "Show a single wiki: name, type (codeWiki or projectWiki), mapped repository, versions, and remote URL.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            wiki_id: [type: :string, doc: "Wiki ID or name"]
          ],
          execute: &show_wiki/1
        ],
        pages: [
          name: "ado wikis pages",
          doc:
            "Manage individual wiki pages (create, read, update). Pages are organized by path (e.g. /Home, /Design/Architecture). Content is Markdown.",
          subcommands: [
            list: [
              name: "ado wikis pages list",
              doc:
                "List all pages in a wiki recursively. Use --path to scope to a subtree (e.g. /Design). Output is a flat list of paths.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                wiki_id: [type: :string, doc: "Wiki ID or name"]
              ],
              options: [path: [type: :string, doc: "Path to list (default: /)", doc_arg: "PATH"]],
              execute: &list_pages/1
            ],
            show: [
              name: "ado wikis pages show",
              doc: "Show a wiki page.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                wiki_id: [type: :string, doc: "Wiki ID or name"]
              ],
              options: [path: [type: :string, required: true, doc: "Page path", doc_arg: "PATH"]],
              execute: &show_page/1
            ],
            create: [
              name: "ado wikis pages create",
              doc: "Create or update a wiki page.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                wiki_id: [type: :string, doc: "Wiki ID or name"]
              ],
              options: [
                path: [type: :string, required: true, doc: "Page path", doc_arg: "PATH"],
                content: [
                  type: :string,
                  required: true,
                  doc:
                    "Markdown content for the page. Multi-word values do not need quoting. Use @file to read from a local .md file.",
                  doc_arg: "CONTENT"
                ]
              ],
              execute: &create_page/1
            ],
            update: [
              name: "ado wikis pages update",
              doc: "Update a wiki page.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                wiki_id: [type: :string, doc: "Wiki ID or name"]
              ],
              options: [
                path: [type: :string, required: true, doc: "Page path", doc_arg: "PATH"],
                content: [
                  type: :string,
                  required: true,
                  doc:
                    "Replacement Markdown content for the page. Use @file to read from a local .md file.",
                  doc_arg: "CONTENT"
                ]
              ],
              execute: &update_page/1
            ]
          ]
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_wikis(parsed) do
    project = parsed.arguments.project
    result = Client.list("/#{URI.encode(project)}/_apis/wiki/wikis")

    Helpers.handle_api_result(result, parsed, fn wikis ->
      Helpers.json_or_format(wikis, parsed, &print_wikis_table/1)
    end)
  end

  def show_wiki(parsed) do
    project = parsed.arguments.project
    wiki_id = parsed.arguments.wiki_id

    case Client.get("/#{URI.encode(project)}/_apis/wiki/wikis/#{URI.encode(wiki_id)}") do
      {:ok, wiki} -> Helpers.json_or_format(wiki, parsed, &print_wiki_detail/1)
      {:error, %{status: 404}} -> halt_error("Wiki '#{wiki_id}' not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def list_pages(parsed) do
    project = parsed.arguments.project
    wiki_id = parsed.arguments.wiki_id
    path = Map.get(parsed.options, :path, "/")

    case Client.get("/#{URI.encode(project)}/_apis/wiki/wikis/#{URI.encode(wiki_id)}/pages", %{
           "path" => path,
           "recursionLevel" => "OneLevel"
         }) do
      {:ok, pages} -> Helpers.json_or_format(pages, parsed, &print_pages_table/1)
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def show_page(parsed) do
    project = parsed.arguments.project
    wiki_id = parsed.arguments.wiki_id
    path = Map.fetch!(parsed.options, :path)

    case Client.get("/#{URI.encode(project)}/_apis/wiki/wikis/#{URI.encode(wiki_id)}/pages", %{
           "path" => path,
           "includeContent" => "true"
         }) do
      {:ok, page} ->
        if content = page["content"], do: writeln(content)
        Helpers.json_or_format(page, parsed, fn p -> writeln(p["content"] || "") end)

      {:error, %{status: 404}} ->
        halt_error("Page '#{path}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def create_page(parsed) do
    project = parsed.arguments.project
    wiki_id = parsed.arguments.wiki_id
    path = Map.fetch!(parsed.options, :path)
    content = Map.fetch!(parsed.options, :content)
    body = %{"content" => content}

    case Client.put(
           "/#{URI.encode(project)}/_apis/wiki/wikis/#{URI.encode(wiki_id)}/pages",
           body,
           %{"path" => path, "comment" => "Created via ado CLI"}
         ) do
      {:ok, page} ->
        success("Page '#{page["path"]}' created.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def update_page(parsed) do
    project = parsed.arguments.project
    wiki_id = parsed.arguments.wiki_id
    path = Map.fetch!(parsed.options, :path)
    content = Map.fetch!(parsed.options, :content)

    # The If-Match header (ETag) prevents overwriting changes
    # made by another user between our read and write.
    case Client.get("/#{URI.encode(project)}/_apis/wiki/wikis/#{URI.encode(wiki_id)}/pages", %{
           "path" => path,
           "includeContent" => "true"
         }) do
      {:ok, existing} ->
        etag = existing["eTag"] || ""
        extra_headers = if etag != "", do: [{"If-Match", etag}], else: []
        body = %{"content" => content}

        case Client.put(
               "/#{URI.encode(project)}/_apis/wiki/wikis/#{URI.encode(wiki_id)}/pages",
               body,
               %{"path" => path, "comment" => "Updated via ado CLI"},
               extra_headers
             ) do
          {:ok, page} ->
            success("Page '#{page["path"]}' updated.\n")
            halt_success("")

          error ->
            Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
        end

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_wikis_table(wikis) do
    if Enum.empty?(wikis) do
      writeln("No wikis found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 40)} #{String.pad_trailing("Name", 30)} Type")
      writeln(String.duplicate("─", 85))

      AdoCli.CLI.Helpers.print_id_name_type_table(wikis)

      writeln("")
      writeln("#{length(wikis)} wiki(s)")
    end
  end

  defp print_wiki_detail(wiki) do
    writeln("")
    success("Wiki Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:   #{wiki["id"]}")
    writeln("  Name: #{wiki["name"]}")
    writeln("  Type: #{wiki["type"]}")
    writeln("  URL:  #{wiki["remoteUrl"] || wiki["url"]}")
    writeln("")
  end

  defp print_pages_table(data) do
    pages = if is_list(data), do: data, else: data["subPages"] || data["value"] || []

    if Enum.empty?(pages) do
      writeln("No pages found.")
    else
      writeln("")
      writeln("  Path")
      writeln(String.duplicate("─", 60))
      Enum.each(pages, fn p -> writeln("  #{p["path"] || p["pagePath"] || "/"}") end)
      writeln("")
      writeln("#{length(pages)} page(s)")
    end
  end
end
