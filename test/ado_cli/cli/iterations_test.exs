defmodule AdoCli.CLI.IterationsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Iterations

  describe "list_iterations" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/test/_apis/work/teamsettings/iterations",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.Iterations, :list_iterations, [
            %{
              options: %{json: true, top: nil},
              arguments: %{project: "test", team: "Default Team"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/work/teamsettings/iterations", 500, "{}", fn ->
        apply(AdoCli.CLI.Iterations, :list_iterations, [
          %{options: %{json: true, top: nil}, arguments: %{project: "test", team: "Default Team"}}
        ])
      end)
    end
  end

  describe "show_iteration" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/test/_apis/work/teamsettings/iterations/1",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.Iterations, :show_iteration, [
            %{
              options: %{json: true},
              arguments: %{project: "test", team: "Default Team", iteration_id: "1"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/work/teamsettings/iterations/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Iterations, :show_iteration, [
          %{
            options: %{json: true},
            arguments: %{project: "test", team: "Default Team", iteration_id: "1"}
          }
        ])
      end)
    end
  end

  describe "create_iteration" do
    test "halts 0 on successful post", %{server: server} do
      expect_post_success(
        server,
        "/test/_apis/work/teamsettings/iterations",
        "",
        "{\"id\":1}",
        fn ->
          apply(AdoCli.CLI.Iterations, :create_iteration, [
            %{
              options: %{
                json: true,
                name: "Sprint 1",
                start_date: "2024-01-01",
                finish_date: "2024-01-14"
              },
              arguments: %{project: "test", team: "Default Team"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/work/teamsettings/iterations", 500, "{}", fn ->
        apply(AdoCli.CLI.Iterations, :create_iteration, [
          %{
            options: %{
              json: true,
              name: "Sprint 1",
              start_date: "2024-01-01",
              finish_date: "2024-01-14"
            },
            arguments: %{project: "test", team: "Default Team"}
          }
        ])
      end)
    end
  end

  describe "update_iteration" do
    test "halts 0 on successful patch", %{server: server} do
      expect_patch_success(
        server,
        "/test/_apis/work/teamsettings/iterations/1",
        "",
        "{\"id\":1}",
        fn ->
          apply(AdoCli.CLI.Iterations, :update_iteration, [
            %{
              options: %{json: true, name: "Sprint 1 Updated"},
              arguments: %{project: "test", team: "Default Team", iteration_id: "1"}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/work/teamsettings/iterations/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Iterations, :update_iteration, [
          %{
            options: %{json: true, name: "Sprint 1 Updated"},
            arguments: %{project: "test", team: "Default Team", iteration_id: "1"}
          }
        ])
      end)
    end
  end

  describe "delete_iteration" do
    test "halts 0 on successful delete", %{server: server} do
      expect_delete_success(server, "/test/_apis/work/teamsettings/iterations/1", fn ->
        apply(AdoCli.CLI.Iterations, :delete_iteration, [
          %{
            options: %{json: true, force: false},
            arguments: %{project: "test", team: "Default Team", iteration_id: "1"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/work/teamsettings/iterations/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Iterations, :delete_iteration, [
          %{
            options: %{json: true, force: false},
            arguments: %{project: "test", team: "Default Team", iteration_id: "1"}
          }
        ])
      end)
    end
  end
end
