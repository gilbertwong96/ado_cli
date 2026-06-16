defmodule AdoCli.CLI.CITest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.CI

  describe "ado ci watch (regression: v0.2.1 bugs)" do
    # Two bugs broke ci watch in v0.2.1:
    #   1. build_path/3 dropped both project and org, producing
    #      /_apis/build/builds/{id} (no project) → API 400
    #      "The project with id 'No project was specified.'"
    #   2. resolve_build_id/3 used `parsed.arguments.build_id`
    #      dot-access, which crashes with KeyError when the
    #      optional `build_id` argument is absent (e.g. when
    #      using --latest).
    # These tests pin both bugs so they don't regress.

    test "watch with explicit build_id hits /{project}/_apis/build/builds/{id}", %{
      server: server
    } do
      # The watcher polls in a loop. We register a single
      # expectation for the first URL it should hit, then a
      # catch-all that records the request path so we can
      # inspect what URLs the watcher tried to hit.
      test_pid = self()

      TestServer.expect(
        server,
        "GET",
        "/testorg/MyProject/_apis/build/builds/9652",
        fn conn ->
          send(test_pid, {:hit, conn.request_path})
          Plug.Conn.resp(conn, 200, ~s({"id":9652,"status":"running"}))
        end
      )

      # Fallback: catch any other request and send the URL to
      # the test process so we can assert on it (and return a
      # non-running status so the watcher exits the loop).
      TestServer.expect(
        server,
        "GET",
        # Wildcard fallback — we register this AFTER the
        # exact match; TestServer's pop is FIFO so the
        # exact path wins first.
        "/__catch_all__",
        fn conn ->
          send(test_pid, {:hit, conn.request_path})
          Plug.Conn.resp(conn, 200, ~s({"id":9652,"status":"completed"}))
        end
      )

      parsed = %{
        arguments: %{project: "MyProject", build_id: 9652},
        options: %{org: nil, latest: false, definition: nil, branch: nil, "poll-interval": 5000}
      }

      try do
        CI.watch(parsed)
      catch
        _kind, _value -> :ok
      end

      # The first URL hit should be the project-scoped one.
      assert_receive {:hit, "/testorg/MyProject/_apis/build/builds/9652"}, 1000
    end

    test "watch with --latest hits /{project}/_apis/build/builds (no KeyError)", %{
      server: server
    } do
      # If resolve_build_id/3 still uses dot-access, this
      # test would crash with KeyError before any HTTP call.
      test_pid = self()

      TestServer.expect(
        server,
        "GET",
        "/testorg/MyProject/_apis/build/builds",
        fn conn ->
          send(test_pid, {:hit, conn.request_path})
          Plug.Conn.resp(conn, 200, ~s({"value":[{"id":42}]}))
        end
      )

      # Fallback: the watcher will keep polling build 42 in a
      # loop, so register a catch-all for subsequent polls.
      TestServer.expect(
        server,
        "GET",
        "/__catch_all__",
        fn conn ->
          send(test_pid, {:hit, conn.request_path})
          Plug.Conn.resp(conn, 200, ~s({"id":42,"status":"completed"}))
        end
      )

      parsed = %{
        arguments: %{project: "MyProject"},
        options: %{
          org: nil,
          latest: true,
          definition: 7,
          branch: "refs/heads/main",
          "poll-interval": 5000
        }
      }

      try do
        CI.watch(parsed)
      catch
        _kind, _value -> :ok
      end

      # Reaching assert_receive proves resolve_build_id/3 didn't
      # raise KeyError on the missing :build_id key, and that
      # the URL it generated was project-scoped.
      assert_receive {:hit, "/testorg/MyProject/_apis/build/builds"}, 1000
    end
  end
end
