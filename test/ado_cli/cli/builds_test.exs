defmodule AdoCli.CLI.BuildsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Builds

  describe "list_builds" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/build/builds", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Builds, :list_builds, [
          %{
            options: %{json: true, top: nil, definitions: nil, branch: nil},
            arguments: %{project: "test"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/build/builds", 500, "{}", fn ->
        apply(AdoCli.CLI.Builds, :list_builds, [
          %{
            options: %{json: true, top: nil, definitions: nil, branch: nil},
            arguments: %{project: "test"}
          }
        ])
      end)
    end
  end

  describe "show_build" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/build/builds/1", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Builds, :show_build, [
          %{options: %{json: true}, arguments: %{project: "test", build_id: 1}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/build/builds/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Builds, :show_build, [
          %{options: %{json: true}, arguments: %{project: "test", build_id: 1}}
        ])
      end)
    end
  end

  describe "queue_build" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(server, "/test/_apis/build/builds", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Builds, :queue_build, [
          %{
            options: %{json: true, definition_id: 1, source_branch: nil, parameters: nil},
            arguments: %{project: "test"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/build/builds", 500, "{}", fn ->
        apply(AdoCli.CLI.Builds, :queue_build, [
          %{
            options: %{json: true, definition_id: 1, source_branch: nil, parameters: nil},
            arguments: %{project: "test"}
          }
        ])
      end)
    end
  end

  describe "cancel_build" do
    test "halts 0 on successful patch", %{server: server} do
      expect_patch_success(server, "/test/_apis/build/builds/1", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Builds, :cancel_build, [
          %{options: %{json: true}, arguments: %{project: "test", build_id: 1}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/build/builds/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Builds, :cancel_build, [
          %{options: %{json: true}, arguments: %{project: "test", build_id: 1}}
        ])
      end)
    end
  end

  describe "list_tags" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/build/builds/1/tags", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Builds, :list_tags, [
          %{options: %{json: true}, arguments: %{project: "test", build_id: 1}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/build/builds/1/tags", 500, "{}", fn ->
        apply(AdoCli.CLI.Builds, :list_tags, [
          %{options: %{json: true}, arguments: %{project: "test", build_id: 1}}
        ])
      end)
    end
  end

  describe "add_tags" do
    test "halts 0 on successful put", %{server: server} do
      expect_put_success(server, "/test/_apis/build/builds/1/tags", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Builds, :add_tags, [
          %{options: %{json: true, tags: "tag1,tag2"}, arguments: %{project: "test", build_id: 1}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/build/builds/1/tags", 500, "{}", fn ->
        apply(AdoCli.CLI.Builds, :add_tags, [
          %{options: %{json: true, tags: "tag1,tag2"}, arguments: %{project: "test", build_id: 1}}
        ])
      end)
    end
  end

  describe "list_definitions" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/build/definitions", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Builds, :list_definitions, [
          %{options: %{json: true, top: nil, name: nil, path: nil}, arguments: %{project: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/build/definitions", 500, "{}", fn ->
        apply(AdoCli.CLI.Builds, :list_definitions, [
          %{options: %{json: true, top: nil, name: nil, path: nil}, arguments: %{project: "test"}}
        ])
      end)
    end
  end
end
