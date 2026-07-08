defmodule AdoCli.Fuzzy do
  @moduledoc """
  Lightweight fuzzy matching with no external dependencies.

  Combines two strategies and ranks results by match quality:

    1. **Substring** (case-insensitive) — highest priority. A query of
       "john" matches "John Doe" directly.
    2. **Subsequence** (fzf-style) — query characters must appear in
       order, not necessarily adjacent. "jsmith" matches "John Smith".

  Each candidate is scored; only matches (score > 0) are returned.
  Substring matches rank higher than subsequence matches, and shorter
  candidates with tighter matches rank higher overall.
  """

  @doc """
  Filter and rank `candidates` (strings) by fuzzy match against `query`.

  Returns a list of `{candidate, score}` tuples sorted best-first.
  Only candidates with a positive score are included.

      iex> AdoCli.Fuzzy.match(["John Doe", "Jane Smith", "Bob Jones"], "john")
      [{"John Doe", _}]

      iex> AdoCli.Fuzzy.match(["John Smith", "Jane Smith"], "jsmith")
      [{"John Smith", _}]
  """
  @spec match([String.t()], String.t()) :: [{String.t(), number()}]
  def match(_candidates, query) when not is_binary(query) and not is_nil(query), do: []
  def match(_candidates, nil), do: []
  def match(_candidates, ""), do: []

  def match(candidates, query) do
    q = String.downcase(query)

    candidates
    |> Enum.map(&{&1, score(&1, q)})
    |> Enum.filter(fn {_, s} -> s > 0 end)
    |> Enum.sort_by(fn {_, s} -> -s end)
  end

  @doc """
  Filter a list of maps/structs by fuzzy-matching one or more fields.

  Returns the items whose any field matches, preserving original order.

      iex> items = [%{name: "John Doe", email: "john@x.com"}]
      iex> AdoCli.Fuzzy.match_fields(items, "john", [:name, :email])
      [%{name: "John Doe", email: "john@x.com"}]
  """
  @spec match_fields([map()], String.t(), [atom()]) :: [map()]
  def match_fields(items, query, fields) do
    case query do
      nil ->
        items

      "" ->
        items

      q when is_binary(q) ->
        down = String.downcase(q)
        Enum.filter(items, &field_matches?(&1, fields, down))

      _ ->
        items
    end
  end

  defp field_matches?(item, fields, query) do
    Enum.any?(fields, fn field ->
      case Map.get(item, field) do
        v when is_binary(v) -> score(v, query) > 0
        _ -> false
      end
    end)
  end

  # Higher score = better match. Substring > subsequence.
  defp score(candidate, q) do
    c = String.downcase(candidate)
    cl = String.length(c)
    ql = String.length(q)

    cond do
      c == q ->
        1000 + ql

      String.starts_with?(c, q) ->
        500 + ql - (cl - ql)

      String.contains?(c, q) ->
        # Substring found; earlier position = better.
        {actual_pos, _} = :binary.match(c, q)
        300 + ql - div(actual_pos, max(byte_size(q), 1))

      subsequence?(c, q) ->
        # Tighter subsequences (fewer gaps) score higher.
        gap = cl - ql
        max(100 - gap, 1)

      true ->
        0
    end
  end

  # True if every char in `query` appears in `candidate` in order.
  defp subsequence?(candidate, query) do
    do_subsequence?(String.to_charlist(candidate), String.to_charlist(query))
  end

  defp do_subsequence?(_candidate, []), do: true
  defp do_subsequence?([], _query), do: false

  defp do_subsequence?([h | ct], [h | qt]) do
    # char matches; advance both (greedy) OR try skipping candidate char
    do_subsequence?(ct, qt) or do_subsequence?(ct, [h | qt])
  end

  defp do_subsequence?([_ | ct], query) do
    do_subsequence?(ct, query)
  end
end
