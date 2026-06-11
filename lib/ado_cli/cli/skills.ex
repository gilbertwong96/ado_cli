defmodule AdoCli.CLI.Skills do
  @moduledoc """
  Embedded skill commands for AI agent integration.

      ado_cli skills list                   # all skills
      ado_cli skills list ado_api           # files under a skill
      ado_cli skills read ado_api           # read SKILL.md (markdown)
      ado_cli skills read ado_api --json    # read as JSON envelope
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.Skills

  @impl true
  def command do
    [
      name: "ado_cli skills",
      doc: "Read embedded skill content (list / read) for AI agents.",
      subcommands: [
        list: [
          name: "ado_cli skills list",
          doc: "List skills, or list files under a skill path (like ls).",
          arguments: [
            path: [type: :string, doc: "Optional: skill name or skill/path", required: false]
          ],
          execute: &list_skills/1
        ],
        read: [
          name: "ado_cli skills read",
          doc: "Read a skill's SKILL.md or a file under the skill.",
          arguments: [
            target: [type: :string, doc: "Skill name[/path] or 'skillname path'"]
          ],
          options: [
            json: [type: :boolean, doc: "Output as JSON envelope instead of raw markdown"]
          ],
          execute: &read_skill/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_skills(parsed) do
    path = get_in(parsed.arguments, [:path])

    if path do
      list_skill_files(path)
    else
      list_all_skills()
    end
  end

  defp list_all_skills do
    all_skills = Skills.list_skills_info()
    writeln("")

    if all_skills == [] do
      writeln("  No skills embedded in this build.")
    else
      Enum.each(all_skills, &print_skill_info/1)
    end

    writeln("")
    halt_success("Done.")
  end

  defp print_skill_info(skill) do
    writeln("  #{skill.name}")
    writeln("    #{skill.description}")
    writeln("    version: #{skill.version}")
    writeln("")
  end

  defp list_skill_files(path) do
    case Skills.list_path(path) do
      {:ok, dir, entries} ->
        writeln("")
        writeln("#{dir}/")

        Enum.each(entries, fn entry ->
          icon = if entry.is_dir, do: "  📁", else: "  📄"
          writeln("#{icon} #{Path.basename(entry.path)}")
        end)

        writeln("")

      {:error, reason} ->
        halt_error(reason)
    end

    halt_success("Done.")
  end

  def read_skill(parsed) do
    target = parsed.arguments.target
    as_json = Map.get(parsed.options, :json, false)
    {name, relpath} = split_skill_path(target)
    result = fetch_skill_content(name, relpath)
    output_skill_content(result, name, as_json)
  end

  defp split_skill_path(target) do
    case String.split(target, "/", parts: 2) do
      [n] -> {n, ""}
      [n, rest] -> {n, rest}
    end
  end

  defp fetch_skill_content(name, "") do
    case Skills.read_skill(name) do
      {:ok, content} -> {:ok, content, "SKILL.md"}
      error -> error
    end
  end

  defp fetch_skill_content(name, relpath) do
    Skills.read_reference(name, relpath)
  end

  defp output_skill_content({:ok, content, path}, name, true) do
    writeln(JSON.encode!(%{skill: name, path: path, content: content}))
    halt_success("Done.")
  end

  defp output_skill_content({:ok, content, _path}, _name, false) do
    writeln(content)
    halt_success("Done.")
  end

  defp output_skill_content({:error, reason}, _name, _json) do
    halt_error(reason)
  end
end
