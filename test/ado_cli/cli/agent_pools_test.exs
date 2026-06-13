defmodule AdoCli.CLI.AgentPoolsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.AgentPools

  describe "list_pools/1" do
    test "halts 0 on success (JSON)", %{server: server} do
      body = ~s({"value":[{"id":1,"name":"Default"}]})

      expect_success_json(server, "/_apis/distributedtask/pools", body, fn ->
        AgentPools.list_pools(%{options: %{json: true, top: nil, skip: nil}})
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/_apis/distributedtask/pools", 500, "{}", fn ->
        AgentPools.list_pools(%{options: %{json: true, top: nil, skip: nil}})
      end)
    end
  end

  describe "show_pool/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"id":1,"name":"Default","size":1})

      expect_success_json(server, "/_apis/distributedtask/pools/1", body, fn ->
        AgentPools.show_pool(%{
          options: %{json: true, include_agents: false, top: nil},
          arguments: %{pool_id: 1}
        })
      end)
    end
  end

  describe "list_queues/1" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"value":[{"id":1,"name":"Default"}]})

      expect_success_json(server, "/test/_apis/distributedtask/queues", body, fn ->
        AgentPools.list_queues(%{
          options: %{json: true, pool: 1, top: nil},
          arguments: %{project: "test"}
        })
      end)
    end
  end
end
