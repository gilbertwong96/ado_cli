defmodule AdoCli.Auth do
  @moduledoc """
  Authentication provider for Azure DevOps.

  Supports multiple auth methods, tried in priority order:

    1. CLI flags (`--org`, `--pat`) — highest priority, set per-invocation
    2. Environment variables (`ADO_ORG`, `ADO_PAT`) — session-based
    3. Azure CLI token (`az account get-access-token`) — if user ran `az login`
    4. Config file (`~/.ado_cli/config.json`) — persistent, set via `ado_cli login`

  ## Auth Methods

    * `:pat` — Personal Access Token (Basic Auth)
    * `:az_cli` — Azure CLI bearer token (no PAT needed if `az login` is active)
    * `:device_code` — Interactive Microsoft Identity Platform device-code OAuth flow
  """

  alias AdoCli.ConfigFile
  alias CliMate.CLI

  @ado_resource "499b84ac-1321-427f-aa17-267ca6975798"
  @tenant "common"

  @doc """
  Returns `{:ok, org, headers}` with ready-to-use HTTP auth headers,
  or `{:error, :not_configured}` if no auth method is available.
  """
  def resolve_auth do
    # Priority 1: CLI flags (already applied to app env by CLI module)
    # Priority 2: Environment variables
    # Priority 3: Azure CLI token
    # Priority 4: Config file

    org = get_org()
    pat = get_pat()

    cond do
      org && pat ->
        {:ok, org, basic_auth_headers(pat)}

      org && az_cli_available?() ->
        case az_cli_token() do
          {:ok, token} -> {:ok, org, bearer_auth_headers(token)}
          {:error, _} -> try_config_file()
        end

      true ->
        try_config_file()
    end
  end

  @doc """
  Authenticate with a PAT and save to config file.

      ado_cli login --method pat --org myorg --pat xxxxx
  """
  def login_pat(org, pat) do
    :ok = ConfigFile.save(%{org: org, method: "pat", pat: pat})
    set_runtime(org, pat)
    {:ok, org}
  rescue
    e in File.Error -> {:error, "Cannot write config: #{e.reason}"}
  end

  @doc """
  Authenticate via Azure Device Code flow and save to config file.
  Opens a browser for the user to complete authentication.

      ado_cli login --method device --org myorg
  """
  def login_device_code(org) do
    case request_device_code(org) do
      {:ok, device_code, user_code, verification_uri, interval} ->
        CLI.writeln("")
        CLI.writeln("To sign in, use a web browser to open:")
        CLI.writeln("  #{CLI.color(verification_uri, :cyan)}")
        CLI.writeln("")
        CLI.writeln("And enter the code: #{CLI.color(user_code, :green)}")
        CLI.writeln("")

        case poll_for_token(org, device_code, interval, 0) do
          {:ok, token} ->
            ConfigFile.save(%{org: org, method: "device_code", token: token})
            CLI.writeln(CLI.success("Authenticated successfully as #{org}."))
            {:ok, org}

          {:error, :timeout} ->
            {:error, "Authentication timed out. Please try again."}

          {:error, reason} ->
            {:error, reason}
        end

      {:error, reason} ->
        {:error, reason}
    end
  end

  @doc """
  Remove stored credentials.
  """
  def logout do
    ConfigFile.delete()
    :ok
  end

  @doc """
  Show current auth status.
  """
  def status do
    config = ConfigFile.load()
    runtime_org = get_org()
    runtime_pat = get_pat()
    server = Application.get_env(:ado_cli, :azure_devops)[:server]

    %{
      configured: config != nil,
      method: config[:method] || (runtime_pat && "pat"),
      org: runtime_org || config[:org],
      server: server,
      az_cli_available: az_cli_available?()
    }
  end

  # ── Internal ──────────────────────────────────────────────────────────

  defp get_org,
    do: Application.get_env(:ado_cli, :azure_devops)[:org] || System.get_env("ADO_ORG")

  defp get_pat,
    do: Application.get_env(:ado_cli, :azure_devops)[:pat] || System.get_env("ADO_PAT")

  defp set_runtime(org, pat) do
    Application.put_env(:ado_cli, :azure_devops, :org, org)
    Application.put_env(:ado_cli, :azure_devops, :pat, pat)
  end

  defp basic_auth_headers(pat), do: [{"Authorization", "Basic #{Base.encode64(":#{pat}")}"}]
  defp bearer_auth_headers(token), do: [{"Authorization", "Bearer #{token}"}]

  defp try_config_file do
    case ConfigFile.load() do
      nil ->
        {:error, :not_configured}

      config ->
        org = config[:org]

        case config[:method] do
          "pat" ->
            set_runtime(org, config[:pat])
            {:ok, org, basic_auth_headers(config[:pat])}

          "device_code" ->
            {:ok, org, bearer_auth_headers(config[:token])}

          _ ->
            {:error, :not_configured}
        end
    end
  end

  # ── Azure CLI token ──────────────────────────────────────────────────

  defp az_cli_available? do
    System.find_executable("az") != nil
  end

  defp az_cli_token do
    args =
      ~w(account get-access-token --resource) ++
        [@ado_resource, "--query", "accessToken", "-o", "tsv"]

    case System.cmd("az", args, env: []) do
      {token, 0} ->
        token = String.trim(token)
        if token == "", do: {:error, :no_token}, else: {:ok, token}

      {_, _} ->
        {:error, :az_cli_failed}
    end
  end

  # ── Device Code flow (Microsoft Identity Platform) ────────────────────

  defp request_device_code(_org) do
    body =
      URI.encode_query(%{
        client_id: @ado_resource,
        resource: @ado_resource
      })

    url = "https://login.microsoftonline.com/#{@tenant}/oauth2/devicecode"

    headers = [{"Content-Type", "application/x-www-form-urlencoded"}]
    request = Finch.build(:post, url, headers, body)

    with {:ok, %Finch.Response{status: 200, body: resp_body}} <-
           Finch.request(request, AdoCli.Finch),
         {:ok,
          %{"device_code" => dc, "user_code" => uc, "verification_uri" => vu, "interval" => iv}} <-
           JSON.decode(resp_body) do
      {:ok, dc, uc, vu, iv}
    else
      {:ok, %Finch.Response{status: status, body: body}} ->
        {:error, "Device code request failed (HTTP #{status}): #{safe_decode(body)}"}

      {:error, reason} ->
        {:error, "Device code request failed: #{inspect(reason)}"}
    end
  end

  defp poll_for_token(_org, _device_code, _interval, attempts) when attempts > 120,
    do: {:error, :timeout}

  defp poll_for_token(_org, device_code, interval, attempts) do
    body =
      URI.encode_query(%{
        grant_type: "urn:ietf:params:oauth:grant-type:device_code",
        client_id: @ado_resource,
        device_code: device_code
      })

    url = "https://login.microsoftonline.com/#{@tenant}/oauth2/token"
    headers = [{"Content-Type", "application/x-www-form-urlencoded"}]
    request = Finch.build(:post, url, headers, body)

    case Finch.request(request, AdoCli.Finch) do
      {:ok, %Finch.Response{status: 200, body: resp_body}} ->
        extract_token(resp_body)

      {:ok, %Finch.Response{status: 400, body: resp_body}} ->
        handle_token_error(resp_body, device_code, interval, attempts)

      _ ->
        {:error, "Token polling failed"}
    end
  end

  defp extract_token(resp_body) do
    case JSON.decode(resp_body) do
      {:ok, %{"access_token" => token}} -> {:ok, token}
      _ -> {:error, "Invalid token response"}
    end
  end

  defp handle_token_error(resp_body, device_code, interval, attempts) do
    case JSON.decode(resp_body) do
      {:ok, %{"error" => "authorization_pending"}} ->
        Process.sleep(interval * 1000)
        poll_for_token(nil, device_code, interval, attempts + 1)

      {:ok, %{"error" => "slow_down"}} ->
        Process.sleep((interval + 5) * 1000)
        poll_for_token(nil, device_code, interval + 5, attempts + 1)

      {:ok, %{"error" => "authorization_declined"}} ->
        {:error, "Authorization declined by user."}

      {:ok, %{"error" => "expired_token"}} ->
        {:error, "Device code expired. Please try again."}

      _ ->
        {:error, "Unknown token error"}
    end
  end

  defp safe_decode(body) when is_binary(body) do
    case JSON.decode(body) do
      {:ok, %{"error_description" => desc}} -> desc
      {:ok, %{"message" => msg}} -> msg
      {:ok, _} -> "Unknown error"
      {:error, _} -> body
    end
  end

  defp safe_decode(body), do: inspect(body)
end
