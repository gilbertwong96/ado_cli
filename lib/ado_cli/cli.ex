defmodule AdoCli.CLI do
  @moduledoc """
  Main CLI entry point for AdoCli.

  Provides a command-line interface to Azure DevOps using sub-commands:

    ado_cli login --method pat|device --org ORG
    ado_cli logout
    ado_cli whoami
    ado_cli projects list|show|create|update|delete
    ado_cli repos list|show|create|delete|branches
    ado_cli workitems list|show|query|create|update
    ado_cli pipelines list|show|run
    ado_cli prs list|show|create|complete|abandon
    ado_cli releases list|show
  """

  import CliMate.CLI

  alias AdoCli.CLI.AuthCommands
  alias AdoCli.CLI.Logout
  alias AdoCli.CLI.Pipelines
  alias AdoCli.CLI.Projects
  alias AdoCli.CLI.PullRequests
  alias AdoCli.CLI.Releases
  alias AdoCli.CLI.Repos
  alias AdoCli.CLI.Whoami
  alias AdoCli.CLI.WorkItems

  @command [
    name: "ado_cli",
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
      workitems: WorkItems
    ]
  ]

  @doc """
  Escript/Burrito entry point. Starts Finch and delegates to `run/1`.
  """
  def main(args \\ System.argv()) do
    start_finch()
    run(args)
  end

  def command_definition, do: @command

  @doc """
  Parses command-line arguments and dispatches to the appropriate sub-command.

  Applies `--org` and `--pat` overrides to the runtime configuration before
  calling the resolved sub-command's execute closure.
  """
  def run(args) do
    parsed = parse_or_halt!(args, @command)

    org = Map.get(parsed.options, :org) || System.get_env("ADO_ORG")
    pat = Map.get(parsed.options, :pat) || System.get_env("ADO_PAT")
    server = Map.get(parsed.options, :server) || System.get_env("ADO_SERVER")

    if org, do: put_app_env(:org, org)
    if pat, do: put_app_env(:pat, pat)
    if server, do: put_app_env(:server, server)

    if parsed.execute do
      parsed.execute.()
    else
      writeln(format_usage(@command))
      halt_success("")
    end
  end

  defp put_app_env(key, value) do
    current = Application.get_env(:ado_cli, :azure_devops, [])
    Application.put_env(:ado_cli, :azure_devops, Keyword.put(current, key, value))
  end

  defp start_finch do
    unless Process.whereis(AdoCli.Finch) do
      Finch.start_link(name: AdoCli.Finch, pools: %{default: [size: 5, count: 1]})
    end
  end
end
