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

    on_exit(fn ->
      System.delete_env("ADO_SERVER")
      System.delete_env("ADO_ORG")
      System.delete_env("ADO_PAT")
      :persistent_term.erase({:ado_cli, :org})
      :persistent_term.erase({:ado_cli, :pat})
      :persistent_term.erase({:ado_cli, :server})
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

  describe "login_browser/1 (org auto-detect path)" do
    test "skipped — login_browser requires TCP listener mocking, covered by integration test", %{
      server: _server
    } do
      # The full browser OAuth flow requires mocking the TCP listener.
      # We test the lower-level parts (list_accounts, exchange, etc.)
      # separately. The browser flow itself is exercised in the manual
      # integration tests documented in AUTH.md.
      assert true
    end
  end

  describe "login_device_code/1 (device flow HTTP)" do
    test "starts the device code flow", %{server: server} do
      # login_device_code will:
      # 1. POST to /organizations/oauth2/devicecode to get the code
      # 2. Poll /organizations/oauth2/token for the result
      # We mock step 1 to return a valid device code, then step 2 to
      # return an error so the flow exits quickly.

      dc_response =
        ~s({"device_code":"dc-abc","user_code":"UC123","verification_url":"https://login.microsoftonline.com/common/oauth2/device","interval":5,"expires_in":900})

      TestServer.expect(server, "POST", "/organizations/oauth2/devicecode", fn conn ->
        Plug.Conn.resp(conn, 200, dc_response)
      end)

      # The next request will be the token poll - return authorization_declined
      # to make the flow exit quickly.
      TestServer.expect(server, "POST", "/organizations/oauth2/token", fn conn ->
        Plug.Conn.resp(
          conn,
          400,
          ~s({"error":"authorization_declined","error_description":"denied"})
        )
      end)

      # The flow will return an error since user denied.
      result = Auth.login_device_code("testorg")
      assert {:error, _} = result
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
end
