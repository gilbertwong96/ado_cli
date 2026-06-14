defmodule AdoCli.CLI.Whoami do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.Auth
  alias AdoCli.CLI.Output

  @impl true
  def command do
    [
      name: "ado whoami",
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
  def run(parsed) do
    status = Auth.status()
    json? = Map.get(parsed.options || %{}, :json, false)

    if json? do
      Output.ok(
        parsed,
        %{
          configured: status.configured,
          org: status.org,
          server: status.server || "dev.azure.com",
          method: status.method,
          config_file: config_file_path(),
          authenticated: status.configured or status.org != nil
        }
      )
    else
      writeln("")

      if status.configured or status.org do
        print_authenticated(status)
      else
        print_unauthenticated(status)
      end

      writeln("")
      halt_success("")
    end
  end

  defp print_authenticated(status) do
    writeln("  Organization: #{status.org || "(not set)"}")
    writeln("  Server:       #{status.server || "dev.azure.com (cloud)"}")
    writeln("  Auth Method:  #{status.method || "none"}")
    writeln("  Config File:  #{config_file_path()}")
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

  defp config_file_path do
    AdoCli.ConfigFile.config_path()
  end
end
