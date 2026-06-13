defmodule AdoCli.Frontmatter do
  @moduledoc """
  Parses YAML frontmatter from Markdown files.

  Pure-Elixir implementation supporting the simple `key: value` subset
  used in our skills. Does NOT depend on `yamerl` or any external
  YAML library — frontmatter is trivial enough that a 30-line parser
  beats pulling in a NIF-bound C library.

  ## Supported syntax

      ---
      description: Some text
      version: "0.2.0"
      any-key: any value
      ---

  ## Quoting

  - Single or double quotes are stripped from values: `key: "value"`
    becomes `key: value`.
  - Empty values parse as empty strings: `key:` becomes `key => ""`.
  - Lines without `:` are silently skipped.
  """

  @doc """
  Parse the frontmatter from the given content.

  Returns a map of `{key, value}` pairs, or `%{}` if the content
  doesn't start with `---\\n`.

  ## Examples

      iex> AdoCli.Frontmatter.parse("---\\nkey: value\\n---\\nbody")
      %{"key" => "value"}

      iex> AdoCli.Frontmatter.parse("description: No frontmatter\\n---\\nbody")
      %{"description" => "No frontmatter"}
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

  defp parse_lines(lines) do
    Enum.reduce(lines, %{}, fn line, acc ->
      case String.split(line, ":", parts: 2) do
        [k, v] ->
          value = strip_quotes(String.trim(v))
          Map.put(acc, String.trim(k), value)

        _ ->
          acc
      end
    end)
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
