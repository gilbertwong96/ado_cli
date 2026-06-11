defmodule AdoCli.Client do
  @moduledoc """
  HTTP client for Azure DevOps REST API.

  Delegates to `az devops invoke` which handles MSAL-based auth,
  tenant discovery, and cookie-based sign-in for all org types
  including MSA-backed personal orgs (`*.visualstudio.com`).

  Falls back to raw Finch-based HTTP for environments where `az` is unavailable.
  """

  @api_version "7.1"

  @doc """
  Makes a GET request to the Azure DevOps API.
  Returns `{:ok, decoded_json}` or `{:error, reason}`.
  """
  def get(path, params \\ %{}) do
    if az_available?(), do: az_get(path, params), else: finch_get(path, params)
  end

  @doc """
  Makes a list (paginated) GET request to the Azure DevOps API.
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
    if az_available?(), do: az_post(path, body, params), else: finch_post(path, body, params)
  end

  @doc """
  Makes a PATCH request.
  """
  def patch(path, body, params \\ %{}) do
    if az_available?(), do: az_patch(path, body, params), else: finch_patch(path, body, params)
  end

  @doc """
  Makes a DELETE request.
  """
  def delete(path, params \\ %{}) do
    if az_available?(), do: az_delete(path, params), else: finch_delete(path, params)
  end

  @doc """
  Makes a PUT request.
  """
  def put(path, body, params \\ %{}) do
    if az_available?(), do: az_put(path, body, params), else: finch_do(:put, path, body, params)
  end

  # ── az devops backend ────────────────────────────────────────────────

  defp az_available?, do: System.find_executable("az") != nil

  defp az_get(path, params) do
    org = runtime_org()
    path |> parse_path() |> do_az_get(org, params)
  end

  defp az_post(path, body, params) do
    org = runtime_org()
    path |> parse_path() |> do_az_post(org, body, params)
  end

  defp az_patch(path, body, params) do
    org = runtime_org()
    path |> parse_path() |> do_az_post(org, body, params)
  end

  defp az_put(path, body, params) do
    org = runtime_org()
    path |> parse_path() |> do_az_post(org, body, params)
  end

  defp az_delete(path, params) do
    org = runtime_org()
    path |> parse_path() |> do_az_delete(org, params)
  end

  # Parse API path into {area, resource, route_params}
  # e.g. "/_apis/projects" → {"core", "projects", %{}}
  # e.g. "/Employee Management/_apis/git/repositories" → {"git", "repositories", %{}}
  # e.g. "/_apis/wit/workitems/42" → {"wit", "workitems", %{ids: "42"}}
  defp parse_path(path) do
    path = String.trim_leading(path, "/")

    case String.split(path, "/_apis/", parts: 2) do
      [project, rest] -> Map.put(parse_api_path(rest), :project, project)
      [rest] -> parse_api_path(rest)
    end
  end

  defp parse_api_path(rest) do
    # Path format after _apis/: {area}/{resource}/... or just {resource}
    # Strip leading "_apis/" if present, then parse remaining segments
    clean = String.replace(rest, ~r/^_apis\//, "")
    segments = String.split(clean, "/")

    {area, resource} =
      case segments do
        [single] -> {"core", single}
        [a, r | _] -> {a, r}
        _ -> {"core", "projects"}
      end

    # Collect remaining segments as route parameters
    # wit/workitems/Task → %{type: "Task"}
    # git/repositories/ID → %{repositoryId: "ID"}
    remaining = Enum.drop(segments, 2)

    route_params =
      case {resource, remaining} do
        {"workitems", [type]} -> %{type: type}
        {"repositories", [repo_id]} -> %{repositoryId: repo_id}
        {_, [id]} -> %{id: id}
        _ -> %{}
      end

    %{area: area, resource: resource, route_params: route_params}
  end

  defp do_az_get(%{area: a, resource: r} = parsed, org, params) do
    p = Map.get(parsed, :project)
    route_params = Map.get(parsed, :route_params, %{})

    args = [
      "devops",
      "invoke",
      "--area",
      a,
      "--resource",
      r,
      "--http-method",
      "GET",
      "--api-version",
      @api_version,
      "--only-show-errors",
      "--org",
      build_org_url(org)
    ]

    args = add_query_params(args, params)
    args = if p, do: args ++ ["--query-parameters", "project=#{p}"], else: args
    args = add_route_params(args, route_params)
    run_az(args)
  end

  defp do_az_post(%{area: a, resource: r} = parsed, org, body, params) do
    p = Map.get(parsed, :project)
    route_params = Map.get(parsed, :route_params, %{})
    tmp = write_temp(body)

    args = [
      "devops",
      "invoke",
      "--area",
      a,
      "--resource",
      r,
      "--http-method",
      "POST",
      "--api-version",
      @api_version,
      "--only-show-errors",
      "--org",
      build_org_url(org),
      "--in-file",
      tmp
    ]

    args = add_query_params(args, params)
    args = if p, do: args ++ ["--query-parameters", "project=#{p}"], else: args
    args = add_route_params(args, route_params)
    result = run_az(args)
    File.rm(tmp)
    result
  end

  defp do_az_delete(%{area: a, resource: r, project: p}, org, params) do
    args = [
      "devops",
      "invoke",
      "--area",
      a,
      "--resource",
      r,
      "--http-method",
      "DELETE",
      "--api-version",
      @api_version,
      "--only-show-errors",
      "--org",
      build_org_url(org)
    ]

    args = add_query_params(args, params)
    args = if p, do: args ++ ["--query-parameters", "project=#{p}"], else: args
    run_az(args)
  end

  defp add_query_params(args, params) when is_map(params) do
    Enum.reduce(params, args, fn {k, v}, acc ->
      acc ++ ["--query-parameters", "#{k}=#{v}"]
    end)
  end

  defp add_route_params(args, route_params)
       when is_map(route_params) and map_size(route_params) > 0 do
    rp_string = Enum.map_join(route_params, " ", fn {k, v} -> "#{k}=#{v}" end)
    args ++ ["--route-parameters", rp_string]
  end

  defp add_route_params(args, _), do: args

  defp build_org_url(org) do
    server = runtime_server()
    if server, do: server, else: "https://dev.azure.com/#{org}"
  end

  defp write_temp(data) do
    path = Path.join(System.tmp_dir!(), "ado_cli_#{System.unique_integer()}.json")
    File.write!(path, JSON.encode!(data))
    path
  end

  defp run_az(args) do
    safe_env = %{"HOME" => System.get_env("HOME", ""), "PATH" => System.get_env("PATH", "")}

    case System.cmd("az", args, stderr_to_stdout: true, env: safe_env) do
      {output, 0} ->
        trimmed = String.trim(output)

        if trimmed == "" do
          :ok
        else
          case JSON.decode(trimmed) do
            {:ok, decoded} -> {:ok, decoded}
            {:error, _} -> {:ok, trimmed}
          end
        end

      {error, _} ->
        {:error, %{status: 500, body: String.trim(error)}}
    end
  end

  defp runtime_org do
    System.get_env("ADO_ORG") || application_org()
  end

  defp application_org do
    with {:ok, config} <- Application.fetch_env(:ado_cli, :azure_devops),
         org when is_binary(org) <- config[:org] do
      org
    else
      _ -> nil
    end
  end

  defp runtime_server do
    System.get_env("ADO_SERVER")
  end

  # ── Finch fallback (unchanged, for environments without az) ──────────

  defp finch_get(path, params) do
    finch_do(:get, path, nil, params)
  end

  defp finch_post(path, body, params) do
    finch_do(:post, path, body, params)
  end

  defp finch_patch(path, body, params) do
    finch_do(:patch, path, body, params)
  end

  defp finch_delete(path, params) do
    case finch_do(:delete, path, nil, params) do
      {:ok, %{status: status}} when status in 200..299 -> :ok
      {:ok, %{status: status, body: body}} -> {:error, %{status: status, body: body}}
      {:error, _} = error -> error
    end
  end

  defp finch_do(method, path, body, params) do
    base = finch_base_url()
    query = URI.encode_query(Map.merge(params, %{"api-version" => @api_version}))
    url = "#{base}/#{String.trim_leading(path, "/")}?#{query}"

    case AdoCli.Auth.resolve_auth() do
      {:ok, _org, auth_headers} ->
        headers = [{"Content-Type", "application/json"} | auth_headers]
        encoded = if body, do: JSON.encode!(body), else: nil
        request = Finch.build(method, url, headers, encoded)

        case Finch.request(request, AdoCli.Finch) do
          {:ok, %Finch.Response{status: status, body: resp_body}} ->
            {:ok, %{status: status, body: resp_body}}

          {:error, reason} ->
            {:error, reason}
        end

      {:error, reason} ->
        {:error, reason}
    end
  end

  defp finch_base_url do
    server = System.get_env("ADO_SERVER")

    case server do
      nil -> "https://dev.azure.com"
      s -> String.trim_trailing(s, "/")
    end
  end
end
