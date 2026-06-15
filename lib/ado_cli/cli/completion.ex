defmodule AdoCli.CLI.Completion do
  @moduledoc """
  Generates shell completion scripts for the `ado` CLI.

  Supports bash, zsh, fish, and PowerShell. The generated script is
  static (generated once, valid until the CLI's command tree
  changes) and case-based: it dispatches on the current subcommand
  path to return the right candidates.

  ## Usage

      eval "$(ado completion -s bash)"      # bash
      ado completion -s zsh > "${fpath[1]}/_ado"  # zsh
      ado completion -s fish | source       # fish
      ado completion -s powershell | Out-String | Invoke-Expression  # pwsh

  The generated script knows about all subcommands in the CliMate
  command tree. When a new subcommand is added to the CLI, just
  rerun `ado completion -s <shell>` to update the installed
  completion script.

  ## Design notes

  Each shell's script uses a different dispatching strategy:

    * **bash** uses a `case` statement on the subcommand path
    * **zsh** uses nested `_describe` calls
    * **fish** uses one `complete -c ado` per (parent_path,
      subcommand) pair, dispatched on `__fish_seen_subcommand_from`
    * **powershell** uses `Register-ArgumentCompleter` with a
      script block that walks the AST to find the current path

  All four are generated from the same `Schema.build_tree/0`
  output, so completion is always in sync with the actual CLI
  surface.
  """

  @behaviour CliMate.CLI.Command
  import CliMate.CLI

  @supported_shells ~w(bash zsh fish powershell)
  @default_shell "bash"

  @impl true
  def command do
    [
      name: "ado completion",
      doc: """
      Generate a shell completion script for the ado CLI.

      Usage:
        eval "$(ado completion bash)"          # bash
        ado completion zsh > "${fpath[1]}/_ado"  # zsh
        ado completion fish | source            # fish
        ado completion powershell | Out-String | Invoke-Expression  # pwsh

      The generated script knows about all subcommands in the
      CliMate command tree. Run it again after upgrading ado
      to pick up new commands.

      With no argument, defaults to 'bash' (since that's what
      most people use interactively for a first try).

      Note: the shell is passed as a positional argument
      (e.g. `ado completion bash`), not as `-s` — `-s` is
      already the global short for `--server`.
      """,
      arguments: [
        shell: [
          type: :string,
          required: false,
          doc:
            "Shell to generate completion for: bash, zsh, fish, " <>
              "powershell. Default: bash"
        ]
      ],
      options: [
        write_to_file: [
          type: :string,
          short: :w,
          doc:
            "Write the script to this file path instead of stdout. " <>
              "Useful for installing to a system fpath (e.g. " <>
              "`ado completion zsh -w ~/.zsh/completions/_ado`).",
          doc_arg: "PATH"
        ]
      ],
      execute: &run/1
    ]
  end

  def run(parsed) do
    args = parsed.arguments || %{}
    opts = parsed.options || %{}

    # parse_shell/1 sends the :halt 1 message via halt_error/1
    # in test mode and returns a 3-tuple (the message). Use a
    # tagged-returns guard so we don't try to use the message
    # tuple as a shell name below (which would crash with
    # "String.Chars not implemented for Tuple").
    case validate_shell(Map.get(args, :shell)) do
      :ok -> do_run(Map.get(args, :shell), Map.get(opts, :write_to_file))
      :halted -> :ok
    end
  end

  defp do_run(shell, write_to_file) do
    shell = shell || @default_shell
    tree = AdoCli.CLI.Schema.build_tree()
    script = generate(shell, tree)

    case write_to_file do
      nil -> IO.puts(script)
      path -> File.write!(path, script)
    end

    halt(0)
  end

  # True if the shell argument is valid (or absent, in which
  # case the default is used). False (with the halt messages
  # already sent) if it was invalid.
  defp validate_shell(nil), do: :ok

  defp validate_shell(shell_str) when is_binary(shell_str) do
    if shell_str in @supported_shells do
      :ok
    else
      halt_error("Unknown shell '#{shell_str}'. Must be one of: #{Enum.join(@supported_shells, ", ")}.")
      :halted
    end
  end

  defp validate_shell(other) do
    halt_error("Shell must be a string, got: #{inspect(other)}")
    :halted
  end

  @doc """
  Returns the list of supported shell names. Public for use in
  tests and for the `ado completion --help` output.
  """
  @spec supported_shells() :: [String.t()]
  def supported_shells, do: @supported_shells

  @doc """
  Returns the default shell when no `-s` flag is given.
  """
  @spec default_shell() :: String.t()
  def default_shell, do: @default_shell

  # Schema.build_tree/0 returns a map with atom keys (because
  # node_to_map/1 builds the result with `Keyword.get` style
  # implicit keys). The subcommands are nested maps, also with
  # atom keys. For our string-keyed access below, normalize
  # each node to have string keys (and recurse into nested maps
  # and lists).
  defp normalize(tree) do
    tree
    |> Enum.into(%{}, fn {k, v} -> {to_string(k), normalize_value(v)} end)
  end

  defp normalize_value(v) when is_map(v) and not is_struct(v), do: normalize(v)
  defp normalize_value(v) when is_list(v), do: Enum.map(v, &normalize/1)
  defp normalize_value(v), do: v

  @doc """
  Returns the full completion script for the given shell.

  Public so tests can inspect the generated script without going
  through the full CLI dispatch. The `tree` argument is the output
  of `AdoCli.CLI.Schema.build_tree/0`; tests can pass a smaller
  fixture tree to keep assertions focused.

  Raises `ArgumentError` for unknown shells (caller's
  responsibility to validate first via `parse_shell/1`).
  """
  @spec generate(String.t(), map()) :: String.t()
  def generate(shell, tree) do
    tree = normalize(tree || %{})

    case shell do
      "bash" -> generate_bash(tree["subcommands"] || [])
      "zsh" -> generate_zsh(tree["subcommands"] || [])
      "fish" -> generate_fish(tree["subcommands"] || [])
      "powershell" -> generate_powershell(tree["subcommands"] || [])
      other -> raise ArgumentError, "Unknown shell: #{other}"
    end
  end

  @doc """
  Normalizes and validates a shell name from user input.

  - `nil` → default shell ("bash")
  - A string is lowercased and matched against the supported list
  - Any other value (e.g., a list, an integer) is rejected

  Halts with an error on invalid input. This is the canonical
  guard to call before `generate/2`.
  """
  @spec parse_shell(term()) :: String.t()
  def parse_shell(nil), do: @default_shell

  def parse_shell(shell) when is_binary(shell) do
    case String.downcase(shell) do
      s when s in @supported_shells ->
        s

      other ->
        halt_error(
          "Unknown shell '#{other}'. Must be one of: #{Enum.join(@supported_shells, ", ")}."
        )
    end
  end

  def parse_shell(other),
    do: halt_error("Shell must be a string, got: #{inspect(other)}")

  # ── Tree walking helpers ────────────────────────────────────────────

  # The Schema tree uses full names like "ado prs diff". These
  # helpers extract the subcommand name (last segment) and the
  # parent path (everything before).
  @doc false
  def last_segment("ado " <> rest), do: rest |> String.split(" ") |> List.last()
  def last_segment(name), do: name |> String.split(" ") |> List.last()

  defp join_path("", name), do: name
  defp join_path(parent, name), do: "#{parent} #{name}"

  defp children_of(sub), do: sub["subcommands"] || []

  # ── bash ────────────────────────────────────────────────────────────

  defp generate_bash(subcommands) do
    top_names = Enum.map(subcommands, &last_segment(&1["name"] || ""))
    case_blocks = Enum.map_join(subcommands, "\n", &bash_case(&1, ""))

    """
    # bash completion for the ado CLI
    # Generated by: ado completion -s bash
    # Source this in your shell:  eval "$(ado completion -s bash)"
    #
    # Supports subcommand completion at every nesting level. Options
    # are not auto-suggested (you can still type them, the script just
    # won't complete them after a subcommand is chosen). The script
    # is regenerated automatically on each run, so it stays in sync
    # with the CliMate command tree.

    _ado_completion() {
        local cur prev words cword
        _init_completion || return

        # Build the current subcommand path (skip flags)
        local path=""
        local i
        for ((i = 1; i < cword; i++)); do
            if [[ "${words[i]}" != -* ]]; then
                path="${path} ${words[i]}"
            fi
        done

        case "$path" in
    #{indent(case_blocks, 4)}
            *)
                COMPREPLY=($(compgen -W "#{Enum.join(top_names, " ")}" -- "$cur"))
                ;;
        esac
    }

    complete -F _ado_completion ado
    """
  end

  defp bash_case(sub, parent) do
    name = last_segment(sub["name"] || "")
    # The bash path always starts with a leading space (the loop
    # prepends " " on the first iteration). So the case pattern
    # for the top-level subcommand "prs" is " prs" (with space).
    # For nested levels, we strip the leading space before passing
    # to the recursive call, then re-add it once at the leaf.
    case_path = " #{join_path(parent, name)}"
    # parent passed to recursive call: no leading space, just the path
    children = children_of(sub)

    cond do
      children == [] ->
        """
            "#{case_path}")
                COMPREPLY=()
                ;;

        """

      true ->
        child_names = Enum.map(children, &last_segment(&1["name"] || ""))
        nested = Enum.map_join(children, "\n", &bash_case(&1, join_path(parent, name)))

        """
            "#{case_path}")
                COMPREPLY=($(compgen -W "#{Enum.join(child_names, " ")}" -- "$cur"))
                ;;

        #{nested}
        """
    end
  end

  # ── zsh ────────────────────────────────────────────────────────────

  defp generate_zsh(subcommands) do
    zsh_describe = zsh_describe_block("command", subcommands)
    zsh_dispatch = zsh_dispatch_block(subcommands, 1)

    """
    #compdef ado
    # zsh completion for the ado CLI
    # Generated by: ado completion -s zsh
    # Install:  ado completion -s zsh > "${fpath[1]}/_ado"

    _ado() {
        local -a commands
        local context state line

        _arguments -C \\
            '1: :->cmd' \\
            '*::arg:->args'

        case $state in
            cmd)
    #{indent(zsh_describe, 4)}
                ;;
            args)
                case $words[1] in
    #{indent(zsh_dispatch, 8)}
                esac
                ;;
        esac
    }

    _ado "$@"
    """
  end

  defp zsh_describe_block(label, subcommands) do
    quoted =
      Enum.map_join(subcommands, "\n", fn sub ->
        name = last_segment(sub["name"] || "")
        doc = shell_escape(sub["doc"] || "")
        "'#{name}:#{doc}'"
      end)

    """
    commands=(
    #{indent(quoted, 2)}
    )
    _describe '#{label}' commands
    """
  end

  defp zsh_dispatch_block(subcommands, depth) do
    Enum.map_join(subcommands, "\n", fn sub ->
      name = last_segment(sub["name"] || "")
      children = children_of(sub)
      indent_str = String.duplicate("    ", depth)

      cond do
        children == [] ->
          # Leaf: just dispatch to the leaf's _describe
          doc = shell_escape(sub["doc"] || "")

          """
          #{indent_str}#{name})
          #{indent_str}    commands=( '#{name}:#{doc}' )
          #{indent_str}    _describe '#{name}' commands
          #{indent_str}    ;;
          """

        true ->
          child_dispatch = zsh_dispatch_block(children, depth + 1)

          """
          #{indent_str}#{name})
          #{indent_str}    case $words[2] in
          #{indent(child_dispatch, 12)}
          #{indent_str}    esac
          #{indent_str}    ;;
          """
      end
    end)
  end

  # ── fish ───────────────────────────────────────────────────────────

  # Fish uses one `complete -c ado` per (parent_path, subcommand)
  # pair. The condition `__fish_seen_subcommand_from <parent>`
  # matches when the user has already typed the parent. Multiple
  # complete entries stack; fish picks the most specific one that
  # matches.
  defp generate_fish(subcommands) do
    top_names = Enum.map(subcommands, &last_segment(&1["name"] || ""))
    nested = Enum.map_join(subcommands, "\n", &fish_block(&1, ""))

    """
    # fish completion for the ado CLI
    # Generated by: ado completion -s fish
    # Install:  ado completion -s fish | source

    # Top-level completion: when no subcommand is typed yet
    complete -c ado -f -n "__fish_use_subcommand" \\
        -a "#{Enum.join(top_names, " ")}"

    # Global options (apply everywhere)
    complete -c ado -l org -d "Azure DevOps organization"
    complete -c ado -l pat -d "Personal Access Token"
    complete -c ado -l server -d "Server URL for self-hosted"
    complete -c ado -l verbose -d "Enable verbose output"
    complete -c ado -l json -d "Output raw JSON"

    #{nested}
    """
  end

  defp fish_block(sub, parent) do
    name = last_segment(sub["name"] || "")
    full_parent = join_path(parent, name)
    children = children_of(sub)

    cond do
      children == [] ->
        # Leaf node - this completes the path. No further candidates.
        ""

      true ->
        child_names = Enum.map(children, &last_segment(&1["name"] || ""))
        nested_blocks = Enum.map_join(children, "\n", &fish_block(&1, full_parent))

        """
        complete -c ado -f -n "__fish_seen_subcommand_from #{full_parent}" \\
            -a "#{Enum.join(child_names, " ")}"
        #{nested_blocks}
        """
    end
  end

  # ── powershell ────────────────────────────────────────────────────

  # PowerShell uses Register-ArgumentCompleter. The completer walks
  # the parsed AST to find the current subcommand path, then returns
  # the right candidates as [CompletionResult] objects.
  defp generate_powershell(subcommands) do
    flat_list = powershell_flatten(subcommands, [], [])
    candidates_ps = powershell_candidates_block(flat_list)
    top_names = Enum.map(subcommands, &last_segment(&1["name"] || ""))

    top_array =
      case top_names do
        [] -> "@()"
        names -> "@('" <> Enum.join(names, "', '") <> "')"
      end

    """
    # PowerShell completion for the ado CLI
    # Generated by: ado completion -s powershell
    # Install:  ado completion -s powershell | Out-String | Invoke-Expression

    using namespace System.Management.Automation
    using namespace System.Management.Automation.Language

    # Top-level candidates (also the fallback)
    $ado_top = #{top_array}

    # Path -> candidates map for nested completion
    # Generated from the CliMate command tree
    $ado_paths = @{
    #{indent(candidates_ps, 4)}
    }

    Register-ArgumentCompleter -Native -CommandName 'ado' -ScriptBlock {
        param($wordToComplete, $commandAst, $cursorPosition)

        $tokens = $null
        $null = [System.Management.Automation.language.Parser]::ParseInput(
            $commandAst.ToString(), [ref]$tokens)

        # Build the current subcommand path (skip flag-style words
        # and the command name itself).
        $path = @()
        foreach ($token in $tokens) {
            if ($token.TokenFlags -band [TokenFlags]::CommandName) {
                continue
            }
            if ($token -is [StringExpandableToken]) {
                $value = $token.Value
                if ($value -and -not $value.StartsWith('-')) {
                    $path += $value
                }
            }
        }

        # Find the most specific path match
        $candidates = $null
        $bestLength = -1
        foreach ($entry in $ado_paths.GetEnumerator()) {
            $key = $entry.Key
            if ($path.Count -ge $key.Count) {
                $match = $true
                for ($i = 0; $i -lt $key.Count; $i++) {
                    if ($path[$i] -ne $key[$i]) { $match = $false; break }
                }
                if ($match -and $key.Count -gt $bestLength) {
                    $candidates = $entry.Value
                    $bestLength = $key.Count
                }
            }
        }

        if ($null -eq $candidates) { $candidates = $ado_top }

        foreach ($c in $candidates) {
            [System.Management.Automation.CompletionResult]::new(
                $c, $c, 'ParameterValue', $c)
        }
    }
    """
  end

  defp powershell_flatten([], _parent_path, acc), do: acc

  defp powershell_flatten(subcommands, parent_path, acc) do
    Enum.reduce(subcommands, acc, fn sub, acc ->
      name = last_segment(sub["name"] || "")
      current_path = parent_path ++ [name]
      children = children_of(sub)

      acc =
        if children != [] do
          child_names = Enum.map(children, &last_segment(&1["name"] || ""))
          # Add entry: when the user has typed `current_path`, here
          # are the next-level candidates.
          [%{path: current_path, candidates: child_names} | acc]
        else
          acc
        end

      # Recurse into children, extending the parent path.
      powershell_flatten(children, current_path, acc)
    end)
  end

  defp powershell_candidates_block(flat_list) do
    Enum.map_join(flat_list, "\n", fn %{path: path, candidates: cands} ->
      path_arr = Enum.map_join(path, ", ", &"'#{&1}'")
      cand_arr = Enum.map_join(cands, ", ", &"'#{&1}'")
      "@(#{path_arr}) = @(#{cand_arr})"
    end)
  end

  # ── shared helpers ──────────────────────────────────────────────────

  defp indent(text, spaces) do
    prefix = String.duplicate(" ", spaces)

    text
    |> String.split("\n")
    |> Enum.map(fn line -> if(line == "", do: line, else: prefix <> line) end)
    |> Enum.join("\n")
  end

  # Escape a string for use in a single-quoted shell context.
  defp shell_escape(text) do
    text
    |> String.replace("'", "'\\''")
    |> String.replace("\n", " ")
  end
end
