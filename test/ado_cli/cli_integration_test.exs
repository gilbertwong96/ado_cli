defmodule AdoCli.CLIIntegrationTest do
  @moduledoc """
  End-to-end integration tests for the CLI dispatcher.

  These run CliMate's parser + dispatch with full command-line
  arguments, exercising the entire CLI surface. The HTTP layer
  is mocked via TestServer (Bandit).
  """
  use ExUnit.Case, async: false

  alias AdoCli.CLI
  alias AdoCli.TestServer

  setup do
    start_supervised!({Finch, name: AdoCli.Finch, pools: %{default: [size: 1, count: 1]}})
    server = start_supervised!({TestServer, []})

    System.put_env("ADO_SERVER", TestServer.url(server))
    System.put_env("ADO_ORG", "testorg")
    System.put_env("ADO_PAT", "testpat")

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

  describe "projects list via CLI dispatch" do
    test "ado projects list (JSON)", %{server: server} do
      body = ~s({"value":[{"id":"p1","name":"Project One"}],"count":1})

      TestServer.expect(server, "GET", api("/_apis/projects"), fn conn ->
        Plug.Conn.resp(conn, 200, body)
      end)

      CLI.run(["projects", "list", "--json"])
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "pipelines list via CLI dispatch" do
    test "ado pipelines list", %{server: server} do
      body = ~s({"value":[{"id":1,"name":"Pipeline1","folder":""}],"count":1})

      TestServer.expect(server, "GET", api("/testorg/_apis/pipelines"), fn conn ->
        Plug.Conn.resp(conn, 200, body)
      end)

      CLI.run(["pipelines", "list", "testorg", "--json"])
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "work items list via CLI dispatch" do
    test "ado workitems list (uses WIQL)", %{server: server} do
      body = ~s({"workItems":[{"id":1}]})

      TestServer.expect(server, "POST", api("/testorg/_apis/wit/wiql"), fn conn ->
        Plug.Conn.resp(conn, 200, body)
      end)

      CLI.run(["workitems", "list", "testorg", "--wiql", "SELECT [System.Id] FROM WorkItems", "--json"])
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "repos list via CLI dispatch" do
    test "ado repos list", %{server: server} do
      body = ~s({"value":[{"id":"r1","name":"Repo1"}]})

      TestServer.expect(server, "GET", api("/testorg/_apis/git/repositories"), fn conn ->
        Plug.Conn.resp(conn, 200, body)
      end)

      CLI.run(["repos", "list", "testorg", "--json"])
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "whoami via CLI dispatch" do
    test "ado whoami (no HTTP needed)", %{server: _server} do
      CLI.run(["whoami"])
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "global --org option" do
    test "ado --org X projects list (overrides env)", %{server: server} do
      body = ~s({"value":[],"count":0})

      TestServer.expect(server, "GET", "/other-org/_apis/projects", fn conn ->
        Plug.Conn.resp(conn, 200, body)
      end)

      CLI.run(["--org", "other-org", "projects", "list", "--json"])
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end
end
