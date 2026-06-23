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
  alias AdoCli.CLI.Output

  @impl true
  def command do
    [
      name: "ado login",
      doc:
        "Authenticate with Azure DevOps. Default (no --method) opens your browser for interactive OAuth sign-in. Run without any flags — the org is auto-detected from your token. For CI or headless environments, use --method pat with a Personal Access Token. Use --method device to print a code+URL for signing in on any device (org also optional). After login, credentials are stored in ~/.ado_cli/config.json with 0600 permissions.",
      options: [
        method: [
          type: :string,
          doc:
            "Auth method. Valid: browser (default — interactive OAuth, supports AAD and MSA orgs), pat (Personal Access Token; required for CI), device (device code flow; visit URL on any device).",
          doc_arg: "METHOD"
        ],
        org: [
          type: :string,
          doc:
            "Azure DevOps organization name. Optional — auto-detected from the token for browser and device login. Required for PAT login. Can also be set via ADO_ORG env var.",
          doc_arg: "ORG"
        ],
        server: [
          type: :string,
          doc:
            "Server URL for self-hosted Azure DevOps Server (e.g. https://ado.example.com). Cloud users can omit this. Can also be set via ADO_SERVER env var.",
          doc_arg: "URL"
        ],
        pat: [
          type: :string,
          doc:
            "Personal Access Token. Only used with --method pat. Generate at https://dev.azure.com/{org}/_usersSettings/tokens. Required scopes depend on usage: vso.work (work items), vso.code (repos, PRs), vso.project (projects/teams), vso.build (pipelines), vso.release (releases). Use 'Full access' for broadest coverage.",
          doc_arg: "TOKEN"
        ]
      ],
      execute: &run/1
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  @doc """
  Authenticates with Azure DevOps using the specified method.

  This is the public entry point. `login/1` is the same function kept
  for backward compat with older tests.
  """
  def run(parsed), do: login(parsed)

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

    if org == nil and method not in ["browser", "device"] do
      Output.error(
        parsed,
        "validation_error",
        "--org is required for method='#{method}' (or set ADO_ORG env var)",
        details: %{"option" => "--org", "env_var" => "ADO_ORG"}
      )
    else
      set_server(opts)
      dispatch_login(parsed, org, opts, method)
    end
  end

  defp set_server(opts) do
    server = Map.get(opts, :server) || System.get_env("ADO_SERVER")
    if server, do: :persistent_term.put({:ado_cli, :server}, server)
  end

  defp dispatch_login(parsed, org, _opts, "browser"), do: login_with_browser(parsed, org)
  defp dispatch_login(parsed, org, opts, "pat"), do: login_with_pat(parsed, org, opts)
  defp dispatch_login(parsed, org, _opts, "device"), do: login_with_device(parsed, org)

  defp dispatch_login(parsed, _org, _opts, other) do
    Output.error(
      parsed,
      "validation_error",
      "Unknown method '#{other}'. Use 'browser', 'pat', or 'device'.",
      details: %{"valid_methods" => ["browser", "pat", "device"]}
    )
  end

  defp login_with_browser(parsed, org) do
    case Auth.login_browser(org) do
      {:ok, org_name} ->
        login_success(parsed, org_name, "browser", nil)

      {:error, reason} ->
        Output.error(
          parsed,
          "auth_required",
          "Login failed: #{reason}",
          details: %{"reason" => inspect(reason)}
        )
    end
  end

  defp login_with_pat(parsed, org, opts) do
    pat = Map.get(opts, :pat) || System.get_env("ADO_PAT")

    if pat do
      server =
        try do
          :persistent_term.get({:ado_cli, :server}, nil)
        catch
          :error, :badarg -> nil
        end || System.get_env("ADO_SERVER")

      case Auth.login_pat(org, pat) do
        {:ok, org_name} ->
          login_success(parsed, org_name, "pat", server)

        {:error, reason} ->
          Output.error(
            parsed,
            "auth_required",
            "Login failed: #{reason}",
            details: %{
              "reason" => inspect(reason),
              "hint" => "Verify your PAT is valid and has the required scopes."
            }
          )
      end
    else
      Output.error(
        parsed,
        "validation_error",
        "--pat is required for method=pat (or set ADO_PAT env var)",
        details: %{"option" => "--pat", "env_var" => "ADO_PAT"}
      )
    end
  end

  defp login_with_device(parsed, org) do
    case Auth.login_device_code(org) do
      {:ok, org_name} ->
        login_success(parsed, org_name, "device", nil)

      {:error, reason} ->
        Output.error(
          parsed,
          "auth_required",
          "Login failed: #{reason}",
          details: %{"reason" => inspect(reason)}
        )
    end
  end

  defp login_success(parsed, org_name, method, server) do
    server_label = if server, do: " (#{server})", else: ""

    Output.ok(
      parsed,
      %{
        org: org_name,
        method: method,
        server: server,
        credentials_saved_to: "~/.ado_cli/config.json"
      },
      :value,
      fn result ->
        writeln("")

        writeln(
          "  Logged in to #{result.org}#{server_label} via #{String.capitalize(result.method)}."
        )

        writeln("  Credentials saved to #{result.credentials_saved_to}")
      end
    )
  end
end
