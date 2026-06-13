defmodule AdoCli.CLI.ExtensionsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Extensions

  describe "list_extensions" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/_apis/extensionmanagement/installedextensions",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.Extensions, :list_extensions, [
            %{options: %{json: true, top: nil, include_disabled: false}}
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/extensionmanagement/installedextensions", 500, "{}", fn ->
        apply(AdoCli.CLI.Extensions, :list_extensions, [
          %{options: %{json: true, top: nil, include_disabled: false}}
        ])
      end)
    end
  end

  describe "show_extension" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/_apis/extensionmanagement/installedextensions/1",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.Extensions, :show_extension, [
            %{options: %{json: true}, arguments: %{ext: "1"}}
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/_apis/extensionmanagement/installedextensions/1",
        500,
        "{}",
        fn ->
          apply(AdoCli.CLI.Extensions, :show_extension, [
            %{options: %{json: true}, arguments: %{ext: "1"}}
          ])
        end
      )
    end
  end

  describe "install_extension" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(
        server,
        "/_apis/extensionmanagement/installedextensions",
        "",
        "{\"id\":1}",
        fn ->
          apply(AdoCli.CLI.Extensions, :install_extension, [
            %{options: %{json: true, publisher: "ms", extension: "vss-services"}}
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/extensionmanagement/installedextensions", 500, "{}", fn ->
        apply(AdoCli.CLI.Extensions, :install_extension, [
          %{options: %{json: true, publisher: "ms", extension: "vss-services"}}
        ])
      end)
    end
  end

  describe "uninstall_extension" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/_apis/extensionmanagement/installedextensions/1", fn ->
        apply(AdoCli.CLI.Extensions, :uninstall_extension, [
          %{
            options: %{json: true, force: false, reason: "test"},
            arguments: %{publisher: "ms", name: "vss-services"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/_apis/extensionmanagement/installedextensions/1",
        500,
        "{}",
        fn ->
          apply(AdoCli.CLI.Extensions, :uninstall_extension, [
            %{
              options: %{json: true, force: false, reason: "test"},
              arguments: %{publisher: "ms", name: "vss-services"}
            }
          ])
        end
      )
    end
  end

  describe "enable_extension" do
    test "halts 0 on successful patch", %{server: server} do
      expect_patch_success(
        server,
        "/_apis/extensionmanagement/installedextensions/1",
        "",
        "{\"id\":1}",
        fn ->
          apply(AdoCli.CLI.Extensions, :enable_extension, [
            %{options: %{json: true}, arguments: %{publisher: "ms", name: "vss-services"}}
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/_apis/extensionmanagement/installedextensions/1",
        500,
        "{}",
        fn ->
          apply(AdoCli.CLI.Extensions, :enable_extension, [
            %{options: %{json: true}, arguments: %{publisher: "ms", name: "vss-services"}}
          ])
        end
      )
    end
  end

  describe "disable_extension" do
    test "halts 0 on successful patch", %{server: server} do
      expect_patch_success(
        server,
        "/_apis/extensionmanagement/installedextensions/1",
        "",
        "{\"id\":1}",
        fn ->
          apply(AdoCli.CLI.Extensions, :disable_extension, [
            %{
              options: %{json: true, reason: "test"},
              arguments: %{publisher: "ms", name: "vss-services"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/_apis/extensionmanagement/installedextensions/1",
        500,
        "{}",
        fn ->
          apply(AdoCli.CLI.Extensions, :disable_extension, [
            %{
              options: %{json: true, reason: "test"},
              arguments: %{publisher: "ms", name: "vss-services"}
            }
          ])
        end
      )
    end
  end
end
