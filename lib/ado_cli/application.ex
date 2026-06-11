defmodule AdoCli.Application do
  @moduledoc """
  Application entry point for Burrito-wrapped binaries.

  When running as a Burrito binary, this is the :mod callback.
  It starts the Finch supervisor, runs the CLI with the argv from Burrito,
  and then halts.
  """

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      {Finch, name: AdoCli.Finch, pools: %{default: [size: 5, count: 1]}}
    ]

    {:ok, _pid} = Supervisor.start_link(children, strategy: :one_for_one)

    raw_args = System.argv()

    # Burrito wrapper passes CLI args via a special mechanism.
    # Try Burrito.Util.Args first, fall back to filtered System.argv()
    args =
      if Code.ensure_loaded?(Burrito.Util.Args) do
        Burrito.Util.Args.argv()
      else
        raw_args
      end

    # Filter out BEAM args (anything before a -- separator)
    args =
      case Enum.split_while(args, &(&1 != "--")) do
        {_beam_args, ["--" | cli_args]} -> cli_args
        {cli_args, []} -> cli_args
      end

    AdoCli.CLI.run(args)

    # Return supervisor pid to satisfy Application behaviour
    {:ok, self()}
  end
end
