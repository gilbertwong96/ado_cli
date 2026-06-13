defmodule AdoCli.CLI.PackagesTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Packages

  describe "list_packages" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/packaging/feeds", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Packages, :list_packages, [
          %{options: %{json: true, top: nil}, arguments: %{project: "test"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/packaging/feeds", 500, "{}", fn ->
        apply(AdoCli.CLI.Packages, :list_packages, [
          %{options: %{json: true, top: nil}, arguments: %{project: "test"}}
        ])
      end)
    end
  end

  describe "list_versions" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/test/_apis/packaging/feeds/1/packages",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.Packages, :list_versions, [
            %{options: %{json: true, top: nil}, arguments: %{project: "test", feed_id: 1}}
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/packaging/feeds/1/packages", 500, "{}", fn ->
        apply(AdoCli.CLI.Packages, :list_versions, [
          %{options: %{json: true, top: nil}, arguments: %{project: "test", feed_id: 1}}
        ])
      end)
    end
  end

  describe "show_package" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/test/_apis/packaging/feeds/1/packages/1",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.Packages, :show_package, [
            %{options: %{json: true}, arguments: %{project: "test", feed_id: 1, package_id: "1"}}
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/packaging/feeds/1/packages/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Packages, :show_package, [
          %{options: %{json: true}, arguments: %{project: "test", feed_id: 1, package_id: "1"}}
        ])
      end)
    end
  end
end
