defmodule AdoCli.CLI.AuthCommands do
  @moduledoc """
  Authentication commands for Azure DevOps CLI.

    ado_cli login  --method METHOD [--org ORG] [--pat TOKEN]
    ado_cli logout
    ado_cli whoami
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.Auth

  @impl true
  def command do
    [
      name: "ado_cli login",
      doc: "Authenticate with Azure DevOps.",
      options: [
        method: [
          type: :string,
          required: true,
          doc: "Auth method: pat, device",
          doc_arg: "METHOD"
        ],
        org: [
          type: :string,
          doc: "Azure DevOps organization / collection name",
          doc_arg: "ORG"
        ],
        server: [
          type: :string,
          doc: "Server URL for self-hosted Azure DevOps Server",
          doc_arg: "URL"
        ],
        pat: [
          type: :string,
          doc: "Personal Access Token (for method=pat)",
          doc_arg: "TOKEN"
        ]
      ],
      execute: &login/1
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  @doc """
  Authenticates with Azure DevOps using the specified method.

  ## Methods

    * `pat` — Personal Access Token (requires `--pat TOKEN`)
    * `device` — Interactive browser-based OAuth device code flow
  """
  def login(parsed) do
    org = parsed.options.org || System.get_env("ADO_ORG")
    unless org, do: halt_error("--org is required (or set ADO_ORG env var)")

    server = parsed.options.server || System.get_env("ADO_SERVER")

    if server do
      current = Application.get_env(:ado_cli, :azure_devops, [])
      Application.put_env(:ado_cli, :azure_devops, Keyword.put(current, :server, server))
    end

    case parsed.options.method do
      "pat" -> login_with_pat(org, parsed.options)
      "device" -> login_with_device(org)
      other -> halt_error("Unknown method '#{other}'. Use 'pat' or 'device'.")
    end
  end

  defp login_with_pat(org, opts) do
    pat = opts.pat || System.get_env("ADO_PAT")
    unless pat, do: halt_error("--pat is required for method=pat (or set ADO_PAT env var)")

    server = Application.get_env(:ado_cli, :azure_devops)[:server]
    server_label = if server, do: " (#{server})", else: ""

    case Auth.login_pat(org, pat) do
      {:ok, org_name} ->
        writeln(success("Logged in to #{org_name}#{server_label} via Personal Access Token."))
        writeln("Credentials saved to ~/.ado_cli/config.json")
        halt_success("")

      {:error, reason} ->
        halt_error("Login failed: #{reason}")
    end
  end

  defp login_with_device(org) do
    case Auth.login_device_code(org) do
      {:ok, _org_name} -> halt_success("")
      {:error, reason} -> halt_error("Login failed: #{reason}")
    end
  end
end
