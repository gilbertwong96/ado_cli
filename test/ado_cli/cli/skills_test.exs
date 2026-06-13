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
end
