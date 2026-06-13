defmodule AdoCli.CLI.SecurityTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Security

  describe "list_groups" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/_apis/graph/groups", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Security, :list_groups, [
          %{options: %{json: true, top: nil, scope_descriptor: nil, subject_types: nil}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/graph/groups", 500, "{}", fn ->
        apply(AdoCli.CLI.Security, :list_groups, [
          %{options: %{json: true, top: nil, scope_descriptor: nil, subject_types: nil}}
        ])
      end)
    end
  end

  describe "show_group" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/_apis/graph/groups/1", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Security, :show_group, [
          %{options: %{json: true, expand: false}, arguments: %{group_id: "1"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/graph/groups/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Security, :show_group, [
          %{options: %{json: true, expand: false}, arguments: %{group_id: "1"}}
        ])
      end)
    end
  end

  describe "create_group" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(server, "/_apis/graph/groups", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Security, :create_group, [
          %{
            options: %{
              json: true,
              display_name: "New Group",
              description: nil,
              scope_descriptor: "scp"
            }
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/graph/groups", 500, "{}", fn ->
        apply(AdoCli.CLI.Security, :create_group, [
          %{
            options: %{
              json: true,
              display_name: "New Group",
              description: nil,
              scope_descriptor: "scp"
            }
          }
        ])
      end)
    end
  end

  describe "delete_group" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/_apis/graph/groups/1", fn ->
        apply(AdoCli.CLI.Security, :delete_group, [
          %{options: %{json: true, force: false}, arguments: %{group_id: "1"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/graph/groups/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Security, :delete_group, [
          %{options: %{json: true, force: false}, arguments: %{group_id: "1"}}
        ])
      end)
    end
  end

  describe "list_members" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/_apis/graph/groups/1/memberships", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Security, :list_members, [
          %{options: %{json: true, top: nil}, arguments: %{group_id: "1"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/graph/groups/1/memberships", 500, "{}", fn ->
        apply(AdoCli.CLI.Security, :list_members, [
          %{options: %{json: true, top: nil}, arguments: %{group_id: "1"}}
        ])
      end)
    end
  end

  describe "list_namespaces" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/_apis/securitynamespaces", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Security, :list_namespaces, [
          %{options: %{json: true, top: nil, local_only: false}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/securitynamespaces", 500, "{}", fn ->
        apply(AdoCli.CLI.Security, :list_namespaces, [
          %{options: %{json: true, top: nil, local_only: false}}
        ])
      end)
    end
  end

  describe "list_permissions" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/_apis/securitynamespaces/2/permissions",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.Security, :list_permissions, [
            %{options: %{json: true, top: nil}, arguments: %{namespace_id: "2"}}
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/securitynamespaces/2/permissions", 500, "{}", fn ->
        apply(AdoCli.CLI.Security, :list_permissions, [
          %{options: %{json: true, top: nil}, arguments: %{namespace_id: "2"}}
        ])
      end)
    end
  end
end
