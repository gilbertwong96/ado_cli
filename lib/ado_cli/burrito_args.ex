defmodule AdoCli.BurritoArgs do
  @moduledoc """
  Parser for the `ADO_ARGS` env var set by the Burrito Zig wrapper.

  In `MIX_ENV=prod` cross-compiled releases, Burrito joins all CLI args
  with single spaces into the `ADO_ARGS` env var (see
  `deps/burrito/src/erlang_launcher.zig`). Naive `String.split/2` breaks
  quoted args containing spaces (e.g. project names like
  `"Employee Management"`), so this module parses them with POSIX-style
  quote/escape handling.

  Upstream `Burrito.Util.Args` does the same thing, but Burrito is
  `runtime: false` in our mix.exs, so its modules are not bundled with
  the release. We vendor a copy here.

  ## When this is used

  `AdoCli.Application.start/2` calls `get_arguments/0` first. If
  `:init.get_plain_arguments/0` is empty (the Burrito case), it falls
  back to parsing `ADO_ARGS`. In escript mode `System.argv()` already
  has the args, so this path is skipped.

  ## Limitations

  Burrito's env-var join is itself lossy — it doesn't escape quotes
  when joining. The parser handles correctly-quoted segments but can't
  reconstruct original quoting from the broken join when args contain
  quotes themselves. In practice the CLI uses quoted args only for
  project/repo names (no internal quotes), so this works.
  """

  @doc """
  Returns CLI arguments, preferring `:init.get_plain_arguments/0` when
  non-empty and falling back to parsing the `ADO_ARGS` env var.
  """
  @spec get_arguments() :: [String.t()]
  def get_arguments do
    case Enum.map(:init.get_plain_arguments(), &to_string/1) do
      [] -> ado_args_from_env()
      args -> args
    end
  end

  # Split a space-separated ADO_ARGS value into a list of arguments,
  # respecting single and double quotes.
  @spec ado_args_from_env() :: [String.t()]
  defp ado_args_from_env do
    case System.get_env("ADO_ARGS") do
      nil -> []
      "" -> []
      value -> split_args(value)
    end
  end

  # Minimal POSIX-style argument splitter. Handles single and double
  # quoted segments, and backslash escapes within double quotes.
  defp split_args(str) do
    {tokens, _} =
      str
      |> String.to_charlist()
      |> do_split([], [], false, nil)

    Enum.map(tokens, &List.to_string/1)
  end

  defp do_split([], acc, tokens, _in_quote, _quote_char) do
    {Enum.reverse(flush_token(acc, tokens)), []}
  end

  defp do_split([?\s | rest], acc, tokens, false, _q) do
    do_split(rest, [], flush_token(acc, tokens), false, nil)
  end

  defp do_split([?\\ | rest], acc, tokens, false, _q) do
    do_split(rest, [?\\ | acc], tokens, false, nil)
  end

  defp do_split([c | rest], acc, tokens, false, nil) when c in [?", ?'] do
    do_split(rest, acc, tokens, true, c)
  end

  defp do_split([c | rest], acc, tokens, true, q) when c == q do
    do_split(rest, acc, tokens, false, nil)
  end

  defp do_split([c | rest], acc, tokens, true, q) do
    do_split(rest, [c | acc], tokens, true, q)
  end

  defp do_split([c | rest], acc, tokens, false, _quote_char) do
    do_split(rest, [c | acc], tokens, false, nil)
  end

  defp flush_token([], tokens), do: tokens
  defp flush_token(acc, tokens), do: [Enum.reverse(acc) | tokens]
end
