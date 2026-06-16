defmodule AdoCli.CLI.OutputTest do
  use ExUnit.Case, async: false

  import ExUnit.CaptureIO
  alias AdoCli.CLI.Output

  setup do
    # Switch to ProcessShell so halt_* doesn't exit the test BEAM
    CliMate.CLI.put_shell(CliMate.CLI.ProcessShell)
    on_exit(fn -> CliMate.CLI.put_shell(CliMate.CLI.DefaultShell) end)
    :ok
  end

  # ── ok/4 (success envelope) ─────────────────────────────────────────

  describe "ok/4 — single value (--json)" do
    test "wraps a value in {ok: true, result: ...}" do
      parsed = %{options: %{json: true}}

      json =
        capture_io(fn ->
          Output.ok(parsed, %{name: "ado", version: "0.1.0"}, :value, fn _ -> :ok end)
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == true
      assert decoded["result"] == %{"name" => "ado", "version" => "0.1.0"}
    end

    test "halts with exit 0" do
      parsed = %{options: %{json: true}}
      capture_io(fn -> Output.ok(parsed, "ok", :value, fn _ -> :ok end) end)
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "calls the formatter when --json is NOT set" do
      parent = self()
      parsed = %{options: %{json: false}}
      Output.ok(parsed, "data", :value, fn val -> send(parent, {:formatted, val}) end)

      assert_receive {:formatted, "data"}, 500
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "ok/4 — list (--json)" do
    test "wraps a list in {ok: true, count: N, items: [...]}" do
      parsed = %{options: %{json: true}}

      json = capture_io(fn -> Output.ok(parsed, ["a", "b", "c"], :list, fn _ -> :ok end) end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == true
      assert decoded["count"] == 3
      assert decoded["items"] == ["a", "b", "c"]
    end

    test "handles empty list" do
      parsed = %{options: %{json: true}}

      json = capture_io(fn -> Output.ok(parsed, [], :list, fn _ -> :ok end) end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["count"] == 0
      assert decoded["items"] == []
    end
  end

  # ── error/4 (error envelope) ────────────────────────────────────────

  describe "error/4 — JSON envelope" do
    test "emits a structured error with code, message, status, details" do
      parsed = %{options: %{json: true}}

      json =
        capture_io(fn ->
          Output.error(
            parsed,
            "not_found",
            "Project 'foo' not found.",
            status: 404,
            details: %{"project" => "foo"}
          )
        end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == false
      assert decoded["error"]["code"] == "not_found"
      assert decoded["error"]["message"] =~ "Project 'foo' not found"
      assert decoded["error"]["status"] == 404
      assert decoded["error"]["details"] == %{"project" => "foo"}
    end

    test "halts with exit 1 (consistent with the rest of the CLI)" do
      parsed = %{options: %{json: true}}
      capture_io(fn -> Output.error(parsed, "validation_error", "bad input") end)
      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end

    test "works without --json (human-readable path)" do
      parsed = %{options: %{json: false}}
      capture_io(fn -> Output.error(parsed, "not_found", "Project 'foo' not found.") end)
      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end

    test "omits status field when not provided" do
      parsed = %{options: %{json: true}}

      json = capture_io(fn -> Output.error(parsed, "validation_error", "missing arg") end)

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      refute Map.has_key?(decoded["error"], "status")
    end
  end

  # ── error code → message classification ───────────────────────────────

  describe "stable error codes" do
    test "each documented error code produces a parseable message" do
      codes = [
        "auth_required",
        "not_found",
        "validation_error",
        "api_error",
        "network_error",
        "forbidden",
        "conflict",
        "cancelled"
      ]

      Enum.each(codes, fn code ->
        parsed = %{options: %{json: true}}

        json = capture_io(fn -> Output.error(parsed, code, "test message for #{code}") end)

        assert_receive {:cli_mate_shell, :halt, 1}, 500
        assert {:ok, decoded} = JSON.decode(String.trim(json))
        assert decoded["error"]["code"] == code
      end)
    end
  end

  # ── ok_message/2 ────────────────────────────────────────────────────

  describe "ok_message/2" do
    test "emits {ok: true, message: ...} with --json" do
      parsed = %{options: %{json: true}}

      json = capture_io(fn -> Output.ok_message(parsed, "Logged out successfully") end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == true
      assert decoded["message"] == "Logged out successfully"
    end

    test "halts 0" do
      parsed = %{options: %{json: true}}
      capture_io(fn -> Output.ok_message(parsed, "done") end)
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  # ── raw/2 (backward compat) ────────────────────────────────────────

  describe "raw/2" do
    test "emits the value as-is with --json (no envelope)" do
      parsed = %{options: %{json: true}}

      json = capture_io(fn -> Output.raw(parsed, %{"foo" => "bar"}) end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded == %{"foo" => "bar"}
    end
  end
end
