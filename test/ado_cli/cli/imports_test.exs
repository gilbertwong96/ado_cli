defmodule AdoCli.CLI.ImportsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Imports

  describe "list_imports" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/git/importRequests", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Imports, :list_imports, [
          %{
            options: %{json: true, top: nil, include_abandoned: false},
            arguments: %{project: "test"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/importRequests", 500, "{}", fn ->
        apply(AdoCli.CLI.Imports, :list_imports, [
          %{
            options: %{json: true, top: nil, include_abandoned: false},
            arguments: %{project: "test"}
          }
        ])
      end)
    end
  end

  describe "show_import" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/git/importRequests/1", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Imports, :show_import, [
          %{options: %{json: true}, arguments: %{project: "test", import_id: 1}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/importRequests/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Imports, :show_import, [
          %{options: %{json: true}, arguments: %{project: "test", import_id: 1}}
        ])
      end)
    end
  end

  describe "create_import" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(server, "/test/_apis/git/importRequests", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Imports, :create_import, [
          %{
            options: %{
              json: true,
              source: "github",
              endpoint: "https://api.github.com",
              repository: "repo"
            },
            arguments: %{project: "test"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/git/importRequests", 500, "{}", fn ->
        apply(AdoCli.CLI.Imports, :create_import, [
          %{
            options: %{
              json: true,
              source: "github",
              endpoint: "https://api.github.com",
              repository: "repo"
            },
            arguments: %{project: "test"}
          }
        ])
      end)
    end
  end
end
