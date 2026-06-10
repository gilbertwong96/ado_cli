defmodule AdoCli.Client do
  @moduledoc """
  HTTP client for Azure DevOps REST API using Finch.

  Supports both cloud (`dev.azure.com`) and self-hosted Azure DevOps Server.

  Set the server URL via:
    - CLI flag: `--server https://ado.example.com`
    - Env var:  `ADO_SERVER=https://ado.example.com`

  When `--server` is set, the `--org` becomes the collection name
  (e.g. `DefaultCollection`).

  ## API Areas Covered

    * Core (projects, teams, processes)
    * Git (repositories, pull requests, branches, commits)
    * Work Item Tracking (work items, queries, classifications)
    * Build (pipelines, builds, definitions)
    * Release (release definitions, releases, environments)
  """

  @api_version "7.1"

  @doc """
  Makes a GET request to the Azure DevOps API.
  Returns `{:ok, decoded_json}` or `{:error, reason}`.
  """
  def get(path, params \\ %{}) do
    base_url()
    |> build_url(path, params)
    |> do_request(:get)
    |> handle_response()
  end

  @doc """
  Makes a GET request to the Azure DevOps VS Release Management API.
  """
  def get_vsrm(path, params \\ %{}) do
    vsrm_base_url()
    |> build_url(path, params)
    |> do_request(:get)
    |> handle_response()
  end

  @doc """
  Makes a list (paginated) GET request to the Azure DevOps API.
  Returns `{:ok, items}` or `{:error, reason}`.
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
  Makes a POST request to the Azure DevOps API.
  """
  def post(path, body, params \\ %{}) do
    base_url()
    |> build_url(path, params)
    |> do_request(:post, JSON.encode!(body))
    |> handle_response()
  end

  @doc """
  Makes a PATCH request to the Azure DevOps API.
  """
  def patch(path, body, params \\ %{}) do
    base_url()
    |> build_url(path, params)
    |> do_request(:patch, JSON.encode!(body))
    |> handle_response()
  end

  @doc """
  Makes a DELETE request to the Azure DevOps API.
  """
  def delete(path, params \\ %{}) do
    case base_url() |> build_url(path, params) |> do_request(:delete) do
      {:ok, %{status: status}} when status in 200..299 -> :ok
      {:ok, %{status: status, body: body}} -> {:error, %{status: status, body: safe_decode(body)}}
      {:error, reason} -> {:error, reason}
    end
  end

  # ── Private helpers ───────────────────────────────────────────────────

  defp handle_response(result) do
    case result do
      {:ok, %{status: status, body: body}} when status in 200..299 ->
        JSON.decode(body)

      {:ok, %{status: status, body: body}} ->
        {:error, %{status: status, body: safe_decode(body)}}

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp build_url(base, path, params) do
    query = URI.encode_query(Map.merge(params, %{"api-version" => api_version()}))
    "#{base}/#{String.trim_leading(path, "/")}?#{query}"
  end

  defp do_request(method, url, body \\ nil) do
    case AdoCli.Auth.resolve_auth() do
      {:ok, _org, auth_headers} ->
        headers = [{"Content-Type", "application/json"} | auth_headers]
        request = Finch.build(method, url, headers, body)

        case Finch.request(request, AdoCli.Finch) do
          {:ok, %Finch.Response{status: status, body: body}} ->
            {:ok, %{status: status, body: body}}

          {:error, reason} ->
            {:error, reason}
        end

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp api_version do
    Application.get_env(:ado_cli, :azure_devops)[:api_version] || @api_version
  end

  defp base_url do
    case Application.get_env(:ado_cli, :azure_devops)[:server] do
      nil -> "https://dev.azure.com"
      server -> String.trim_trailing(server, "/")
    end
  end

  defp vsrm_base_url do
    case Application.get_env(:ado_cli, :azure_devops)[:server] do
      nil -> "https://vsrm.dev.azure.com"
      server -> String.trim_trailing(server, "/")
    end
  end

  defp safe_decode(body) when is_binary(body) do
    case JSON.decode(body) do
      {:ok, decoded} -> decoded
      {:error, _} -> body
    end
  end

  defp safe_decode(body), do: body
end
