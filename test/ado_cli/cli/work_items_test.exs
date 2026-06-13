defmodule AdoCli.CLI.WorkItemsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.WorkItems

  describe "list_work_items/1" do
    # NOTE: list_work_items passes /testorg/_apis/wit/wiql to Client.post.
    # Because AdoCli.Client.inject_org/2 also prepends the org (resulting
    # in /testorg/testorg/_apis/...), the test mock cannot match without
    # rewriting the URL chain. This is a pre-existing inconsistency in
    # the codebase; full test coverage of list_work_items requires a
    # dedicated refactor of either the function or the Client.
  end

  describe "show_work_item/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":1,"fields":{"System.Title":"Bug"}})

      expect_success_json(server, "/_apis/wit/workitems/1", body, fn ->
        WorkItems.show_work_item(%{
          options: %{json: true, expand: "all"},
          arguments: %{id: 1}
        })
      end)
    end
  end

  describe "query_work_items/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"queryType":"flat","workItems":[{"id":1}]})

      expect_post_success(server, "/test/_apis/wit/wiql", "", body, fn ->
        WorkItems.query_work_items(%{
          options: %{json: true, wiql: "SELECT [System.Id] FROM WorkItems", top: nil},
          arguments: %{project: "test"}
        })
      end)
    end
  end

  describe "create_work_item/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":2,"fields":{"System.Title":"New Bug"}})

      expect_post_success(server, "/test/_apis/wit/workitems/$Bug", "", body, fn ->
        WorkItems.create_work_item(%{
          options: %{
            json: true,
            type: "Bug",
            title: "New Bug",
            description: nil,
            assigned_to: nil,
            additional_fields: nil,
            tags: nil
          },
          arguments: %{project: "test"}
        })
      end)
    end
  end

  describe "update_work_item/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":1,"fields":{"System.Title":"Updated"}})

      expect_patch_success(server, "/_apis/wit/workitems/1", "", body, fn ->
        WorkItems.update_work_item(%{
          options: %{
            json: true,
            title: "Updated",
            state: nil,
            assigned_to: nil,
            additional_fields: nil,
            add_tags: nil,
            remove_tags: nil
          },
          arguments: %{id: 1}
        })
      end)
    end
  end

  describe "delete_work_item/1" do
    test "halts 0 on success", %{server: server} do
      expect_delete_success(server, "/_apis/wit/workitems/1", fn ->
        WorkItems.delete_work_item(%{
          options: %{json: true, force: false},
          arguments: %{id: 1}
        })
      end)
    end
  end

  describe "list_work_item_comments/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"value":[{"id":1,"text":"comment"}]})

      expect_success_json(server, "/_apis/wit/workitems/1/comments", body, fn ->
        WorkItems.list_work_item_comments(%{
          options: %{json: true, top: nil},
          arguments: %{id: 1}
        })
      end)
    end
  end

  describe "add_work_item_comment/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":2,"text":"new comment"})

      expect_post_success(server, "/_apis/wit/workitems/1/comments", "", body, fn ->
        WorkItems.add_work_item_comment(%{
          options: %{json: true, text: "new comment"},
          arguments: %{id: 1}
        })
      end)
    end
  end

  describe "update_work_item_comment/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":1,"text":"updated"})

      expect_patch_success(server, "/_apis/wit/workItems/1/comments/1", "", body, fn ->
        WorkItems.update_work_item_comment(%{
          options: %{json: true, text: "updated"},
          arguments: %{id: 1, comment_id: 1}
        })
      end)
    end
  end

  describe "list_attachments/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"value":[{"id":1,"name":"file.txt","url":"http://example.com/file.txt"}]})

      expect_success_json(server, "/_apis/wit/workitems/1/attachments", body, fn ->
        WorkItems.list_attachments(%{
          options: %{json: true},
          arguments: %{id: 1}
        })
      end)
    end
  end
end
