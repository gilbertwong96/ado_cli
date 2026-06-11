defmodule AdoCli.CLI do
  @moduledoc """
  Main CLI entry point for AdoCli.

  Provides a command-line interface to Azure DevOps using sub-commands:

    ado login --method pat|device --org ORG
    ado logout
    ado whoami
    ado projects list|show|create|update|delete
    ado repos list|show|create|delete|branches
    ado workitems list|show|query|create|update
    ado pipelines list|show|run
    ado prs list|show|create|complete|abandon
    ado releases list|show
  """

  import CliMate.CLI

  alias AdoCli.CLI.AuthCommands
  alias AdoCli.CLI.Logout
  alias AdoCli.CLI.Pipelines
  alias AdoCli.CLI.Projects
  alias AdoCli.CLI.PullRequests
  alias AdoCli.CLI.Releases
  alias AdoCli.CLI.Repos
  alias AdoCli.CLI.Skills
  alias AdoCli.CLI.Whoami
  alias AdoCli.CLI.WorkItems

  @command [
    name: "ado",
    version: "0.1.0",
    doc:
      "Azure DevOps CLI - Manage Azure DevOps projects, repos, work items, and pipelines from the terminal.",
    options: [
      org: [
        type: :string,
        short: :o,
        doc: "Azure DevOps organization name (or set ADO_ORG env var)",
        doc_arg: "ORG"
      ],
      pat: [
        type: :string,
        short: :t,
        doc: "Personal Access Token (or set ADO_PAT env var)",
        doc_arg: "TOKEN"
      ],
      server: [
        type: :string,
        short: :s,
        doc: "Azure DevOps Server URL for self-hosted (or set ADO_SERVER env var)",
        doc_arg: "URL"
      ],
      verbose: [type: :boolean, short: :v, default: false, doc: "Enable verbose output"],
      json: [type: :boolean, default: false, doc: "Output raw JSON"]
    ],
    subcommands: [
      login: AuthCommands,
      logout: Logout,
      whoami: Whoami,
      pipelines: Pipelines,
      projects: Projects,
      prs: PullRequests,
      releases: Releases,
      repos: Repos,
      skills: Skills,
      workitems: WorkItems
    ]
  ]

  @doc """
  Escript/Burrito entry point. Starts Finch and delegates to `run/1`.
  """
  def main(args \\ nil) do
    start_finch()

    # Burrito passes CLI args via ADO_ARGS env var (set by Zig wrapper).
    # System.argv() in Burrito mode includes BEAM flags; prefer env var.
    cli_args =
      case System.get_env("ADO_ARGS") do
        nil -> args || System.argv()
        "" -> args || System.argv()
        str -> String.split(str, " ")
      end

    run(cli_args)
  end

  # Filter BEAM flags from argv (look for -extra separator)
  defp clean_args(argv) do
    case Enum.split_while(argv, &(&1 != "-extra")) do
      {_beam, ["-extra" | cli]} -> cli
      {all, []} -> all
    end
  end

  def command_definition, do: @command

  @doc """
  Parses command-line arguments and dispatches to the appropriate sub-command.

  Applies `--org` and `--pat` overrides to the runtime configuration before
  calling the resolved sub-command's execute closure.
  """
  def run(args) do
    parsed = parse_or_halt!(args, @command)

    # Apply global options to runtime env for auth resolution
    apply_global_opts(parsed.options)

    parsed.execute.()
  end

  defp apply_global_opts(opts) do
    if org = opts[:org], do: :persistent_term.put({:ado_cli, :org}, org)
    if pat = opts[:pat], do: :persistent_term.put({:ado_cli, :pat}, pat)
    if server = opts[:server], do: :persistent_term.put({:ado_cli, :server}, server)
  end

  defp start_finch do
    Finch.start_link(name: AdoCli.Finch, pools: %{default: [size: 5, count: 1]})
  end
end
