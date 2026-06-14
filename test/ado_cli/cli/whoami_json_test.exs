defmodule AdoCli.CLI.WhoamiJsonTest do
  # Targeted test for the --json output path of whoami, which was
  # previously broken (the command wrote human-readable text instead
  # of JSON, regardless of the --json flag). This test exercises the
  # new structured output path.
  use ExUnit.Case, async: false
  use AdoCli.CLI.TestHelper
  import ExUnit.CaptureIO

  alias AdoCli.CLI.{Whoami, Logout, AuthCommands}

  describe "whoami --json (the path that was broken)" do
    test "emits a structured {ok: true, result: ...} envelope" do
      json =
        capture_io(fn ->
          Whoami.run(%{options: %{json: true}, arguments: %{}})
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == true
      assert is_map(decoded["result"])
      assert Map.has_key?(decoded["result"], "configured")
      assert Map.has_key?(decoded["result"], "org")
      assert Map.has_key?(decoded["result"], "server")
      assert Map.has_key?(decoded["result"], "method")
      assert Map.has_key?(decoded["result"], "config_file")
      assert Map.has_key?(decoded["result"], "authenticated")
    end

    test "still works without --json (human-readable path)" do
      # Without --json, whoami prints the table to stdout and halts 0.
      Whoami.run(%{options: %{json: false}, arguments: %{}})
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "auth_commands (login) -- validation errors" do
    # The TestHelper sets ADO_ORG=testorg in the env, so we need to
    # explicitly unset it to test the "no org provided" path.
    setup do
      System.delete_env("ADO_ORG")
      System.delete_env("ADO_PAT")

      on_exit(fn ->
        System.put_env("ADO_ORG", "testorg")
        System.put_env("ADO_PAT", "testpat")
      end)

      :ok
    end

    test "missing --org for pat method → halts 1 with validation_error" do
      json =
        capture_io(fn ->
          AuthCommands.run(%{
            options: %{method: "pat", org: nil, pat: "fake", json: true},
            arguments: %{}
          })
        end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == false
      assert decoded["error"]["code"] == "validation_error"
      assert decoded["error"]["message"] =~ "--org"
      assert decoded["error"]["details"]["option"] == "--org"
    end

    test "missing --pat for pat method → halts 1 with validation_error" do
      json =
        capture_io(fn ->
          AuthCommands.run(%{
            options: %{method: "pat", org: "myorg", pat: nil, json: true},
            arguments: %{}
          })
        end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["error"]["code"] == "validation_error"
      assert decoded["error"]["message"] =~ "--pat"
    end

    test "unknown method → halts 1 with validation_error + valid_methods list" do
      json =
        capture_io(fn ->
          AuthCommands.run(%{
            options: %{method: "magic", org: "myorg", json: true},
            arguments: %{}
          })
        end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["error"]["code"] == "validation_error"
      assert decoded["error"]["details"]["valid_methods"] == ["browser", "pat", "device"]
    end
  end

  describe "logout --json" do
    test "emits a structured {ok: true, message: ...} envelope" do
      json =
        capture_io(fn ->
          Logout.run(%{options: %{json: true}, arguments: %{}})
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == true
      assert decoded["message"] =~ "Logged out"
    end
  end
end
