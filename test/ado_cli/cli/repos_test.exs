defmodule AdoCli.CLI.ReposTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Repos

  describe "list_repos" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/git/repositories", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Repos, :list_repos, [
          %{
            options: %{json: true, top: nil, include_links: false, include_all_urls: false},
            arguments: %{project: "test"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/repositories", 500, "{}", fn ->
        apply(AdoCli.CLI.Repos, :list_repos, [
          %{
            options: %{json: true, top: nil, include_links: false, include_all_urls: false},
            arguments: %{project: "test"}
          }
        ])
      end)
    end
  end

  describe "show_repo" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/git/repositories/test", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Repos, :show_repo, [
          %{
            options: %{json: true, include_links: false},
            arguments: %{project: "test", repo_id: "test"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/repositories/test", 500, "{}", fn ->
        apply(AdoCli.CLI.Repos, :show_repo, [
          %{
            options: %{json: true, include_links: false},
            arguments: %{project: "test", repo_id: "test"}
          }
        ])
      end)
    end
  end

  describe "list_branches" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/test/_apis/git/repositories/test/refs",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.Repos, :list_branches, [
            %{
              options: %{json: true, top: nil, filter: nil},
              arguments: %{project: "test", repo_id: "test"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/repositories/test/refs", 500, "{}", fn ->
        apply(AdoCli.CLI.Repos, :list_branches, [
          %{
            options: %{json: true, top: nil, filter: nil},
            arguments: %{project: "test", repo_id: "test"}
          }
        ])
      end)
    end
  end

  describe "create_repo" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(server, "/test/_apis/git/repositories", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Repos, :create_repo, [
          %{
            options: %{json: true, name: "new-repo", default_branch: nil, parent_repo: nil},
            arguments: %{project: "test"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/repositories", 500, "{}", fn ->
        apply(AdoCli.CLI.Repos, :create_repo, [
          %{
            options: %{json: true, name: "new-repo", default_branch: nil, parent_repo: nil},
            arguments: %{project: "test"}
          }
        ])
      end)
    end
  end

  describe "delete_repo" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/test/_apis/git/repositories/test", fn ->
        apply(AdoCli.CLI.Repos, :delete_repo, [
          %{options: %{json: true, force: false}, arguments: %{project: "test", repo_id: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/repositories/test", 500, "{}", fn ->
        apply(AdoCli.CLI.Repos, :delete_repo, [
          %{options: %{json: true, force: false}, arguments: %{project: "test", repo_id: "test"}}
        ])
      end)
    end
  end
end
