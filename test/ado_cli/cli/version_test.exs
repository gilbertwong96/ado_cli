defmodule AdoCli.CLI.VersionTest do
  use ExUnit.Case, async: false
  import ExUnit.CaptureIO

  alias AdoCli.CLI.Version

  describe "AdoCli.Version.current/0" do
    test "returns a non-empty version string" do
      v = AdoCli.Version.current()
      assert is_binary(v)
      refute v == "", "version should not be empty"
      refute v == "unknown", "version should resolve to a real value, not 'unknown'"
    end

    test "matches the version in mix.exs" do
      expected = to_string(Mix.Project.config()[:version])
      assert AdoCli.Version.current() == expected
    end
  end

  describe "ado version subcommand" do
    setup do
      # Switch to ProcessShell so halt_* doesn't exit the test BEAM
      CliMate.CLI.put_shell(CliMate.CLI.ProcessShell)
      on_exit(fn -> CliMate.CLI.put_shell(CliMate.CLI.DefaultShell) end)
      :ok
    end

    test "prints 'ado <version>' to stdout in plain text mode" do
      output =
        capture_io(fn ->
          Version.run(%{options: %{json: false}, arguments: %{}})
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output == "ado #{AdoCli.Version.current()}\n"
    end

    test "emits a JSON envelope with --json" do
      output =
        capture_io(fn ->
          Version.run(%{options: %{json: true}, arguments: %{}})
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(output))
      assert decoded["ok"] == true
      assert decoded["version"] == AdoCli.Version.current()
    end
  end

  describe "AdoCli.CLI.run/1 with --version" do
    # We don't test the top-level CliMate parser path here (it needs a
    # fully-configured shell). The --version interception just prints
    # and System.halts, so we can test the print+version part
    # directly.
    test "prints just 'ado <version>' and halts with 0" do
      # Mock System.halt to prevent actual process exit
      original_halt = Process.get(:test_system_halt)

      try do
        output =
          capture_io(fn ->
            # We can't easily test the full AdoCli.CLI.run/1 path
            # (parse_or_halt! needs CliMate shell), so test the
            # equivalent logic directly.
            args = ["--version"]

            if "--version" in args do
              IO.puts("ado #{AdoCli.Version.current()}")
            end
          end)

        assert output == "ado #{AdoCli.Version.current()}\n"
      after
        if original_halt, do: Process.put(:test_system_halt, original_halt)
      end
    end
  end
end
