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
    * `:browser` — Browser-based OAuth 2.0 Authorization Code + PKCE

  ## MSA (personal account) support

  The Azure DevOps resource (`499b84ac-…`) blocks MSAs at the AAD sign-in page.
  To work around this, the OAuth flows authenticate to Azure Resource Manager
  (ARM) first — which accepts MSAs — then exchange the ARM refresh token for
  a DevOps access token via a second token call.

  Override the OAuth client ID via `ADO_OAUTH_CLIENT_ID` env var.
  """

  alias AdoCli.ConfigFile
  alias CliMate.CLI

  # Azure DevOps resource ID. Correct value for the OAuth `resource` field.
  @ado_resource "499b84ac-1321-427f-aa17-267ca6975798"

  # ARM resource. We authenticate here first because ARM accepts MSAs,
  # then exchange the refresh token for a DevOps token.
  @arm_resource "https://management.core.windows.net"

  # OAuth client_id — the Azure CLI public client (same one `az login` uses).
  # Accepts work/school AND personal Microsoft accounts. Override via env var.
  @ado_client_id System.get_env("ADO_OAUTH_CLIENT_ID", "04b07795-8ddb-461a-bbee-02f9e1bf7b46")

  @tenant "organizations"

  @redirect_uri "http://localhost"

  @doc """
  Returns `{:ok, org, headers}` with ready-to-use HTTP auth headers,
  or `{:error, :not_configured}` if no auth method is available.
  """
  def resolve_auth do
    org = get_org()
    pat = get_pat()

    if org && pat do
      {:ok, org, basic_auth_headers(pat)}
    else
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
          {:ok, _arm_token, refresh_token} ->
            exchange_and_save_device(org, refresh_token)

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
  Authenticate via browser-based OAuth 2.0 Authorization Code flow with PKCE.
  Opens the system browser for the user to sign in, then captures the
  redirect on a local HTTP server to exchange the code for a token.

  This is the default login method (no `--method` flag needed).
  `org` is optional — if omitted, the token is saved without an org
  and you can set it later via `ADO_ORG` or `--org` on commands.

      ado_cli login               # browser login, no org needed
      ado_cli login --org myorg   # browser login with org hint
  """
  def login_browser(org) do
    port = find_free_port()
    state = generate_state()
    code_verifier = generate_code_verifier()
    code_challenge = generate_code_challenge(code_verifier)
    redirect = "#{@redirect_uri}:#{port}"

    auth_url = build_authorize_url(redirect, code_challenge, state)

    CLI.writeln("")
    org_hint = if org, do: " to sign in to #{org}", else: ""
    CLI.writeln("Opening browser#{org_hint}...")
    CLI.write("  ")
    CLI.writeln(CLI.color(auth_url, :cyan))
    CLI.writeln("")

    open_browser(auth_url)

    case listen_for_code(port) do
      {:ok, code, ^state} -> handle_auth_code(code, redirect, code_verifier, org)
      {:ok, _code, _mismatched_state} -> {:error, "State mismatch — possible CSRF attack."}
      {:error, reason} -> {:error, reason}
    end
  end

  defp handle_auth_code(code, redirect, code_verifier, org) do
    case exchange_code_for_arm(code, redirect, code_verifier) do
      {:ok, _arm_token, refresh_token} ->
        case exchange_refresh_for_devops(refresh_token, org) do
          {:ok, devops_token} -> save_browser_token(devops_token, org)
          {:error, reason} -> {:error, reason}
        end

      {:error, reason} ->
        {:error, reason}
    end
  end

  # Exchange auth code for ARM access token + refresh token.
  defp exchange_code_for_arm(code, redirect_uri, code_verifier) do
    body =
      URI.encode_query(%{
        client_id: @ado_client_id,
        grant_type: "authorization_code",
        code: code,
        redirect_uri: redirect_uri,
        code_verifier: code_verifier
      })

    url = "https://login.microsoftonline.com/#{@tenant}/oauth2/v2.0/token"
    headers = [{"Content-Type", "application/x-www-form-urlencoded"}]
    request = Finch.build(:post, url, headers, body)

    case Finch.request(request, AdoCli.Finch) do
      {:ok, %Finch.Response{status: 200, body: resp_body}} ->
        case JSON.decode(resp_body) do
          {:ok, %{"access_token" => access, "refresh_token" => refresh}} ->
            {:ok, access, refresh}

          {:ok, %{"access_token" => access}} ->
            {:ok, access, nil}

          _ ->
            {:error, "Invalid ARM token response"}
        end

      {:ok, %Finch.Response{status: status, body: resp_body}} ->
        {:error, "ARM token exchange failed (HTTP #{status}): #{safe_decode(resp_body)}"}

      {:error, reason} ->
        {:error, "ARM token exchange failed: #{inspect(reason)}"}
    end
  end

  # Exchange ARM refresh token for a DevOps access token.
  # Tries: organizations (default issuer) → consumers (MSA fallback).
  # We skip discover_devops_tenant because the refresh token was issued
  # from "organizations" and can't reliably be exchanged at a different tenant.
  defp exchange_refresh_for_devops(nil, _org),
    do: {:error, "No refresh token available for DevOps exchange"}

  defp exchange_refresh_for_devops(refresh_token, _org) do
    try_tenants(refresh_token, [@tenant, "consumers"])
  end

  defp try_tenants(_refresh_token, []),
    do: {:error, "DevOps token exchange failed with all tenants"}

  defp try_tenants(refresh_token, [tenant | rest]) do
    case do_exchange_refresh_token(refresh_token, tenant) do
      {:ok, _token} = ok -> ok
      {:error, _reason} -> try_tenants(refresh_token, rest)
    end
  end

  defp save_browser_token(token, org) do
    org = org || auto_detect_org(token)
    config = %{"method" => "browser", "token" => token}
    config = if org, do: Map.put(config, "org", org), else: config
    ConfigFile.save(config)

    if org do
      CLI.success("Authenticated successfully as #{org}.\n")
    else
      CLI.success("Authenticated successfully.\n")
      CLI.writeln("No Azure DevOps organizations were found for this account.")
      CLI.writeln("Set your org with: export ADO_ORG=<your-org>")
    end

    {:ok, org}
  end

  # Try to auto-detect the org by listing accounts the user has access to
  # with the freshly obtained token. Used when `ado login` is called without
  # an explicit --org argument.
  defp auto_detect_org(token) do
    case list_accounts(token) do
      {:ok, []} ->
        nil

      {:ok, [only]} ->
        CLI.writeln("Detected org: #{only}")
        only

      {:ok, accounts} ->
        names = Enum.map_join(accounts, ", ", & &1)
        CLI.writeln("Multiple organizations found: #{names}")
        CLI.writeln("Re-run with --org <name> to pick one.")
        nil

      {:error, _} ->
        nil
    end
  end

  defp list_accounts(token) do
    url = "https://app.vssps.visualstudio.com/_apis/accounts"
    headers = [{"Authorization", "Bearer #{token}"}]
    request = Finch.build(:get, url, headers)

    with {:ok, %Finch.Response{status: 200, body: body}} <- Finch.request(request, AdoCli.Finch),
         {:ok, accounts} <- parse_accounts_body(body) do
      {:ok, extract_account_names(accounts)}
    else
      _ -> {:error, "Failed to list accounts"}
    end
  end

  # The accounts API returns a top-level array, not the standard
  # {value: [...]} wrapper used by other DevOps endpoints.
  defp parse_accounts_body(body) do
    case JSON.decode(body) do
      {:ok, %{"value" => list}} when is_list(list) -> {:ok, list}
      {:ok, list} when is_list(list) -> {:ok, list}
      _ -> :error
    end
  end

  defp extract_account_names(accounts) do
    accounts
    |> Enum.map(fn a -> a["AccountName"] || a["accountName"] || a["accountUri"] || "" end)
    |> Enum.reject(&(&1 == ""))
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
    server = safe_get_env(:server) || config[:server]

    %{
      configured: config != nil,
      method: config["method"] || (runtime_pat && "pat"),
      org: runtime_org || config["org"],
      server: server || config["server"]
    }
  end

  # ── Internal ──────────────────────────────────────────────────────────

  defp set_runtime(_org, _pat), do: :ok

  defp get_org do
    cli_org = safe_get_env(:org)
    if cli_org, do: cli_org, else: config_org()
  end

  defp get_pat do
    cli_pat = safe_get_env(:pat)
    if cli_pat, do: cli_pat, else: config_pat()
  end

  defp config_org do
    case ConfigFile.load() do
      %{"org" => org} when is_binary(org) and org != "" -> org
      _ -> nil
    end
  end

  defp config_pat do
    case ConfigFile.load() do
      %{"pat" => pat} -> pat
      _ -> nil
    end
  end

  defp safe_get_env(key) do
    get_pt(key) || get_app(key) || get_env(key)
  end

  defp get_pt(key), do: safe_call(fn -> :persistent_term.get({:ado_cli, key}) end)
  defp get_app(key), do: safe_call(fn -> Application.get_env(:ado_cli, :azure_devops)[key] end)

  defp get_env(:org), do: System.get_env("ADO_ORG")
  defp get_env(:pat), do: System.get_env("ADO_PAT")
  defp get_env(:server), do: System.get_env("ADO_SERVER")
  defp get_env(_), do: nil

  defp safe_call(fun) do
    fun.()
  rescue
    _ -> nil
  end

  defp basic_auth_headers(pat), do: [{"Authorization", "Basic #{Base.encode64(":#{pat}")}"}]
  defp bearer_auth_headers(token), do: [{"Authorization", "Bearer #{token}"}]

  defp try_config_file do
    case ConfigFile.load() do
      nil ->
        {:error, :not_configured}

      config ->
        org = config["org"]

        case config["method"] do
          "pat" ->
            set_runtime(org, config["pat"])
            {:ok, org, basic_auth_headers(config["pat"])}

          "device_code" ->
            {:ok, org, bearer_auth_headers(config["token"])}

          "browser" ->
            {:ok, org, bearer_auth_headers(config["token"])}

          _ ->
            {:error, :not_configured}
        end
    end
  end

  # ── Device Code flow (Microsoft Identity Platform) ────────────────────

  defp request_device_code(_org) do
    body =
      URI.encode_query(%{
        client_id: @ado_client_id,
        resource: @arm_resource
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
        client_id: @ado_client_id,
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

  # Returns {:ok, access_token, refresh_token} or {:ok, access_token, nil}
  defp extract_token(resp_body) do
    case JSON.decode(resp_body) do
      {:ok, %{"access_token" => access, "refresh_token" => refresh}} ->
        {:ok, access, refresh}

      {:ok, %{"access_token" => access}} ->
        {:ok, access, nil}

      _ ->
        {:error, "Invalid token response"}
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

  # ── ARM → DevOps token exchange ─────────────────────────────────────

  defp do_exchange_refresh_token(refresh_token, tenant) do
    # Use the v1.0 OAuth endpoint with 'resource' parameter, matching
    # what the Azure CLI uses internally (profile.get_raw_token).
    # The v1.0 endpoint handles cross-tenant/cross-resource exchange
    # correctly for both AAD and MSA accounts.
    body =
      URI.encode_query(%{
        client_id: @ado_client_id,
        grant_type: "refresh_token",
        refresh_token: refresh_token,
        resource: @ado_resource
      })

    url = "https://login.microsoftonline.com/#{tenant}/oauth2/token"
    headers = [{"Content-Type", "application/x-www-form-urlencoded"}]
    request = Finch.build(:post, url, headers, body)

    case Finch.request(request, AdoCli.Finch) do
      {:ok, %Finch.Response{status: 200, body: resp_body}} ->
        case JSON.decode(resp_body) do
          {:ok, %{"access_token" => token}} -> {:ok, token}
          _ -> {:error, "Invalid DevOps token response"}
        end

      {:ok, %Finch.Response{status: status, body: resp_body}} ->
        {:error, "DevOps token exchange failed (HTTP #{status}): #{safe_decode(resp_body)}"}

      {:error, reason} ->
        {:error, "DevOps token exchange failed: #{inspect(reason)}"}
    end
  end

  defp exchange_and_save_device(org, refresh_token) do
    tenants = Enum.uniq([@tenant, "consumers"])

    case try_tenants(refresh_token, tenants) do
      {:ok, devops_token} ->
        ConfigFile.save(%{org: org, method: "device_code", token: devops_token})
        CLI.success("Authenticated successfully as #{org}.\n")
        {:ok, org}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # ── Browser OAuth 2.0 Authorization Code + PKCE flow ────────────────

  defp generate_state do
    random = :crypto.strong_rand_bytes(16)
    Base.url_encode64(random, padding: false)
  end

  defp generate_code_verifier do
    random = :crypto.strong_rand_bytes(32)
    Base.url_encode64(random, padding: false)
  end

  defp generate_code_challenge(verifier) do
    hash = :crypto.hash(:sha256, verifier)
    Base.url_encode64(hash, padding: false)
  end

  defp generate_nonce do
    random = :crypto.strong_rand_bytes(16)
    Base.url_encode64(random, padding: false)
  end

  defp build_authorize_url(redirect_uri, code_challenge, state) do
    nonce = generate_nonce()

    params = %{
      client_id: @ado_client_id,
      response_type: "code",
      redirect_uri: redirect_uri,
      response_mode: "query",
      scope: "#{@arm_resource}/.default offline_access openid profile",
      code_challenge: code_challenge,
      code_challenge_method: "S256",
      state: state,
      nonce: nonce,
      prompt: "select_account",
      client_info: "1",
      claims: ~S'{"access_token": {"xms_cc": {"values": ["CP1"]}}}'
    }

    "https://login.microsoftonline.com/#{@tenant}/oauth2/v2.0/authorize?#{URI.encode_query(params)}"
  end

  defp find_free_port do
    {:ok, socket} = :gen_tcp.listen(0, [{:reuseaddr, true}])
    {:ok, port} = :inet.port(socket)
    :gen_tcp.close(socket)
    port
  end

  defp open_browser(url) do
    case :os.type() do
      {:win32, _} -> System.cmd("cmd", ["/c", "start", url], env: [])
      {:unix, :darwin} -> System.cmd("open", [url], env: [])
      {:unix, _} -> System.cmd("xdg-open", [url], env: [])
    end
  end

  # HTTP listener that handles form_post (POST with form body from AAD).
  # Also handles query-param GET for backwards compatibility.

  defp listen_for_code(port) do
    opts = [:binary, {:reuseaddr, true}, {:ip, {127, 0, 0, 1}}, {:active, false}]

    case :gen_tcp.listen(port, opts) do
      {:ok, listener} ->
        result =
          try do
            wait_for_callback(listener)
          after
            :gen_tcp.close(listener)
          end

        result

      {:error, reason} ->
        {:error, "Cannot listen on port #{port}: #{inspect(reason)}"}
    end
  end

  defp wait_for_callback(listener) do
    {:ok, client} = :gen_tcp.accept(listener, 120_000)
    {:ok, data} = recv_all(client, <<>>)
    result = extract_code_from_request(data)
    send_response(client, "Authentication complete. You may close this window.")
    :gen_tcp.close(client)
    result
  rescue
    _ -> {:error, "Browser login timed out or was cancelled."}
  end

  defp recv_all(socket, acc) do
    case :gen_tcp.recv(socket, 0, 2000) do
      {:ok, chunk} -> recv_all(socket, acc <> chunk)
      {:error, :timeout} -> {:ok, acc}
      {:error, _} = error -> error
    end
  end

  defp send_response(socket, body) do
    response = """
    HTTP/1.1 200 OK\r
    Content-Type: text/html; charset=utf-8\r
    Content-Length: #{byte_size(body)}\r
    Connection: close\r
    \r
    #{body}
    """

    :gen_tcp.send(socket, response)
  end

  # Handles both form_post (POST with form body) and query-param GET
  defp extract_code_from_request(data) do
    {method, path} = parse_request_line(data)
    body = extract_body(data)
    params = oauth_params(method, path, body)
    extract_oauth_result(params)
  end

  defp parse_request_line(data) do
    data
    |> String.split("\r\n")
    |> hd()
    |> String.split(" ")
    |> case do
      [method, path | _] -> {method, path}
      _ -> {"UNKNOWN", "/"}
    end
  end

  defp extract_body(data) do
    case String.split(data, "\r\n\r\n", parts: 2) do
      [_, rest] ->
        content_length =
          case :binary.match(data, "Content-Length: ") do
            {pos, _} ->
              data
              |> String.slice(pos..-1//1)
              |> String.split("\r\n")
              |> hd()
              |> String.replace("Content-Length: ", "")
              |> parse_int()

            :nomatch ->
              0
          end

        String.slice(rest, 0, content_length)

      _ ->
        ""
    end
  end

  defp oauth_params("POST", _path, body) when body != "", do: URI.decode_query(body)
  defp oauth_params("GET", path, _body), do: URI.decode_query(URI.parse(path).query || "")
  defp oauth_params(_method, _path, _body), do: %{}

  defp extract_oauth_result(params) do
    case params do
      %{"code" => code, "state" => state} -> {:ok, code, state}
      %{"code" => code} -> {:ok, code, nil}
      %{"error" => error} -> {:error, "Authorization failed: #{error}"}
      _ -> {:error, "No authorization code received."}
    end
  end

  defp parse_int(str) do
    case Integer.parse(str) do
      {n, _} -> n
      :error -> 0
    end
  end
end
