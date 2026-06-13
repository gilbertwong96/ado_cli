defmodule AdoCli.CLI.Packages do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado packages",
      doc: "Manage Azure Artifacts universal packages.",
      subcommands: [
        list: [
          name: "ado packages list",
          doc: "List packages in a feed.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            feed_id: [type: :string, doc: "Feed name or ID"]
          ],
          execute: &list_packages/1
        ],
        versions: [
          name: "ado packages versions",
          doc: "List versions of a package.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            feed_id: [type: :string, doc: "Feed name or ID"],
            package_name: [type: :string, doc: "Package name"]
          ],
          execute: &list_versions/1
        ],
        show: [
          name: "ado packages show",
          doc: "Show details of a package version.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            feed_id: [type: :string, doc: "Feed name or ID"],
            package_name: [type: :string, doc: "Package name"],
            package_version: [type: :string, doc: "Package version"]
          ],
          execute: &show_package/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_packages(parsed) do
    %{project: project, feed_id: feed_id} = parsed.arguments
    path = "/#{URI.encode(project)}/_apis/packaging/feeds/#{URI.encode(feed_id)}/packages"
    result = Client.list(path, %{"protocolType" => "UPack"})

    Helpers.handle_api_result(result, parsed, fn packages ->
      Helpers.json_or_format(packages, parsed, &print_packages_table/1)
    end)
  end

  def list_versions(parsed) do
    %{project: project, feed_id: feed_id, package_name: package_name} = parsed.arguments

    path =
      "/#{URI.encode(project)}/_apis/packaging/feeds/#{URI.encode(feed_id)}/packages/#{URI.encode(package_name)}/versions"

    result = Client.list(path)

    Helpers.handle_api_result(result, parsed, fn versions ->
      Helpers.json_or_format(versions, parsed, &print_versions_table/1)
    end)
  end

  def show_package(parsed) do
    %{
      project: project,
      feed_id: feed_id,
      package_name: package_name,
      package_version: package_version
    } = parsed.arguments

    path =
      "/#{URI.encode(project)}/_apis/packaging/feeds/#{URI.encode(feed_id)}/packages/#{URI.encode(package_name)}/versions/#{URI.encode(package_version)}"

    case Client.get(path) do
      {:ok, pkg} ->
        Helpers.json_or_format(pkg, parsed, &print_package_detail/1)

      {:error, %{status: 404}} ->
        halt_error("Package '#{package_name}@#{package_version}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_packages_table(packages) do
    if Enum.empty?(packages) do
      writeln("No packages found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("Name", 40)} #{String.pad_trailing("Protocol", 12)} Versions"
      )

      writeln(String.duplicate("-", 70))

      Enum.each(packages, fn p ->
        writeln(
          "#{String.pad_trailing(p["name"] || "", 40)} #{String.pad_trailing(p["protocolType"] || "", 12)} #{length(p["versions"] || [])}"
        )
      end)

      writeln("")
      writeln("#{length(packages)} package(s)")
    end
  end

  defp print_versions_table(versions) do
    if Enum.empty?(versions) do
      writeln("No versions found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("Version", 20)} #{String.pad_trailing("Status", 12)} Publish Date"
      )

      writeln(String.duplicate("-", 60))

      Enum.each(versions, fn v ->
        writeln(
          "#{String.pad_trailing(v["version"] || "", 20)} #{String.pad_trailing((v["isLatest"] == true && "latest") || (v["isDeleted"] == true && "deleted") || "normal", 12)} #{v["publishDate"] || ""}"
        )
      end)

      writeln("")
      writeln("#{length(versions)} version(s)")
    end
  end

  defp print_package_detail(pkg) do
    writeln("")
    success("Package Details\n")
    writeln(String.duplicate("-", 60))
    writeln("  Name:     #{pkg["name"]}")
    writeln("  Version:  #{pkg["version"]}")
    writeln("  Protocol: #{pkg["protocolType"]}")
    writeln("  Status:   #{(pkg["isLatest"] == true && "latest") || "normal"}")
    writeln("  Size:     #{pkg["size"] || "?"}")
    writeln("  Published:#{pkg["publishDate"]}")
    writeln("")
  end
end
