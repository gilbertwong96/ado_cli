defmodule AdoCli.CLI.ExtensionsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Extensions

  describe "list_extensions/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"value":[{"extensionName":"vss-services","publisherId":"ms","version":"1.0.0"}]})

      expect_success_json(server, "/_apis/extensionmanagement/installedextensions", body, fn ->
        Extensions.list_extensions(%{
          options: %{json: true, top: nil, include_disabled: false}
        })
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/extensionmanagement/installedextensions", 500, "{}", fn ->
        Extensions.list_extensions(%{
          options: %{json: true, top: nil, include_disabled: false}
        })
      end)
    end
  end

  describe "show_extension/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"extensionName":"vss-services","publisherId":"ms","version":"1.0.0"})

      expect_success_json(
        server,
        "/_apis/extensionmanagement/installedextensions/ms.vss-services",
        body,
        fn ->
          Extensions.show_extension(%{
            options: %{json: true},
            arguments: %{extension_id: "ms.vss-services"}
          })
        end
      )
    end
  end

  describe "install_extension/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"extensionName":"new-ext","publisherId":"ms","version":"1.0.0"})

      expect_post_success(
        server,
        "/_apis/extensionmanagement/installedextensions",
        "",
        body,
        fn ->
          Extensions.install_extension(%{
            options: %{json: true, publisher: "ms", extension: "new-ext", version: "1.0.0"}
          })
        end
      )
    end
  end

  describe "uninstall_extension/1" do
    test "halts 0 on success", %{server: server} do
      expect_delete_success(
        server,
        "/_apis/extensionmanagement/installedextensions/ms.vss-services",
        fn ->
          Extensions.uninstall_extension(%{
            options: %{json: true, force: false, reason: "test"},
            arguments: %{publisher: "ms", name: "vss-services"}
          })
        end
      )
    end
  end

  describe "enable_extension/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"extensionName":"vss-services","publisherId":"ms"})

      expect_patch_success(
        server,
        "/_apis/extensionmanagement/installedextensions/ms.vss-services",
        "",
        body,
        fn ->
          Extensions.enable_extension(%{
            options: %{json: true},
            arguments: %{publisher: "ms", name: "vss-services"}
          })
        end
      )
    end
  end

  describe "disable_extension/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"extensionName":"vss-services","publisherId":"ms"})

      expect_patch_success(
        server,
        "/_apis/extensionmanagement/installedextensions/ms.vss-services",
        "",
        body,
        fn ->
          Extensions.disable_extension(%{
            options: %{json: true, reason: "test"},
            arguments: %{publisher: "ms", name: "vss-services"}
          })
        end
      )
    end
  end
end
