defmodule AdoCli.CLI.Version do
  @moduledoc """
  Print the ado version and exit.

      ado version            # plain text: "ado 0.1.0"
      ado version --json     # structured: {"ok": true, "version": "0.1.0"}
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  @impl true
  def command do
    [
      name: "ado version",
      doc: "Print the ado version and exit.",
      options: [
        json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
      ],
      execute: &run/1
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def run(parsed) do
    json? = Map.get(parsed.options, :json, false)
    version = AdoCli.Version.current()

    if json? do
      # Use IO.puts (not writeln) to avoid ANSI color codes from
      # halt_success that would pollute the JSON envelope.
      IO.puts(JSON.encode!(%{ok: true, version: version}))
    else
      IO.puts("ado #{version}")
    end

    halt(0)
  end
end
