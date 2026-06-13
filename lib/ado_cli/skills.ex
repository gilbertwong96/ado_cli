defmodule AdoCli.Skills do
  @moduledoc """
  Compile-time embedded skill content for AI agents.

  Skills are YAML-frontmatter `.md` files stored under `priv/skills/`.
  They are embedded in the binary at build time so AI agents can fetch
  up-to-date instructions via `ado_cli skills read <name>`.

  ## Design

    * `ado_cli skills list` — list available skills
    * `ado_cli skills read <name>[/<path>]` — read SKILL.md or a file under the skill
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

             {description, version} =
               if File.exists?(skill_md_path) do
                 content = File.read!(skill_md_path)
                 fm = AdoCli.Frontmatter.parse(content)
                 {Map.get(fm, "description", ""), Map.get(fm, "version", "")}
               else
                 {"", ""}
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
  Returns skill info for listing (name, description, version).
  """
  @spec list_skills_info() :: [map()]
  def list_skills_info do
    skills_list =
      Enum.map(@skills, fn {name, info} ->
        %{
          name: name,
          description: Keyword.get(info, :description, ""),
          version: Keyword.get(info, :version, "")
        }
      end)

    Enum.sort_by(skills_list, & &1.name)
  end

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
        |> Map.keys()
        |> Enum.filter(&String.starts_with?(&1, prefix))
        |> Enum.uniq_by(fn path ->
          rest = String.trim_leading(path, prefix)
          hd(String.split(rest, "/"))
        end)
        |> Enum.map(fn path ->
          %{path: path, is_dir: not String.contains?(path, ".")}
        end)
        |> Enum.sort_by(& &1.path)

      {:ok, dir, entries}
    else
      {:error,
       "unknown skill #{inspect(skill)}. Run 'ado_cli skills list' to see available skills"}
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
      {:error,
       "unknown skill #{inspect(name)}. Run 'ado_cli skills list' to see available skills"}
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
      {:error,
       "unknown skill #{inspect(name)}. Run 'ado_cli skills list' to see available skills"}
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
