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

    # Burrito's Zig wrapper passes CLI args via the ADO_ARGS env var
    # (see deps/burrito/src/erlang_launcher.zig) and :init.get_plain_arguments
    # is empty in MIX_ENV=prod releases. AdoCli.BurritoArgs.get_arguments/0
    # falls back to parsing ADO_ARGS with a POSIX-style parser that
    # respects quoted args (e.g. project names like "Employee Management").
    args = AdoCli.BurritoArgs.get_arguments()

    AdoCli.CLI.run(args)

    # Unreachable — CliMate always halts after execution.
    # Return a supervisor spec anyway to satisfy the Application behaviour.
    {:ok, self()}
  end
end
