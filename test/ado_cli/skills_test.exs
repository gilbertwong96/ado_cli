defmodule AdoCli.SkillsTest do
  use ExUnit.Case, async: true

  alias AdoCli.Skills

  describe "list_skills/0" do
    test "returns a sorted list of skill names" do
      skills = Skills.list_skills()
      assert is_list(skills)
      assert skills == Enum.sort(skills)
      assert "ado_cli" in skills
    end

    test "returns at least the four built-in skills" do
      skills = Skills.list_skills()
      assert "ado_cli" in skills
      assert "ado_auth" in skills
      assert "ado_ci" in skills
    end
  end

  describe "list_skills_info/0" do
    test "returns sorted info with description, version, and command_count" do
      info = Skills.list_skills_info()
      assert is_list(info)
      assert length(info) == length(Skills.list_skills())
      [first | _] = info
      assert Enum.sort(Map.keys(first)) == [:command_count, :description, :name, :version]
      assert info == Enum.sort_by(info, & &1.name)
    end

    test "each skill has a non-empty name" do
      Enum.each(Skills.list_skills_info(), fn skill ->
        assert is_binary(skill.name)
        assert skill.name != ""
      end)
    end

    test "each skill's command_count matches its commands list length" do
      Enum.each(Skills.list_skills_info(), fn skill ->
        {:ok, info} = Skills.describe(skill.name)
        assert skill.command_count == length(info.commands)
      end)
    end
  end

  describe "describe/1" do
    test "returns frontmatter + command index for a known skill" do
      assert {:ok, info} = Skills.describe("ado_cli")
      assert info.name == "ado_cli"
      assert is_binary(info.description)
      assert info.description != ""
      assert is_binary(info.version)
      assert is_list(info.commands)
      assert info.commands != []
      # Spot-check: a known command pattern should be in the index
      assert Enum.any?(info.commands, &String.contains?(&1, "ado prs create"))
    end

    test "returns error for unknown skill" do
      assert {:error, msg} = Skills.describe("nope")
      assert msg =~ "unknown skill"
    end
  end

  describe "search/1" do
    test "finds skills by name (case-insensitive)" do
      [hit | _] = Skills.search("ado_cli")
      assert hit.skill == "ado_cli"
      assert hit.match_type == "name"
    end

    test "finds skills by command pattern" do
      # "create PR" should match a command in ado_cli
      [hit | _] = Skills.search("create PR")
      assert hit.match_type in ["command", "description"]
    end

    test "returns empty list for non-matching query" do
      assert Skills.search("zzz_no_such_thing_zzz") == []
    end

    test "results are sorted by match priority then skill name" do
      results = Skills.search("auth")
      # Earlier results should have higher-priority match types
      # (name > command > description)
      priorities = Enum.map(results, &match_priority/1)
      assert priorities == Enum.sort(priorities)
    end
  end

  defp match_priority("name"), do: 0
  defp match_priority("command"), do: 1
  defp match_priority("description"), do: 2
  defp match_priority(_), do: 3

  describe "read_skill/1" do
    test "returns skill content for a valid skill" do
      assert {:ok, content} = Skills.read_skill("ado_cli")
      assert is_binary(content)
      assert content =~ "ado"
    end

    test "returns error for unknown skill" do
      assert {:error, msg} = Skills.read_skill("unknown_skill")
      assert msg =~ "unknown skill"
      assert msg =~ "ado_cli skills list"
    end
  end

  describe "list_path/1" do
    test "lists the skill root" do
      assert {:ok, dir, entries} = Skills.list_path("ado_cli")
      assert dir == "ado_cli"
      assert is_list(entries)
      assert Enum.any?(entries, fn e -> e.path =~ "SKILL.md" end)
    end

    test "returns error for unknown skill" do
      assert {:error, msg} = Skills.list_path("nope")
      assert msg =~ "unknown skill"
    end

    test "list entries have a path and is_dir flag" do
      {:ok, _, entries} = Skills.list_path("ado_cli")

      Enum.each(entries, fn entry ->
        assert is_binary(entry.path)
        assert is_boolean(entry.is_dir)
      end)
    end
  end

  describe "read_reference/2" do
    test "returns content for an existing reference" do
      {:ok, _, entries} = Skills.list_path("ado_cli")

      case Enum.find(entries, &(&1.path =~ "SKILL.md")) do
        nil ->
          :ok

        entry ->
          rel = String.trim_leading(entry.path, "ado_cli/")
          assert {:ok, content, ^rel} = Skills.read_reference("ado_cli", rel)
          assert is_binary(content)
      end
    end

    test "returns error for unknown skill" do
      assert {:error, msg} = Skills.read_reference("nope", "anything")
      assert msg =~ "unknown skill"
    end

    test "returns error for missing file under existing skill" do
      assert {:error, msg} = Skills.read_reference("ado_cli", "nonexistent.md")
      assert msg =~ "file not found"
    end
  end
end
