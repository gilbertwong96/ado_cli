defmodule AdoCli.CLI.FoldersTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Folders

  describe "list_folders" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/pipelines/folders", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Folders, :list_folders, [
          %{options: %{json: true, top: nil, query: nil}, arguments: %{project: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/pipelines/folders", 500, "{}", fn ->
        apply(AdoCli.CLI.Folders, :list_folders, [
          %{options: %{json: true, top: nil, query: nil}, arguments: %{project: "test"}}
        ])
      end)
    end
  end

  describe "create_folder" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(server, "/test/_apis/pipelines/folders", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Folders, :create_folder, [
          %{options: %{json: true, path: "/test"}, arguments: %{project: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/pipelines/folders", 500, "{}", fn ->
        apply(AdoCli.CLI.Folders, :create_folder, [
          %{options: %{json: true, path: "/test"}, arguments: %{project: "test"}}
        ])
      end)
    end
  end

  describe "delete_folder" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/test/_apis/pipelines/folders/test", fn ->
        apply(AdoCli.CLI.Folders, :delete_folder, [
          %{options: %{json: true, force: false}, arguments: %{project: "test", path: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/pipelines/folders/test", 500, "{}", fn ->
        apply(AdoCli.CLI.Folders, :delete_folder, [
          %{options: %{json: true, force: false}, arguments: %{project: "test", path: "test"}}
        ])
      end)
    end
  end
end
