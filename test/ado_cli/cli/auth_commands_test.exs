defmodule AdoCli.CLI.AuthCommandsTest do
  @moduledoc """
  Tests for the `ado login` command dispatch logic.

  These tests verify that the auth method is correctly inferred from
  the flags provided, so that passing `--pat` without `--method pat`
  does not silently trigger a browser OAuth flow.
  """

  use ExUnit.Case, async: false

  alias AdoCli.CLI.AuthCommands
  alias AdoCli.ConfigFile

  setup do
    CliMate.CLI.put_shell(CliMate.CLI.ProcessShell)

    tmp_config =
      Path.join(
        System.tmp_dir!(),
        "ado_cli_login_test_#{System.unique_integer([:positive])}.json"
      )

    Application.put_env(:ado_cli, :config_path, tmp_config)
    ConfigFile.delete()

    on_exit(fn ->
      CliMate.CLI.put_shell(CliMate.CLI.DefaultShell)
      System.delete_env("ADO_ORG")
      System.delete_env("ADO_PAT")
      File.rm_rf(tmp_config)
      Application.delete_env(:ado_cli, :config_path)
    end)

    :ok
  end

  describe "login/1 method inference" do
    test "infers pat method when --pat is given without --method" do
      # CliMate omits unset string options from the map
      parsed = %{options: %{org: "myorg", pat: "tok123", server: nil}}

      AuthCommands.login(parsed)

      config = ConfigFile.load()
      assert config["method"] == "pat"
      assert config["org"] == "myorg"
      assert config["pat"] == "tok123"

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "infers pat method when ADO_PAT env var is set without --method" do
      System.put_env("ADO_PAT", "envtoken")

      parsed = %{options: %{org: "envorg", server: nil}}

      AuthCommands.login(parsed)

      config = ConfigFile.load()
      assert config["method"] == "pat"
      assert config["org"] == "envorg"
      assert config["pat"] == "envtoken"

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "does NOT open browser when --pat given without --method" do
      parsed = %{options: %{org: "myorg", pat: "tok123", server: nil}}

      AuthCommands.login(parsed)

      # If the browser path were taken, it would never reach halt_success
      # (it blocks on a TCP listener for 120s). halt 0 means the PAT path ran.
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "explicit --method pat still works" do
      parsed = %{options: %{org: "myorg", pat: "tok123", method: "pat", server: nil}}

      AuthCommands.login(parsed)

      config = ConfigFile.load()
      assert config["method"] == "pat"
      assert config["pat"] == "tok123"

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "explicit --method browser is respected even when --pat is present" do
      # When the user explicitly asks for browser, respect it even if --pat
      # is present (don't override an explicit choice).
      parsed = %{options: %{org: nil, pat: "tok123", method: "browser", server: nil}}

      # The browser flow blocks on a TCP listener for 120s. Run it in a
      # separate process with a short timeout to verify it does NOT complete
      # via the PAT path (which would save config and halt 0 immediately).
      caller = self()

      pid =
        spawn(fn ->
          AuthCommands.login(parsed)
          send(caller, :login_completed)
        end)

      refute_receive :login_completed, 100
      Process.exit(pid, :kill)

      refute ConfigFile.configured?()
    end
  end
end
