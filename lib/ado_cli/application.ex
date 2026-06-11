defmodule AdoCli.Application do
  @moduledoc """
  Application entry point for Burrito-wrapped binaries.
  Uses Burrito.Util.Args.argv() — the recommended best practice.
  """

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      {Finch, name: AdoCli.Finch, pools: %{default: [size: 5, count: 1]}}
    ]

    {:ok, _pid} = Supervisor.start_link(children, strategy: :one_for_one)

    # Best practice: use Burrito.Util.Args.argv()
    args =
      if Code.ensure_loaded?(Burrito.Util.Args) do
        Burrito.Util.Args.argv()
      else
        System.argv()
      end

    AdoCli.CLI.run(args)

    {:ok, self()}
  end
end
