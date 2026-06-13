defmodule AdoCli.CLI.PipelinesTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Pipelines

  describe "list_pipelines/1" do
    test "halts 0 on success (JSON)", %{server: server} do
      body = ~s({"value":[{"id":1,"name":"Pipeline 1","folder":""}],"count":1})

      expect_success_json(server, "/testorg/_apis/pipelines", body, fn ->
        Pipelines.list_pipelines(%{
          options: %{json: true, top: nil, folder: nil},
          arguments: %{project: "testorg"}
        })
      end)
    end

    test "halts 1 on error", %{server: server} do
      expect_api_error(server, "/testorg/_apis/pipelines", 404, "{}", fn ->
        Pipelines.list_pipelines(%{
          options: %{json: true, top: nil, folder: nil},
          arguments: %{project: "testorg"}
        })
      end)
    end
  end

  describe "show_pipeline/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":1,"name":"Pipeline 1"})

      expect_success_json(server, "/testorg/_apis/pipelines/1", body, fn ->
        Pipelines.show_pipeline(%{
          options: %{json: true},
          arguments: %{project: "testorg", pipeline_id: 1}
        })
      end)
    end
  end

  describe "run_pipeline/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":100,"state":"inProgress"})

      expect_post_success(server, "/testorg/_apis/pipelines/1/runs", "", body, fn ->
        Pipelines.run_pipeline(%{
          options: %{json: true, branch: nil, variables: nil},
          arguments: %{project: "testorg", pipeline_id: 1}
        })
      end)
    end
  end

  describe "create_pipeline/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":2,"name":"New Pipeline"})

      expect_post_success(server, "/testorg/_apis/pipelines", "", body, fn ->
        Pipelines.create_pipeline(%{
          options: %{
            json: true,
            name: "New",
            repo: "repo",
            path: "azure-pipelines.yml",
            folder: nil
          },
          arguments: %{project: "testorg"}
        })
      end)
    end
  end

  describe "update_pipeline/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":1,"name":"Updated"})

      expect_patch_success(server, "/testorg/_apis/pipelines/1", "", body, fn ->
        Pipelines.update_pipeline(%{
          options: %{json: true, name: "Updated", path: "pipeline.yml", folder: nil},
          arguments: %{project: "testorg", pipeline_id: 1}
        })
      end)
    end
  end

  describe "delete_pipeline/1" do
    test "halts 0 on success", %{server: server} do
      expect_delete_success(server, "/testorg/_apis/pipelines/1", fn ->
        Pipelines.delete_pipeline(%{
          options: %{json: true, force: false},
          arguments: %{project: "testorg", pipeline_id: 1}
        })
      end)
    end
  end

  describe "list_var_groups/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"value":[{"id":1,"name":"my-vars","variables":{}}]})

      expect_success_json(server, "/testorg/_apis/distributedtask/variablegroups", body, fn ->
        Pipelines.list_var_groups(%{
          options: %{json: true, top: nil},
          arguments: %{project: "testorg"}
        })
      end)
    end
  end

  describe "show_var_group/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":1,"name":"my-vars","variables":{}})

      expect_success_json(server, "/testorg/_apis/distributedtask/variablegroups/1", body, fn ->
        Pipelines.show_var_group(%{
          options: %{json: true},
          arguments: %{project: "testorg", group_id: 1}
        })
      end)
    end
  end

  describe "create_var_group/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":2,"name":"new-vars"})

      expect_post_success(server, "/testorg/_apis/distributedtask/variablegroups", "", body, fn ->
        Pipelines.create_var_group(%{
          options: %{
            json: true,
            name: "new-vars",
            description: nil,
            variables: nil,
            secret: nil,
            type: nil
          },
          arguments: %{project: "testorg"}
        })
      end)
    end
  end
end
