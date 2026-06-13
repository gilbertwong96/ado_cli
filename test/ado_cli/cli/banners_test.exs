defmodule AdoCli.CLI.BannersTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Banners

  describe "show_banner" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/_apis/notification/banners", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Banners, :show_banner, [%{options: %{json: true}}])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/notification/banners", 500, "{}", fn ->
        apply(AdoCli.CLI.Banners, :show_banner, [%{options: %{json: true}}])
      end)
    end
  end

  describe "set_banner" do
    test "halts 0 on successful put", %{server: server} do
      expect_put_success(server, "/_apis/notification/banners", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Banners, :set_banner, [%{options: %{json: true, message: "test"}}])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/notification/banners", 500, "{}", fn ->
        apply(AdoCli.CLI.Banners, :set_banner, [%{options: %{json: true, message: "test"}}])
      end)
    end
  end

  describe "delete_banner" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/_apis/notification/banners", fn ->
        apply(AdoCli.CLI.Banners, :delete_banner, [%{options: %{json: true, force: false}}])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/notification/banners", 500, "{}", fn ->
        apply(AdoCli.CLI.Banners, :delete_banner, [%{options: %{json: true, force: false}}])
      end)
    end
  end
end
