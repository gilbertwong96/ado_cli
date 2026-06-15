defmodule AdoCli.Frontmatter do
  @moduledoc """
  Parses frontmatter from Markdown files used by `ado skills`.

  The frontmatter is intentionally simple — a small subset of YAML that
  covers what an LLM agent needs to decide whether to load a skill and
  how to construct commands. Pure-Elixir implementation; no external
  YAML library.

  ## Supported syntax

      ---
      description: Short summary shown in `ado skills list`
      version: "0.2.0"
      commands:                ← optional: a YAML block list of CLI
        - ado projects list     patterns this skill teaches the agent.
        - ado prs create        Used by `ado skills describe` and
        - ado pipelines run     `ado skills search`.
      ---

  ## Quoting

    * Single or double quotes are stripped from values.
    * Empty values parse as empty strings: `key:` becomes `key => ""`.
    * Lines without `:` are silently skipped.

  ## The `commands` field

  When `commands:` is followed by one or more indented lines starting
  with `  - `, each `- foo` is collected into a list under `:commands`.
  The lines can be plain `ado ...` invocations or include a description
  after `#`. Both forms are kept as-is in the parsed output (the LLM
  decides how to interpret the trailing comment).

  Example:

      commands:
        - ado projects list ORG             # list projects
        - ado prs create PROJECT REPO --title TEXT --source BRANCH

  Parses to `%{..., "commands" => ["ado projects list ORG  # list projects",
  "ado prs create PROJECT REPO --title TEXT --source BRANCH"]}`.
  """

  @doc """
  Parse the frontmatter from the given content.

  Returns a map of `{key, value}` pairs, where the value is always a
  string. For the `commands` key, the value is a newline-joined string
  of all list items (since the underlying value is plain Markdown,
  not a structured list — the LLM can parse it as needed).

  Use `parse/1` for the raw map. For the `commands` field specifically,
  the consumer (the `Skills` module) splits it back into a list.

  Returns `%{}` if the content doesn't start with `---\\n`.
  """
  @spec parse(String.t()) :: %{optional(String.t()) => String.t()}
  def parse(content) do
    case String.split(content, "\n") do
      ["---" | rest] ->
        {fm_lines, _} = Enum.split_while(rest, &(&1 != "---"))
        parse_lines(fm_lines)

      _ ->
        %{}
    end
  end

  @doc """
  Parse and return just the `commands` field as a list of strings.

  Strips the leading `  - ` from each item and trims whitespace. Empty
  lines and lines that don't start with `- ` are skipped. Returns `[]`
  if the frontmatter has no `commands` field or no content.

  ## Example

      iex> fm = "description: x\\ncommands:\\n  - foo\\n  - bar\\n---\\nbody"
      iex> AdoCli.Frontmatter.parse_commands(fm)
      ["foo", "bar"]
  """
  @spec parse_commands(String.t()) :: [String.t()]
  def parse_commands(content) do
    case parse(content) do
      %{"commands" => raw} when is_binary(raw) and raw != "" ->
        raw
        |> String.split("\n", trim: true)
        |> Enum.map(&strip_command_prefix/1)
        |> Enum.reject(&(&1 == ""))

      _ ->
        []
    end
  end

  defp strip_command_prefix(line) do
    cond do
      String.starts_with?(line, "  - ") -> line |> String.trim_leading("  - ") |> String.trim()
      String.starts_with?(line, "- ") -> line |> String.trim_leading("- ") |> String.trim()
      true -> ""
    end
  end

  # ── private ──────────────────────────────────────────────────────────

  defp parse_lines(lines) do
    # Two-pass parse:
    #
    #   Pass 1 — fold the lines into a map. For a `commands:` key (or any
    #            other key that the consumer knows is multi-line), the
    #            value is the raw indented block joined with newlines.
    #   Pass 2 — strip the `  - ` prefix from each line so the LLM
    #            gets clean command strings.
    #
    # The map is keyed by the lowercased frontmatter key (matching
    # what the previous parser did).
    merged =
      Enum.reduce(lines, %{}, fn line, acc ->
        case String.split(line, ":", parts: 2) do
          [k, v] ->
            key = String.trim(k)
            value = collect_value(line, v, lines, acc)
            Map.put_new(acc, key, value)

          _ ->
            # Continuation line of a previous multi-line value. If
            # we have a `commands:` key already in the acc, append
            # this line to its value.
            case Map.fetch(acc, "commands") do
              {:ok, _} ->
                Map.update!(acc, "commands", &(&1 <> "\n" <> line))

              :error ->
                acc
            end
        end
      end)

    Map.new(merged)
  end

  # When we see `commands:`, the value we just read is empty (because
  # the list is on the following indented lines). The reduce loop
  # will append the continuation lines via the `_ ->` branch above.
  # For other keys, the value is whatever came after the colon on
  # the same line.
  defp collect_value(_line, v, _lines, _acc) do
    stripped = String.trim(v)

    case stripped do
      "" -> ""
      _ -> strip_quotes(stripped)
    end
  end

  defp strip_quotes(""), do: ""

  defp strip_quotes("\"" <> rest) do
    case String.reverse(rest) do
      "\"" <> inner -> inner |> String.reverse() |> strip_quotes()
      _ -> rest
    end
  end

  defp strip_quotes("'" <> rest) do
    case String.reverse(rest) do
      "'" <> inner -> inner |> String.reverse() |> strip_quotes()
      _ -> rest
    end
  end

  defp strip_quotes(other), do: other
end
