# Standalone mock Azure DevOps server for end-to-end CLI testing.
#
# Listens on a random localhost port and serves the service-connection
# endpoints used by `ado connections list/show/create/update/delete`.
# Run with:
#
#   mix run --no-halt scripts/mock_ado_server.exs
#
# It blocks forever; use Ctrl-C to stop.

defmodule MockAdoServer.Plug do
  @behaviour Plug
  import Plug.Conn

  @impl true
  def init(opts), do: opts

  @impl true
  def call(conn, _opts) do
    {conn, body} = read_body_safe(conn)

    case {conn.method, conn.path_info, body} do
      {"GET", ["testorg", _project, "_apis", "serviceendpoint", "endpoints"], _} ->
        payload =
          Jason.encode!(%{
            "value" => [
              %{
                "id" => "existing-1",
                "name" => "Pre-existing",
                "type" => "github",
                "url" => "https://github.com"
              }
            ],
            "count" => 1
          })

        send_json(conn, 200, payload)

      {"GET", ["testorg", _project, "_apis", "serviceendpoint", "endpoints", id], _} ->
        payload =
          Jason.encode!(%{
            "id" => id,
            "name" => "Pre-existing",
            "type" => "github",
            "url" => "https://github.com",
            "isReady" => true
          })

        send_json(conn, 200, payload)

      {"POST", ["testorg", _project, "_apis", "serviceendpoint", "endpoints"], body} ->
        request = decode(body)
        created_id = "created-" <> Integer.to_string(System.unique_integer([:positive]))
        created =
          Map.merge(default_endpoint(created_id), request)
          |> Map.put("isReady", false)

        log("POST  /_apis/serviceendpoint/endpoints  id=" <> created_id <> "  name=" <> to_str(request["name"]) <> "  type=" <> to_str(request["type"]))
        send_json(conn, 200, Jason.encode!(created))

      {"PUT", ["testorg", _project, "_apis", "serviceendpoint", "endpoints", "missing-id"], _} ->
        send_json(conn, 404, Jason.encode!(%{"message" => "Service connection missing-id not found"}))

      {"PUT", ["testorg", _project, "_apis", "serviceendpoint", "endpoints", id], body} ->
        request = decode(body)

        updated =
          Map.merge(default_endpoint(id), request)
          |> Map.put("id", id)
          |> Map.put("isReady", true)

        log("PUT   /_apis/serviceendpoint/endpoints/" <> id <> "  keys=" <> inspect(Map.keys(request)))
        send_json(conn, 200, Jason.encode!(updated))

      {"DELETE", ["testorg", _project, "_apis", "serviceendpoint", "endpoints", "missing-id"], _} ->
        send_json(conn, 404, Jason.encode!(%{"message" => "Not found"}))

      {"DELETE", ["testorg", _project, "_apis", "serviceendpoint", "endpoints", id], _} ->
        log("DELETE /_apis/serviceendpoint/endpoints/" <> id)
        send_resp(conn, 204, "")

      {method, path, _} ->
        log("UNHANDLED  " <> method <> " /" <> Enum.join(path, "/"))
        send_json(conn, 404, Jason.encode!(%{"message" => "Not found"}))
    end
  end

  defp read_body_safe(conn) do
    case read_body(conn) do
      {:ok, body, conn} -> {conn, body}
      {:more, body, conn} -> {conn, body}
      {:error, _} -> {conn, ""}
    end
  end

  defp decode(""), do: %{}

  defp decode(body) do
    case Jason.decode(body) do
      {:ok, decoded} -> decoded
      {:error, _} -> %{}
    end
  end

  defp to_str(nil), do: ""
  defp to_str(v), do: to_string(v)

  defp log(line) do
    IO.puts("[mock] " <> line)
  end

  defp send_json(conn, status, body) do
    conn
    |> put_resp_header("content-type", "application/json")
    |> send_resp(status, body)
  end

  defp default_endpoint(id) do
    %{
      "id" => id,
      "name" => "unnamed",
      "type" => "generic",
      "url" => "",
      "isReady" => false,
      "authorization" => %{"scheme" => "None", "parameters" => %{}}
    }
  end
end

{:ok, pid} = Bandit.start_link(plug: MockAdoServer.Plug, port: 0)
{:ok, {_ip, port}} = ThousandIsland.listener_info(pid)
port_s = Integer.to_string(port)
url = "http://127.0.0.1:" <> port_s

File.write!("scripts/.mock_ado_port", port_s)

IO.puts("[mock] listening on " <> url)
IO.puts("[mock] point the CLI at it with:")
IO.puts("    export ADO_SERVER=" <> url)
IO.puts("    export ADO_ORG=testorg")
IO.puts("    export ADO_PAT=mock-pat")
IO.puts("")

Process.sleep(:infinity)
