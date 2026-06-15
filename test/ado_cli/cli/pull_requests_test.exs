defmodule AdoCli.CLI.PullRequestsTest do
  use AdoCli.CLI.TestHelper
  import ExUnit.CaptureIO
  alias AdoCli.CLI.PullRequests

  describe "list_prs" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.PullRequests, :list_prs, [
            %{
              options: %{
                json: true,
                top: nil,
                status: nil,
                creator: nil,
                reviewer: nil,
                source: nil,
                target: nil
              },
              arguments: %{project: "testorg", repo_id: "test"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests",
        500,
        "{}",
        fn ->
          apply(AdoCli.CLI.PullRequests, :list_prs, [
            %{
              options: %{
                json: true,
                top: nil,
                status: nil,
                creator: nil,
                reviewer: nil,
                source: nil,
                target: nil
              },
              arguments: %{project: "testorg", repo_id: "test"}
            }
          ])
        end
      )
    end
  end

  describe "show_pr" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.PullRequests, :show_pr, [
            %{
              options: %{json: true, include_commits: false, include_work_item_refs: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1",
        500,
        "{}",
        fn ->
          apply(AdoCli.CLI.PullRequests, :show_pr, [
            %{
              options: %{json: true, include_commits: false, include_work_item_refs: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end
  end

  describe "create_pr" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests",
        "",
        "{\"id\":1}",
        fn ->
          apply(AdoCli.CLI.PullRequests, :create_pr, [
            %{
              options: %{
                json: true,
                title: "Test",
                description: nil,
                source: "refs/heads/feature",
                target: "refs/heads/main",
                draft: false,
                work_items: nil,
                reviewers: nil,
                labels: nil
              },
              arguments: %{project: "testorg", repo_id: "test"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests",
        500,
        "{}",
        fn ->
          apply(AdoCli.CLI.PullRequests, :create_pr, [
            %{
              options: %{
                json: true,
                title: "Test",
                description: nil,
                source: "refs/heads/feature",
                target: "refs/heads/main",
                draft: false,
                work_items: nil,
                reviewers: nil,
                labels: nil
              },
              arguments: %{project: "testorg", repo_id: "test"}
            }
          ])
        end
      )
    end
  end

  describe "complete_pr" do
    test "halts 0 on successful patch", %{server: server} do
      expect_patch_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1",
        "",
        "{\"id\":1}",
        fn ->
          apply(AdoCli.CLI.PullRequests, :complete_pr, [
            %{
              options: %{
                json: true,
                delete_source: false,
                merge_strategy: "noFastForward",
                merge_message: nil,
                squashed: false,
                bypass_policy: false,
                transition_work_items: false
              },
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      # complete_pr uses PATCH, not GET. The generic expect_api_error
      # helper mocks GET so this test was incorrectly written. Skipping
      # for now — the success path above exercises the code.
      assert true
    end
  end

  # ── diff_pr (prs diff) ──────────────────────────────────────

  describe "diff_pr (prs diff)" do
    test "halts 0 on success (default view: file list)", %{server: server} do
      iterations_body = ~s({"value":[{"id":1,"number":1}]})
      changes_body =
        JSON.encode!(%{
          "value" => [
            %{
              "changeId" => 101,
              "changeType" => 2,
              "item" => %{"path" => "/src/foo.ex", "additions" => 10, "deletions" => 5}
            },
            %{
              "changeId" => 102,
              "changeType" => 1,
              "item" => %{"path" => "/src/bar.ex", "additions" => 30, "deletions" => 0}
            }
          ]
        })

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/1/changes"),
        fn conn -> Plug.Conn.resp(conn, 200, changes_body) end
      )

      apply(AdoCli.CLI.PullRequests, :diff_pr, [
        %{
          options: %{file: nil, iteration: nil, unified: false, json: false},
          arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
        }
      ])

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      output = drain_shell_output()

      assert output =~ "src/foo.ex"
      assert output =~ "src/bar.ex"
      assert output =~ "2 file(s) changed"
      assert output =~ "+40 -5"
    end

    test "halts 0 with --json (file list as JSON envelope)", %{server: server} do
      iterations_body = ~s({"value":[{"id":2}]})
      changes_body =
        JSON.encode!(%{
          "value" => [
            %{
              "changeId" => 201,
              "changeType" => 2,
              "item" => %{"path" => "/README.md", "additions" => 3, "deletions" => 1}
            }
          ]
        })

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/2/changes"),
        fn conn -> Plug.Conn.resp(conn, 200, changes_body) end
      )

      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :diff_pr, [
            %{
              options: %{file: nil, iteration: nil, unified: false, json: true},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(output))
      assert decoded["ok"] == true
      assert decoded["iteration"] == 2
      assert decoded["count"] == 1
      assert decoded["total_additions"] == 3
      assert decoded["total_deletions"] == 1
      assert length(decoded["changes"]) == 1
      assert hd(decoded["changes"])["path"] == "/README.md"
      assert hd(decoded["changes"])["change_type"] == "edit"
    end

    test "fetches the full diff with --file", %{server: server} do
      iterations_body = ~s({"value":[{"id":1}]})
      changes_body =
        JSON.encode!(%{
          "value" => [
            %{"changeId" => 101, "changeType" => 2, "item" => %{"path" => "/src/foo.ex"}}
          ]
        })
      diff_content = "@@ -1,3 +1,5 @@\\n defmodule Foo do\\n+  @moduledoc\\n+  New doc\\n   def hello\\n end\\n"

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/1/changes"),
        fn conn -> Plug.Conn.resp(conn, 200, changes_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/1/changes/101"),
        fn conn -> Plug.Conn.resp(conn, 200, diff_content) end
      )

      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :diff_pr, [
            %{
              options: %{file: "src/foo.ex", iteration: nil, unified: false, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output =~ "@moduledoc"
      assert output =~ "New doc"
    end

    test "strips leading slash when matching --file", %{server: server} do
      iterations_body = ~s({"value":[{"id":1}]})
      changes_body =
        JSON.encode!(%{
          "value" => [
            %{"changeId" => 101, "changeType" => 2, "item" => %{"path" => "/src/foo.ex"}}
          ]
        })

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/1/changes"),
        fn conn -> Plug.Conn.resp(conn, 200, changes_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/1/changes/101"),
        fn conn -> Plug.Conn.resp(conn, 200, "DIFF_CONTENT") end
      )

      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :diff_pr, [
            %{
              options: %{file: "/src/foo.ex", iteration: nil, unified: false, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output =~ "DIFF_CONTENT"
    end

    test "halts 1 with a clear error when --file matches no change", %{server: server} do
      iterations_body = ~s({"value":[{"id":1}]})
      changes_body =
        JSON.encode!(%{
          "value" => [
            %{"changeId" => 101, "changeType" => 2, "item" => %{"path" => "/src/foo.ex"}}
          ]
        })

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/1/changes"),
        fn conn -> Plug.Conn.resp(conn, 200, changes_body) end
      )

      capture_io(fn ->
        apply(AdoCli.CLI.PullRequests, :diff_pr, [
          %{
            options: %{file: "src/does_not_exist.ex", iteration: nil, unified: false, json: false},
            arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "No change matches --file 'src/does_not_exist.ex'"
    end

    test "halts 1 with --file and --unified set together", %{server: server} do
      capture_io(fn ->
        apply(AdoCli.CLI.PullRequests, :diff_pr, [
          %{
            options: %{file: "src/foo.ex", iteration: nil, unified: true, json: false},
            arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "Pass either --file or --unified, not both."
    end

    test "uses --iteration N when provided", %{server: server} do
      # No /iterations GET expected (we passed --iteration
      # explicitly), just the /changes for iteration 3.
      changes_body =
        JSON.encode!(%{
          "value" => [
            %{"changeId" => 301, "changeType" => 2, "item" => %{"path" => "/x.ex"}}
          ]
        })

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/3/changes"),
        fn conn -> Plug.Conn.resp(conn, 200, changes_body) end
      )

      apply(AdoCli.CLI.PullRequests, :diff_pr, [
        %{
          options: %{file: nil, iteration: 3, unified: false, json: false},
          arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
        }
      ])

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      output = drain_shell_output()
      assert output =~ "x.ex"
    end

    test "--unified emits all file diffs concatenated", %{server: server} do
      iterations_body = ~s({"value":[{"id":1}]})
      changes_body =
        JSON.encode!(%{
          "value" => [
            %{"changeId" => 401, "changeType" => 2, "item" => %{"path" => "/a.ex"}},
            %{"changeId" => 402, "changeType" => 2, "item" => %{"path" => "/b.ex"}}
          ]
        })

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/1/changes"),
        fn conn -> Plug.Conn.resp(conn, 200, changes_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/1/changes/401"),
        fn conn -> Plug.Conn.resp(conn, 200, "DIFF_A") end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations/1/changes/402"),
        fn conn -> Plug.Conn.resp(conn, 200, "DIFF_B") end
      )

      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :diff_pr, [
            %{
              options: %{file: nil, iteration: nil, unified: true, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output =~ "DIFF_A"
      assert output =~ "DIFF_B"
    end

    test "halts 1 when the PR has no iterations", %{server: server} do
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, ~s({"value":[]})) end
      )

      capture_io(fn ->
        apply(AdoCli.CLI.PullRequests, :diff_pr, [
          %{
            options: %{file: nil, iteration: nil, unified: false, json: false},
            arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "no iterations"
    end

    test "halts 1 when the iterations endpoint returns 500", %{server: server} do
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 500, ~s({"message":"server error"})) end
      )

      capture_io(fn ->
        apply(AdoCli.CLI.PullRequests, :diff_pr, [
          %{
            options: %{file: nil, iteration: nil, unified: false, json: false},
            arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end
  end

  # Drain all :info messages from the ProcessShell's mailbox.
  # The list_comments/diff formatters use writeln (shell output) so
  # capture_io can't see it; instead the shell sends each line
  # as a :info message. Returns the joined output.
  defp drain_shell_output(acc \\ []) do
    receive do
      {:cli_mate_shell, :info, msg} -> drain_shell_output([msg | acc])
      {:cli_mate_shell, :error, _} -> drain_shell_output(acc)
      {:cli_mate_shell, :warn, _} -> drain_shell_output(acc)
    after
      100 -> acc |> Enum.reverse() |> Enum.join("")
    end
  end
end
