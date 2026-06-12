defmodule AdoCli.Client do
  @moduledoc """
  HTTP client for Azure DevOps REST API v7.1 using Finch.

  Self-contained — no dependency on `az` CLI. Uses PAT or browser OAuth
  bearer tokens via `AdoCli.Auth.resolve_auth/0`.

  ## Supported API Areas

    * **Core** — projects, teams
    * **Git** — repositories, pull requests, branches
    * **Work Item Tracking** — work items, WIQL queries
    * **Build** — pipelines, builds
    * **Release** — releases
  """

  @api_version "7.1"

  @doc """
  Makes a GET request to the Azure DevOps API.
  """
  def get(path, params \\ %{}) do
    handle_response(do_request(:get, path, nil, params))
  end

  @doc """
  Makes a GET request returning raw binary (for file downloads).
  """
  def get_raw(path, params \\ %{}) do
    do_request_raw(:get, path, nil, params)
  end

  @doc """
  Makes a list (paginated) GET request. Extracts the `value` array.
  """
  def list(path, params \\ %{}) do
    case get(path, params) do
      {:ok, %{"value" => items}} -> {:ok, items}
      {:ok, items} when is_list(items) -> {:ok, items}
      {:ok, _} = ok -> ok
      {:error, _} = error -> error
    end
  end

  @doc """
  Makes a POST request.
  """
  def post(path, body, params \\ %{}) do
    handle_response(do_request(:post, path, body, params))
  end

  @doc """
  Makes a PATCH request.
  """
  def patch(path, body, params \\ %{}) do
    handle_response(do_request(:patch, path, body, params))
  end

  @doc """
  Makes a DELETE request.
  """
  def delete(path, params \\ %{}) do
    case do_request(:delete, path, nil, params) do
      {:ok, %{status: status}} when status in 200..299 -> :ok
      {:ok, %{status: status, body: body}} -> {:error, %{status: status, body: body}}
      {:error, _} = error -> error
    end
  end

  @doc """
  Makes a PUT request.
  """
  def put(path, body, params \\ %{}) do
    handle_response(do_request(:put, path, body, params))
  end

  # ── Private ──────────────────────────────────────────────────────────

  defp handle_response({:ok, %{status: status, body: body}}) when status in 200..299 do
    JSON.decode(body)
  end

  defp handle_response({:ok, %{status: status, body: body}}) do
    {:error, %{status: status, body: safe_decode(body)}}
  end

  defp handle_response({:error, _} = error), do: error

  defp safe_decode(body) when is_binary(body) do
    case JSON.decode(body) do
      {:ok, decoded} -> decoded
      {:error, _} -> body
    end
  end

  defp safe_decode(body), do: body

  defp do_request(method, path, body, params, attempt \\ 0)

  defp do_request(_method, _path, _body, _params, 3),
    do: {:error, %{status: 302, body: "Too many redirects"}}

  defp do_request(method, path, body, params, attempt) do
    with {:ok, org, auth_headers} <- AdoCli.Auth.resolve_auth() do
      do_request_with_auth(method, path, body, params, attempt, org, auth_headers)
    end
  end

  defp do_request_with_auth(method, path, body, params, attempt, org, auth_headers) do
    url = build_url(path, params)
    full_url = inject_org(url, org)
    headers = [{"Content-Type", "application/json"} | auth_headers]
    encoded = if body, do: JSON.encode!(body)

    case Finch.request(Finch.build(method, full_url, headers, encoded), AdoCli.Finch) do
      {:ok, %Finch.Response{status: status, headers: resp_headers}}
      when status in [301, 302, 307, 308] ->
        handle_redirect(method, path, body, params, resp_headers, auth_headers, attempt)

      {:ok, %Finch.Response{status: status, body: resp_body}} ->
        {:ok, %{status: status, body: resp_body}}

      {:error, reason} ->
        {:error, reason}
    end
  end

  # The DevOps API may return 302 to a sign-in page for orgs that require
  # browser-based identity establishment (MSA personal orgs, etc.).
  # This cannot be automated — the user must authenticate interactively
  # via `ado login` first.
  defp handle_redirect(_method, _path, _body, _params, resp_headers, _auth_headers, _attempt) do
    location =
      Enum.find_value(resp_headers, fn
        {"location", v} -> v
        _ -> nil
      end)

    body =
      case location do
        nil -> "API redirected without a Location header. Run 'ado login' to authenticate."
        _ -> "API redirected to sign-in page. Run 'ado login' to authenticate."
      end

    {:error, %{status: 302, body: body}}
  end

  defp do_request_raw(method, path, _body, params) do
    url = build_url(path, params)

    with {:ok, org, auth_headers} <- AdoCli.Auth.resolve_auth(),
         {:ok, %Finch.Response{status: status, body: body}} <-
           Finch.request(Finch.build(method, inject_org(url, org), auth_headers), AdoCli.Finch) do
      if status in 200..299, do: {:ok, body}, else: {:error, %{status: status}}
    end
  end

  defp build_url(path, params) do
    query = URI.encode_query(Map.merge(params, %{"api-version" => @api_version}))
    base = base_url()
    "#{base}/#{String.trim_leading(path, "/")}?#{query}"
  end

  # Inject org into URL: https://dev.azure.com/_apis/... → https://{org}.visualstudio.com/_apis/...
  defp inject_org(url, org) do
    server = System.get_env("ADO_SERVER")

    if server do
      String.replace(url, ~r{^(https?://[^/]+)/}, "\\1/#{org}/")
    else
      String.replace(url, ~r{^https?://[^/]+}, "https://#{org}.visualstudio.com")
    end
  end

  defp base_url do
    case System.get_env("ADO_SERVER") do
      nil -> "https://dev.azure.com"
      s -> String.trim_trailing(s, "/")
    end
  end
end
