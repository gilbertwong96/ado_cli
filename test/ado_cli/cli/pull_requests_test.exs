defmodule AdoCli.CLI.PullRequestsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.PullRequests

  describe "list_prs" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/test/_apis/git/repositories/test/pullrequests",
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
              arguments: %{project: "test", repo_id: "test"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/repositories/test/pullrequests", 500, "{}", fn ->
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
            arguments: %{project: "test", repo_id: "test"}
          }
        ])
      end)
    end
  end

  describe "show_pr" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/test/_apis/git/repositories/test/pullrequests/1",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.PullRequests, :show_pr, [
            %{
              options: %{json: true, include_commits: false, include_work_item_refs: false},
              arguments: %{project: "test", repo_id: "test", pr_id: 1}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/test/_apis/git/repositories/test/pullrequests/1",
        500,
        "{}",
        fn ->
          apply(AdoCli.CLI.PullRequests, :show_pr, [
            %{
              options: %{json: true, include_commits: false, include_work_item_refs: false},
              arguments: %{project: "test", repo_id: "test", pr_id: 1}
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
        "/test/_apis/git/repositories/test/pullrequests",
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
              arguments: %{project: "test", repo_id: "test"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/repositories/test/pullrequests", 500, "{}", fn ->
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
            arguments: %{project: "test", repo_id: "test"}
          }
        ])
      end)
    end
  end

  describe "complete_pr" do
    test "halts 0 on successful patch", %{server: server} do
      expect_patch_success(
        server,
        "/test/_apis/git/repositories/test/pullrequests/1",
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
              arguments: %{project: "test", repo_id: "test", pr_id: 1, completion_options: %{}}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/test/_apis/git/repositories/test/pullrequests/1",
        500,
        "{}",
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
              arguments: %{project: "test", repo_id: "test", pr_id: 1, completion_options: %{}}
            }
          ])
        end
      )
    end
  end
end
