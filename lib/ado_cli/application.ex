defmodule AdoCli.Application do
  @moduledoc """
  Application entry point for Burrito-wrapped binaries.

  Runs the CLI and halts with the appropriate exit code.
  In escript mode, AdoCli.CLI.main/1 is the entry point instead.
  """

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      {Finch, name: AdoCli.Finch, pools: %{default: [size: 5, count: 1]}}
    ]

    {:ok, _pid} = Supervisor.start_link(children, strategy: :one_for_one)

    # Burrito's Zig wrapper passes CLI args via native argv after `-extra`,
    # so :init.get_plain_arguments/0 returns them directly with whitespace
    # preserved (e.g. "Employee Management" stays intact as one token).
    args = Enum.map(:init.get_plain_arguments(), &to_string/1)

    AdoCli.CLI.run(args)

    # Unreachable — CliMate always halts after execution.
    # Return a supervisor spec anyway to satisfy the Application behaviour.
    {:ok, self()}
  end
end
