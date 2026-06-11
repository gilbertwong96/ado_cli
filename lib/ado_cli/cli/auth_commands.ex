defmodule AdoCli.CLI.AuthCommands do
  @moduledoc """
  Authentication commands for Azure DevOps CLI.

    ado_cli login  [--method METHOD] --org ORG [--pat TOKEN]
    ado_cli logout
    ado_cli whoami

  Default login opens your browser for interactive sign-in (like `az login`).
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
          doc: "Auth method: browser (default), pat, device",
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
    opts = parsed.options
    method = Map.get(opts, :method, "browser")
    org = Map.get(opts, :org) || System.get_env("ADO_ORG")

    unless org || method == "browser" do
      halt_error("--org is required for method='#{method}' (or set ADO_ORG env var)")
    end

    set_server(opts)
    dispatch_login(org, opts, method)
  end

  defp set_server(opts) do
    server = Map.get(opts, :server) || System.get_env("ADO_SERVER")
    if server, do: :persistent_term.put({:ado_cli, :server}, server)
  end

  defp dispatch_login(org, _opts, "browser"), do: login_with_browser(org)
  defp dispatch_login(org, opts, "pat"), do: login_with_pat(org, opts)
  defp dispatch_login(org, _opts, "device"), do: login_with_device(org)

  defp dispatch_login(_org, _opts, other),
    do: halt_error("Unknown method '#{other}'. Use 'browser', 'pat', or 'device'.")

  defp login_with_browser(org) do
    case Auth.login_browser(org) do
      {:ok, _org_name} -> halt_success("Done.")
      {:error, reason} -> halt_error("Login failed: #{reason}")
    end
  end

  defp login_with_pat(org, opts) do
    pat = Map.get(opts, :pat) || System.get_env("ADO_PAT")
    unless pat, do: halt_error("--pat is required for method=pat (or set ADO_PAT env var)")

    server =
      try do
        :persistent_term.get({:ado_cli, :server}, nil)
      catch
        :error, :badarg -> nil
      end || System.get_env("ADO_SERVER")

    server_label = if server, do: " (#{server})", else: ""

    case Auth.login_pat(org, pat) do
      {:ok, org_name} ->
        halt_success(
          "Logged in to #{org_name}#{server_label} via Personal Access Token.\nCredentials saved to ~/.ado_cli/config.json"
        )

      {:error, reason} ->
        halt_error("Login failed: #{reason}")
    end
  end

  defp login_with_device(org) do
    case Auth.login_device_code(org) do
      {:ok, _org_name} -> halt_success("Done.")
      {:error, reason} -> halt_error("Login failed: #{reason}")
    end
  end
end
