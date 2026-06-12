defmodule AdoCli.ClientTest do
  use ExUnit.Case, async: false

  alias AdoCli.Client

  setup do
    start_supervised!({Finch, name: AdoCli.Finch, pools: %{default: [size: 1, count: 1]}})

    bypass = Bypass.open()

    System.put_env("ADO_SERVER", "http://localhost:#{bypass.port}")
    System.put_env("ADO_ORG", "testorg")
    System.put_env("ADO_PAT", "testpat")

    on_exit(fn ->
      System.delete_env("ADO_SERVER")
      System.delete_env("ADO_ORG")
      System.delete_env("ADO_PAT")
    end)

    {:ok, bypass: bypass}
  end

  defp api(path), do: "/testorg#{path}"

  describe "GET" do
    test "returns decoded JSON on 200", %{bypass: bypass} do
      Bypass.expect(bypass, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 200, ~s({"value":[],"count":0}))
      end)

      assert {:ok, %{"value" => [], "count" => 0}} = Client.get("/_apis/projects")
    end

    test "returns error on 404", %{bypass: bypass} do
      Bypass.expect(bypass, "GET", api("/_apis/bad"), fn conn ->
        Plug.Conn.resp(conn, 404, ~s({"message":"Not found"}))
      end)

      assert {:error, %{status: 404}} = Client.get("/_apis/bad")
    end
  end

  describe "redirect handling" do
    @tag :skip
    test "follows 302 and retries with cookies", %{bypass: bypass} do
      port = bypass.port

      Bypass.expect(bypass, "GET", api("/_apis/projects"), fn conn ->
        conn
        |> Plug.Conn.put_resp_header("location", "http://localhost:#{port}/signin")
        |> Plug.Conn.put_resp_header("set-cookie", "session=s1; Path=/")
        |> Plug.Conn.resp(302, "")
      end)

      Bypass.expect(bypass, "GET", "/signin", fn conn ->
        conn
        |> Plug.Conn.put_resp_header("set-cookie", "auth=a1; Path=/")
        |> Plug.Conn.resp(200, "OK")
      end)

      Bypass.expect(bypass, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 200, ~s({"value":[],"count":0}))
      end)

      assert {:ok, _} = Client.get("/_apis/projects")
    end
  end

  describe "list/2" do
    test "extracts value array", %{bypass: bypass} do
      Bypass.expect(bypass, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 200, ~s({"value":[{"id":1}],"count":1}))
      end)

      assert {:ok, [%{"id" => 1}]} = Client.list("/_apis/projects")
    end
  end

  describe "auth" do
    test "returns not_configured without credentials" do
      System.delete_env("ADO_ORG")
      System.delete_env("ADO_PAT")

      Application.put_env(
        :ado_cli,
        :config_path,
        "/tmp/ado_test_nonexistent_#{System.unique_integer()}.json"
      )

      on_exit(fn -> Application.delete_env(:ado_cli, :config_path) end)

      assert {:error, :not_configured} = Client.get("/_apis/projects")
    end
  end
end
