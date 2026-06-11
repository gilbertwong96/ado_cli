defmodule AdoCli.Application do
  @moduledoc """
  Application entry point for Burrito-wrapped binaries.

  When running as a Burrito binary, this is the :mod callback.
  It starts the Finch supervisor, runs the CLI, and then exits.
  """

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      {Finch, name: AdoCli.Finch, pools: %{default: [size: 5, count: 1]}}
    ]

    {:ok, _pid} = Supervisor.start_link(children, strategy: :one_for_one)

    # Burrito passes CLI args via -extra in the BEAM command line.
    # Use :init.get_plain_arguments() which reads -extra args.
    args = :init.get_plain_arguments() |> Enum.map(&List.to_string/1)
    args = if args == [], do: System.argv(), else: args
    AdoCli.CLI.run(args)

    {:ok, self()}
  end
end
