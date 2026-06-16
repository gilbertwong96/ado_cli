defmodule AdoCli.CLI.Schema do
  @moduledoc """
  Dumps the entire CLI command tree as a structured JSON object.

  LLMs can use this to discover the full surface of the CLI in a
  single round trip, without parsing colored `--help` output.

  ## Usage

      ado schema                # human-readable tree (huge — prefer --json)
      ado schema --json         # full command tree as JSON
      ado schema NAME --json    # one command + its descendants

  ## Output shape (when `--json` is set)

      {
        "name": "ado",
        "version": "0.1.0",
        "doc": "Azure DevOps CLI - Manage Azure DevOps ...",
        "options": [
          { "name": "org", "short": "o", "type": "string", "doc": "..." },
          ...
        ],
        "subcommands": [
          { "name": "ado projects", "doc": "...", "subcommands": [ ... ] },
          ...
        ]
      }

  The `options` field is included for every level. The `subcommands`
  field is omitted (or empty) for leaf commands (those with no
  further subcommands).
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  @impl true
  def command do
    [
      name: "ado schema",
      doc: "Dump the CLI command tree as structured JSON for LLM agents.",
      arguments: [
        name: [
          type: :string,
          required: false,
          doc: "Optional: dump only this command + descendants"
        ]
      ],
      options: [
        json: [type: :boolean, default: false, doc: "Output as JSON envelope"]
      ],
      execute: &run/1
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  @doc """
  Dumps the command tree starting from the top-level command (or the
  specified subcommand).
  """
  def run(parsed) do
    json? = Map.get(parsed.options, :json, false)
    target = get_in(parsed.arguments, [:name])

    tree = build_tree(target)
    version = AdoCli.Version.current()

    if json? do
      payload = Map.put(tree, "version", version)
      # Use IO.puts (not writeln) to avoid ANSI color codes from
      # halt_success/1 polluting the JSON envelope.
      IO.puts(JSON.encode!(%{ok: true, schema: payload}))
    else
      print_human(tree, version)
    end

    halt(0)
  end

  # ── tree building ────────────────────────────────────────────────────

  @doc """
  Recursively walks the CliMate command tree and converts each node
  into a structured map. Public so it can be tested directly.
  """
  @spec build_tree() :: map()
  def build_tree, do: build_tree(nil)

  @spec build_tree(String.t() | nil) :: map()
  def build_tree(nil) do
    cmd_def = AdoCli.CLI.command_definition()
    node_to_map(cmd_def)
  end

  def build_tree(name) when is_binary(name) do
    cmd_def = AdoCli.CLI.command_definition()
    find_and_dump(cmd_def, name)
  end

  defp find_and_dump(node, name) do
    base = strip_ado_prefix(Keyword.get(node, :name, ""))

    if matches?(base, name) do
      node_to_map(node)
    else
      search_subcommands(Keyword.get(node, :subcommands, []), name)
    end
  end

  defp strip_ado_prefix("ado " <> rest), do: rest
  defp strip_ado_prefix(other), do: other

  defp matches?(base, name) do
    base == name or String.ends_with?(base, " " <> name)
  end

  defp search_subcommands([], name), do: not_found(name)

  defp search_subcommands(subs, name) do
    found = Enum.find_value(subs, fn sub -> match_subcommand(sub, name) end)
    handle_match_result(found, name)
  end

  defp handle_match_result(nil, name), do: not_found(name)

  defp handle_match_result({sub_name, mod}, name) when is_atom(mod) and not is_nil(mod) do
    cmd_def = safe_command_def(mod, name: sub_name)
    recurse_into(cmd_def, sub_name, name)
  end

  defp handle_match_result({sub_name, def}, name) when is_list(def) do
    recurse_into(def, sub_name, name)
  end

  defp handle_match_result({sub_name, _}, name) do
    recurse_into([name: sub_name], sub_name, name)
  end

  defp safe_command_def(mod, fallback) do
    if Code.ensure_loaded?(mod), do: mod.command(), else: fallback
  rescue
    # Bare rescue replaced with explicit exception types (reach).
    # Mod.command/0 can raise UndefinedFunctionError if the module
    # doesn't implement the CliMate behaviour, or ArgumentError for
    # other arity mismatches.
    UndefinedFunctionError -> fallback
    ArgumentError -> fallback
  end

  # If the user asked for a path like "pipelines list", check whether
  # `name` matches the full path now (i.e. "pipelines list") and return
  # the node as-is. Otherwise, drill into the nested subcommands looking
  # for the remaining path segment.
  defp recurse_into(def, sub_name, name) do
    if sub_name == name or String.ends_with?(sub_name, " " <> name) do
      node_to_map(Keyword.merge([name: sub_name], def))
    else
      # The user asked for "pipelines list" but we just found "pipelines".
      # Strip "pipelines" from the name and look in the nested subcommands.
      rest =
        case String.split(name, " ", parts: 2) do
          [^sub_name, remaining] -> remaining
          [_, remaining] -> remaining
          _ -> name
        end

      nested = Keyword.get(def, :subcommands, [])
      find_in_subcommands(nested, sub_name, rest)
    end
  end

  defp find_in_subcommands([], _parent_name, _rest), do: not_found("")

  defp find_in_subcommands(subs, parent_name, rest) do
    found = Enum.find_value(subs, fn sub -> match_subcommand(sub, rest) end)

    case found do
      {sub_name, mod} when is_atom(mod) and not is_nil(mod) ->
        cmd_def =
          try do
            if Code.ensure_loaded?(mod), do: mod.command(), else: [name: sub_name]
          rescue
            UndefinedFunctionError -> [name: sub_name]
            ArgumentError -> [name: sub_name]
          end

        full_name = "#{parent_name} #{sub_name}"
        node_to_map(Keyword.merge([name: full_name], cmd_def))

      {sub_name, def} when is_list(def) ->
        full_name = "#{parent_name} #{sub_name}"
        node_to_map(Keyword.merge([name: full_name], def))

      {sub_name, _} ->
        full_name = "#{parent_name} #{sub_name}"
        node_to_map(name: full_name)

      nil ->
        not_found("#{parent_name} #{rest}")
    end
  end

  # Match a subcommand entry `{name_atom, mod}` against the requested name.
  # Returns `{matched_name, mod}` if it matches, or nil.
  #
  # Match rules (most specific first):
  #   1. exact match (`sub_name == name`)
  #   2. sub_name is a prefix of name followed by a space (e.g. "pipelines"
  #      matches "pipelines list" — caller will recurse to find "list")
  #   3. name ends with " " <> sub_name (e.g. "ado pipelines list" ends with
  #      " pipelines")
  defp match_subcommand({name_atom, mod}, name) do
    sub_name = stringify(name_atom)

    cond do
      sub_name == name ->
        {sub_name, mod}

      # "pipelines" is a prefix of "pipelines list" — caller will recurse
      String.starts_with?(name, sub_name <> " ") ->
        {sub_name, mod}

      # "ado pipelines list" ends with " pipelines" or " pipelines list"
      String.ends_with?(name, " " <> sub_name) ->
        {sub_name, mod}

      true ->
        nil
    end
  end

  defp not_found(name) do
    %{
      error: %{
        code: "not_found",
        message: "no command named #{inspect(name)}. Run `ado schema` to see all commands."
      }
    }
  end

  # ── node → map conversion ───────────────────────────────────────────

  defp node_to_map(node) do
    # Prefer the :name from the def (which is the full path like
    # "ado projects list") over the key from the keyword list (which
    # is just "list").
    name =
      case Keyword.get(node, :name) do
        nil -> ""
        n when is_binary(n) -> n
        n -> stringify(n)
      end

    %{
      name: name,
      doc: stringify(Keyword.get(node, :doc, "")),
      arguments: arguments_to_list(Keyword.get(node, :arguments, [])),
      options: options_to_list(Keyword.get(node, :options, [])),
      subcommands: subcommands_to_list(Keyword.get(node, :subcommands, []))
    }
  end

  defp arguments_to_list(args) do
    args
    |> Enum.reject(&match?({:subcommands, _}, &1))
    |> Enum.map(fn {key, meta} ->
      %{
        name: to_string(key),
        type: stringify(Keyword.get(meta, :type, "string")),
        required: Keyword.get(meta, :required, false),
        doc: stringify(Keyword.get(meta, :doc, ""))
      }
    end)
  end

  defp options_to_list(opts) do
    opts
    |> Enum.reject(&match?({:subcommands, _}, &1))
    |> Enum.map(fn {key, meta} ->
      %{
        name: to_string(key),
        type: stringify(Keyword.get(meta, :type, "string")),
        short: stringify(Keyword.get(meta, :short, "")),
        default: meta |> Keyword.get(:default) |> stringify(),
        required: Keyword.get(meta, :required, false),
        doc: stringify(Keyword.get(meta, :doc, ""))
      }
    end)
  end

  defp subcommands_to_list(subs) do
    # subs is a list of `{name_atom, module_or_def}` pairs.
    # - The top-level subcommands are {atom, module} where module
    #   implements the CliMate.CLI.Command behaviour.
    # - The nested subcommands (inside a command's `subcommands:` keyword)
    #   are {atom, keyword_list} where the keyword list is the inline
    #   command definition.
    Enum.map(subs, fn {name_atom, mod} ->
      sub_name = stringify(name_atom)
      cmd_def = resolve_subcommand_def(mod, sub_name)
      node = Keyword.merge([name: sub_name], cmd_def)
      node_to_map(node)
    end)
  end

  defp resolve_subcommand_def(mod, sub_name) do
    cond do
      is_list(mod) or is_map(mod) ->
        # mod is already a keyword list / map (inline subcommand def)
        mod

      is_atom(mod) and not is_nil(mod) ->
        # mod is a module — call its command/0
        try do
          if Code.ensure_loaded?(mod), do: mod.command(), else: [name: sub_name]
        rescue
          UndefinedFunctionError -> [name: sub_name]
          ArgumentError -> [name: sub_name]
        end

      true ->
        [name: sub_name]
    end
  end

  defp stringify(nil), do: ""
  defp stringify(v) when is_binary(v), do: v
  defp stringify(v) when is_atom(v) and not is_nil(v), do: to_string(v)
  defp stringify(v), do: inspect(v)

  # ── human-readable printing ─────────────────────────────────────────

  defp print_human(tree, version) do
    writeln("")
    writeln("ado v#{version} — command tree")
    writeln(String.duplicate("─", 60))
    print_node(tree, 0)
    writeln("")
    writeln("Run with --json for a structured tree.")
    writeln("")
  end

  defp print_node(node, depth) do
    indent = String.duplicate("  ", depth)
    writeln("#{indent}#{node.name}")
    writeln("#{indent}  #{node.doc}")

    if node.options != [] do
      opts_str =
        Enum.map_join(node.options, "\n", fn opt ->
          short = if opt.short != "", do: "-#{opt.short} ", else: ""
          default = if opt.default != "", do: " (default: #{opt.default})", else: ""
          "  #{indent}    --#{opt.name} #{short}<#{opt.type}>#{default}"
        end)

      writeln("#{indent}  options:")
      writeln(opts_str)
    end

    if node.subcommands != [] do
      writeln("#{indent}  subcommands:")
      Enum.each(node.subcommands, fn sub -> print_node(sub, depth + 2) end)
    end
  end
end
