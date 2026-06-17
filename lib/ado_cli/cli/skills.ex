defmodule AdoCli.CLI.Skills do
  @moduledoc """
  Embedded skill commands for AI agent integration.

      ado skills list                          # all skills (name + desc + command count)
      ado skills list ado-api                  # files under a skill (like ls)
      ado skills describe ado-api              # frontmatter + command index (no body)
      ado skills read ado-cli                  # full SKILL.md content (markdown)
      ado skills read ado-cli --json           # as JSON envelope, including commands
      ado skills search "create PR"           # find skills by name/desc/command
      ado skills search "pipeline" --json      # machine-parseable

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

  alias AdoCli.CLI.Output
  alias AdoCli.Skills

  @impl true
  def command do
    [
      name: "ado skills",
      doc:
        "Read embedded skill content for AI agents (pi, Claude Code, Cursor, Copilot). Commands: list all skills, describe one (frontmatter only), read full content, search by topic, install to agent directories.",
      subcommands: [
        list: [
          name: "ado skills list",
          doc:
            "List all embedded skills with name, description, version, and command count. Use --json for structured output suitable for agent discovery.",
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
            "Return the YAML frontmatter and command index for a skill (no body text). " <>
              "Use this to check version/description before loading the full content with read.",
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
            "Find skills by keyword search (name, description, or command list). " <>
              "Case-insensitive. Use for discovery when you do not know the exact skill name.",
          arguments: [
            query: [type: :string, doc: "Search query (e.g. 'create PR', 'pipeline', 'auth')"]
          ],
          options: [
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &search_skills/1
        ],
        install: [
          name: "ado skills install",
          doc:
            "Install the embedded skills to an LLM agent's skill directory " <>
              "(pi, Claude Code, Cursor, GitHub Copilot, or a custom path). " <>
              "Lets agents discover ado as a native skill on startup, " <>
              "instead of shelling out to `ado skills read`.",
          options: [
            target: [
              type: :string,
              default: "all",
              doc:
                "Where to install: 'pi' (~/.pi/agent/skills/), " <>
                  "'claude' (~/.claude/skills/), 'cursor' (~/.cursor/skills/), " <>
                  "'codex' (~/.codex/skills/), " <>
                  "'copilot' (per-repo, requires --repo or cwd to be a git repo; " <>
                  "writes to <repo>/.github/ado-cli/). " <>
                  "Default: 'all' (installs to every per-user target above; " <>
                  "copilot is NOT included because it needs a repo)."
            ],
            repo: [
              type: :string,
              doc:
                "Path to a local git repository. Used by --target=copilot " <>
                  "(writes to <repo>/.github/ado-cli/); default: current " <>
                  "working directory. Ignored for other targets."
            ],
            skill: [
              type: :string,
              doc: "Install only this skill (default: all embedded skills)"
            ],
            force: [
              type: :boolean,
              default: false,
              doc: "Overwrite existing files (default: skip them)"
            ],
            json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
          ],
          execute: &install_skills/1
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
      read_top_level_skill(name, json?)
    else
      read_skill_reference(name, rest, json?)
    end
  end

  defp read_top_level_skill(name, json?) do
    case Skills.read_skill(name) do
      {:ok, content} ->
        output_skill_content(name, "SKILL.md", strip_frontmatter(content), json?)
        halt_success("")

      {:error, reason} ->
        report_read_error(reason)
    end
  end

  defp read_skill_reference(name, rest, json?) do
    case Skills.read_reference(name, rest) do
      {:ok, content, relpath} ->
        output_skill_content(name, relpath, content, json?)
        halt_success("")

      {:error, reason} ->
        report_read_error(reason)
    end
  end

  defp output_skill_content(name, path, content, true = _json?) do
    info = describe_for_json(name)
    payload = build_skill_payload(name, path, content, info)
    writeln(JSON.encode!(payload))
  end

  defp output_skill_content(_name, _path, content, false = _json?) do
    writeln(content)
  end

  defp describe_for_json(name) do
    case Skills.describe(name) do
      {:ok, info} -> info
      _ -> %{}
    end
  end

  defp build_skill_payload(name, path, content, info) do
    base = %{ok: true, skill: name, path: path, content: content}

    if map_size(info) == 0 do
      base
    else
      Map.put(base, :metadata, Map.take(info, [:description, :version, :commands]))
    end
  end

  defp report_read_error(reason) do
    writeln("")
    writeln("xx  #{reason}")
    halt_error("")
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

  @doc """
  Install the embedded skills to one or more LLM agent skill directories.

  Targets:
    - "pi"     -> ~/.pi/agent/skills/
    - "claude" -> ~/.claude/skills/
    - "cursor" -> ~/.cursor/skills/
    - "/path"  -> any absolute path
    - "all"    -> install to every known target (default)

  Each skill lands as {target}/{skill_name}/SKILL.md. Any other files in
  the skill directory (e.g. reference files) are also copied.

  Returns a list of {target, skill, path, status} tuples where status
  is one of: :installed, :skipped (file already exists), :error.
  """
  def install_skills(parsed) do
    target_spec = Map.get(parsed.options, :target, "all")
    repo_spec = Map.get(parsed.options, :repo)
    skill_filter = Map.get(parsed.options, :skill)
    force? = Map.get(parsed.options, :force, false)
    json? = Map.get(parsed.options, :json, false)

    case resolve_target_dirs(target_spec, repo_spec, System.user_home!()) do
      {:error, reason} ->
        message = "could not resolve --target=#{target_spec}: #{reason}"

        if json? do
          Output.error(parsed, "validation_error", message)
        else
          raise_validation_error(message)
        end

      {:ok, target_dirs} ->
        skills_to_install = filter_skills(skill_filter)

        results =
          Enum.flat_map(target_dirs, fn {target_name, target_dir} ->
            Enum.map(skills_to_install, fn skill ->
              install_one_skill(target_name, target_dir, skill, force?)
            end)
          end)

        if json? do
          payload = build_install_payload(target_dirs, results)
          writeln(JSON.encode!(%{ok: true, result: payload}))
          halt_success("")
        else
          print_install_results(target_dirs, results)
          halt_success("")
        end
    end
  end

  defp raise_validation_error(message) do
    writeln("xx  #{message}")
    halt_error("")
  end

  defp filter_skills(nil), do: Skills.list_skills()
  defp filter_skills(name), do: [name]

  # Per-user agent skill directories. Adding a new agent is a one-line
  # change here + a help-doc update + a test.
  @per_user_targets %{
    "pi" => ".pi/agent/skills",
    "claude" => ".claude/skills",
    "cursor" => ".cursor/skills",
    "codex" => ".codex/skills"
  }

  # Resolve the target spec into a list of {name, expanded_path} pairs.
  # Returns {:ok, [{name, path}...]} on success, {:error, reason} on bad input.
  #
  # `home` is the user home directory (from System.user_home!/0, which
  # reads USERPROFILE on Windows and $HOME on Unix). It's a parameter
  # (not a hardcoded call) so tests can exercise this code without
  # touching the real filesystem.
  #
  #   "all"     -> all per-user targets (pi, claude, cursor, codex)
  #   <name>    -> one per-user target (must be a key in @per_user_targets)
  #   "copilot" -> <repo>/.github/ado-cli (requires --repo, defaults to cwd)
  #   "/path"   -> custom absolute path
  def resolve_target_dirs("all", _repo, home) do
    targets =
      Enum.map(@per_user_targets, fn {name, subdir} -> {name, Path.join(home, subdir)} end)

    {:ok, targets}
  end

  def resolve_target_dirs(name, _repo, home) when is_map_key(@per_user_targets, name) do
    subdir = Map.fetch!(@per_user_targets, name)
    {:ok, [{name, Path.join(home, subdir)}]}
  end

  def resolve_target_dirs("copilot", repo, _home) do
    repo_path = resolve_copilot_repo(repo)

    case repo_path do
      {:ok, path} ->
        # One subdir per skill, all under <repo>/.github/ado-cli/
        # (matches the per-skill pattern of pi/claude/cursor).
        target = Path.join([path, ".github", "ado-cli"])
        {:ok, [{"copilot", target}]}

      {:error, _} = err ->
        err
    end
  end

  def resolve_target_dirs(path, _repo, _home) do
    {:ok, [{"custom", Path.expand(path)}]}
  end

  # GitHub Copilot is per-repository. The repo path defaults to cwd
  # so users can `cd` into the repo and run `ado skills install
  # --target copilot` without a flag.
  defp resolve_copilot_repo(nil) do
    cwd = File.cwd!()
    {:ok, cwd}
  end

  defp resolve_copilot_repo(repo) do
    expanded = Path.expand(repo)

    if File.dir?(expanded) do
      {:ok, expanded}
    else
      {:error, "--repo=#{repo} does not exist or is not a directory"}
    end
  end

  # Try to install one skill into one target. Returns a result tuple.
  defp install_one_skill(target_name, target_dir, skill_name, force?) do
    skill_dir = Path.join(target_dir, skill_name)
    file_path = Path.join(skill_dir, "SKILL.md")

    cond do
      File.exists?(file_path) and not force? ->
        {:skipped, target_name, skill_name, file_path,
         "already exists (use --force to overwrite)"}

      not File.dir?(skill_dir) ->
        case File.mkdir_p(skill_dir) do
          :ok -> write_skill_files(target_name, skill_dir, skill_name, file_path)
          {:error, reason} -> {:error, target_name, skill_name, file_path, reason}
        end

      true ->
        write_skill_files(target_name, skill_dir, skill_name, file_path)
    end
  end

  defp write_skill_files(target_name, _skill_dir, skill_name, file_path) do
    case Skills.read_skill(skill_name) do
      {:ok, content} ->
        case File.write(file_path, content) do
          :ok -> {:installed, target_name, skill_name, file_path, "ok"}
          {:error, reason} -> {:error, target_name, skill_name, file_path, reason}
        end

      {:error, reason} ->
        {:error, target_name, skill_name, file_path, "skill not embedded: #{reason}"}
    end
  end

  # Group results by status for the human-readable summary.
  defp build_install_payload(target_dirs, results) do
    %{
      targets: Enum.map(target_dirs, fn {name, path} -> %{name: name, path: path} end),
      installed: format_results(results, :installed),
      skipped: format_results(results, :skipped),
      errors: format_results(results, :error)
    }
  end

  defp format_results(results, status) do
    results
    |> Enum.filter(fn {s, _, _, _, _} -> s == status end)
    |> Enum.map(fn {_, target, skill, path, _msg} ->
      %{target: target, skill: skill, path: path}
    end)
  end

  defp print_install_results(target_dirs, results) do
    writeln("")

    writeln(
      "  Installing #{length(Skills.list_skills())} skills to #{length(target_dirs)} target(s):"
    )

    Enum.each(target_dirs, fn {name, path} ->
      writeln("    - #{name}: #{path}")
    end)

    # Copilot is intentionally not in --target=all because it
    # installs per-repository (to <repo>/.github/ado-cli/), not
    # per-user. When --target=all is the default and the user
    # didn't explicitly ask for copilot, print a one-liner
    # telling them how to install to it.
    if Enum.all?(target_dirs, fn {name, _} -> name != "copilot" end) do
      writeln("")
      writeln("  Note: copilot installs per-repo (to <repo>/.github/ado-cli/).")
      writeln("        Run from inside your repo: ado skills install --target copilot")
    end

    writeln("")

    summarize_install_results(results)
    writeln("")
  end

  defp summarize_install_results(results) do
    counts = Enum.frequencies_by(results, fn {s, _, _, _, _} -> s end)

    installed = Map.get(counts, :installed, 0)
    skipped = Map.get(counts, :skipped, 0)
    errors = Map.get(counts, :error, 0)

    writeln("  Installed: #{installed}")

    writeln(
      "  Skipped:   #{skipped}#{if skipped > 0, do: " (use --force to overwrite)", else: ""}"
    )

    writeln("  Errors:    #{errors}")

    Enum.each(results, fn
      {:error, target, skill, path, msg} ->
        writeln("")
        writeln("    xx  #{target}/#{skill}: #{msg}")
        writeln("        #{path}")

      _ ->
        :ok
    end)
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
        render_skill_dir(dir, entries, json?)
        halt_success("")

      {:error, reason} ->
        writeln("")
        writeln("xx  #{reason}")
        halt_error("")
    end
  end

  defp render_skill_dir(dir, entries, true = _json?) do
    writeln(JSON.encode!(%{ok: true, dir: dir, entries: entries}))
  end

  defp render_skill_dir(dir, entries, false = _json?) do
    writeln("")
    writeln("  #{dir}/")

    Enum.each(entries, fn entry ->
      suffix = if entry.is_dir, do: "/", else: ""
      writeln("    #{entry.path}#{suffix}")
    end)

    writeln("")
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

      grouped = Enum.group_by(results, & &1.skill)
      Enum.each(grouped, &print_skill_group/1)
    end

    writeln("")
  end

  defp print_skill_group({skill, hits}) do
    writeln("  #{skill}")

    Enum.each(hits, fn hit ->
      writeln("    [#{hit.match_type}] #{hit.matched}")
    end)

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
