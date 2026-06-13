defmodule AdoCli.CLI.WikisTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Wikis

  describe "list_wikis" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/wiki/wikis", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Wikis, :list_wikis, [
          %{options: %{json: true, top: nil}, arguments: %{project: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wiki/wikis", 500, "{}", fn ->
        apply(AdoCli.CLI.Wikis, :list_wikis, [
          %{options: %{json: true, top: nil}, arguments: %{project: "test"}}
        ])
      end)
    end
  end

  describe "show_wiki" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/wiki/wikis/1", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Wikis, :show_wiki, [
          %{options: %{json: true}, arguments: %{project: "test", wiki_id: "1"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wiki/wikis/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Wikis, :show_wiki, [
          %{options: %{json: true}, arguments: %{project: "test", wiki_id: "1"}}
        ])
      end)
    end
  end

  describe "list_pages" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/wiki/wikis/1/pages", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Wikis, :list_pages, [
          %{
            options: %{json: true, top: nil, path: nil, recursion_level: nil},
            arguments: %{project: "test", wiki_id: "1"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wiki/wikis/1/pages", 500, "{}", fn ->
        apply(AdoCli.CLI.Wikis, :list_pages, [
          %{
            options: %{json: true, top: nil, path: nil, recursion_level: nil},
            arguments: %{project: "test", wiki_id: "1"}
          }
        ])
      end)
    end
  end

  describe "show_page" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/wiki/wikis/1/pages/test", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Wikis, :show_page, [
          %{
            options: %{json: true, include_content: true, recursion_level: "full"},
            arguments: %{project: "test", wiki_id: "1", path: "test"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wiki/wikis/1/pages/test", 500, "{}", fn ->
        apply(AdoCli.CLI.Wikis, :show_page, [
          %{
            options: %{json: true, include_content: true, recursion_level: "full"},
            arguments: %{project: "test", wiki_id: "1", path: "test"}
          }
        ])
      end)
    end
  end

  describe "create_page" do
    test "halts 0 on successful put", %{server: server} do
      expect_put_success(server, "/test/_apis/wiki/wikis/1/pages", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Wikis, :create_page, [
          %{
            options: %{json: true, path: "/test", content: "test"},
            arguments: %{project: "test", wiki_id: "1"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wiki/wikis/1/pages", 500, "{}", fn ->
        apply(AdoCli.CLI.Wikis, :create_page, [
          %{
            options: %{json: true, path: "/test", content: "test"},
            arguments: %{project: "test", wiki_id: "1"}
          }
        ])
      end)
    end
  end

  describe "update_page" do
    test "halts 0 on successful patch", %{server: server} do
      expect_patch_success(server, "/test/_apis/wiki/wikis/1/pages", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Wikis, :update_page, [
          %{
            options: %{json: true, path: "/test", content: "updated"},
            arguments: %{project: "test", wiki_id: "1"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/wiki/wikis/1/pages", 500, "{}", fn ->
        apply(AdoCli.CLI.Wikis, :update_page, [
          %{
            options: %{json: true, path: "/test", content: "updated"},
            arguments: %{project: "test", wiki_id: "1"}
          }
        ])
      end)
    end
  end
end
