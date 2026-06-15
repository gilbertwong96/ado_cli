defmodule AdoCli.AuthTest do
  @moduledoc """
  Tests for the Auth module, focusing on the HTTP-call branches.
  """

  use ExUnit.Case, async: false

  alias AdoCli.Auth
  alias AdoCli.ConfigFile
  alias AdoCli.TestServer

  setup do
    start_supervised!({Finch, name: AdoCli.Finch, pools: %{default: [size: 1, count: 1]}})
    server = start_supervised!({TestServer, []})

    System.put_env("ADO_SERVER", TestServer.url(server))
    System.put_env("ADO_ORG", "testorg")
    System.put_env("ADO_PAT", "testpat")

    # Wipe any persistent_term state from previous tests
    :persistent_term.erase({:ado_cli, :org})
    :persistent_term.erase({:ado_cli, :pat})
    :persistent_term.erase({:ado_cli, :server})

    # Wipe any config file left over from previous tests
    tmp_config =
      Path.join(System.tmp_dir!(), "ado_cli_auth_test_#{System.unique_integer([:positive])}.json")

    Application.put_env(:ado_cli, :config_path, tmp_config)
    ConfigFile.delete()

    on_exit(fn ->
      System.delete_env("ADO_SERVER")
      System.delete_env("ADO_ORG")
      System.delete_env("ADO_PAT")
      :persistent_term.erase({:ado_cli, :org})
      :persistent_term.erase({:ado_cli, :pat})
      :persistent_term.erase({:ado_cli, :server})
      File.rm_rf(tmp_config)
      Application.delete_env(:ado_cli, :config_path)
    end)

    {:ok, server: server}
  end

  describe "login_pat/2" do
    test "saves config and returns ok", %{server: _server} do
      # login_pat doesn't make HTTP calls — it just saves to config
      assert {:ok, "testorg"} = Auth.login_pat("testorg", "test_pat_value")
      config = ConfigFile.load()
      assert config["org"] == "testorg"
      assert config["pat"] == "test_pat_value"
      assert config["method"] == "pat"
    end
  end

  describe "logout/0" do
    test "removes stored credentials" do
      Auth.login_pat("testorg", "token")
      assert ConfigFile.configured?()
      assert :ok = Auth.logout()
      refute ConfigFile.configured?()
    end
  end

  describe "status/0" do
    test "reports not configured when no auth" do
      status = Auth.status()
      assert status.configured == false
    end

    test "reports method after PAT login" do
      Auth.login_pat("testorg", "token")
      status = Auth.status()
      assert status.configured == true
      assert status.method == "pat"
    end
  end

  describe "current_user_id/0" do
    setup do
      # Reset the user-id cache so each test sees a fresh fetch.
      Process.delete({Auth, :user_id})
      :ok
    end

    test "returns the authenticated user GUID on first call" do
      TestServer.expect(
        TestServer,
        "GET",
        "/testorg/_apis/connectionData",
        fn conn -> Plug.Conn.resp(conn, 200, ~s({"authenticatedUser":{"id":"abc-123"}})) end
      )

      assert {:ok, "abc-123"} = Auth.current_user_id()
    end

    test "caches the result on subsequent calls (no second HTTP request)" do
      TestServer.expect(
        TestServer,
        "GET",
        "/testorg/_apis/connectionData",
        fn conn -> Plug.Conn.resp(conn, 200, ~s({"authenticatedUser":{"id":"abc-123"}})) end
      )

      assert {:ok, "abc-123"} = Auth.current_user_id()
      # Second call: if this made another HTTP request, the
      # test server would return 500 (no expectation matched).
      assert {:ok, "abc-123"} = Auth.current_user_id()
    end

    test "returns an error tuple if the response is missing the user ID" do
      TestServer.expect(
        TestServer,
        "GET",
        "/testorg/_apis/connectionData",
        fn conn -> Plug.Conn.resp(conn, 200, ~s({})) end
      )

      assert {:error, msg} = Auth.current_user_id()
      assert msg =~ "did not include an authenticated user ID"
    end

    test "returns an error tuple if the HTTP call fails" do
      TestServer.expect(
        TestServer,
        "GET",
        "/testorg/_apis/connectionData",
        fn conn -> Plug.Conn.resp(conn, 500, ~s({"message":"boom"})) end
      )

      assert {:error, msg} = Auth.current_user_id()
      assert msg =~ "Could not fetch connection data"
    end
  end
end
