defmodule AdoCli.CLI.ConnectionsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Connections

  describe "list_connections" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(server, "/test/_apis/serviceendpoint/endpoints", ~s({"value":[]}), fn ->
        apply(AdoCli.CLI.Connections, :list_connections, [
          %{
            options: %{json: true, top: nil, type: nil, include_details: false},
            arguments: %{project: "test"}
          }
        ])
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/serviceendpoint/endpoints", 500, "{}", fn ->
        apply(AdoCli.CLI.Connections, :list_connections, [
          %{
            options: %{json: true, top: nil, type: nil, include_details: false},
            arguments: %{project: "test"}
          }
        ])
      end)
    end
  end

  describe "show_connection" do
    test "halts 0 on successful get", %{server: server} do
      expect_success_json(
        server,
        "/test/_apis/serviceendpoint/endpoints/1",
        ~s({"value":[]}),
        fn ->
          apply(AdoCli.CLI.Connections, :show_connection, [
            %{
              options: %{json: true, include_details: false},
              arguments: %{project: "test", connection_id: 1}
            }
          ])
        end
      )
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/test/_apis/serviceendpoint/endpoints/1", 500, "{}", fn ->
        apply(AdoCli.CLI.Connections, :show_connection, [
          %{
            options: %{json: true, include_details: false},
            arguments: %{project: "test", connection_id: 1}
          }
        ])
      end)
    end
  end
end
