defmodule AdoCli.CLI.Skills do
  @moduledoc """
  Embedded skill commands for AI agent integration.

      ado_cli skills list                       # all skills (name + desc + command count)
      ado_cli skills list ado_api               # files under a skill (like ls)
      ado_cli skills describe ado_api           # frontmatter + command index (no body)
      ado_cli skills read ado_api               # full SKILL.md content (markdown)
      ado_cli skills read ado_api --json        # as JSON envelope, including commands
      ado_cli skills search "create PR"        # find skills by name/desc/command
      ado_cli skills search "pipeline" --json   # machine-parseable

  ## Recommended LLM workflow

  The skills interface is designed for the typical LLM agent loop.
  For best results, follow this order:

    1. `ado skills list` — see what's available (small payload)
    2. `ado skills search "<query>"` — find a relevant skill by topic
    3. `ado skills describe <name>` — confirm it teaches the commands
                                       you need (small payload, no body)
    4. `ado skills read <name> [--json]` — load the full content
    5. Construct and run the commands, parsing `--json` output

  All subcommands support `--json` for machine parsing.
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.Skills

  @impl true
  def command do
    [
      name: "ado skills",
      doc: "Read embedded skill content (list / describe / read / search) for AI agents.",
      subcommands: [
        list: [
          name: "ado skills list",
          doc: "List all skills with name + description + version + command count.",
          arguments: [
            path: [type: :string, doc: "Optional: skill name or skill/path", required: false]
          ],
          options: [
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &list_skills/1
        ],
        describe: [
          name: "ado skills describe",
          doc:
            "Return the frontmatter + command index for a skill (no body). " <>
              "Use this to decide whether to load the full content with `read`.",
          arguments: [
            name: [type: :string, doc: "Skill name"]
          ],
          options: [
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &describe_skill/1
        ],
        read: [
          name: "ado skills read",
          doc:
            "Read a skill's SKILL.md (or a file under the skill). " <>
              "Returns the full Markdown body for human or LLM consumption.",
          arguments: [
            target: [type: :string, doc: "Skill name[/path] or 'skillname path'"]
          ],
          options: [
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &read_skill/1
        ],
        search: [
          name: "ado skills search",
          doc:
            "Find skills whose name, description, or command index matches a query. " <>
              "Case-insensitive substring search.",
          arguments: [
            query: [type: :string, doc: "Search query (e.g. 'create PR', 'pipeline', 'auth')"]
          ],
          options: [
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &search_skills/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_skills(parsed) do
    path = get_in(parsed.arguments, [:path])
    json? = Map.get(parsed.options, :json, false)

    if path do
      list_skill_files(path, json?)
    else
      list_all_skills(json?)
    end
  end

  def describe_skill(parsed) do
    name = get_in(parsed.arguments, [:name])
    json? = Map.get(parsed.options, :json, false)

    case Skills.describe(name) do
      {:ok, info} ->
        if json? do
          writeln(JSON.encode!(%{ok: true, result: info}))
        else
          print_describe(info)
        end

        halt_success("")

      {:error, reason} ->
        writeln("")
        writeln("xx  #{reason}")
        halt_error("")
    end
  end

  def read_skill(parsed) do
    target = get_in(parsed.arguments, [:target])
    json? = Map.get(parsed.options, :json, false)
    [name, rest] = split_target(target)

    if rest == "" do
      # Top-level skill read
      case Skills.read_skill(name) do
        {:ok, content} ->
          if json? do
            case Skills.describe(name) do
              {:ok, info} ->
                writeln(
                  JSON.encode!(%{
                    ok: true,
                    skill: name,
                    path: "SKILL.md",
                    content: strip_frontmatter(content),
                    metadata: Map.take(info, [:description, :version, :commands])
                  })
                )
            end
          else
            writeln(content)
          end

          halt_success("")

        {:error, reason} ->
          writeln("")
          writeln("xx  #{reason}")
          halt_error("")
      end
    else
      # Reference file read
      case Skills.read_reference(name, rest) do
        {:ok, content, relpath} ->
          if json? do
            writeln(JSON.encode!(%{ok: true, skill: name, path: relpath, content: content}))
          else
            writeln(content)
          end

          halt_success("")

        {:error, reason} ->
          writeln("")
          writeln("xx  #{reason}")
          halt_error("")
      end
    end
  end

  def search_skills(parsed) do
    query = get_in(parsed.arguments, [:query])
    json? = Map.get(parsed.options, :json, false)
    results = Skills.search(query)

    if json? do
      writeln(JSON.encode!(%{ok: true, query: query, count: length(results), results: results}))
      halt_success("")
    else
      print_search_results(query, results)
      halt_success("")
    end
  end

  # ── printing (human-readable) ────────────────────────────────────────

  defp list_all_skills(json?) do
    all_skills = Skills.list_skills_info()

    if json? do
      writeln(JSON.encode!(%{ok: true, count: length(all_skills), skills: all_skills}))
      halt_success("")
    else
      writeln("")

      if all_skills == [] do
        writeln("  No skills embedded in this build.")
      else
        Enum.each(all_skills, &print_skill_info/1)
      end

      writeln("")
      halt_success("")
    end
  end

  defp print_skill_info(skill) do
    writeln("  #{skill.name}")
    writeln("    #{skill.description}")
    writeln("    version: #{skill.version}  ·  commands: #{skill.command_count}")

    if skill.command_count > 0 do
      writeln("    run: ado skills describe #{skill.name}     # see commands")
    end

    writeln("")
  end

  defp list_skill_files(path, json?) do
    case Skills.list_path(path) do
      {:ok, dir, entries} ->
        if json? do
          writeln(JSON.encode!(%{ok: true, dir: dir, entries: entries}))
          halt_success("")
        else
          writeln("")
          writeln("  #{dir}/")

          Enum.each(entries, fn entry ->
            suffix = if entry.is_dir, do: "/", else: ""
            writeln("    #{entry.path}#{suffix}")
          end)

          writeln("")
          halt_success("")
        end

      {:error, reason} ->
        writeln("")
        writeln("xx  #{reason}")
        halt_error("")
    end
  end

  defp print_describe(info) do
    writeln("")
    writeln("  #{info.name}")
    writeln("    #{info.description}")
    writeln("    version: #{info.version}")
    writeln("    commands: #{length(info.commands)}")

    if info.commands != [] do
      writeln("")
      writeln("    Commands covered by this skill:")
      Enum.each(info.commands, fn cmd -> writeln("      • #{cmd}") end)
    end

    writeln("")
    writeln("    Run `ado skills read #{info.name}` to load the full body.")
    writeln("")
  end

  defp print_search_results(query, results) do
    writeln("")

    if results == [] do
      writeln("  No matches for #{inspect(query)}.")
      writeln("  Try `ado skills list` to see all available skills.")
    else
      writeln("  Matches for #{inspect(query)} (#{length(results)}):")
      writeln("")

      Enum.group_by(results, & &1.skill)
      |> Enum.each(fn {skill, hits} ->
        writeln("  #{skill}")

        Enum.each(hits, fn hit ->
          writeln("    [#{hit.match_type}] #{hit.matched}")
        end)

        writeln("")
      end)
    end

    writeln("")
  end

  # ── helpers ──────────────────────────────────────────────────────────

  defp split_target(target) do
    case String.split(target, "/", parts: 2) do
      [name] -> [name, ""]
      [name, rest] -> [name, rest]
    end
  end

  # Strip the YAML frontmatter from the SKILL.md content so the LLM
  # doesn't see the same metadata twice (it's already in the JSON
  # envelope under "metadata").
  defp strip_frontmatter(content) do
    case String.split(content, "\n") do
      ["---" | rest] ->
        {_, body} = Enum.split_while(rest, &(&1 != "---"))

        body
        |> Enum.drop_while(&(&1 == "---" || &1 == ""))
        |> Enum.join("\n")
        |> String.trim()

      _ ->
        content
    end
  end
end
