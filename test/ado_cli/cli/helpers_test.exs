defmodule AdoCli.CLI.HelpersTest do
  use ExUnit.Case, async: false
  import ExUnit.CaptureIO

  alias AdoCli.CLI.Helpers

  setup do
    CliMate.CLI.put_shell(CliMate.CLI.ProcessShell)
    on_exit(fn -> CliMate.CLI.put_shell(CliMate.CLI.DefaultShell) end)
    :ok
  end

  describe "classify_api_error/2" do
    test "302 → auth_required" do
      assert {"auth_required", msg} = Helpers.classify_api_error(302, "<html>sign in</html>")
      assert msg =~ "sign-in"
    end

    test "401 → auth_required" do
      assert {"auth_required", _} = Helpers.classify_api_error(401, "token expired")
      assert {code, _} = Helpers.classify_api_error(401, "anything")
      assert code == "auth_required"
    end

    test "403 → forbidden (PAT lacks scope)" do
      assert {"forbidden", msg} = Helpers.classify_api_error(403, "missing scope")
      assert msg =~ "scope"
    end

    test "404 → not_found" do
      assert {"not_found", _} = Helpers.classify_api_error(404, "no such project")
    end

    test "409 → conflict" do
      assert {"conflict", _} = Helpers.classify_api_error(409, "already exists")
    end

    test "422 → validation_error" do
      assert {"validation_error", _} = Helpers.classify_api_error(422, "bad request")
    end

    test "429 → forbidden (rate limited)" do
      assert {"forbidden", msg} = Helpers.classify_api_error(429, "rate limited")
      assert msg =~ "Rate limit"
    end

    test "5xx → api_error (server error, retry later)" do
      for status <- [500, 502, 503, 504] do
        assert {"api_error", msg} = Helpers.classify_api_error(status, "oops")
        assert msg =~ "Retry later" or msg =~ "API error"
      end
    end

    test "unknown status falls back to api_error with status in message" do
      assert {"api_error", msg} = Helpers.classify_api_error(418, "teapot")
      assert msg =~ "418"
    end
  end

  describe "classify_network_error/1" do
    test ":timeout → network_error" do
      assert {"network_error", msg} = Helpers.classify_network_error(:timeout)
      assert msg =~ "timed out"
    end

    test ":nxdomain → network_error" do
      assert {"network_error", msg} = Helpers.classify_network_error(:nxdomain)
      assert msg =~ "DNS"
    end

    test ":econnrefused → network_error" do
      assert {"network_error", msg} = Helpers.classify_network_error(:econnrefused)
      assert msg =~ "refused"
    end

    test "unknown atom → network_error with reason in message" do
      assert {"network_error", msg} = Helpers.classify_network_error(:weird_thing)
      assert msg =~ "weird_thing"
    end

    test "non-atom reason → network_error with inspect" do
      assert {"network_error", msg} = Helpers.classify_network_error({:bad_response, "<html>"})
      assert msg =~ "bad_response"
    end
  end

  describe "handle_api_result/3 — error path is structured JSON" do
    test "renders :not_configured as auth_required with helpful details" do
      json =
        capture_io(fn ->
          Helpers.handle_api_result(
            {:error, :not_configured},
            %{options: %{json: true}},
            fn _ -> :ok end
          )
        end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == false
      assert decoded["error"]["code"] == "auth_required"
      assert decoded["error"]["details"]["scopes"] =~ "vso.work"
      assert decoded["error"]["details"]["hint"] =~ "ADO_ORG"
    end

    test "renders HTTP error as api_error with status and body" do
      json =
        capture_io(fn ->
          Helpers.handle_api_result(
            {:error, %{status: 404, body: "no such project"}},
            %{options: %{json: true}},
            fn _ -> :ok end
          )
        end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == false
      assert decoded["error"]["code"] == "not_found"
      assert decoded["error"]["status"] == 404
      assert decoded["error"]["details"]["body"] == "no such project"
    end

    test "renders network error as network_error" do
      json =
        capture_io(fn ->
          Helpers.handle_api_result(
            {:error, :nxdomain},
            %{options: %{json: true}},
            fn _ -> :ok end
          )
        end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["error"]["code"] == "network_error"
    end

    test "halts with exit 1 (consistent with the rest of the CLI)" do
      capture_io(fn ->
        Helpers.handle_api_result(
          {:error, :not_configured},
          %{options: %{json: true}},
          fn _ -> :ok end
        )
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end

    test "human-readable path doesn't emit JSON envelope" do
      capture_io(fn ->
        Helpers.handle_api_result(
          {:error, :not_configured},
          %{options: %{json: false}},
          fn _ -> :ok end
        )
      end)

      # No :info message (the JSON envelope), but :halt still fires
      refute_receive {:cli_mate_shell, :info, _}, 100
      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end
  end
end
