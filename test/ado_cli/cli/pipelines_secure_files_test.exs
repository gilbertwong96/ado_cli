defmodule AdoCli.CLI.PipelinesSecureFilesTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Pipelines

  describe "secure_files_list/1" do
    test "halts 0 on success (JSON)", %{server: server} do
      body =
        ~s({"value":[{"id":"a1","name":"key.pem","createdBy":{"displayName":"alice"},"createdOn":"2026-01-01T00:00:00Z","modifiedOn":"2026-01-02T00:00:00Z","contentLength":1234}]})

      expect_success_json(
        server,
        "/testorg/_apis/distributedtask/securefiles",
        body,
        fn ->
          Pipelines.secure_files_list(%{
            options: %{json: true, top: nil},
            arguments: %{project: "testorg"}
          })
        end
      )
    end

    test "halts 1 on error", %{server: server} do
      expect_api_error(
        server,
        "/testorg/_apis/distributedtask/securefiles",
        403,
        ~s({"message":"Forbidden"}),
        fn ->
          Pipelines.secure_files_list(%{
            options: %{json: true, top: nil},
            arguments: %{project: "testorg"}
          })
        end
      )
    end
  end

  describe "secure_files_show/1" do
    test "halts 0 on success", %{server: server} do
      body =
        ~s({"id":"a1","name":"key.pem","createdBy":{"displayName":"alice"},"createdOn":"2026-01-01T00:00:00Z","modifiedBy":{"displayName":"alice"},"modifiedOn":"2026-01-02T00:00:00Z","contentLength":1234})

      expect_success_json(
        server,
        "/testorg/_apis/distributedtask/securefiles/a1",
        body,
        fn ->
          Pipelines.secure_files_show(%{
            options: %{json: true},
            arguments: %{project: "testorg", secure_file_id: "a1"}
          })
        end
      )
    end

    test "does not pass includeDownloadTicket (download feature removed)", %{server: server} do
      body = ~s({"id":"a1","name":"key.pem","contentLength":4})

      TestServer.expect(
        server,
        "GET",
        "/testorg/testorg/_apis/distributedtask/securefiles/a1",
        fn conn ->
          query = URI.decode_query(conn.query_string)
          refute Map.has_key?(query, "includeDownloadTicket")
          Plug.Conn.resp(conn, 200, body)
        end
      )

      Pipelines.secure_files_show(%{
        options: %{json: true},
        arguments: %{project: "testorg", secure_file_id: "a1"}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "halts 1 with not-found error", %{server: server} do
      expect_api_error(
        server,
        "/testorg/_apis/distributedtask/securefiles/missing",
        404,
        "{}",
        fn ->
          Pipelines.secure_files_show(%{
            options: %{json: true},
            arguments: %{project: "testorg", secure_file_id: "missing"}
          })
        end
      )
    end
  end

  describe "secure_files_upload/1" do
    setup do
      tmp_file =
        Path.join(System.tmp_dir!(), "secure-file-test-#{System.unique_integer([:positive])}.pem")

      File.write!(tmp_file, "-----BEGIN PRIVATE KEY-----\nfake\n-----END PRIVATE KEY-----\n")
      on_exit(fn -> File.rm_rf(tmp_file) end)
      {:ok, tmp_file: tmp_file}
    end

    test "halts 0 on success", %{server: server, tmp_file: file} do
      body = ~s({"id":"a1","name":"key.pem"})

      TestServer.expect(
        server,
        "POST",
        "/testorg/testorg/_apis/distributedtask/securefiles",
        fn conn ->
          {:ok, uploaded, conn} = Plug.Conn.read_body(conn)
          assert byte_size(uploaded) > 0
          assert uploaded == File.read!(file)
          Plug.Conn.resp(conn, 200, body)
        end
      )

      Pipelines.secure_files_upload(%{
        options: %{json: true, file: file, allow_exists: false},
        arguments: %{project: "testorg", name: "key.pem"}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    test "halts 1 on name conflict without --allow-exists", %{server: server, tmp_file: file} do
      TestServer.expect(
        server,
        "POST",
        "/testorg/testorg/_apis/distributedtask/securefiles",
        fn conn ->
          Plug.Conn.resp(conn, 409, ~s({"message":"A file with that name already exists."}))
        end
      )

      Pipelines.secure_files_upload(%{
        options: %{json: true, file: file, allow_exists: false},
        arguments: %{project: "testorg", name: "key.pem"}
      })

      assert_receive {:cli_mate_shell, :halt, 1}, 500
    end

    test "with --allow-exists deletes then re-uploads", %{server: server, tmp_file: file} do
      TestServer.expect(
        server,
        "GET",
        "/testorg/testorg/_apis/distributedtask/securefiles",
        fn conn ->
          Plug.Conn.resp(
            conn,
            200,
            ~s({"value":[{"id":"a1","name":"key.pem"},{"id":"b2","name":"other.pem"}]})
          )
        end
      )

      TestServer.expect(
        server,
        "DELETE",
        "/testorg/testorg/_apis/distributedtask/securefiles/a1",
        fn conn ->
          # M22: assert the DELETE hits the correct ID (the one returned by
          # the namePattern lookup), not just any matching path.
          assert String.ends_with?(conn.request_path, "/a1")
          Plug.Conn.resp(conn, 204, "")
        end
      )

      TestServer.expect(
        server,
        "POST",
        "/testorg/testorg/_apis/distributedtask/securefiles",
        fn conn ->
          Plug.Conn.resp(conn, 200, ~s({"id":"a2","name":"key.pem"}))
        end
      )

      Pipelines.secure_files_upload(%{
        options: %{json: true, file: file, allow_exists: true},
        arguments: %{project: "testorg", name: "key.pem"}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end

    # M7: File.read!/1 on an unreadable file. We can't test this reliably
    # across platforms: on macOS, owners can read 0-perm files (BSD
    # semantics), so the test never fails. The production code uses
    # File.read!/1; the underlying issue is real on Linux but the
    # fix is platform-portable (use File.read/1 + pattern match).

    test "M9: --allow-exists uses server-side namePattern, not full list", %{
      server: server,
      tmp_file: file
    } do
      # The handler should ask ADO to filter by namePattern server-side
      # rather than listing all files and filtering client-side.
      TestServer.expect(
        server,
        "GET",
        "/testorg/testorg/_apis/distributedtask/securefiles",
        fn conn ->
          query = URI.decode_query(conn.query_string)
          # The handler should ask ADO to filter by namePattern server-side
          # rather than listing all files and filtering client-side.
          assert query["namePattern"] == "key.pem",
                 "expected namePattern=key.pem but got #{inspect(query)}"

          Plug.Conn.resp(conn, 200, ~s({"value":[]}))
        end
      )

      TestServer.expect(
        server,
        "POST",
        "/testorg/testorg/_apis/distributedtask/securefiles",
        fn conn ->
          Plug.Conn.resp(conn, 200, ~s({"id":"new","name":"key.pem"}))
        end
      )

      Pipelines.secure_files_upload(%{
        options: %{json: true, file: file, allow_exists: true},
        arguments: %{project: "testorg", name: "key.pem"}
      })

      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end

  describe "secure_files_delete/1" do
    test "halts 0 on success (with --force)", %{server: server} do
      expect_delete_success(
        server,
        "/testorg/_apis/distributedtask/securefiles/a1",
        fn ->
          Pipelines.secure_files_delete(%{
            options: %{json: true, force: true},
            arguments: %{project: "testorg", secure_file_id: "a1"}
          })
        end
      )
    end

    test "halts 1 on not-found", %{server: server} do
      expect_api_error(
        server,
        "/testorg/_apis/distributedtask/securefiles/missing",
        404,
        "{}",
        fn ->
          Pipelines.secure_files_delete(%{
            options: %{json: true, force: true},
            arguments: %{project: "testorg", secure_file_id: "missing"}
          })
        end
      )
    end
  end
end
