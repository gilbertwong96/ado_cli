defmodule AdoCli.Skills.SearchResult do
  @moduledoc """
  A single search hit returned by `AdoCli.Skills.search/1`.
  """
  @derive {JSON.Encoder, only: [:skill, :match_type, :matched, :context]}
  defstruct [:skill, :match_type, :matched, :context]

  @type t :: %__MODULE__{
          skill: String.t(),
          match_type: String.t(),
          matched: String.t(),
          context: String.t()
        }
end

defmodule AdoCli.Skills do
  @moduledoc """
  Compile-time embedded skill content for AI agents.

  Skills are YAML-frontmatter `.md` files stored under `priv/skills/`.
  They are embedded in the binary at build time so AI agents can fetch
  up-to-date instructions via `ado skills read <name>`.

  ## Design

  The skills interface is designed for **LLM agent workflows**. Three
  access patterns are supported, ordered by payload size:

    1. `ado skills list`         — all skills (name + description)
    2. `ado skills describe NAME` — small payload (just frontmatter +
                                     command index). Use this to decide
                                     whether to load the full skill.
    3. `ado skills read NAME`     — full Markdown content (~500 tokens)
    4. `ado skills search QUERY`  — find skills whose name, description,
                                     or command index matches a query

  All commands support `--json` for machine parsing.

  ## Skill frontmatter

  Each `SKILL.md` starts with YAML frontmatter:

      ---
      description: One-line summary (shown in `ado skills list`)
      version: "0.2.0"
      commands:                 ← optional
        - ado projects list      ← command patterns this skill teaches
        - ado prs create         ← (used by `describe` and `search`)
      ---

  The `commands` field is a YAML block list. Each item is the canonical
  CLI invocation. Trailing `# comment` is preserved verbatim and is
  useful for LLM hints (e.g. "PROJECT here is a project name or ID").
  """

  # ═════════════════════════════════════════════════════════════════════
  # Embedded skills — loaded at compile time from priv/skills/
  # ═════════════════════════════════════════════════════════════════════

  @external_resource "priv/skills"

  skills_dir = Path.join(__DIR__, "../../priv/skills")

  @skills (for dir <- File.ls!(skills_dir),
               File.dir?(Path.join(skills_dir, dir)),
               into: %{} do
             skill_path = Path.join(skills_dir, dir)
             skill_md_path = Path.join(skill_path, "SKILL.md")

             {description, version, commands} =
               if File.exists?(skill_md_path) do
                 content = File.read!(skill_md_path)
                 fm = AdoCli.Frontmatter.parse(content)

                 {Map.get(fm, "description", ""), Map.get(fm, "version", ""),
                  AdoCli.Frontmatter.parse_commands(content)}
               else
                 {"", "", []}
               end

             files =
               skill_path
               |> then(&Path.wildcard(Path.join(&1, "**/*")))
               |> Enum.filter(&File.regular?/1)
               |> Enum.map(fn full_path ->
                 rel = Path.relative_to(full_path, skills_dir)
                 {rel, File.read!(full_path)}
               end)
               |> Map.new()

             {dir,
              [
                description: description,
                version: version,
                commands: commands,
                files: files
              ]}
           end)

  # ═════════════════════════════════════════════════════════════════════
  # Public API
  # ═════════════════════════════════════════════════════════════════════

  @doc """
  Returns a list of embedded skill names.
  """
  @spec list_skills() :: [String.t()]
  def list_skills do
    Enum.sort(Map.keys(@skills))
  end

  @doc """
  Returns a small summary for each skill: name, description, version,
  and command count. Used by `ado skills list`.
  """
  @spec list_skills_info() :: [map()]
  def list_skills_info do
    skills_list =
      Enum.map(@skills, fn {name, info} ->
        %{
          name: name,
          description: Keyword.get(info, :description, ""),
          version: Keyword.get(info, :version, ""),
          command_count: length(Keyword.get(info, :commands, []))
        }
      end)

    Enum.sort_by(skills_list, & &1.name)
  end

  @doc """
  Returns a "describe" view of a skill: frontmatter + command index,
  but NOT the full Markdown body. This is the small-payload call
  for "should I load this skill?".

  The shape is:

      %{
        "name" => "ado_cli",
        "description" => "...",
        "version" => "0.2.0",
        "commands" => ["ado projects list", "ado prs create", ...]
      }

  Returns `{:error, reason}` if the skill is unknown.
  """
  @spec describe(String.t()) :: {:ok, map()} | {:error, String.t()}
  def describe(name) do
    case Map.fetch(@skills, name) do
      {:ok, info} ->
        {:ok,
         %{
           name: name,
           description: Keyword.get(info, :description, ""),
           version: Keyword.get(info, :version, ""),
           commands: Keyword.get(info, :commands, [])
         }}

      :error ->
        {:error, "unknown skill #{inspect(name)}. Run 'ado skills list' to see available skills"}
    end
  end

  @doc """
  Searches all skills for `query` in their name, description, and
  command index. Case-insensitive substring match.

  Returns a list of matches, each shaped like:

      %{
        skill: "ado_cli",
        match_type: "command" | "description" | "name",
        matched: "ado prs create",  ← the specific text that matched
        context: "PR automation"   ← extra context (e.g. first line of the section)
      }

  Sorted by `match_type` (name > command > description) then by skill name.
  """
  @spec search(String.t()) :: [AdoCli.Skills.SearchResult.t()]
  def search(query) when is_binary(query) do
    needle = String.downcase(query)

    @skills
    |> Enum.flat_map(fn {skill_name, info} ->
      search_skill(needle, skill_name, info)
    end)
    |> Enum.sort_by(&{match_priority(&1.match_type), &1.skill})
  end

  defp search_skill(needle, skill_name, info) do
    matches = []

    matches =
      if String.contains?(String.downcase(skill_name), needle) do
        [
          %AdoCli.Skills.SearchResult{
            skill: skill_name,
            match_type: "name",
            matched: skill_name,
            context: ""
          }
          | matches
        ]
      else
        matches
      end

    matches =
      if String.contains?(String.downcase(Keyword.get(info, :description, "")), needle) do
        [
          %{
            skill: skill_name,
            match_type: "description",
            matched: Keyword.get(info, :description, ""),
            context: ""
          }
          | matches
        ]
      else
        matches
      end

    info
    |> Keyword.get(:commands, [])
    |> Enum.reduce(matches, fn cmd, acc ->
      if String.contains?(String.downcase(cmd), needle) do
        [
          %AdoCli.Skills.SearchResult{
            skill: skill_name,
            match_type: "command",
            matched: cmd,
            context: ""
          }
          | acc
        ]
      else
        acc
      end
    end)
  end

  defp match_priority("name"), do: 0
  defp match_priority("command"), do: 1
  defp match_priority("description"), do: 2
  defp match_priority(_), do: 3

  @doc """
  Lists files under a skill path (one level deep, like `ls`).
  """
  @spec list_path(String.t()) :: {:ok, String.t(), [map()]} | {:error, String.t()}
  def list_path(arg) do
    {skill, sub} = split_arg(arg)

    if Map.has_key?(@skills, skill) do
      files = Keyword.get(@skills[skill], :files, %{})
      dir = if sub == "", do: skill, else: "#{skill}/#{sub}"
      prefix = dir <> "/"

      entries =
        files
        |> Enum.filter(fn {path, _v} -> String.starts_with?(path, prefix) end)
        |> Enum.uniq_by(fn {path, _v} ->
          rest = String.trim_leading(path, prefix)
          hd(String.split(rest, "/", parts: 2))
        end)
        |> Enum.map(fn {path, _v} ->
          %{path: path, is_dir: not String.contains?(path, ".")}
        end)
        |> Enum.sort_by(& &1.path)

      {:ok, dir, entries}
    else
      {:error, "unknown skill #{inspect(skill)}. Run 'ado skills list' to see available skills"}
    end
  end

  @doc """
  Reads a skill's main SKILL.md file.
  """
  @spec read_skill(String.t()) :: {:ok, String.t()} | {:error, String.t()}
  def read_skill(name) do
    if Map.has_key?(@skills, name) do
      read_file(name, "SKILL.md")
    else
      {:error, "unknown skill #{inspect(name)}. Run 'ado skills list' to see available skills"}
    end
  end

  @doc """
  Reads a reference file under a skill.
  """
  @spec read_reference(String.t(), String.t()) ::
          {:ok, String.t(), String.t()} | {:error, String.t()}
  def read_reference(name, relpath) do
    if Map.has_key?(@skills, name) do
      files = Keyword.get(@skills[name], :files, %{})
      full = "#{name}/#{relpath}"

      case Map.get(files, full) do
        nil -> {:error, "file not found: #{full}"}
        content -> {:ok, content, relpath}
      end
    else
      {:error, "unknown skill #{inspect(name)}. Run 'ado skills list' to see available skills"}
    end
  end

  # ── Private ──────────────────────────────────────────────────────────

  defp read_file(skill, path) do
    files = Keyword.get(@skills[skill], :files, %{})
    full = "#{skill}/#{path}"

    case Map.get(files, full) do
      nil -> {:error, "file not found: #{full}"}
      content -> {:ok, content}
    end
  end

  defp split_arg(arg) do
    case String.split(arg, "/", parts: 2) do
      [name] -> {name, ""}
      [name, rest] -> {name, rest}
    end
  end
end
