defmodule AdoCli.ConfigFileTest do
  use ExUnit.Case, async: false
  alias AdoCli.ConfigFile

  setup do
    # Sandbox: use a temp file instead of ~/.ado_cli/config.json
    tmp = Path.join(System.tmp_dir!(), "ado_cli_test_#{System.unique_integer([:positive])}.json")
    Application.put_env(:ado_cli, :config_path, tmp)

    on_exit(fn ->
      File.rm_rf(tmp)
      Application.delete_env(:ado_cli, :config_path)
    end)

    {:ok, path: tmp}
  end

  describe "save/1" do
    test "writes a new config file", %{path: path} do
      assert :ok = ConfigFile.save(%{org: "myorg", method: "pat", pat: "token123"})
      assert File.exists?(path)

      {:ok, decoded} = JSON.decode(File.read!(path))
      assert decoded["org"] == "myorg"
      assert decoded["method"] == "pat"
      assert decoded["pat"] == "token123"
    end

    test "creates the parent directory if missing" do
      base = Path.join(System.tmp_dir!(), "ado_cli_nested_#{System.unique_integer([:positive])}")
      tmp = Path.join([base, "sub", "config.json"])
      Application.put_env(:ado_cli, :config_path, tmp)
      on_exit(fn -> File.rm_rf(base) end)

      assert :ok = ConfigFile.save(%{org: "test"})
      assert File.exists?(tmp)
    end

    test "merges with existing config" do
      ConfigFile.save(%{org: "myorg", method: "pat", pat: "old_token"})
      ConfigFile.save(%{pat: "new_token", extra: "value"})

      config = ConfigFile.load()
      assert config["org"] == "myorg"
      assert config["method"] == "pat"
      assert config["pat"] == "new_token"
      assert config["extra"] == "value"
    end

    test "overwrites scalar values" do
      ConfigFile.save(%{org: "old_org"})
      ConfigFile.save(%{org: "new_org"})
      assert ConfigFile.load()["org"] == "new_org"
    end

    test "writes valid JSON for nested structures" do
      ConfigFile.save(%{org: "myorg", tags: ["a", "b", "c"], nested: %{x: 1}})
      assert {:ok, _} = JSON.decode(File.read!(ConfigFile.config_path()))
    end
  end

  describe "load/0" do
    test "returns nil when no config exists" do
      assert ConfigFile.load() == nil
    end

    test "returns nil for corrupted JSON" do
      File.write!(ConfigFile.config_path(), "{not valid json")
      assert ConfigFile.load() == nil
    end

    test "returns nil when JSON root is not a map" do
      File.write!(ConfigFile.config_path(), "[\"array\"]")
      assert ConfigFile.load() == nil
    end

    test "returns parsed config map" do
      File.write!(ConfigFile.config_path(), ~s({"org":"test","method":"pat"}))
      config = ConfigFile.load()
      assert config["org"] == "test"
      assert config["method"] == "pat"
    end
  end

  describe "delete/0" do
    test "removes the config file" do
      ConfigFile.save(%{org: "myorg"})
      assert File.exists?(ConfigFile.config_path())
      ConfigFile.delete()
      refute File.exists?(ConfigFile.config_path())
    end

    test "does not error when no config exists" do
      refute File.exists?(ConfigFile.config_path())
      assert match?({:ok, _}, ConfigFile.delete())
    end
  end

  describe "configured?/0" do
    test "returns false when no config" do
      refute ConfigFile.configured?()
    end

    test "returns true when config exists" do
      ConfigFile.save(%{org: "myorg"})
      assert ConfigFile.configured?()
    end
  end

  describe "config_path/0" do
    test "returns Application-overridden path when set" do
      tmp = "/tmp/ado_cli_overridden_#{System.unique_integer([:positive])}.json"
      Application.put_env(:ado_cli, :config_path, tmp)
      on_exit(fn -> Application.delete_env(:ado_cli, :config_path) end)

      assert ConfigFile.config_path() == tmp
    end

    test "returns default path when not overridden" do
      Application.delete_env(:ado_cli, :config_path)
      assert ConfigFile.config_path() == Path.join(Path.expand("~/.ado_cli"), "config.json")
    end
  end
end
