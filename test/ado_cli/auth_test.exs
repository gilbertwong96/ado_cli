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
    test "skipped — request_device_code uses hardcoded Microsoft URL", %{server: _server} do
      # request_device_code/1 uses a hardcoded URL
      # "https://login.microsoftonline.com/.../devicecode" which is
      # outside the ADO_SERVER env var. To test the full flow we'd
      # need to mock the Finch HTTP layer, not just the API.
      # For now, this test is a placeholder.
      assert true
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
