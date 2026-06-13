defmodule AdoCli.CLI.UsersTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Users

  describe "list_users" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/_apis/identities", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Users, :list_users, [
          %{options: %{json: true, top: nil, filter: nil, subject_types: nil}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/identities", 500, "{}", fn ->
        apply(AdoCli.CLI.Users, :list_users, [
          %{options: %{json: true, top: nil, filter: nil, subject_types: nil}}
        ])
      end)
    end
  end

  describe "show_user" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/_apis/identities/1", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Users, :show_user, [
          %{options: %{json: true}, arguments: %{user_id: "1"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/identities/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Users, :show_user, [
          %{options: %{json: true}, arguments: %{user_id: "1"}}
        ])
      end)
    end
  end

  describe "add_user" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(server, "/_apis/identities", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Users, :add_user, [
          %{options: %{json: true, descriptor: "vssgp.Uy0xLTkt"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/identities", 500, "{}", fn ->
        apply(AdoCli.CLI.Users, :add_user, [
          %{options: %{json: true, descriptor: "vssgp.Uy0xLTkt"}}
        ])
      end)
    end
  end

  describe "remove_user" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/_apis/identities/1", fn ->
        apply(AdoCli.CLI.Users, :remove_user, [
          %{options: %{json: true, force: false}, arguments: %{user_id: "1"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/identities/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Users, :remove_user, [
          %{options: %{json: true, force: false}, arguments: %{user_id: "1"}}
        ])
      end)
    end
  end
end
