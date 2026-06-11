defmodule AdoCli.Application do
  @moduledoc """
  Application entry point for Burrito-wrapped binaries.
  """

  use Application

  @impl true
  def start(_type, _args) do
    # Write a marker file we can find after the binary exits
    File.mkdir_p!(Path.join(System.tmp_dir!(), "ado_test"))
    File.write!(Path.join(System.tmp_dir!(), "ado_test", "app_started"), "yes")

    children = [
      {Finch, name: AdoCli.Finch, pools: %{default: [size: 5, count: 1]}}
    ]

    {:ok, _pid} = Supervisor.start_link(children, strategy: :one_for_one)

    args =
      case System.get_env("ADO_ARGS") do
        nil -> System.argv()
        "" -> System.argv()
        str -> String.split(str, " ")
      end

    File.write!(Path.join(System.tmp_dir!(), "ado_test", "cli_args"), inspect(args))
    AdoCli.CLI.run(args)

    {:ok, self()}
  end
end
