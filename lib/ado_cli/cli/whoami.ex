defmodule AdoCli.CLI.Whoami do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  @impl true
  def command do
    [
      name: "ado_cli whoami",
      doc: "Show current authentication status.",
      execute: &run/1
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  @doc """
  Shows the current authentication status: organization, auth method,
  config file path, and Azure CLI availability.
  """
  def run(_parsed) do
    status = AdoCli.Auth.status()
    writeln("")

    if status.configured or status.org do
      print_authenticated(status)
    else
      print_unauthenticated(status)
    end

    writeln("")
    halt_success("")
  end

  defp print_authenticated(status) do
    writeln("  Organization: #{status.org || "(not set)"}")
    writeln("  Server:       #{status.server || "dev.azure.com (cloud)"}")
    writeln("  Auth Method:  #{status.method || "none"}")
    writeln("  Config File:  #{config_file_label(status)}")
    writeln("  Az CLI:       #{az_cli_label(status)}")
  end

  defp print_unauthenticated(status) do
    writeln("  Server:       #{status.server || "dev.azure.com (cloud)"}")
    writeln("  Not authenticated.")
    writeln("")
    writeln("  Authenticate with:")
    writeln("    ado_cli login --method pat --org ORG --pat TOKEN")
    writeln("    ado_cli login --method device --org ORG")
    writeln("  Or set environment variables: ADO_ORG + ADO_PAT")
  end

  defp config_file_label(status),
    do: if(status.configured, do: "~/.ado_cli/config.json", else: "(none)")

  defp az_cli_label(status),
    do: if(status.az_cli_available, do: "available", else: "not available")
end
