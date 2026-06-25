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
  alias AdoCli.CLI.Completion
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
  alias AdoCli.CLI.TestCoverage
  alias AdoCli.CLI.TestResults
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
      completion: Completion,
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
      "test-results": TestResults,
      "test-coverage": TestCoverage,
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

    # Burrito's Zig wrapper passes CLI args via native argv after `-extra`,
    # so :init.get_plain_arguments/0 returns them directly with whitespace
    # preserved. System.argv() in escript mode also has the args.
    cli_args = args || System.argv() || plain_args()

    run(cli_args)
  end

  defp plain_args do
    Enum.map(:init.get_plain_arguments(), &to_string/1)
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

    args = join_multivalue_opts(args)
    parsed = parse_or_halt!(args, @command)

    # Apply global options to runtime env for auth resolution
    apply_global_opts(parsed.options)

    parsed.execute.()
  end

  # String options that should consume all subsequent non-flag tokens.
  # Without this, `--content foo bar baz` would parse as content=foo
  # and bar/baz as extra positional args, which CliMate rejects.
  # This is the same convention as `curl --data`, `tar -cf`, etc.
  @multivalue_opts ~w(--content --message --body --description --reason --summary --text)

  def join_multivalue_opts(args) do
    join_multivalue_opts(args, [])
  end

  defp join_multivalue_opts([], result), do: Enum.reverse(result)

  defp join_multivalue_opts([arg | rest], result) do
    if arg in @multivalue_opts do
      {joined, remaining} = take_multivalue_tokens(rest)
      join_multivalue_opts(remaining, ["#{arg}=#{joined}" | result])
    else
      join_multivalue_opts(rest, [arg | result])
    end
  end

  # Collects consecutive non-flag tokens after a multivalue flag like
  # --content, stopping at the next flag. Returns the joined string
  # and the leftover args (so a subsequent --status can still parse).
  defp take_multivalue_tokens(tokens) do
    take_multivalue_tokens(tokens, [])
  end

  defp take_multivalue_tokens([arg | rest] = tokens, acc) do
    if String.starts_with?(arg, "-") and arg != "-" do
      {join_words(Enum.reverse(acc)), tokens}
    else
      take_multivalue_tokens(rest, [arg | acc])
    end
  end

  defp take_multivalue_tokens([], acc), do: {join_words(Enum.reverse(acc)), []}

  defp join_words(words), do: Enum.join(words, " ")

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
