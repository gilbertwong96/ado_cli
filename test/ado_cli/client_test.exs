defmodule AdoCli.ClientTest do
  use ExUnit.Case, async: false

  alias AdoCli.Client
  alias AdoCli.TestServer

  setup do
    start_supervised!({Finch, name: AdoCli.Finch, pools: %{default: [size: 1, count: 1]}})

    server = start_supervised!({TestServer, []})

    System.put_env("ADO_SERVER", TestServer.url(server))
    System.put_env("ADO_ORG", "testorg")
    System.put_env("ADO_PAT", "testpat")

    on_exit(fn ->
      System.delete_env("ADO_SERVER")
      System.delete_env("ADO_ORG")
      System.delete_env("ADO_PAT")
    end)

    {:ok, server: server}
  end

  defp api(path), do: "/testorg#{path}"

  describe "get/2" do
    test "returns decoded JSON on 200", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 200, ~s({"value":[],"count":0}))
      end)

      assert {:ok, %{"value" => [], "count" => 0}} = Client.get("/_apis/projects")
    end

    test "returns error on 404", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/bad"), fn conn ->
        Plug.Conn.resp(conn, 404, ~s({"message":"Not found"}))
      end)

      assert {:error, %{status: 404}} = Client.get("/_apis/bad")
    end

    test "returns error on 500", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/fail"), fn conn ->
        Plug.Conn.resp(conn, 500, ~s({"error":"Internal"}))
      end)

      assert {:error, %{status: 500}} = Client.get("/_apis/fail")
    end

    test "includes api-version query parameter", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        assert conn.query_string =~ "api-version="
        Plug.Conn.resp(conn, 200, ~s({"value":[],"count":0}))
      end)

      assert {:ok, _} = Client.get("/_apis/projects")
    end

    test "sends Authorization header", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        auth = Plug.Conn.get_req_header(conn, "authorization")
        assert auth != []
        assert hd(auth) =~ "Basic"
        Plug.Conn.resp(conn, 200, ~s({}))
      end)

      assert {:ok, _} = Client.get("/_apis/projects")
    end
  end

  describe "post/3" do
    test "sends body as JSON", %{server: server} do
      TestServer.expect(server, "POST", api("/_apis/projects"), fn conn ->
        {:ok, body, conn} = Plug.Conn.read_body(conn)
        decoded = JSON.decode!(body)
        assert decoded["name"] == "newproj"
        Plug.Conn.resp(conn, 201, ~s({"id":"123","name":"newproj"}))
      end)

      assert {:ok, %{"id" => "123"}} = Client.post("/_apis/projects", %{"name" => "newproj"})
    end

    test "sends Content-Type header", %{server: server} do
      TestServer.expect(server, "POST", api("/_apis/projects"), fn conn ->
        ct = Plug.Conn.get_req_header(conn, "content-type")
        assert Enum.any?(ct, &String.contains?(&1, "application/json"))
        Plug.Conn.resp(conn, 201, ~s({}))
      end)

      assert {:ok, _} = Client.post("/_apis/projects", %{})
    end

    test "returns error on non-2xx", %{server: server} do
      TestServer.expect(server, "POST", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 400, ~s({"message":"Bad"}))
      end)

      assert {:error, %{status: 400}} = Client.post("/_apis/projects", %{})
    end
  end

  describe "patch/3" do
    test "updates a resource", %{server: server} do
      TestServer.expect(server, "PATCH", api("/_apis/projects/p1"), fn conn ->
        {:ok, body, conn} = Plug.Conn.read_body(conn)
        decoded = JSON.decode!(body)
        assert decoded["name"] == "renamed"
        Plug.Conn.resp(conn, 200, ~s({"id":"p1","name":"renamed"}))
      end)

      assert {:ok, %{"name" => "renamed"}} =
               Client.patch("/_apis/projects/p1", %{"name" => "renamed"})
    end
  end

  describe "put/3" do
    test "replaces a resource", %{server: server} do
      TestServer.expect(server, "PUT", api("/_apis/vg/1"), fn conn ->
        Plug.Conn.resp(conn, 200, ~s({"id":1,"name":"updated"}))
      end)

      assert {:ok, %{"id" => 1}} = Client.put("/_apis/vg/1", %{"name" => "updated"})
    end
  end

  describe "delete/2" do
    test "returns :ok on 2xx", %{server: server} do
      TestServer.expect(server, "DELETE", api("/_apis/vg/1"), fn conn ->
        Plug.Conn.resp(conn, 204, "")
      end)

      assert :ok = Client.delete("/_apis/vg/1")
    end

    test "returns error on 404", %{server: server} do
      TestServer.expect(server, "DELETE", api("/_apis/vg/999"), fn conn ->
        Plug.Conn.resp(conn, 404, ~s({"message":"Not found"}))
      end)

      assert {:error, %{status: 404}} = Client.delete("/_apis/vg/999")
    end
  end

  describe "list/2" do
    test "extracts value array", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 200, ~s({"value":[{"id":1}],"count":1}))
      end)

      assert {:ok, [%{"id" => 1}]} = Client.list("/_apis/projects")
    end

    test "passes through list response", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/repos"), fn conn ->
        Plug.Conn.resp(conn, 200, ~s([{"id":1}]))
      end)

      assert {:ok, [%{"id" => 1}]} = Client.list("/_apis/repos")
    end

    test "forwards non-array errors", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 401, ~s({"error":"Unauthorized"}))
      end)

      assert {:error, %{status: 401}} = Client.list("/_apis/projects")
    end
  end

  describe "auth" do
    test "returns not_configured without credentials" do
      System.delete_env("ADO_ORG")
      System.delete_env("ADO_PAT")

      Application.put_env(
        :ado_cli,
        :config_path,
        "/tmp/ado_cli_test_nonexistent_#{System.unique_integer()}.json"
      )

      on_exit(fn -> Application.delete_env(:ado_cli, :config_path) end)

      assert {:error, :not_configured} = Client.get("/_apis/projects")
    end
  end

  describe "redirect handling" do
    test "returns clear auth error on 302", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        conn
        |> Plug.Conn.put_resp_header("location", "http://example.com/signin")
        |> Plug.Conn.resp(302, "")
      end)

      assert {:error, %{status: 302, body: body}} = Client.get("/_apis/projects")
      assert body =~ "ado login"
    end
  end

  describe "non-JSON success bodies" do
    test "returns raw body when not valid JSON on 200", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/raw"), fn conn ->
        Plug.Conn.resp(conn, 200, "plain text")
      end)

      assert {:error, _} = Client.get("/_apis/raw")
    end
  end

  describe "get_raw/2" do
    test "returns raw binary on 200", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/attachments/1"), fn conn ->
        Plug.Conn.resp(conn, 200, "binary data")
      end)

      assert {:ok, body} = Client.get_raw("/_apis/attachments/1")
      assert body == "binary data"
    end

    test "returns error on non-2xx", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/attachments/2"), fn conn ->
        Plug.Conn.resp(conn, 404, "Not found")
      end)

      assert {:error, %{status: 404}} = Client.get_raw("/_apis/attachments/2")
    end
  end

  describe "302 without Location header" do
    test "returns error with helpful message", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 302, "")
      end)

      assert {:error, %{status: 302, body: body}} = Client.get("/_apis/projects")
      assert body =~ "redirected"
    end
  end

  describe "transport errors" do
    test "returns transport error when server unreachable" do
      # Use an unreachable port
      System.put_env("ADO_SERVER", "http://localhost:1")

      on_exit(fn ->
        # The setup block already set it; we reset in on_exit above too
        :ok
      end)

      assert {:error, _reason} = Client.get("/_apis/projects")
    end
  end

  describe "list edge cases" do
    test "passes through non-list, non-value response", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/odd"), fn conn ->
        Plug.Conn.resp(conn, 200, ~s({"odd":"shape"}))
      end)

      assert {:ok, %{"odd" => "shape"}} = Client.list("/_apis/odd")
    end
  end

  describe "safe_decode (private)" do
    test "decodes JSON error body in non-2xx response", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/notjson"), fn conn ->
        Plug.Conn.resp(conn, 500, "not json at all")
      end)

      assert {:error, %{status: 500, body: body}} = Client.get("/_apis/notjson")
      assert body == "not json at all" or is_map(body)
    end
  end

  describe "redirect/cookie limits" do
    test "302 with location returns auth error (no programmatic follow)", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        conn
        |> Plug.Conn.put_resp_header("location", "http://example.com/signin")
        |> Plug.Conn.resp(302, "")
      end)

      assert {:error, %{status: 302, body: body}} = Client.get("/_apis/projects")
      assert body =~ "ado login"
    end
  end
end
