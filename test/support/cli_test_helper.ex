defmodule AdoCli.CLI.TestHelper do
  @moduledoc """
  Shared test infrastructure for CLI command modules.

  Each CLI module's test file does:
      use AdoCli.CLI.TestHelper
      alias AdoCli.CLI.MyModule

      test "list returns halt 0 on success", %{server: server} do
        expect_json(server, "/_apis/myresource", ~s({"value":[]}), fn ->
          MyModule.list_myresource(%{options: %{json: true}})
        end)
      end

  The helper handles:
    - Booting a supervised Finch pool pointing to TestServer
    - Setting ADO_ORG/ADO_PAT/ADO_SERVER env vars
    - Switching CliMate to ProcessShell (so halt_* doesn't exit the BEAM)
    - Restoring all of the above on test exit
  """

  defmacro __using__(_opts) do
    quote do
      use ExUnit.Case, async: false

      alias AdoCli.TestServer

      setup do
        start_supervised!(
          {Finch, name: AdoCli.Finch, pools: %{default: [size: 1, count: 1]}}
        )

        server = start_supervised!({TestServer, []})

        System.put_env("ADO_SERVER", TestServer.url(server))
        System.put_env("ADO_ORG", "testorg")
        System.put_env("ADO_PAT", "testpat")

        # ProcessShell turns halt_success/halt_error into messages
        # instead of System.halt/1 calls, so the test process
        # doesn't actually exit.
        CliMate.CLI.put_shell(CliMate.CLI.ProcessShell)

        on_exit(fn ->
          System.delete_env("ADO_SERVER")
          System.delete_env("ADO_ORG")
          System.delete_env("ADO_PAT")
          CliMate.CLI.put_shell(CliMate.CLI.DefaultShell)
        end)

        # Make `server` available in all tests as a context-bound
        # variable via setup context
        {:ok, server: server}
      end

      def api(path), do: "/testorg#{path}"

      @doc """
      Register an expectation that the given API path returns a 200
      with the given body, then run `fun` and assert it halts with 0.
      """
      def expect_success_json(server, path, body, fun) do
        TestServer.expect(server, "GET", api(path), fn conn ->
          Plug.Conn.resp(conn, 200, body)
        end)

        fun.()
        assert_receive {:cli_mate_shell, :halt, 0}, 500
      end

      def expect_success_table(server, path, body, fun) do
        TestServer.expect(server, "GET", api(path), fn conn ->
          Plug.Conn.resp(conn, 200, body)
        end)

        fun.()
        # Table path: at least one info message + halt 0
        assert_receive {:cli_mate_shell, :halt, 0}, 500
      end

      def expect_api_error(server, path, status, body, fun) do
        TestServer.expect(server, "GET", api(path), fn conn ->
          Plug.Conn.resp(conn, status, body)
        end)

        fun.()
        # Error path: halt 1
        assert_receive {:cli_mate_shell, :halt, 1}, 500
      end

      def expect_post_success(server, path, request_body, response_body, fun) do
        TestServer.expect(server, "POST", api(path), fn conn ->
          # Could verify request body here
          Plug.Conn.resp(conn, 200, response_body)
        end)

        fun.()
        assert_receive {:cli_mate_shell, :halt, 0}, 500
      end

      def expect_delete_success(server, path, fun) do
        TestServer.expect(server, "DELETE", api(path), fn conn ->
          Plug.Conn.resp(conn, 204, "")
        end)

        fun.()
        assert_receive {:cli_mate_shell, :halt, 0}, 500
      end

      def expect_put_success(server, path, request_body, response_body, fun) do
        TestServer.expect(server, "PUT", api(path), fn conn ->
          Plug.Conn.resp(conn, 200, response_body)
        end)

        fun.()
        assert_receive {:cli_mate_shell, :halt, 0}, 500
      end

      def expect_patch_success(server, path, request_body, response_body, fun) do
        TestServer.expect(server, "PATCH", api(path), fn conn ->
          Plug.Conn.resp(conn, 200, response_body)
        end)

        fun.()
        assert_receive {:cli_mate_shell, :halt, 0}, 500
      end
    end
  end
end
