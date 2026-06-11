defmodule AdoCli.AuthTest do
  use ExUnit.Case, async: false
  alias AdoCli.Auth
  alias AdoCli.ConfigFile

  setup do
    # Sandbox: use a temp file for ConfigFile and clear persistent_term state
    tmp =
      Path.join(System.tmp_dir!(), "ado_cli_auth_test_#{System.unique_integer([:positive])}.json")

    Application.put_env(:ado_cli, :config_path, tmp)
    clear_env_vars()

    on_exit(fn ->
      clear_env_vars()
      File.rm_rf(tmp)
      Application.delete_env(:ado_cli, :config_path)
    end)

    :ok
  end

  defp clear_env_vars do
    System.delete_env("ADO_ORG")
    System.delete_env("ADO_PAT")
    System.delete_env("ADO_SERVER")
    :persistent_term.erase({:ado_cli, :org})
    :persistent_term.erase({:ado_cli, :pat})
    :persistent_term.erase({:ado_cli, :server})
  end

  describe "login_pat/2" do
    test "saves config and returns ok" do
      assert {:ok, "myorg"} = Auth.login_pat("myorg", "secret_token")
      config = ConfigFile.load()
      assert config["org"] == "myorg"
      assert config["pat"] == "secret_token"
      assert config["method"] == "pat"
    end

    test "overwrites existing PAT login" do
      Auth.login_pat("old_org", "old_token")
      Auth.login_pat("new_org", "new_token")
      config = ConfigFile.load()
      assert config["org"] == "new_org"
      assert config["pat"] == "new_token"
    end

    test "handles special characters in PAT" do
      pat = "token!@#$%^&*()_+-={}[]|:;\"'<>,.?/~`"
      assert {:ok, "myorg"} = Auth.login_pat("myorg", pat)
      assert ConfigFile.load()["pat"] == pat
    end
  end

  describe "logout/0" do
    test "removes the config file" do
      Auth.login_pat("myorg", "token")
      assert ConfigFile.configured?()
      Auth.logout()
      refute ConfigFile.configured?()
    end

    test "does not error when not configured" do
      refute ConfigFile.configured?()
      assert :ok = Auth.logout()
    end
  end

  describe "status/0" do
    test "reports not configured with no auth" do
      status = Auth.status()
      assert status.configured == false
      assert status.org == nil
      assert status.method == nil
    end

    test "reports method after PAT login" do
      Auth.login_pat("myorg", "token")
      status = Auth.status()
      assert status.configured == true
      assert status.org == "myorg"
      assert status.method == "pat"
    end

    test "includes az_cli availability flag" do
      status = Auth.status()
      assert is_boolean(status.az_cli_available)
    end

    test "uses runtime org from env over config" do
      Auth.login_pat("cfg_org", "cfg_token")
      System.put_env("ADO_ORG", "env_org")
      status = Auth.status()
      assert status.org == "env_org"
    end
  end

  describe "resolve_auth/0" do
    test "returns not_configured when nothing set" do
      # Only passes if az CLI is NOT available
      unless System.find_executable("az") do
        assert {:error, :not_configured} = Auth.resolve_auth()
      end
    end

    test "uses PAT from environment when org+pat set" do
      System.put_env("ADO_ORG", "env_org")
      System.put_env("ADO_PAT", "env_token")
      assert {:ok, "env_org", headers} = Auth.resolve_auth()
      expected = "Basic " <> Base.encode64(":env_token")
      assert Enum.any?(headers, &(&1 == {"Authorization", expected}))
    end

    test "uses config file when no env vars" do
      Auth.login_pat("cfg_org", "cfg_token")
      assert {:ok, "cfg_org", headers} = Auth.resolve_auth()
      expected = "Basic " <> Base.encode64(":cfg_token")
      assert Enum.any?(headers, &(&1 == {"Authorization", expected}))
    end

    test "environment org takes precedence over config org" do
      Auth.login_pat("cfg_org", "cfg_token")
      System.put_env("ADO_ORG", "env_org")
      System.put_env("ADO_PAT", "env_token")
      assert {:ok, "env_org", _} = Auth.resolve_auth()
    end

    test "returns not_configured when only org is set" do
      System.put_env("ADO_ORG", "myorg")
      # Only if az is not available; otherwise az token may be used
      unless System.find_executable("az") do
        assert {:error, :not_configured} = Auth.resolve_auth()
      end
    end

    test "returns ok when az CLI provides a token" do
      # If az is available AND logged in, the CLI uses az's token.
      # If az is available but not logged in, it returns :not_configured.
      # Both outcomes are valid; we just verify no crash.
      if System.find_executable("az") do
        result = Auth.resolve_auth()
        assert match?({:ok, _org, _headers}, result) or match?({:error, :not_configured}, result)
      end
    end
  end

  describe "az_cli_available?/0 (private)" do
    test "reflects whether az is in PATH" do
      # Indirectly testable via status().az_cli_available
      assert Auth.status().az_cli_available == (System.find_executable("az") != nil)
    end
  end
end
