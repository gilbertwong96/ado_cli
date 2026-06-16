defmodule AdoCli.CLI.TestResultsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.TestResults
  alias AdoCli.CLI.TestCoverage

  describe "test-results list" do
    test "halts 0 on success with runs", %{server: server} do
      body =
        ~s({"value":[{"id":1,"name":"Unit Tests","state":"Completed","runStatistics":[{"outcome":"Passed","count":42}]}]})

      expect_success_json(server, "/MyProject/_apis/test/runs", body, fn ->
        TestResults.list_runs(%{
          arguments: %{project: "MyProject"},
          options: %{json: true, top: nil, "build-id": nil, "min-last-updated": nil}
        })
      end)
    end

    test "halts 0 on empty list", %{server: server} do
      body = ~s({"value":[]})

      expect_success_json(server, "/MyProject/_apis/test/runs", body, fn ->
        TestResults.list_runs(%{
          arguments: %{project: "MyProject"},
          options: %{json: true, top: nil, "build-id": nil, "min-last-updated": nil}
        })
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(server, "/MyProject/_apis/test/runs", 500, ~s({"message":"fail"}), fn ->
        TestResults.list_runs(%{
          arguments: %{project: "MyProject"},
          options: %{json: true, top: nil, "build-id": nil, "min-last-updated": nil}
        })
      end)
    end
  end

  describe "test-results show" do
    test "halts 0 on success", %{server: server} do
      body =
        ~s({"id":1,"name":"Unit Tests","state":"Completed","runStatistics":[{"outcome":"Passed","count":42}]})

      expect_success_json(server, "/MyProject/_apis/test/runs/1", body, fn ->
        TestResults.show_run(%{
          arguments: %{project: "MyProject", run_id: 1},
          options: %{json: true}
        })
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/MyProject/_apis/test/runs/99",
        404,
        ~s({"message":"not found"}),
        fn ->
          TestResults.show_run(%{
            arguments: %{project: "MyProject", run_id: 99},
            options: %{json: false}
          })
        end
      )
    end
  end

  describe "test-coverage show" do
    test "halts 0 on success", %{server: server} do
      body = ~s({"coverageData":[{"coverageStats":[{"label":"Lines","total":100,"covered":85}]}]})

      expect_success_json(server, "/MyProject/_apis/test/codecoverage", body, fn ->
        TestCoverage.show_coverage(%{
          arguments: %{project: "MyProject", build_id: 42},
          options: %{json: true}
        })
      end)
    end

    test "halts 0 on empty coverage", %{server: server} do
      body = ~s({"coverageData":[]})

      expect_success_json(server, "/MyProject/_apis/test/codecoverage", body, fn ->
        TestCoverage.show_coverage(%{
          arguments: %{project: "MyProject", build_id: 42},
          options: %{json: true}
        })
      end)
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/MyProject/_apis/test/codecoverage",
        500,
        ~s({"message":"fail"}),
        fn ->
          TestCoverage.show_coverage(%{
            arguments: %{project: "MyProject", build_id: 42},
            options: %{json: false}
          })
        end
      )
    end
  end
end
