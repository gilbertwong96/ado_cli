defmodule AdoCli.CLI.BranchPoliciesTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.BranchPolicies

  describe "list_policies" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/policy/configurations", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.BranchPolicies, :list_policies, [
          %{options: %{json: true, ref_name: nil}, arguments: %{project: "test", repo_id: "repo"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/policy/configurations", 500, "{}", fn ->
        apply(AdoCli.CLI.BranchPolicies, :list_policies, [
          %{options: %{json: true, ref_name: nil}, arguments: %{project: "test", repo_id: "repo"}}
        ])
      end)
    end
  end

  describe "show_policy" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/policy/configurations/1", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.BranchPolicies, :show_policy, [
          %{options: %{json: true}, arguments: %{project: "test", repo_id: "repo", policy_id: 1}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/policy/configurations/1", 500, "{}", fn ->
        apply(AdoCli.CLI.BranchPolicies, :show_policy, [
          %{options: %{json: true}, arguments: %{project: "test", repo_id: "repo", policy_id: 1}}
        ])
      end)
    end
  end

  describe "create_policy" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(server, "/test/_apis/policy/configurations", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.BranchPolicies, :create_policy, [
          %{
            options: %{json: true, type: "required reviewers", settings: %{}, ref_name: "main"},
            arguments: %{project: "test", repo_id: "repo"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/policy/configurations", 500, "{}", fn ->
        apply(AdoCli.CLI.BranchPolicies, :create_policy, [
          %{
            options: %{json: true, type: "required reviewers", settings: %{}, ref_name: "main"},
            arguments: %{project: "test", repo_id: "repo"}
          }
        ])
      end)
    end
  end

  describe "update_policy" do
    test "halts 0 on successful put", %{server: server} do
      expect_put_success(server, "/test/_apis/policy/configurations/1", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.BranchPolicies, :update_policy, [
          %{
            options: %{json: true, is_enabled: true, is_blocking: false, settings: %{}},
            arguments: %{project: "test", repo_id: "repo", policy_id: 1}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/policy/configurations/1", 500, "{}", fn ->
        apply(AdoCli.CLI.BranchPolicies, :update_policy, [
          %{
            options: %{json: true, is_enabled: true, is_blocking: false, settings: %{}},
            arguments: %{project: "test", repo_id: "repo", policy_id: 1}
          }
        ])
      end)
    end
  end

  describe "delete_policy" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/test/_apis/policy/configurations/1", fn ->
        apply(AdoCli.CLI.BranchPolicies, :delete_policy, [
          %{
            options: %{json: true, force: false},
            arguments: %{project: "test", repo_id: "repo", policy_id: 1}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/policy/configurations/1", 500, "{}", fn ->
        apply(AdoCli.CLI.BranchPolicies, :delete_policy, [
          %{
            options: %{json: true, force: false},
            arguments: %{project: "test", repo_id: "repo", policy_id: 1}
          }
        ])
      end)
    end
  end
end
