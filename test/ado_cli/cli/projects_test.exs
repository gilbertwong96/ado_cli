defmodule AdoCli.CLI.ProjectsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Projects

  describe "list_projects/1" do
    test "halts 0 on success (JSON)", %{server: server} do
      body = ~s({"value":[{"id":"p1","name":"Project One"}],"count":1})

      expect_success_json(server, "/_apis/projects", body, fn ->
        Projects.list_projects(%{
          options: %{json: true, top: nil, skip: nil, state: nil}
        })
      end)
    end

    test "halts 0 on success (table)", %{server: server} do
      body = ~s({"value":[{"id":"p1","name":"Project One"}],"count":1})

      expect_success_table(server, "/_apis/projects", body, fn ->
        Projects.list_projects(%{
          options: %{json: false, top: nil, skip: nil, state: nil}
        })
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/projects", 404, ~s({"message":"Not found"}), fn ->
        Projects.list_projects(%{
          options: %{json: false, top: nil, skip: nil, state: nil}
        })
      end)
    end

    test "builds query params from options", %{server: server} do
      body = ~s({"value":[],"count":0})

      expect_success_json(server, "/_apis/projects", body, fn ->
        Projects.list_projects(%{
          options: %{json: true, top: 10, skip: nil, state: "wellFormed"}
        })
      end)
    end
  end

  describe "show_project/1" do
    test "halts 0 on success (JSON)", %{server: server} do
      body = ~s({"id":"p1","name":"Project One","description":"A test project"})

      expect_success_json(server, "/_apis/projects/p1", body, fn ->
        Projects.show_project(%{
          options: %{json: true, capabilities: false},
          arguments: %{project_id: "p1"}
        })
      end)
    end
  end

  describe "create_project/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":"p2","name":"New Project","state":"wellFormed"})

      expect_post_success(server, "/_apis/projects", "", body, fn ->
        Projects.create_project(%{
          options: %{
            json: true,
            description: "New",
            visibility: "private",
            process: nil,
            source_control: nil
          },
          arguments: %{name: "New Project"}
        })
      end)
    end
  end

  describe "update_project/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":"p1","name":"Updated Name"})

      expect_patch_success(server, "/_apis/projects/p1", "", body, fn ->
        Projects.update_project(%{
          options: %{json: true, name: "Updated", description: nil, abort: false},
          arguments: %{project_id: "p1"}
        })
      end)
    end
  end

  describe "delete_project/1" do
    test "halts 1 on error when deleting non-existent", %{server: server} do
      # delete_project prompts for confirmation via IO.gets. We can't
      # easily mock that, so just test the error path.
      expect_api_error(server, "/_apis/projects/p1", 404, ~s({"message":"Not found"}), fn ->
        Projects.delete_project(%{
          options: %{json: true, force: false},
          arguments: %{project_id: "p1"}
        })
      end)
    end
  end
end
