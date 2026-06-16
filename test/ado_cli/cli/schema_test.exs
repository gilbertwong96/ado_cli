defmodule AdoCli.CLI.SchemaTest do
  use ExUnit.Case, async: false
  import ExUnit.CaptureIO

  alias AdoCli.CLI.Schema

  describe "build_tree/0" do
    test "returns a map with the top-level command metadata" do
      tree = Schema.build_tree()
      assert tree.name =~ "ado"
      assert is_binary(tree.doc)
      assert is_list(tree.options)
      assert is_list(tree.subcommands)
    end

    test "includes the global --org, --pat, --server options" do
      tree = Schema.build_tree()
      opt_names = Enum.map(tree.options, & &1.name)
      assert "org" in opt_names
      assert "pat" in opt_names
      assert "server" in opt_names
      assert "json" in opt_names
    end

    test "includes all 24+ service area subcommands" do
      tree = Schema.build_tree()
      sub_names = Enum.map(tree.subcommands, & &1.name)

      # Each subcommand name has the "ado " prefix.
      for name <- ~w(login logout whoami ci projects repos workitems pipelines
                       prs releases iterations areas wikis teams users extensions
                       agent-pools connections security banners packages skills schema) do
        full_name = "ado #{name}"

        assert full_name in sub_names,
               "expected subcommand '#{full_name}' in #{inspect(sub_names)}"
      end
    end

    test "subcommand entries have all required fields" do
      tree = Schema.build_tree()
      [sample | _] = tree.subcommands

      for field <- [:name, :doc, :arguments, :options, :subcommands] do
        assert Map.has_key?(sample, field), "missing field #{field} in subcommand"
      end
    end

    test "options have name, type, and doc fields" do
      tree = Schema.build_tree()
      [sample | _] = tree.subcommands
      [opt | _] = sample.options
      assert is_binary(opt.name)
      assert is_binary(opt.type)
      assert is_boolean(opt.required)
    end
  end

  describe "build_tree/1 (single subcommand)" do
    test "returns the requested subcommand when name matches" do
      node = Schema.build_tree("projects")
      assert node.name == "ado projects"
      assert is_list(node.subcommands)
      sub_names = Enum.map(node.subcommands, & &1.name)
      # Nested subcommands have full path names like "ado projects list"
      assert "ado projects list" in sub_names
    end

    test "follows nested subcommands to find a leaf" do
      node = Schema.build_tree("pipelines list")
      # pipelines list doesn't have its own subcommands
      assert node.name == "ado pipelines list"
      assert node.subcommands == []
    end

    test "returns an error envelope for an unknown subcommand" do
      result = Schema.build_tree("nope")
      assert Map.has_key?(result, :error)
      assert result.error.code == "not_found"
      assert result.error.message =~ "nope"
    end
  end

  describe "schema command run/1 (--json)" do
    setup do
      CliMate.CLI.put_shell(CliMate.CLI.ProcessShell)
      on_exit(fn -> CliMate.CLI.put_shell(CliMate.CLI.DefaultShell) end)
      :ok
    end

    test "outputs the full schema as a JSON envelope with --json" do
      json =
        capture_io(fn ->
          Schema.run(%{options: %{json: true}, arguments: %{name: nil}})
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["ok"] == true
      assert is_map(decoded["schema"])
      assert decoded["schema"]["name"] =~ "ado"
      assert is_list(decoded["schema"]["subcommands"])
    end

    test "dumps a specific subcommand when name is given" do
      json =
        capture_io(fn ->
          Schema.run(%{options: %{json: true}, arguments: %{name: "projects"}})
        end)

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      assert decoded["schema"]["name"] == "ado projects"
    end

    test "emits an error envelope when the requested subcommand is unknown" do
      json =
        capture_io(fn ->
          Schema.run(%{options: %{json: true}, arguments: %{name: "nope"}})
        end)

      assert {:ok, decoded} = JSON.decode(String.trim(json))
      # The schema is a not_found error envelope, not the schema tree
      assert decoded["schema"]["error"]["code"] == "not_found"
    end
  end
end
