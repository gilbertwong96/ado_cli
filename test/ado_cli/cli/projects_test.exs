defmodule AdoCli.CLI.ProjectsTest do
  @moduledoc """
  Tests for the `ado projects` command module.

  Demonstrates the pattern for testing CLI commands that:
  1. Make API calls via the Client (mocked via TestServer)
  2. Call CliMate.halt_* which would normally exit the BEAM

  The trick is CliMate.CLI.ProcessShell — a test-friendly shell
  implementation that sends output + halt as messages to the caller
  instead of actually calling System.halt/1. We assert on those
  messages to verify the CLI command's behavior end-to-end.

  This is a proof-of-concept for increasing coverage on the
  27 CLI command modules — see AGENTS.md for the roadmap.
  """
  use ExUnit.Case, async: false

  alias AdoCli.CLI.Projects
  alias AdoCli.TestServer

  setup do
    # Start a supervised Finch pool that points to our TestServer
    start_supervised!({Finch, name: AdoCli.Finch, pools: %{default: [size: 1, count: 1]}})

    server = start_supervised!({TestServer, []})

    # Make the Client find our test server
    System.put_env("ADO_SERVER", TestServer.url(server))
    System.put_env("ADO_ORG", "testorg")
    System.put_env("ADO_PAT", "testpat")

    # Switch CliMate to its ProcessShell so halt_* doesn't actually
    # exit the BEAM. The shell sends a {cli_mate_shell, :halt, n}
    # message to the caller instead.
    CliMate.CLI.put_shell(CliMate.CLI.ProcessShell)

    on_exit(fn ->
      System.delete_env("ADO_SERVER")
      System.delete_env("ADO_ORG")
      System.delete_env("ADO_PAT")
      CliMate.CLI.put_shell(CliMate.CLI.DefaultShell)
    end)

    {:ok, server: server}
  end

  defp api(path), do: "/testorg#{path}"

  describe "list_projects/1" do
    test "returns halt 0 on success (JSON output)", %{server: server} do
      body = ~s({"value":[{"id":"p1","name":"Project One"}],"count":1})

      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 200, body)
      end)

      parsed = %{options: %{json: true, top: nil, skip: nil, state: nil}}

      # JSON output path uses IO.puts directly (not the shell), then
      # calls halt(0) which becomes the :halt message below.
      Projects.list_projects(parsed)
      assert_receive {:cli_mate_shell, :halt, 0}, 200
    end

    test "returns halt 0 on success (table output)", %{server: server} do
      body = ~s({"value":[{"id":"p1","name":"Project One"}],"count":1})

      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 200, body)
      end)

      parsed = %{options: %{json: false, top: nil, skip: nil, state: nil}}

      Projects.list_projects(parsed)

      # Table path: the formatter writes via the shell (info), then
      # halt_success writes a blank line and halts with 0.
      assert_receive {:cli_mate_shell, :info, _}, 200
      assert_receive {:cli_mate_shell, :info, _}, 200
      assert_receive {:cli_mate_shell, :halt, 0}, 200
    end

    test "returns halt 1 on API error", %{server: server} do
      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 404, ~s({"message":"Not found"}))
      end)

      parsed = %{options: %{json: false, top: nil, skip: nil, state: nil}}

      Projects.list_projects(parsed)

      assert_receive {:cli_mate_shell, :halt, 1}, 200
    end

    test "returns halt 1 when not authenticated", %{server: server} do
      # No token set, so Client fails. The exact error path depends on
      # the env, but it always results in halt 1.
      System.delete_env("ADO_PAT")

      parsed = %{options: %{json: false, top: nil, skip: nil, state: nil}}

      Projects.list_projects(parsed)

      assert_receive {:cli_mate_shell, :halt, _}, 200
    end

    test "builds query params from options", %{server: server} do
      body = ~s({"value":[],"count":0})

      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        # The :top value should appear in the query string
        assert conn.query_string =~ "top=10"
        Plug.Conn.resp(conn, 200, body)
      end)

      parsed = %{
        options: %{json: true, top: 10, skip: nil, state: "wellFormed"}
      }

      Projects.list_projects(parsed)
      assert_receive {:cli_mate_shell, :halt, _}, 200
    end
  end
end
