defmodule AdoCli.CLI.Extensions do
  @moduledoc """
  Commands for managing Azure DevOps marketplace extensions.

    ado extensions list [--search SEARCH]
    ado extensions show EXTENSION_ID
    ado extensions install --publisher PUBLISHER --name EXTENSION_NAME
    ado extensions uninstall --publisher PUBLISHER --name EXTENSION_NAME
    ado extensions enable --publisher PUBLISHER --name EXTENSION_NAME
    ado extensions disable --publisher PUBLISHER --name EXTENSION_NAME
  """

  @behaviour CliMate.CLI.Command
  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado extensions",
      doc: "Manage marketplace extensions.",
      subcommands: [
        list: [
          name: "ado extensions list",
          doc: "List installed extensions.",
          options: [search: [type: :string, doc: "Search by extension name", doc_arg: "SEARCH"]],
          execute: &list_extensions/1
        ],
        show: [
          name: "ado extensions show",
          doc: "Show details of an extension.",
          arguments: [extension_id: [type: :string, doc: "Extension ID (pub.name)"]],
          execute: &show_extension/1
        ],
        install: [
          name: "ado extensions install",
          doc: "Install an extension from the marketplace.",
          options: [
            publisher: [
              type: :string,
              required: true,
              doc: "Publisher name",
              doc_arg: "PUBLISHER"
            ],
            name: [type: :string, required: true, doc: "Extension name", doc_arg: "NAME"]
          ],
          execute: &install_extension/1
        ],
        uninstall: [
          name: "ado extensions uninstall",
          doc: "Uninstall an extension.",
          options: [
            publisher: [
              type: :string,
              required: true,
              doc: "Publisher name",
              doc_arg: "PUBLISHER"
            ],
            name: [type: :string, required: true, doc: "Extension name", doc_arg: "NAME"]
          ],
          execute: &uninstall_extension/1
        ],
        enable: [
          name: "ado extensions enable",
          doc: "Enable an extension.",
          options: [
            publisher: [
              type: :string,
              required: true,
              doc: "Publisher name",
              doc_arg: "PUBLISHER"
            ],
            name: [type: :string, required: true, doc: "Extension name", doc_arg: "NAME"]
          ],
          execute: &enable_extension/1
        ],
        disable: [
          name: "ado extensions disable",
          doc: "Disable an extension.",
          options: [
            publisher: [
              type: :string,
              required: true,
              doc: "Publisher name",
              doc_arg: "PUBLISHER"
            ],
            name: [type: :string, required: true, doc: "Extension name", doc_arg: "NAME"]
          ],
          execute: &disable_extension/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_extensions(parsed) do
    result = Client.list("/_apis/extensionmanagement/installedextensions")

    data =
      if search = Map.get(parsed.options, :search) do
        case result do
          {:ok, exts} ->
            {:ok,
             Enum.filter(exts, fn e ->
               String.contains?(
                 String.downcase(e["extensionName"] || ""),
                 String.downcase(search)
               )
             end)}

          err ->
            err
        end
      else
        result
      end

    Helpers.handle_api_result(data, parsed, fn exts ->
      Helpers.json_or_format(exts, parsed, &print_extensions_table/1)
    end)
  end

  def show_extension(parsed) do
    ext = parsed.arguments.extension_id

    case Client.get("/_apis/extensionmanagement/installedextensions/#{URI.encode(ext)}") do
      {:ok, ext_data} -> Helpers.json_or_format(ext_data, parsed, &print_extension_detail/1)
      {:error, %{status: 404}} -> halt_error("Extension '#{ext}' not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def install_extension(parsed) do
    pub = Map.fetch!(parsed.options, :publisher)
    name = Map.fetch!(parsed.options, :name)
    body = %{"publisherId" => pub, "extensionName" => name}

    case Client.post("/_apis/extensionmanagement/installedextensions", body) do
      {:ok, _ext} ->
        success("Extension '#{pub}.#{name}' installed.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def uninstall_extension(parsed) do
    pub = Map.fetch!(parsed.options, :publisher)
    name = Map.fetch!(parsed.options, :name)

    case Client.delete("/_apis/extensionmanagement/installedextensions/#{pub}.#{name}") do
      :ok ->
        success("Extension '#{pub}.#{name}' uninstalled.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Extension '#{pub}.#{name}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def enable_extension(parsed) do
    pub = Map.fetch!(parsed.options, :publisher)
    name = Map.fetch!(parsed.options, :name)

    body = %{
      "publisherId" => pub,
      "extensionName" => name,
      "installState" => %{"flags" => "none"}
    }

    case Client.patch("/_apis/extensionmanagement/installedextensions/#{pub}.#{name}", body) do
      {:ok, _} ->
        success("Extension '#{pub}.#{name}' enabled.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def disable_extension(parsed) do
    pub = Map.fetch!(parsed.options, :publisher)
    name = Map.fetch!(parsed.options, :name)

    body = %{
      "publisherId" => pub,
      "extensionName" => name,
      "installState" => %{"flags" => "disabled"}
    }

    case Client.patch("/_apis/extensionmanagement/installedextensions/#{pub}.#{name}", body) do
      {:ok, _} ->
        success("Extension '#{pub}.#{name}' disabled.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_extensions_table(exts) do
    if Enum.empty?(exts) do
      writeln("No extensions found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("Publisher.Name", 45)} #{String.pad_trailing("Version", 12)} State"
      )

      writeln(String.duplicate("─", 75))

      Enum.each(exts, fn e ->
        id = "#{e["publisherId"]}.#{e["extensionName"]}"
        vs = String.slice(e["version"] || "", 0, 10)
        st = if e["installState"]["flags"] == "disabled", do: "disabled", else: "enabled"
        writeln("#{String.pad_trailing(id, 45)} #{String.pad_trailing(vs, 12)} #{st}")
      end)

      writeln("")
      writeln("#{length(exts)} extension(s)")
    end
  end

  defp print_extension_detail(ext) do
    writeln("")
    success("Extension Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  Publisher: #{ext["publisherId"]}")
    writeln("  Name:      #{ext["extensionName"]}")
    writeln("  Version:   #{ext["version"]}")
    st = if ext["installState"]["flags"] == "disabled", do: "disabled", else: "enabled"
    writeln("  State:     #{st}")
    writeln("")
  end
end
