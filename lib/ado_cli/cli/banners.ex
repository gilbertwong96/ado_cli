defmodule AdoCli.CLI.Banners do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado banners",
      doc:
        "Manage the organization-wide notification banner that appears at the top of the Azure DevOps web UI for every user. Useful for maintenance windows or org-wide announcements.",
      subcommands: [
        show: [
          name: "ado banners show",
          doc:
            "Show the current organization banner. Prints 'No banner configured.' if no banner is set, or the message, type, and audience level otherwise. Pass --json for raw output.",
          execute: &show_banner/1
        ],
        set: [
          name: "ado banners set",
          doc:
            "Set or update the organization banner. The banner appears immediately for all users in the org (or for the chosen audience level). Replaces any existing banner.",
          options: [
            message: [
              type: :string,
              required: true,
              doc:
                "Banner text shown to users. Markdown is not supported; the text is rendered as plain text. Multi-word values do not need quoting (joined until next flag). Use @<file> or - to read from a file/stdin.",
              doc_arg: "MSG"
            ],
            type: [
              type: :string,
              doc:
                "Visual style. Valid: info (default — blue), warning (yellow), error (red). Controls the icon and color in the web UI.",
              doc_arg: "TYPE"
            ],
            level: [
              type: :string,
              doc:
                "Audience level. Valid: projectCollection (default — whole org), project (specific project — requires the project context).",
              doc_arg: "LEVEL"
            ]
          ],
          execute: &set_banner/1
        ],
        delete: [
          name: "ado banners delete",
          doc:
            "Remove the organization banner. Errors if no banner is set; otherwise the banner disappears immediately for all users.",
          execute: &delete_banner/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  @settings_path "/_apis/settings/entries/banners"

  def show_banner(parsed) do
    case Client.get(@settings_path) do
      {:ok, entry} ->
        display_banner(entry["value"] || %{}, parsed)

      {:error, %{status: 404}} ->
        no_banner_message()
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp display_banner(value, parsed) do
    Helpers.json_or_format(value, parsed, fn val ->
      writeln("")

      if val == "" or val == %{} do
        writeln("No banner configured.")
      else
        writeln("Current banner:")
        writeln("  Message: #{val["message"] || "(empty)"}")
        writeln("  Type:    #{val["type"] || "info"}")
        writeln("  Level:   #{val["level"] || "projectCollection"}")
      end

      writeln("")
      halt_success("")
    end)
  end

  defp no_banner_message do
    writeln("")
    writeln("No banner configured.")
    writeln("")
  end

  def set_banner(parsed) do
    message = Map.fetch!(parsed.options, :message)
    banner_type = Map.get(parsed.options, :type, "info")
    level = Map.get(parsed.options, :level, "projectCollection")

    body = %{
      "value" => %{
        "message" => message,
        "type" => banner_type,
        "level" => level
      }
    }

    case Client.put(@settings_path, body) do
      {:ok, _} ->
        success("Banner set: \"#{message}\"\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_banner(parsed) do
    case Client.delete(@settings_path) do
      :ok ->
        success("Banner removed.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("No banner to delete.")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end
end
