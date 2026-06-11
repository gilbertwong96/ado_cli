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

    # Burrito passes CLI args via ADO_ARGS env var (set by Zig wrapper).
    # Falls back to System.argv() for escript/release boot.
    args =
      case System.get_env("ADO_ARGS") do
        nil -> System.argv()
        "" -> System.argv()
        str -> String.split(str, " ")
      end

    # CliMate.CLI run/1 handles exit codes internally via System.halt(0)/halt(1).
    # The exit code propagates to the Zig wrapper through the OS process exit.
    AdoCli.CLI.run(args)

    # Unreachable — CliMate always halts after execution.
    # Return a supervisor spec anyway to satisfy the Application behaviour.
    {:ok, self()}
  end
end
