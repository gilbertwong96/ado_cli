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
    ado pipelines list|show|run|vars
    ado prs list|show|create|complete|abandon|comments
    ado releases list|show
    ado teams list|show|create|update|delete|members
    ado users list|show|add|remove
    ado iterations list|show|create|update|delete
    ado wikis list|show|pages
    ado connections list|show
    ado security groups list|show|create|delete|members
    ado extensions list|show|install|uninstall|enable|disable
    ado agent-pools list|show|queues
  """

  import CliMate.CLI

  alias AdoCli.CLI.AgentPools
  alias AdoCli.CLI.Areas
  alias AdoCli.CLI.AuthCommands
  alias AdoCli.CLI.Banners
  alias AdoCli.CLI.BranchPolicies
  alias AdoCli.CLI.Builds
  alias AdoCli.CLI.CI
  alias AdoCli.CLI.Connections
  alias AdoCli.CLI.Extensions
  alias AdoCli.CLI.Folders
  alias AdoCli.CLI.Imports
  alias AdoCli.CLI.Iterations
  alias AdoCli.CLI.Logout
  alias AdoCli.CLI.Packages
  alias AdoCli.CLI.Pipelines
  alias AdoCli.CLI.Projects
  alias AdoCli.CLI.PullRequests
  alias AdoCli.CLI.Releases
  alias AdoCli.CLI.Repos
  alias AdoCli.CLI.RunArtifacts
  alias AdoCli.CLI.Schema
  alias AdoCli.CLI.Security
  alias AdoCli.CLI.Skills
  alias AdoCli.CLI.Teams
  alias AdoCli.CLI.Users
  alias AdoCli.CLI.Version
  alias AdoCli.CLI.Whoami
  alias AdoCli.CLI.Wikis
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
      version: Version,
      ci: CI,
      schema: Schema,
      "agent-pools": AgentPools,
      areas: Areas,
      connections: Connections,
      extensions: Extensions,
      iterations: Iterations,
      imports: Imports,
      pipelines: Pipelines,
      "pipelines-builds": Builds,
      "pipelines-folders": Folders,
      "pipelines-artifacts": RunArtifacts,
      packages: Packages,
      projects: Projects,
      prs: PullRequests,
      releases: Releases,
      repos: Repos,
      "branch-policies": BranchPolicies,
      banners: Banners,
      security: Security,
      skills: Skills,
      teams: Teams,
      users: Users,
      wikis: Wikis,
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

  def command_definition, do: @command

  @doc """
  Parses command-line arguments and dispatches to the appropriate sub-command.

  Applies `--org` and `--pat` overrides to the runtime configuration before
  calling the resolved sub-command's execute closure.
  """
  def run(args) do
    # Short-circuit `--version` before CliMate sees it. Otherwise it
    # would dump the full help text. This is the standard CLI
    # convention (git, npm, cargo all do this).
    #
    # Use System.halt/1 instead of CliMate's halt/1 because the
    # latter requires a configured shell, which the test harness
    # doesn't have set up. System.halt/1 works in all contexts.
    if "--version" in args do
      IO.puts("ado #{AdoCli.Version.current()}")
      System.halt(0)
    end

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
    # OTP applications must be started manually in escript mode.
    # Mint's SSL transport calls Application.spec(:ssl, :vsn) which
    # returns nil if :ssl isn't started.
    Application.ensure_all_started(:ssl)

    Finch.start_link(name: AdoCli.Finch, pools: %{default: [size: 5, count: 1]})
  end
end
