defmodule AdoCli.CLI.ReleasesTest do
  use AdoCli.CLI.TestHelper

  describe "list_releases" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/testorg/_apis/release/releases", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Releases, :list_releases, [
          %{
            options: %{json: true, top: nil, definition_id: nil},
            arguments: %{project: "testorg"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/testorg/_apis/release/releases", 500, "{}", fn ->
        apply(AdoCli.CLI.Releases, :list_releases, [
          %{
            options: %{json: true, top: nil, definition_id: nil},
            arguments: %{project: "testorg"}
          }
        ])
      end)
    end
  end

  describe "show_release" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/testorg/_apis/release/releases/1", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Releases, :show_release, [
          %{
            options: %{json: true, include_artifacts: false, expand: "none"},
            arguments: %{project: "testorg", release_id: 1}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/testorg/_apis/release/releases/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Releases, :show_release, [
          %{
            options: %{json: true, include_artifacts: false, expand: "none"},
            arguments: %{project: "testorg", release_id: 1}
          }
        ])
      end)
    end
  end
end
