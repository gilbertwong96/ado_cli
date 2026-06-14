defmodule AdoCli.CLI.Logout do
  @moduledoc false

  @behaviour CliMate.CLI.Command

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
