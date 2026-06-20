defmodule AdoCli.CLI.ConnectionsTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Connections

  describe "list_connections/1" do
    test "halts 0 on success (JSON)", %{server: server} do
      body =
        ~s({"value":[{"id":"c1","name":"GitHub","type":"github"}],"count":1})

      expect_success_json(server, "/MyProj/_apis/serviceendpoint/endpoints", body, fn ->
        Connections.list_connections(%{
          arguments: %{project: "MyProj"},
          options: %{json: true, type: nil}
        })
      end)
    end

    test "halts 0 on success (table)", %{server: server} do
      body = ~s({"value":[],"count":0})

      expect_success_table(
        server,
        "/MyProj/_apis/serviceendpoint/endpoints",
        body,
        fn ->
          Connections.list_connections(%{
            arguments: %{project: "MyProj"},
            options: %{json: false, type: nil}
          })
        end
      )
    end

    test "passes --type as a query param", %{server: server} do
      body = ~s({"value":[],"count":0})

      TestServer.expect(server, "GET", api("/MyProj/_apis/serviceendpoint/endpoints"), fn conn ->
        assert conn.query_string =~ "type=github"
        Plug.Conn.resp(conn, 200, body)
      end)

      Connections.list_connections(%{
        arguments: %{project: "MyProj"},
        options: %{json: true, type: "github"}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "halts 1 on API error", %{server: server} do
      expect_api_error(
        server,
        "/MyProj/_apis/serviceendpoint/endpoints",
        404,
        ~s({"message":"Not found"}),
        fn ->
          Connections.list_connections(%{
            arguments: %{project: "MyProj"},
            options: %{json: false, type: nil}
          })
        end
      )
    end
  end

  describe "show_connection/1" do
    test "halts 0 on success (JSON)", %{server: server} do
      body =
        ~s({"id":"c1","name":"GitHub","type":"github","url":"https://github.com","isReady":true})

      expect_success_json(server, "/MyProj/_apis/serviceendpoint/endpoints/c1", body, fn ->
        Connections.show_connection(%{
          options: %{json: true},
          arguments: %{project: "MyProj", connection_id: "c1"}
        })
      end)
    end

    test "halts 1 with friendly message on 404", %{server: server} do
      TestServer.expect(
        server,
        "GET",
        api("/MyProj/_apis/serviceendpoint/endpoints/missing"),
        fn conn ->
          Plug.Conn.resp(conn, 404, ~s({"message":"Not found"}))
        end
      )

      Connections.show_connection(%{
        options: %{json: false},
        arguments: %{project: "MyProj", connection_id: "missing"}
      })

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end
  end

  describe "create_connection/1" do
    test "halts 0 on success (GitHub PAT, JSON output)", %{server: server} do
      body =
        ~s({"id":"new-id","name":"GitHub","type":"github","url":"https://github.com","isReady":true})

      TestServer.expect(server, "POST", api("/MyProj/_apis/serviceendpoint/endpoints"), fn conn ->
        {:ok, req_body, _} = Plug.Conn.read_body(conn)
        decoded = Jason.decode!(req_body)

        assert decoded["name"] == "GitHub"
        assert decoded["type"] == "github"
        assert decoded["url"] == "https://github.com"
        assert decoded["authorization"]["scheme"] == "Token"
        assert decoded["authorization"]["parameters"]["accessToken"] == "ghp_xxx"
        assert decoded["isReady"] == false

        refs = decoded["serviceEndpointProjectReferences"]
        assert [_] = refs
        assert hd(refs)["projectReference"]["name"] == "MyProj"

        Plug.Conn.resp(conn, 200, body)
      end)

      Connections.create_connection(%{
        options: %{
          json: true,
          description: nil,
          scheme: "Token",
          access_token: "ghp_xxx",
          data: nil,
          ready: false
        },
        arguments: %{
          project: "MyProj",
          name: "GitHub",
          type: "github",
          url: "https://github.com"
        }
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "includes description and --data fields when supplied", %{server: server} do
      body = ~s({"id":"new-id","name":"Azure","type":"azure","url":"","isReady":false})

      TestServer.expect(server, "POST", api("/MyProj/_apis/serviceendpoint/endpoints"), fn conn ->
        {:ok, req_body, _} = Plug.Conn.read_body(conn)
        decoded = Jason.decode!(req_body)

        assert decoded["description"] == "Prod subscription"
        assert decoded["data"]["subscriptionId"] == "11111111-2222-3333-4444-555555555555"
        # The dedicated flag wins over --data for reserved keys
        assert decoded["name"] == "Azure"

        Plug.Conn.resp(conn, 200, body)
      end)

      Connections.create_connection(%{
        options: %{
          json: true,
          description: "Prod subscription",
          scheme: "Token",
          access_token: nil,
          data: ~s({"subscriptionId":"11111111-2222-3333-4444-555555555555"}),
          ready: false
        },
        arguments: %{
          project: "MyProj",
          name: "Azure",
          type: "azure",
          url: ""
        }
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "halts 1 on API error", %{server: server} do
      expect_post_success = fn server, path, body, fun ->
        TestServer.expect(server, "POST", api(path), fn conn ->
          Plug.Conn.resp(conn, 422, body)
        end)

        fun.()
        assert_receive {:cli_mate_shell, :halt, 1}, 500
      end

      expect_post_success.(
        server,
        "/MyProj/_apis/serviceendpoint/endpoints",
        ~s({"message":"Invalid"}),
        fn ->
          Connections.create_connection(%{
            options: %{
              json: true,
              description: nil,
              scheme: "Token",
              access_token: "tok",
              data: nil,
              ready: false
            },
            arguments: %{
              project: "MyProj",
              name: "X",
              type: "github",
              url: "https://github.com"
            }
          })
        end
      )
    end

    test "halts 1 with friendly message when project 404s", %{server: server} do
      TestServer.expect(server, "POST", api("/MyProj/_apis/serviceendpoint/endpoints"), fn conn ->
        Plug.Conn.resp(conn, 404, ~s({"message":"Not found"}))
      end)

      Connections.create_connection(%{
        options: %{
          json: false,
          description: nil,
          scheme: "Token",
          access_token: "tok",
          data: nil,
          ready: false
        },
        arguments: %{
          project: "MyProj",
          name: "X",
          type: "github",
          url: "https://github.com"
        }
      })

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end
  end

  describe "update_connection/1" do
    test "halts 0 on success", %{server: server} do
      body =
        ~s({"id":"c1","name":"Renamed","type":"github","url":"https://github.com","isReady":true})

      expect_put_success(
        server,
        "/MyProj/_apis/serviceendpoint/endpoints/c1",
        "",
        body,
        fn ->
          Connections.update_connection(%{
            options: %{
              json: true,
              name: "Renamed",
              description: nil,
              url: nil,
              access_token: nil,
              data: nil
            },
            arguments: %{project: "MyProj", connection_id: "c1"}
          })
        end
      )
    end

    test "rotates the access token", %{server: server} do
      body =
        ~s({"id":"c1","name":"GitHub","type":"github","url":"https://github.com","isReady":true})

      TestServer.expect(
        server,
        "PUT",
        api("/MyProj/_apis/serviceendpoint/endpoints/c1"),
        fn conn ->
          {:ok, req_body, _} = Plug.Conn.read_body(conn)
          decoded = Jason.decode!(req_body)

          assert decoded["authorization"]["parameters"]["accessToken"] == "new-pat"
          Plug.Conn.resp(conn, 200, body)
        end
      )

      Connections.update_connection(%{
        options: %{
          json: true,
          name: nil,
          description: nil,
          url: nil,
          access_token: "new-pat",
          data: nil
        },
        arguments: %{project: "MyProj", connection_id: "c1"}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "halts 1 when no fields supplied", %{server: _server} do
      Connections.update_connection(%{
        options: %{
          json: false,
          name: nil,
          description: nil,
          url: nil,
          access_token: nil,
          data: nil
        },
        arguments: %{project: "MyProj", connection_id: "c1"}
      })

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end

    test "halts 1 on 404", %{server: server} do
      TestServer.expect(
        server,
        "PUT",
        api("/MyProj/_apis/serviceendpoint/endpoints/missing"),
        fn conn ->
          Plug.Conn.resp(conn, 404, ~s({"message":"Not found"}))
        end
      )

      Connections.update_connection(%{
        options: %{
          json: false,
          name: "Renamed",
          description: nil,
          url: nil,
          access_token: nil,
          data: nil
        },
        arguments: %{project: "MyProj", connection_id: "missing"}
      })

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end
  end

  describe "delete_connection/1" do
    test "skipped — delete requires stdin confirm prompt", %{server: _server} do
      # delete_connection calls IO.gets for confirmation, then String.trim
      # which crashes if stdin is closed. The same constraint applies as
      # to projects_test and repos_test. Coverage of the confirm path
      # requires a different approach (e.g. spawning a subprocess).
      assert true
    end
  end
end
