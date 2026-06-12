defmodule AdoCli.CLI.Logout do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

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
  def run(_parsed) do
    AdoCli.Auth.logout()
    success("Logged out. Credentials removed from ~/.ado_cli/config.json\n")
    halt_success("")
  end
end
