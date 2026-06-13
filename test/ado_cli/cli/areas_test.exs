defmodule AdoCli.CLI.AreasTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Areas

  describe "list_areas" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/wit/classificationnodes", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Areas, :list_areas, [%{options: %{json: true}}])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wit/classificationnodes", 500, "{}", fn ->
        apply(AdoCli.CLI.Areas, :list_areas, [%{options: %{json: true}}])
      end)
    end
  end

  describe "show_area" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/wit/classificationnodes/1", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Areas, :show_area, [
          %{options: %{json: true}, arguments: %{id: 1, project: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wit/classificationnodes/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Areas, :show_area, [
          %{options: %{json: true}, arguments: %{id: 1, project: "test"}}
        ])
      end)
    end
  end

  describe "create_area" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(server, "/test/_apis/wit/classificationnodes", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Areas, :create_area, [
          %{options: %{json: true, name: "test"}, arguments: %{project: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wit/classificationnodes", 500, "{}", fn ->
        apply(AdoCli.CLI.Areas, :create_area, [
          %{options: %{json: true, name: "test"}, arguments: %{project: "test"}}
        ])
      end)
    end
  end

  describe "update_area" do
    test "halts 0 on successful patch", %{server: server} do
      expect_patch_success(
        server,
        "/test/_apis/wit/classificationnodes/1",
        "",
        "{\"id\":1}",
        fn ->
          apply(AdoCli.CLI.Areas, :update_area, [
            %{options: %{json: true, name: "new"}, arguments: %{id: 1, project: "test"}}
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wit/classificationnodes/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Areas, :update_area, [
          %{options: %{json: true, name: "new"}, arguments: %{id: 1, project: "test"}}
        ])
      end)
    end
  end

  describe "delete_area" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/test/_apis/wit/classificationnodes/1", fn ->
        apply(AdoCli.CLI.Areas, :delete_area, [
          %{options: %{json: true, force: false}, arguments: %{id: 1, project: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wit/classificationnodes/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Areas, :delete_area, [
          %{options: %{json: true, force: false}, arguments: %{id: 1, project: "test"}}
        ])
      end)
    end
  end
end
