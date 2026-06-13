defmodule AdoCli.CLI.AreasTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Areas

  describe "list_areas/1" do
    test "halts 0 on success (JSON)", %{server: server} do
      body = ~s({"value":[{"id":1,"name":"Area1","path":"\\test\\area1"}],"count":1})

      expect_success_json(server, "/test/_apis/wit/classificationNodes/areas", body, fn ->
        Areas.list_areas(%{
          options: %{json: true, depth: nil},
          arguments: %{project: "test"}
        })
      end)
    end

    test "halts 0 on success with depth param", %{server: server} do
      body = ~s({"value":[]})

      expect_success_json(server, "/test/_apis/wit/classificationNodes/areas", body, fn ->
        Areas.list_areas(%{
          options: %{json: true, depth: 5},
          arguments: %{project: "test"}
        })
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wit/classificationNodes/areas", 500, "{}", fn ->
        Areas.list_areas(%{
          options: %{json: true, depth: nil},
          arguments: %{project: "test"}
        })
      end)
    end
  end

  describe "show_area/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":1,"name":"Area1","path":"\\test\\area1"})

      expect_success_json(server, "/test/_apis/wit/classificationNodes/areas/area1", body, fn ->
        Areas.show_area(%{
          options: %{json: true},
          arguments: %{project: "test", area_path: "area1"}
        })
      end)
    end
  end

  describe "create_area/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":2,"name":"NewArea","path":"\\test\\NewArea"})

      expect_post_success(server, "/test/_apis/wit/classificationNodes", "", body, fn ->
        Areas.create_area(%{
          options: %{json: true, name: "NewArea", parent: nil},
          arguments: %{project: "test"}
        })
      end)
    end
  end

  describe "update_area/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":1,"name":"RenamedArea","path":"\\test\\area1"})

      expect_patch_success(
        server,
        "/test/_apis/wit/classificationNodes/areas/area1",
        "",
        body,
        fn ->
          Areas.update_area(%{
            options: %{json: true, name: "RenamedArea"},
            arguments: %{project: "test", area_path: "area1"}
          })
        end
      )
    end
  end

  describe "delete_area/1" do
    test "halts 0 on success", %{server: server} do
      expect_delete_success(server, "/test/_apis/wit/classificationNodes/areas/area1", fn ->
        Areas.delete_area(%{
          options: %{json: true, force: false},
          arguments: %{project: "test", area_path: "area1"}
        })
      end)
    end
  end
end
