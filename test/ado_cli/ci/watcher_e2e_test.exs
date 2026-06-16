defmodule AdoCli.CI.WatcherE2ETest do
  @moduledoc """
  End-to-end verification of `AdoCli.CI.Watcher.watch/4`.

  Uses the existing `AdoCli.CLI.TestHelper` which starts a
  TestServer + Finch pool and configures CliMate to use
  ProcessShell. The watcher is spawned as a separate process
  (it loops forever) and we assert on the first batch of
  output, then kill it.
  """
  use AdoCli.CLI.TestHelper
  alias AdoCli.CI.Watcher

  test "watch/4 polls the project-scoped build URL and prints status", %{server: server} do
    parent = self()
    print = fn line -> send(parent, {:output, line}) end

    # Mock the build endpoint
    build_status = %{
      "id" => 9655,
      "status" => "inProgress",
      "result" => nil,
      "definition" => %{"name" => "ci"},
      "sourceBranch" => "refs/heads/main"
    }

    TestServer.expect(server, "GET", "/testorg/MyProject/_apis/build/builds/9655", fn conn ->
      Plug.Conn.resp(conn, 200, Jason.encode!(build_status))
    end)

    TestServer.expect(
      server,
      "GET",
      "/testorg/MyProject/_apis/build/builds/9655/timeline",
      fn conn ->
        Plug.Conn.resp(conn, 200, Jason.encode!(%{"records" => []}))
      end
    )

    # Trap exits so we can report the crash reason if the watcher dies
    Process.flag(:trap_exit, true)

    ref =
      spawn(fn ->
        Watcher.watch(9655, "MyProject", "testorg", poll_ms: 200, print_callback: print)
      end)

    # Wait for the first status output (watcher polls every 200ms)
    first_line =
      Enum.reduce_while(1..20, nil, fn _i, acc ->
        receive do
          {:output, line} -> {:halt, line}
          {:EXIT, ^ref, reason} -> {:halt, {:crashed, reason}}
        after
          100 -> {:cont, acc}
        end
      end)

    Process.exit(ref, :kill)

    case first_line do
      nil ->
        flunk("Expected at least one status output within 2s; watcher produced nothing")

      {:crashed, reason} ->
        flunk("""
        Watcher process crashed: #{inspect(reason)}

        If BadArityError: the print callback was called with wrong arity.
        """)

      line ->
        assert line =~ "Build 9655"
        assert line =~ "running for"
        assert line =~ "\n"
    end
  end

  test "watch/4 with the DEFAULT print callback does not crash (BadArityError regression)", %{
    server: server
  } do
    # This is the actual v0.2.2 bug:
    #   (BadArityError) &IO.write/2 with arity 2 called with 1 argument
    # The default :print_callback was &IO.write/2 (2-arity) but
    # render_status/3 called it as `print.(line)` (1-arity). The
    # fix changed the default to &IO.write(:stdio, &1) which is
    # 1-arity. This test runs the watcher with NO print_callback
    # (using the default) and asserts it doesn't crash.
    TestServer.expect(server, "GET", "/testorg/MyProject/_apis/build/builds/9655", fn conn ->
      Plug.Conn.resp(
        conn,
        200,
        Jason.encode!(%{
          "id" => 9655,
          "status" => "inProgress",
          "result" => nil,
          "definition" => %{"name" => "ci"},
          "sourceBranch" => "refs/heads/main"
        })
      )
    end)

    TestServer.expect(
      server,
      "GET",
      "/testorg/MyProject/_apis/build/builds/9655/timeline",
      fn conn ->
        Plug.Conn.resp(conn, 200, Jason.encode!(%{"records" => []}))
      end
    )

    # Trap exits so we can report the crash reason
    Process.flag(:trap_exit, true)

    # NO :print_callback — uses the default
    ref =
      spawn(fn ->
        Watcher.watch(9655, "MyProject", "testorg", poll_ms: 200)
      end)

    # Wait long enough for at least 2 polls (200ms each)
    Process.sleep(600)

    # Check for crash
    crashed =
      receive do
        {:EXIT, ^ref, reason} -> {:crashed, reason}
      after
        0 -> :alive
      end

    Process.exit(ref, :kill)

    case crashed do
      {:crashed, reason} ->
        flunk("""
        Watcher process crashed with default print callback: #{inspect(reason)}

        This is the v0.2.2 bug. The default :print_callback should
        be a 1-arity function (e.g. &IO.write(:stdio, &1)).
        """)

      :alive ->
        :ok
    end
  end
end
