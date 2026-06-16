defmodule AdoCli.CLI.TeamsTest do
  use AdoCli.CLI.TestHelper

  describe "list_teams" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/testorg/_apis/teams", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Teams, :list_teams, [
          %{
            options: %{
              json: true,
              top: nil,
              mine: false,
              expand_identity: false,
              subject_types: nil
            },
            arguments: %{project: "testorg"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/testorg/_apis/teams", 500, "{}", fn ->
        apply(AdoCli.CLI.Teams, :list_teams, [
          %{
            options: %{
              json: true,
              top: nil,
              mine: false,
              expand_identity: false,
              subject_types: nil
            },
            arguments: %{project: "testorg"}
          }
        ])
      end)
    end
  end

  describe "show_team" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/testorg/_apis/teams/1", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Teams, :show_team, [
          %{
            options: %{json: true, expand_identity: false},
            arguments: %{project: "testorg", team_id: "1"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/testorg/_apis/teams/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Teams, :show_team, [
          %{
            options: %{json: true, expand_identity: false},
            arguments: %{project: "testorg", team_id: "1"}
          }
        ])
      end)
    end
  end

  describe "create_team" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(server, "/testorg/_apis/teams", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Teams, :create_team, [
          %{
            options: %{json: true, name: "New Team", description: nil},
            arguments: %{project: "testorg"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/testorg/_apis/teams", 500, "{}", fn ->
        apply(AdoCli.CLI.Teams, :create_team, [
          %{
            options: %{json: true, name: "New Team", description: nil},
            arguments: %{project: "testorg"}
          }
        ])
      end)
    end
  end

  describe "update_team" do
    test "halts 0 on successful patch", %{server: server} do
      expect_patch_success(server, "/testorg/_apis/teams/1", "", "{\"id\":1}", fn ->
        apply(AdoCli.CLI.Teams, :update_team, [
          %{
            options: %{json: true, name: "Updated Team", description: nil},
            arguments: %{project: "testorg", team_id: "1"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/testorg/_apis/teams/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Teams, :update_team, [
          %{
            options: %{json: true, name: "Updated Team", description: nil},
            arguments: %{project: "testorg", team_id: "1"}
          }
        ])
      end)
    end
  end

  describe "delete_team" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/testorg/_apis/teams/1", fn ->
        apply(AdoCli.CLI.Teams, :delete_team, [
          %{options: %{json: true, force: false}, arguments: %{project: "testorg", team_id: "1"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/testorg/_apis/teams/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Teams, :delete_team, [
          %{options: %{json: true, force: false}, arguments: %{project: "testorg", team_id: "1"}}
        ])
      end)
    end
  end

  describe "list_team_members" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/testorg/_apis/teams/1/members", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Teams, :list_team_members, [
          %{options: %{json: true, top: nil}, arguments: %{project: "testorg", team_id: "1"}}
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/testorg/_apis/teams/1/members", 500, "{}", fn ->
        apply(AdoCli.CLI.Teams, :list_team_members, [
          %{options: %{json: true, top: nil}, arguments: %{project: "testorg", team_id: "1"}}
        ])
      end)
    end
  end
end
