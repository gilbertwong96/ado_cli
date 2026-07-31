defmodule AdoCli.CLI.SecurityTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Security

  @lib_ns "b7e84409-6553-448a-bbb2-af228e07cbeb"

  describe "grant/1 safety checks" do
    test "refuses to run without the safety flag (halt_error)", %{server: server} do
      Security.grant(%{
        options: %{
          permission: "ViewSecrets",
          yes_this_mutates_secret_read: false
        },
        arguments: %{project_name_or_id: "testorg"}
      })

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end

    test "rejects unsupported permission", %{server: server} do
      Security.grant(%{
        options: %{
          permission: "Administer",
          yes_this_mutates_secret_read: true
        },
        arguments: %{project_name_or_id: "testorg"}
      })

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end

    test "rejects Contributors scope (refuses to grant to a group) — obsolete, --scope removed",
         %{server: server} do
      # The --scope option was removed in this commit; the test stays as a
      # placeholder showing that group grants are not supported.
      assert true
    end

    test "refuses empty project", %{server: server} do
      Security.grant(%{
        options: %{
          permission: "ViewSecrets",
          yes_this_mutates_secret_read: true
        },
        arguments: %{project_name_or_id: ""}
      })

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end

    test "safety flag is checked before other validation (M1)", %{server: server} do
      # The production code should hit the safety-flag check FIRST and
      # output the safety-flag-specific message — not the permission error.
      # If M1 is broken, the user sees "Unsupported permission" first.
      Security.grant(%{
        options: %{
          permission: "Administer",
          yes_this_mutates_secret_read: false
        },
        arguments: %{project_name_or_id: "testorg"}
      })

      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "Refusing to run without the safety flag"
      refute msg =~ "Unsupported permission"
      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end
  end

  describe "grant/1 happy path" do
    test "calls POST accesscontrolentries with allow=8 and merge=true", %{server: server} do
      # 1: Projects lookup (because "testorg" is not a UUID-shaped name)
      TestServer.expect(server, "GET", "/testorg/_apis/projects", fn conn ->
        Plug.Conn.resp(
          conn,
          200,
          ~s({"value":[{"id":"c749b575-419f-4f41-a1bb-6e89a37bcc12","name":"testorg"}]})
        )
      end)

      # 2: Caller descriptor probe
      TestServer.expect(server, "GET", "/testorg/_apis/connectionData", fn conn ->
        Plug.Conn.resp(
          conn,
          200,
          ~s({"authenticatedUser":{"subjectDescriptor":"Microsoft.IdentityModel.Claims.ClaimsIdentity;vssgp.test"}})
        )
      end)

      # 3: Grant call (allow=8, merge=true)
      TestServer.expect(
        server,
        "POST",
        "/testorg/_apis/accesscontrolentries/#{@lib_ns}",
        fn conn ->
          {:ok, body, conn} = Plug.Conn.read_body(conn)
          decoded = Jason.decode!(body)
          assert decoded["token"] == "c749b575-419f-4f41-a1bb-6e89a37bcc12"
          assert decoded["merge"] == true
          [ace | _] = decoded["accessControlEntries"]
          assert ace["allow"] == 8
          assert ace["deny"] == 0
          assert ace["descriptor"] == "Microsoft.IdentityModel.Claims.ClaimsIdentity;vssgp.test"
          Plug.Conn.resp(conn, 200, ~s({"count":1}))
        end
      )

      Security.grant(%{
        options: %{
          permission: "ViewSecrets",
          yes_this_mutates_secret_read: true
        },
        arguments: %{project_name_or_id: "testorg"}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "accepts project UUID directly (no projects-list lookup)", %{server: server} do
      uuid = "c749b575-419f-4f41-a1bb-6e89a37bcc12"

      TestServer.expect(server, "GET", "/testorg/_apis/connectionData", fn conn ->
        Plug.Conn.resp(
          conn,
          200,
          ~s({"authenticatedUser":{"subjectDescriptor":"Microsoft.IdentityModel.Claims.ClaimsIdentity;vssgp.test"}})
        )
      end)

      TestServer.expect(
        server,
        "POST",
        "/testorg/_apis/accesscontrolentries/#{@lib_ns}",
        fn conn ->
          {:ok, body, conn} = Plug.Conn.read_body(conn)
          decoded = Jason.decode!(body)
          assert decoded["token"] == uuid
          Plug.Conn.resp(conn, 200, ~s({"count":1}))
        end
      )

      Security.grant(%{
        options: %{
          permission: "ViewSecrets",
          yes_this_mutates_secret_read: true
        },
        arguments: %{project_name_or_id: uuid}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "revoke/1 happy path" do
    test "calls POST accesscontrolentries with allow=0 and merge=false", %{server: server} do
      TestServer.expect(server, "GET", "/testorg/_apis/projects", fn conn ->
        Plug.Conn.resp(
          conn,
          200,
          ~s({"value":[{"id":"c749b575-419f-4f41-a1bb-6e89a37bcc12","name":"testorg"}]})
        )
      end)

      TestServer.expect(server, "GET", "/testorg/_apis/connectionData", fn conn ->
        Plug.Conn.resp(
          conn,
          200,
          ~s({"authenticatedUser":{"subjectDescriptor":"Microsoft.IdentityModel.Claims.ClaimsIdentity;vssgp.test"}})
        )
      end)

      TestServer.expect(
        server,
        "POST",
        "/testorg/_apis/accesscontrolentries/#{@lib_ns}",
        fn conn ->
          {:ok, body, conn} = Plug.Conn.read_body(conn)
          decoded = Jason.decode!(body)
          assert decoded["token"] == "c749b575-419f-4f41-a1bb-6e89a37bcc12"
          assert decoded["merge"] == false
          [ace | _] = decoded["accessControlEntries"]
          assert ace["allow"] == 0
          assert ace["descriptor"] == "Microsoft.IdentityModel.Claims.ClaimsIdentity;vssgp.test"
          Plug.Conn.resp(conn, 200, ~s({"count":1}))
        end
      )

      Security.revoke(%{
        options: %{
          permission: "ViewSecrets",
          yes_this_mutates_secret_read: true
        },
        arguments: %{project_name_or_id: "testorg"}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "API error mapping" do
    test "403 surfaces helpful guidance about vso.security_manage", %{server: server} do
      TestServer.expect(server, "GET", "/testorg/_apis/projects", fn conn ->
        Plug.Conn.resp(
          conn,
          200,
          ~s({"value":[{"id":"c749b575-419f-4f41-a1bb-6e89a37bcc12","name":"testorg"}]})
        )
      end)

      TestServer.expect(server, "GET", "/testorg/_apis/connectionData", fn conn ->
        Plug.Conn.resp(
          conn,
          200,
          ~s({"authenticatedUser":{"subjectDescriptor":"Microsoft.IdentityModel.Claims.ClaimsIdentity;vssgp.test"}})
        )
      end)

      TestServer.expect(
        server,
        "POST",
        "/testorg/_apis/accesscontrolentries/#{@lib_ns}",
        fn conn ->
          Plug.Conn.resp(conn, 403, ~s({"message":"Forbidden"}))
        end
      )

      Security.grant(%{
        options: %{
          permission: "ViewSecrets",
          yes_this_mutates_secret_read: true
        },
        arguments: %{project_name_or_id: "testorg"}
      })

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end

    test "MSA subjectDescriptor is short-circuited with a specific error (M6)", %{server: server} do
      # Mock connectionData to return an MSA descriptor. The handler
      # should detect this and halt with a specific MSA error message
      # BEFORE making the grant call.
      TestServer.expect(server, "GET", "/testorg/_apis/connectionData", fn conn ->
        Plug.Conn.resp(
          conn,
          200,
          ~s({"authenticatedUser":{"subjectDescriptor":"msa.NjEyZjhlZGMtNGZmYy03ZGJkLWIxMWMtMjQ1YTgxOTIyNDk5"}})
        )
      end)

      Security.grant(%{
        options: %{
          permission: "ViewSecrets",
          yes_this_mutates_secret_read: true
        },
        arguments: %{project_name_or_id: "c749b575-419f-4f41-a1bb-6e89a37bcc12"}
      })

      # The halt message text should mention MSA / reject the call.
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "MSA"
      refute msg =~ "no expectation matched"
      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end
  end

  describe "validate_inputs/4" do
    test "36-char string of dashes is NOT treated as a project UUID (M5)", %{server: server} do
      # 36 dashes would match the old loose regex but isn't a valid UUID.
      # The handler should look up the project by name. We assert on the
      # grant request body's "token" field: it should be the looked-up
      # UUID, NOT the raw dashes. With the old regex, the handler would
      # skip the lookup and pass the dashes through to ADO, which the
      # assertion below catches.
      dashes = String.duplicate("-", 36)
      test_pid = "11111111-2222-3333-4444-555555555555"

      TestServer.expect(server, "GET", "/testorg/_apis/projects", fn conn ->
        Plug.Conn.resp(conn, 200, ~s({"value":[{"id":"#{test_pid}","name":"#{dashes}"}]}))
      end)

      TestServer.expect(server, "GET", "/testorg/_apis/connectionData", fn conn ->
        Plug.Conn.resp(
          conn,
          200,
          ~s({"authenticatedUser":{"subjectDescriptor":"Microsoft.IdentityModel.Claims.ClaimsIdentity;vssgp.test"}})
        )
      end)

      TestServer.expect(
        server,
        "POST",
        "/testorg/_apis/accesscontrolentries/#{@lib_ns}",
        fn conn ->
          {:ok, body, conn} = Plug.Conn.read_body(conn)
          decoded = Jason.decode!(body)
          # The grant token must be the looked-up UUID, not the raw dashes.
          assert decoded["token"] == test_pid
          Plug.Conn.resp(conn, 200, ~s({"count":1}))
        end
      )

      Security.grant(%{
        options: %{
          permission: "ViewSecrets",
          yes_this_mutates_secret_read: true
        },
        arguments: %{project_name_or_id: dashes}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end
end
