defmodule AdoCli.CLI.Logout do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  # reach: '3 modules expose the same 3 public callbacks' — correct.
  # Logout, Version, Whoami all implement `@behaviour CliMate.CLI.Command`
  # (command/0, execute/1, run/1). They are NOT interchangeable
  # modules and shouldn't be unified behind a custom behaviour;
  # the CliMate behaviour IS the shared contract.

  import CliMate.CLI

  alias AdoCli.CLI.Output

  @impl true
  def command do
    [
      name: "ado logout",
      doc: "Remove stored credentials.",
      execute: &run/1
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  @doc """
  Removes stored credentials from `~/.ado_cli/config.json`.
  """
  def run(parsed) do
    AdoCli.Auth.logout()

    if Map.get(parsed.options || %{}, :json, false) do
      Output.ok_message(parsed, "Logged out. Credentials removed from ~/.ado_cli/config.json")
    else
      success("Logged out. Credentials removed from ~/.ado_cli/config.json\n")
      halt_success("")
    end
  end
end
