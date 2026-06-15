defmodule AdoCli.CLI.SkillsTest do
  use ExUnit.Case, async: false

  alias AdoCli.CLI.Skills

  setup do
    # Switch to ProcessShell so halt_* doesn't exit
    CliMate.CLI.put_shell(CliMate.CLI.ProcessShell)
    on_exit(fn -> CliMate.CLI.put_shell(CliMate.CLI.DefaultShell) end)
    :ok
  end

  describe "list_skills/1" do
    test "lists all skills when no path given" do
      Skills.list_skills(%{options: %{json: true}, arguments: %{path: nil}})
      # Should either halt with 0 (success) or 1 (no skills found)
      assert_receive {:cli_mate_shell, :halt, _}, 1000
    end

    test "lists files under a specific skill" do
      Skills.list_skills(%{
        options: %{json: true},
        arguments: %{path: "ado_cli"}
      })

      assert_receive {:cli_mate_shell, :halt, _}, 1000
    end
  end

  describe "read_skill/1" do
    test "reads the SKILL.md of a skill" do
      Skills.read_skill(%{
        options: %{json: false},
        arguments: %{target: "ado_cli"}
      })

      assert_receive {:cli_mate_shell, :halt, _}, 1000
    end

    test "reads SKILL.md as JSON" do
      Skills.read_skill(%{
        options: %{json: true},
        arguments: %{target: "ado_cli"}
      })

      assert_receive {:cli_mate_shell, :halt, _}, 1000
    end
  end

  describe "install_skills/1" do
    setup do
      tmpdir =
        Path.join(System.tmp_dir!(), "ado_install_test_#{System.unique_integer([:positive])}")

      File.mkdir_p!(tmpdir)
      on_exit(fn -> File.rm_rf!(tmpdir) end)
      {:ok, tmpdir: tmpdir}
    end

    test "installs all skills to a custom target", %{tmpdir: tmpdir} do
      Skills.install_skills(%{
        options: %{target: tmpdir, force: false, json: true, skill: nil},
        arguments: %{}
      })

      assert_receive {:cli_mate_shell, :info, msg}, 1000
      assert_receive {:cli_mate_shell, :halt, 0}, 1000

      {:ok, decoded} = JSON.decode(msg)
      assert decoded["ok"] == true
      assert [_x, _y, _z] = decoded["result"]["installed"]
      assert decoded["result"]["skipped"] == []
      assert decoded["result"]["errors"] == []

      # Verify files were actually written
      for skill <- ~w(ado_cli ado_auth ado_ci) do
        path = Path.join([tmpdir, skill, "SKILL.md"])
        assert File.exists?(path), "Expected #{path} to exist"
        content = File.read!(path)
        assert String.starts_with?(content, "---\nname: #{skill}")
      end
    end

    test "skips existing files without --force", %{tmpdir: tmpdir} do
      # Pre-create one file
      skill_path = Path.join([tmpdir, "ado_cli", "SKILL.md"])
      File.mkdir_p!(Path.dirname(skill_path))
      File.write!(skill_path, "pre-existing content\n")

      Skills.install_skills(%{
        options: %{target: tmpdir, force: false, json: true, skill: nil},
        arguments: %{}
      })

      assert_receive {:cli_mate_shell, :info, msg}, 1000
      {:ok, decoded} = JSON.decode(msg)

      # The pre-existing file should be in skipped
      skipped_skills = Enum.map(decoded["result"]["skipped"], & &1["skill"])
      assert "ado_cli" in skipped_skills

      # The other two should have been installed
      installed_skills = Enum.map(decoded["result"]["installed"], & &1["skill"])
      assert "ado_auth" in installed_skills
      assert "ado_ci" in installed_skills

      # Verify pre-existing file was NOT overwritten
      assert File.read!(skill_path) == "pre-existing content\n"
    end

    test "overwrites existing files with --force", %{tmpdir: tmpdir} do
      skill_path = Path.join([tmpdir, "ado_cli", "SKILL.md"])
      File.mkdir_p!(Path.dirname(skill_path))
      File.write!(skill_path, "stale content\n")

      Skills.install_skills(%{
        options: %{target: tmpdir, force: true, json: true, skill: nil},
        arguments: %{}
      })

      assert_receive {:cli_mate_shell, :info, msg}, 1000
      {:ok, decoded} = JSON.decode(msg)

      # Nothing should be skipped when --force
      assert decoded["result"]["skipped"] == []
      installed_skills = Enum.map(decoded["result"]["installed"], & &1["skill"])
      assert "ado_cli" in installed_skills

      # Verify the file content was actually replaced (not just timestamp)
      content = File.read!(skill_path)
      refute content == "stale content\n"
      assert String.starts_with?(content, "---\nname: ado_cli")
    end

    test "installs only the specified skill when --skill is given", %{tmpdir: tmpdir} do
      Skills.install_skills(%{
        options: %{target: tmpdir, force: false, json: true, skill: "ado_ci"},
        arguments: %{}
      })

      assert_receive {:cli_mate_shell, :info, msg}, 1000
      {:ok, decoded} = JSON.decode(msg)

      assert [_x] = decoded["result"]["installed"]
      assert hd(decoded["result"]["installed"])["skill"] == "ado_ci"

      assert File.exists?(Path.join([tmpdir, "ado_ci", "SKILL.md"]))
      refute File.exists?(Path.join([tmpdir, "ado_cli", "SKILL.md"]))
      refute File.exists?(Path.join([tmpdir, "ado_auth", "SKILL.md"]))
    end

    test "expands ~ in custom target paths", %{tmpdir: _tmpdir} do
      home = Path.expand("~")
      home_subdir = Path.join([home, ".ado_test_skills_#{System.unique_integer([:positive])}"])
      on_exit(fn -> File.rm_rf!(home_subdir) end)

      Skills.install_skills(%{
        options: %{target: home_subdir, force: false, json: true, skill: "ado_cli"},
        arguments: %{}
      })

      assert_receive {:cli_mate_shell, :info, msg}, 1000
      {:ok, decoded} = JSON.decode(msg)
      assert [_x] = decoded["result"]["installed"]
      assert File.exists?(Path.join([home_subdir, "ado_cli", "SKILL.md"]))
    end

    test "JSON envelope has the expected structure", %{tmpdir: tmpdir} do
      Skills.install_skills(%{
        options: %{target: tmpdir, force: false, json: true, skill: nil},
        arguments: %{}
      })

      assert_receive {:cli_mate_shell, :info, msg}, 1000
      {:ok, decoded} = JSON.decode(msg)

      sorted_top = Enum.sort(Map.keys(decoded))
      assert sorted_top == ["ok", "result"]

      sorted_result = Enum.sort(Map.keys(decoded["result"]))
      assert sorted_result == ["errors", "installed", "skipped", "targets"]

      assert is_list(decoded["result"]["targets"])
      [target | _] = decoded["result"]["targets"]
      sorted_target = Enum.sort(Map.keys(target))
      assert sorted_target == ["name", "path"]
    end
  end
end
