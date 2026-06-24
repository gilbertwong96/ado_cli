defmodule AdoCli.CLI.PullRequestsTest do
  use AdoCli.CLI.TestHelper
  import ExUnit.CaptureIO

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
    test "halts 0 on successful complete (GET pr + PATCH)", %{server: server} do
      # Register the GET expectation first (to fetch lastMergeSourceCommit)
      pr_data = %{
        "pullRequestId" => 1,
        "status" => "active",
        "lastMergeSourceCommit" => %{"commitId" => "abc123def456"}
      }

      TestServer.expect(
        server,
        "GET",
        "/testorg/MyProject/_apis/git/repositories/test/pullrequests/1",
        fn conn -> Plug.Conn.resp(conn, 200, JSON.encode!(pr_data)) end
      )

      # Then the PATCH to complete it
      expect_patch_success(
        server,
        "/MyProject/_apis/git/repositories/test/pullrequests/1",
        "",
        JSON.encode!(%{"pullRequestId" => 1, "status" => "completed"}),
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
              arguments: %{project: "MyProject", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end

    test "halts 1 when the PR has no lastMergeSourceCommit", %{server: server} do
      pr_data = %{"pullRequestId" => 1, "status" => "active"}

      TestServer.expect(
        server,
        "GET",
        "/testorg/MyProject/_apis/git/repositories/test/pullrequests/1",
        fn conn -> Plug.Conn.resp(conn, 200, JSON.encode!(pr_data)) end
      )

      capture_io(fn ->
        apply(AdoCli.CLI.PullRequests, :complete_pr, [
          %{
            options: %{json: false, delete_source: false, merge_strategy: nil},
            arguments: %{project: "MyProject", repo_id: "test", pr_id: 1}
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
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
              "changeType" => "edit",
              "item" => %{"path" => "/src/foo.ex", "additions" => 10, "deletions" => 5}
            },
            %{
              "changeId" => 102,
              "changeType" => "add",
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
              "changeType" => "edit",
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
      assert match?([_], decoded["changes"])
      assert hd(decoded["changes"])["path"] == "/README.md"
      assert hd(decoded["changes"])["change_type"] == "edit"
    end

    test "fetches the full diff with --file", %{server: server} do
      iterations_body =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "sourceRefCommit" => %{"commitId" => "src"},
              "targetRefCommit" => %{"commitId" => "tgt"}
            }
          ]
        })

      changes_body =
        JSON.encode!(%{
          "changeEntries" => [
            %{"changeId" => 101, "changeType" => "edit", "item" => %{"path" => "/src/foo.ex"}}
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

      # fetch_iteration_data calls iterations again
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      # Old file content (targetRefCommit)
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 200, "defmodule Foo do\n  def hello\nend\n") end
      )

      # New file content (sourceRefCommit)
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn ->
          Plug.Conn.resp(
            conn,
            200,
            "defmodule Foo do\n  @moduledoc\n  New doc\n  def hello\nend\n"
          )
        end
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
      assert output =~ "+++ b/src/foo.ex"
      assert output =~ "@moduledoc"
    end

    test "strips leading slash when matching --file", %{server: server} do
      iterations_body =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "sourceRefCommit" => %{"commitId" => "src"},
              "targetRefCommit" => %{"commitId" => "tgt"}
            }
          ]
        })

      changes_body =
        JSON.encode!(%{
          "changeEntries" => [
            %{"changeId" => 101, "changeType" => "edit", "item" => %{"path" => "/src/foo.ex"}}
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
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 200, "old") end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 200, "new") end
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
      assert output =~ "diff --git"
    end

    test "halts 1 with a clear error when --file matches no change", %{server: server} do
      iterations_body = ~s({"value":[{"id":1}]})

      changes_body =
        JSON.encode!(%{
          "value" => [
            %{"changeId" => 101, "changeType" => "edit", "item" => %{"path" => "/src/foo.ex"}}
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

    test "halts 1 with --file and --unified set together", %{server: _server} do
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
      iterations_body =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 3,
              "sourceRefCommit" => %{"commitId" => "src"},
              "targetRefCommit" => %{"commitId" => "tgt"}
            }
          ]
        })

      changes_body =
        JSON.encode!(%{
          "changeEntries" => [
            %{"changeId" => 301, "changeType" => "edit", "item" => %{"path" => "/x.ex"}}
          ]
        })

      # resolve_iteration with explicit id
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

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
      iterations_body =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "sourceRefCommit" => %{"commitId" => "src"},
              "targetRefCommit" => %{"commitId" => "tgt"}
            }
          ]
        })

      changes_body =
        JSON.encode!(%{
          "changeEntries" => [
            %{"changeId" => 401, "changeType" => "edit", "item" => %{"path" => "/a.ex"}},
            %{"changeId" => 402, "changeType" => "edit", "item" => %{"path" => "/b.ex"}}
          ]
        })

      diffs_body =
        JSON.encode!(%{
          "changes" => [
            %{"item" => %{"path" => "/a.ex", "objectId" => "a1", "originalObjectId" => "a0"}},
            %{"item" => %{"path" => "/b.ex", "objectId" => "b1", "originalObjectId" => "b0"}}
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

      # fetch_iteration_data re-fetches iterations
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/diffs/commits"),
        fn conn -> Plug.Conn.resp(conn, 200, diffs_body) end
      )

      # Old/new content for a.ex
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 200, "old content\n") end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 200, "old content\nnew line\n") end
      )

      # Old/new content for b.ex
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 200, "b old\n") end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 200, "b new\n") end
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
      assert output =~ "diff --git"
      assert output =~ "+++ b/a.ex"
      assert output =~ "+++ b/b.ex"
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

    test "--file with a new (add) file: tolerates 404 on old content", %{server: server} do
      iterations_body =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "sourceRefCommit" => %{"commitId" => "src"},
              "targetRefCommit" => %{"commitId" => "tgt"}
            }
          ]
        })

      changes_body =
        JSON.encode!(%{
          "changeEntries" => [
            %{"changeId" => 101, "changeType" => "add", "item" => %{"path" => "/src/new_file.ex"}}
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
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      # LIFO matching: the FIRST GET /items matches the LAST expectation.
      # We want 1st call (old content, base) → 404, 2nd call (new content, source) → 200.
      # So register 200 FIRST (matched last), 404 LAST (matched first).

      # New content — file exists in source commit (registered first → matched last)
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn ->
          Plug.Conn.resp(conn, 200, "defmodule NewFile do\n  @moduledoc \"New file\"\nend\n")
        end
      )

      # Old content — file doesn't exist in base commit (registered last → matched first)
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 404, ~s({"message":"not found"})) end
      )

      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :diff_pr, [
            %{
              options: %{file: "src/new_file.ex", iteration: nil, unified: false, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output =~ "diff --git"
      assert output =~ "+++ b/src/new_file.ex"
      # All lines should be additions (start with +)
      assert output =~ "+defmodule NewFile do"
      refute output =~ "-defmodule NewFile do"
    end

    test "--file with a deleted file: tolerates 404 on new content", %{server: server} do
      iterations_body =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "sourceRefCommit" => %{"commitId" => "src"},
              "targetRefCommit" => %{"commitId" => "tgt"}
            }
          ]
        })

      changes_body =
        JSON.encode!(%{
          "changeEntries" => [
            %{
              "changeId" => 102,
              "changeType" => "delete",
              "item" => %{"path" => "/src/deleted.ex"}
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

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      # LIFO matching: 1st GET /items → 200 (old content), 2nd → 404 (new content).
      # Register 404 first (matched last), 200 last (matched first).

      # New content — file doesn't exist in source commit (registered first → matched last)
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 404, ~s({"message":"not found"})) end
      )

      # Old content — file exists in base commit (registered last → matched first)
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn ->
          Plug.Conn.resp(conn, 200, "defmodule Deleted do\n  def bye, do: :ok\nend\n")
        end
      )

      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :diff_pr, [
            %{
              options: %{file: "src/deleted.ex", iteration: nil, unified: false, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output =~ "diff --git"
      assert output =~ "--- a/src/deleted.ex"
      # All lines should be deletions (start with -)
      assert output =~ "-defmodule Deleted do"
      refute output =~ "+defmodule Deleted do"
    end

    test "--unified includes new files (tolerates 404 on base)", %{server: server} do
      iterations_body =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "sourceRefCommit" => %{"commitId" => "src"},
              "targetRefCommit" => %{"commitId" => "tgt"}
            }
          ]
        })

      changes_body =
        JSON.encode!(%{
          "changeEntries" => [
            %{"changeId" => 501, "changeType" => "add", "item" => %{"path" => "/src/new.ex"}}
          ]
        })

      diffs_body =
        JSON.encode!(%{
          "changes" => [
            %{
              "changeType" => "add",
              "item" => %{"path" => "/src/new.ex", "objectId" => "n1", "originalObjectId" => "n0"}
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

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/iterations"),
        fn conn -> Plug.Conn.resp(conn, 200, iterations_body) end
      )

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/diffs/commits"),
        fn conn -> Plug.Conn.resp(conn, 200, diffs_body) end
      )

      # New content only — fetch_or_empty_raw short-circuits old for add
      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/items"),
        fn conn -> Plug.Conn.resp(conn, 200, "defmodule New do\nend\n") end
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
      assert output =~ "diff --git"
      assert output =~ "+++ b/src/new.ex"
      assert output =~ "+defmodule New do"
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
      100 -> acc |> Enum.reverse() |> Enum.join()
    end
  end

  # ── add_comment (prs comments add) ───────────────────────────────

  describe "add_comment (prs comments add)" do
    test "halts 0 on successful general thread creation", %{server: server} do
      response =
        JSON.encode!(%{
          "id" => 42,
          "comments" => [
            %{"id" => 100, "content" => "LGTM!"}
          ]
        })

      expect_post_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads",
        JSON.encode!(%{
          "comments" => [
            %{
              "content" => "LGTM!",
              "parentCommentId" => 0,
              "commentType" => "text"
            }
          ],
          "status" => "active"
        }),
        response,
        fn ->
          apply(AdoCli.CLI.PullRequests, :add_comment, [
            %{
              options: %{
                content: "LGTM!",
                file_path: nil,
                line: nil,
                thread_id: nil,
                comment_id: nil,
                status: "active",
                json: false
              },
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end

    test "halts 0 on successful inline thread creation with file context", %{server: server} do
      response = JSON.encode!(%{"id" => 43, "comments" => [%{"id" => 101}]})

      expect_post_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads",
        JSON.encode!(%{
          "comments" => [
            %{
              "content" => "Use a guard clause here",
              "parentCommentId" => 0,
              "commentType" => "text"
            }
          ],
          "status" => "active",
          "threadContext" => %{
            "filePath" => "src/foo.ex",
            "rightFileStart" => %{"line" => 42, "offset" => 1},
            "rightFileEnd" => %{"line" => 42, "offset" => 2}
          }
        }),
        response,
        fn ->
          apply(AdoCli.CLI.PullRequests, :add_comment, [
            %{
              options: %{
                content: "Use a guard clause here",
                file_path: "src/foo.ex",
                line: 42,
                thread_id: nil,
                comment_id: nil,
                status: "active",
                json: false
              },
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end

    test "halts 0 on successful reply to existing thread", %{server: server} do
      response = JSON.encode!(%{"id" => 102, "content" => "Fixed in abc123"})

      expect_post_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads/5/comments",
        JSON.encode!(%{
          "content" => "Fixed in abc123",
          "parentCommentId" => 0,
          "commentType" => "text"
        }),
        response,
        fn ->
          apply(AdoCli.CLI.PullRequests, :add_comment, [
            %{
              options: %{
                content: "Fixed in abc123",
                file_path: nil,
                line: nil,
                thread_id: 5,
                comment_id: 0,
                status: "active",
                json: false
              },
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end

    test "halts 1 with a clear error on invalid --status", %{server: _server} do
      # No expectation registered: the function must halt BEFORE
      # the HTTP call. If the function ever hits the network,
      # the test fails with "no expectation matched".
      capture_io(fn ->
        apply(AdoCli.CLI.PullRequests, :add_comment, [
          %{
            options: %{
              content: "x",
              file_path: nil,
              line: nil,
              thread_id: nil,
              comment_id: nil,
              status: "bogus",
              json: false
            },
            arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "Invalid --status 'bogus'"
      assert msg =~ "active, fixed, wontFix, closed, byDesign"
    end

    test "reads --content @<file> from a file", %{server: server} do
      file_content = "Line one.\nLine two.\nLine three.\n"

      file_path =
        Path.join(System.tmp_dir!(), "ado_comment_#{System.unique_integer([:positive])}.md")

      File.write!(file_path, file_content)
      on_exit(fn -> File.rm_rf(file_path) end)

      response = JSON.encode!(%{"id" => 7, "comments" => [%{"id" => 70}]})

      expect_post_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads",
        JSON.encode!(%{
          "comments" => [
            %{
              "content" => "Line one.\nLine two.\nLine three.",
              "parentCommentId" => 0,
              "commentType" => "text"
            }
          ],
          "status" => "active"
        }),
        response,
        fn ->
          apply(AdoCli.CLI.PullRequests, :add_comment, [
            %{
              options: %{
                content: "@" <> file_path,
                file_path: nil,
                line: nil,
                thread_id: nil,
                comment_id: nil,
                status: "active",
                json: false
              },
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end

    test "reads --content - from stdin", %{server: server} do
      response = JSON.encode!(%{"id" => 8, "comments" => [%{"id" => 80}]})

      expect_post_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads",
        JSON.encode!(%{
          "comments" => [
            %{
              "content" => "first\nsecond",
              "parentCommentId" => 0,
              "commentType" => "text"
            }
          ],
          "status" => "active"
        }),
        response,
        fn ->
          capture_io("first\nsecond\n", fn ->
            apply(AdoCli.CLI.PullRequests, :add_comment, [
              %{
                options: %{
                  content: "-",
                  file_path: nil,
                  line: nil,
                  thread_id: nil,
                  comment_id: nil,
                  status: "active",
                  json: false
                },
                arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
              }
            ])
          end)
        end
      )
    end

    test "halts 1 with a clear error when --content @<missing-file> cannot be read",
         %{server: _server} do
      missing = "/tmp/ado-missing-#{System.unique_integer([:positive])}.md"

      capture_io(fn ->
        apply(AdoCli.CLI.PullRequests, :add_comment, [
          %{
            options: %{
              content: "@" <> missing,
              file_path: nil,
              line: nil,
              thread_id: nil,
              comment_id: nil,
              status: "active",
              json: false
            },
            arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "Cannot read comment file"
      assert msg =~ missing
    end

    test "emits a JSON envelope with --json", %{server: server} do
      response = JSON.encode!(%{"id" => 42, "comments" => [%{"id" => 100}]})

      TestServer.expect(
        server,
        "POST",
        api("/testorg/_apis/git/repositories/test/pullrequests/1/threads"),
        fn conn -> Plug.Conn.resp(conn, 200, response) end
      )

      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :add_comment, [
            %{
              options: %{
                content: "hi",
                file_path: nil,
                line: nil,
                thread_id: nil,
                comment_id: nil,
                status: "active",
                json: true
              },
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert {:ok, decoded} = JSON.decode(String.trim(output))
      assert decoded["ok"] == true
      assert decoded["thread_id"] == 42
      assert decoded["comment_id"] == 100
    end
  end

  # ── delete_comment (prs comments delete) ───────────────────────

  describe "delete_comment (prs comments delete)" do
    test "halts 0 when deleting a specific comment with --force", %{server: server} do
      expect_delete_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads/5/comments/10",
        fn ->
          apply(AdoCli.CLI.PullRequests, :delete_comment, [
            %{
              options: %{comment_id: 10, force: true, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1, thread_id: 5}
            }
          ])
        end
      )
    end

    test "halts 0 when closing a thread with --force", %{server: server} do
      expect_patch_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads/5",
        %{"status" => "closed"},
        ~s({"id":5,"status":"closed"}),
        fn ->
          apply(AdoCli.CLI.PullRequests, :delete_comment, [
            %{
              options: %{comment_id: nil, force: true, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1, thread_id: 5}
            }
          ])
        end
      )
    end

    test "emits JSON envelope with --json", %{server: server} do
      TestServer.expect(
        server,
        "DELETE",
        api("/testorg/_apis/git/repositories/test/pullrequests/1/threads/5/comments/10"),
        fn conn -> Plug.Conn.resp(conn, 204, "") end
      )

      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :delete_comment, [
            %{
              options: %{comment_id: 10, force: true, json: true},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1, thread_id: 5}
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert {:ok, decoded} = JSON.decode(String.trim(output))
      assert decoded["ok"] == true
    end

    test "halts 1 on API error (thread close fails)", %{server: server} do
      TestServer.expect(
        server,
        "PATCH",
        api("/testorg/_apis/git/repositories/test/pullrequests/1/threads/5"),
        fn conn -> Plug.Conn.resp(conn, 404, ~s({"message":"not found"})) end
      )

      apply(AdoCli.CLI.PullRequests, :delete_comment, [
        %{
          options: %{comment_id: nil, force: true, json: false},
          arguments: %{project: "testorg", repo_id: "test", pr_id: 1, thread_id: 5}
        }
      ])

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end
  end

  # ── resolve_thread (prs comments resolve) ───────────────────────

  describe "resolve_thread (prs comments resolve)" do
    test "halts 0 on successful resolve (default: fixed)", %{server: server} do
      expect_patch_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads/5",
        %{"status" => "fixed"},
        ~s({"id":5,"status":"fixed"}),
        fn ->
          apply(AdoCli.CLI.PullRequests, :resolve_thread, [
            %{
              options: %{status: "fixed", resolved_by_me: false, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1, thread_id: 5}
            }
          ])
        end
      )
    end

    test "resolves with a custom status", %{server: server} do
      expect_patch_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads/5",
        %{"status" => "wontFix"},
        ~s({"id":5,"status":"wontFix"}),
        fn ->
          apply(AdoCli.CLI.PullRequests, :resolve_thread, [
            %{
              options: %{status: "wontFix", resolved_by_me: false, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1, thread_id: 5}
            }
          ])
        end
      )
    end

    test "resolves with --resolved-by-me", %{server: server} do
      :erlang.put({AdoCli.Auth, :user_id}, "user-guid-123")

      expect_patch_success(
        server,
        "/testorg/_apis/git/repositories/test/pullrequests/1/threads/5",
        %{"status" => "fixed", "resolvedBy" => %{"id" => "user-guid-123"}},
        ~s({"id":5,"status":"fixed"}),
        fn ->
          apply(AdoCli.CLI.PullRequests, :resolve_thread, [
            %{
              options: %{status: "fixed", resolved_by_me: true, json: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1, thread_id: 5}
            }
          ])
        end
      )
    after
      :erlang.erase({AdoCli.Auth, :user_id})
    end

    test "emits JSON envelope with --json", %{server: server} do
      TestServer.expect(
        server,
        "PATCH",
        api("/testorg/_apis/git/repositories/test/pullrequests/1/threads/5"),
        fn conn -> Plug.Conn.resp(conn, 200, ~s({"id":5,"status":"fixed"})) end
      )

      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :resolve_thread, [
            %{
              options: %{status: "fixed", resolved_by_me: false, json: true},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1, thread_id: 5}
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert {:ok, decoded} = JSON.decode(String.trim(output))
      assert decoded["ok"] == true
    end
  end

  # ── list_comments --all (prs comments list) ─────────────────────

  describe "list_comments (prs comments list)" do
    test "halts 0 on success (default view: thread headers only)", %{server: server} do
      threads =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "status" => "active",
              "comments" => [
                %{"id" => 10, "author" => %{"displayName" => "alice"}, "content" => "LGTM!"}
              ]
            }
          ]
        })

      expect_success_json(
        server,
        "/testorg/_apis/git/repositories/test/pullRequests/1/threads",
        threads,
        fn ->
          apply(AdoCli.CLI.PullRequests, :list_comments, [
            %{
              options: %{json: true, all: false},
              arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end

    test "--all shows file path for inline threads", %{server: server} do
      threads =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "status" => "active",
              "threadContext" => %{"filePath" => "src/foo.ex"},
              "comments" => [
                %{"id" => 10, "author" => %{"displayName" => "alice"}, "content" => "fix me"}
              ]
            }
          ]
        })

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/threads"),
        fn conn -> Plug.Conn.resp(conn, 200, threads) end
      )

      apply(AdoCli.CLI.PullRequests, :list_comments, [
        %{
          options: %{json: false, all: true},
          arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
        }
      ])

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      output = drain_shell_output()
      assert output =~ "Thread 1 [active] on src/foo.ex"
      assert output =~ "fix me"
    end

    test "--all shows full multi-line content (not truncated)", %{server: server} do
      long_content = String.duplicate("a", 200)

      threads =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "status" => "active",
              "comments" => [
                %{"id" => 10, "author" => %{"displayName" => "alice"}, "content" => long_content}
              ]
            }
          ]
        })

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/threads"),
        fn conn -> Plug.Conn.resp(conn, 200, threads) end
      )

      apply(AdoCli.CLI.PullRequests, :list_comments, [
        %{
          options: %{json: false, all: true},
          arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
        }
      ])

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      output = drain_shell_output()
      # The 80-char-truncated view would only contain 80 'a's.
      assert String.contains?(output, long_content)
    end

    test "--all shows reply markers for parented comments", %{server: server} do
      threads =
        JSON.encode!(%{
          "value" => [
            %{
              "id" => 1,
              "status" => "active",
              "comments" => [
                %{
                  "id" => 10,
                  "author" => %{"displayName" => "alice"},
                  "content" => "parent",
                  "parentCommentId" => 0
                },
                %{
                  "id" => 11,
                  "author" => %{"displayName" => "bob"},
                  "content" => "reply",
                  "parentCommentId" => 10
                }
              ]
            }
          ]
        })

      TestServer.expect(
        server,
        "GET",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/threads"),
        fn conn -> Plug.Conn.resp(conn, 200, threads) end
      )

      apply(AdoCli.CLI.PullRequests, :list_comments, [
        %{
          options: %{json: false, all: true},
          arguments: %{project: "testorg", repo_id: "test", pr_id: 1}
        }
      ])

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      output = drain_shell_output()
      assert output =~ "[10] alice:"
      assert output =~ "[11] (reply to 10) bob:"
    end
  end

  # ── update_comment (prs comments update) ─────────────────────────

  describe "update_comment (prs comments update)" do
    test "halts 1 when neither --content nor --status is given", %{server: _server} do
      capture_io(fn ->
        apply(AdoCli.CLI.PullRequests, :update_comment, [
          %{
            options: %{content: nil, status: nil, json: false},
            arguments: %{
              project: "testorg",
              repo_id: "test",
              pr_id: 1,
              thread_id: 5,
              comment_id: 10
            }
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "Must pass --content and/or --status"
    end

    test "PATCHes the comment endpoint with --content (legacy path)", %{server: server} do
      response = JSON.encode!(%{"id" => 10, "content" => "Updated text"})

      expect_patch_success(
        server,
        "/testorg/_apis/git/repositories/test/pullRequests/1/threads/5/comments/10",
        JSON.encode!(%{"content" => "Updated text"}),
        response,
        fn ->
          apply(AdoCli.CLI.PullRequests, :update_comment, [
            %{
              options: %{content: "Updated text", status: nil, json: false},
              arguments: %{
                project: "testorg",
                repo_id: "test",
                pr_id: 1,
                thread_id: 5,
                comment_id: 10
              }
            }
          ])
        end
      )
    end

    test "PATCHes the thread endpoint with --status (no comment call)", %{server: server} do
      response = JSON.encode!(%{"id" => 5, "status" => "fixed"})

      expect_patch_success(
        server,
        "/testorg/_apis/git/repositories/test/pullRequests/1/threads/5",
        JSON.encode!(%{"status" => "fixed"}),
        response,
        fn ->
          apply(AdoCli.CLI.PullRequests, :update_comment, [
            %{
              options: %{content: nil, status: "fixed", json: false},
              arguments: %{
                project: "testorg",
                repo_id: "test",
                pr_id: 1,
                thread_id: 5,
                comment_id: 10
              }
            }
          ])
        end
      )
    end

    test "PATCHes BOTH endpoints with --content and --status", %{server: server} do
      thread_response = JSON.encode!(%{"id" => 5, "status" => "fixed"})
      comment_response = JSON.encode!(%{"id" => 10, "content" => "Updated"})

      TestServer.expect(
        server,
        "PATCH",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/threads/5"),
        fn conn -> Plug.Conn.resp(conn, 200, thread_response) end
      )

      TestServer.expect(
        server,
        "PATCH",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/threads/5/comments/10"),
        fn conn -> Plug.Conn.resp(conn, 200, comment_response) end
      )

      apply(AdoCli.CLI.PullRequests, :update_comment, [
        %{
          options: %{content: "Updated", status: "fixed", json: false},
          arguments: %{
            project: "testorg",
            repo_id: "test",
            pr_id: 1,
            thread_id: 5,
            comment_id: 10
          }
        }
      ])

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "halts 1 with a clear error on invalid --status", %{server: _server} do
      capture_io(fn ->
        apply(AdoCli.CLI.PullRequests, :update_comment, [
          %{
            options: %{content: nil, status: "bogus", json: false},
            arguments: %{
              project: "testorg",
              repo_id: "test",
              pr_id: 1,
              thread_id: 5,
              comment_id: 10
            }
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "Invalid --status 'bogus'"
      assert msg =~ "active, fixed, wontFix, closed, byDesign"
    end

    test "reads --content @<file> from a file", %{server: server} do
      file_content = "Line one.\nLine two.\nLine three.\n"

      file_path =
        Path.join(System.tmp_dir!(), "ado_update_#{System.unique_integer([:positive])}.md")

      File.write!(file_path, file_content)
      on_exit(fn -> File.rm_rf(file_path) end)

      response = JSON.encode!(%{"id" => 10, "content" => "Line one.\nLine two.\nLine three."})

      expect_patch_success(
        server,
        "/testorg/_apis/git/repositories/test/pullRequests/1/threads/5/comments/10",
        JSON.encode!(%{"content" => "Line one.\nLine two.\nLine three."}),
        response,
        fn ->
          apply(AdoCli.CLI.PullRequests, :update_comment, [
            %{
              options: %{content: "@" <> file_path, status: nil, json: false},
              arguments: %{
                project: "testorg",
                repo_id: "test",
                pr_id: 1,
                thread_id: 5,
                comment_id: 10
              }
            }
          ])
        end
      )
    end

    test "reads --content - from stdin", %{server: server} do
      response = JSON.encode!(%{"id" => 10, "content" => "first\nsecond"})

      TestServer.expect(
        server,
        "PATCH",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/threads/5/comments/10"),
        fn conn -> Plug.Conn.resp(conn, 200, response) end
      )

      capture_io("first\nsecond\n", fn ->
        apply(AdoCli.CLI.PullRequests, :update_comment, [
          %{
            options: %{content: "-", status: nil, json: false},
            arguments: %{
              project: "testorg",
              repo_id: "test",
              pr_id: 1,
              thread_id: 5,
              comment_id: 10
            }
          }
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "--dry-run with --content prints the would-be PATCH and halts 0 (no API call)",
         %{server: _server} do
      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :update_comment, [
            %{
              options: %{content: "new text", status: nil, dry_run: true, json: false},
              arguments: %{
                project: "testorg",
                repo_id: "test",
                pr_id: 1,
                thread_id: 5,
                comment_id: 10
              }
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(output))
      assert decoded["ok"] == true
      assert decoded["dry_run"] == true
      assert match?([_], decoded["actions"])

      [action] = decoded["actions"]
      assert action["method"] == "PATCH"

      assert action["path"] ==
               "/testorg/_apis/git/repositories/test/pullRequests/1/threads/5/comments/10"

      assert action["body"] == %{"content" => "new text"}
    end

    test "--dry-run with --status prints the would-be thread PATCH", %{server: _server} do
      output =
        capture_io(fn ->
          apply(AdoCli.CLI.PullRequests, :update_comment, [
            %{
              options: %{content: nil, status: "fixed", dry_run: true, json: false},
              arguments: %{
                project: "testorg",
                repo_id: "test",
                pr_id: 1,
                thread_id: 5,
                comment_id: 10
              }
            }
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(output))
      assert match?([_], decoded["actions"])

      [action] = decoded["actions"]
      assert action["path"] == "/testorg/_apis/git/repositories/test/pullRequests/1/threads/5"
      assert action["body"] == %{"status" => "fixed"}
    end

    test "--resolved-by-me adds resolvedBy to the thread PATCH body", %{server: server} do
      :erlang.put({AdoCli.Auth, :user_id}, nil)
      capture_key = {:__test_capture__, :thread_body}
      :persistent_term.put(capture_key, nil)

      TestServer.expect(
        server,
        "GET",
        api("/_apis/connectionData"),
        fn conn ->
          Plug.Conn.resp(conn, 200, ~s({"authenticatedUser":{"id":"user-guid-123"}}))
        end
      )

      TestServer.expect(
        server,
        "PATCH",
        api("/testorg/_apis/git/repositories/test/pullRequests/1/threads/5"),
        fn conn ->
          {:ok, raw_body, conn} = Plug.Conn.read_body(conn)
          :persistent_term.put(capture_key, JSON.decode!(raw_body))
          Plug.Conn.resp(conn, 200, ~s({"id":5,"status":"fixed"}))
        end
      )

      apply(AdoCli.CLI.PullRequests, :update_comment, [
        %{
          options: %{
            content: nil,
            status: "fixed",
            resolved_by_me: true,
            json: false
          },
          arguments: %{
            project: "testorg",
            repo_id: "test",
            pr_id: 1,
            thread_id: 5,
            comment_id: 10
          }
        }
      ])

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      body = :persistent_term.get(capture_key)
      assert body == %{"status" => "fixed", "resolvedBy" => %{"id" => "user-guid-123"}}
    end
  end
end
