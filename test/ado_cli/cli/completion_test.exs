defmodule AdoCli.CLI.CompletionTest do
  use ExUnit.Case, async: false

  alias AdoCli.CLI.Completion

  setup do
    # Switch to ProcessShell so halt/1 sends messages to the test
    # process instead of calling System.halt/1 (which would
    # actually kill the test BEAM). The default shell exits the
    # process on halt, which is the right behavior in production
    # but breaks test assertions on :halt messages.
    CliMate.CLI.put_shell(CliMate.CLI.ProcessShell)
    on_exit(fn -> CliMate.CLI.put_shell(CliMate.CLI.DefaultShell) end)
    :ok
  end

  # A small fixture tree. Avoids the full Schema.build_tree/0
  # output (which has 100+ nodes) so tests are focused and fast.
  @fixture %{
    "name" => "ado",
    "subcommands" => [
      %{
        "name" => "ado prs",
        "doc" => "Manage pull requests",
        "subcommands" => [
          %{
            "name" => "ado prs diff",
            "doc" => "Show the diff",
            "subcommands" => []
          },
          %{
            "name" => "ado prs comments",
            "doc" => "Manage comments",
            "subcommands" => [
              %{"name" => "ado prs comments add", "doc" => "Add", "subcommands" => []},
              %{"name" => "ado prs comments list", "doc" => "List", "subcommands" => []}
            ]
          }
        ]
      },
      %{"name" => "ado projects", "doc" => "Manage projects", "subcommands" => []}
    ]
  }

  describe "supported_shells/0 and default_shell/0" do
    test "returns the four supported shells" do
      assert Completion.supported_shells() == ["bash", "zsh", "fish", "powershell"]
    end

    test "defaults to bash" do
      assert Completion.default_shell() == "bash"
    end
  end

  describe "parse_shell/1" do
    test "nil → default (bash)" do
      assert Completion.parse_shell(nil) == "bash"
    end

    test "lowercases valid input" do
      assert Completion.parse_shell("BASH") == "bash"
      assert Completion.parse_shell("Zsh") == "zsh"
      assert Completion.parse_shell("FISH") == "fish"
    end

    test "halts on unknown shell" do
      Completion.parse_shell("tcsh")

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "Unknown shell 'tcsh'"
    end

    test "halts on non-string input" do
      Completion.parse_shell(42)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "Shell must be a string"
    end
  end

  describe "generate/2 — bash" do
    setup do
      [script: Completion.generate("bash", @fixture)]
    end

    test "has the right header and complete registration", %{script: script} do
      assert script =~ "# bash completion for the ado CLI"
      assert script =~ "Source this in your shell"
      assert script =~ "complete -F _ado_completion ado"
    end

    test "top-level subcommands appear in the *) case", %{script: script} do
      # The *) case has all top-level subcommands
      assert script =~ ~s|COMPREPLY=($(compgen -W "prs projects" -- "$cur"))|
    end

    test "nested case for 'prs' suggests its children", %{script: script} do
      assert script =~ ~s|" prs")|
      assert script =~ ~s|COMPREPLY=($(compgen -W "diff comments" -- "$cur"))|
    end

    test "deeper nesting: 'prs comments' suggests add/list", %{script: script} do
      assert script =~ ~s|" prs comments")|
      assert script =~ ~s|COMPREPLY=($(compgen -W "add list" -- "$cur"))|
    end

    test "leaf nodes have COMPREPLY=()", %{script: script} do
      assert script =~ ~s|" prs diff")|
      assert script =~ ~s|COMPREPLY=()|
    end
  end

  describe "generate/2 — zsh" do
    setup do
      [script: Completion.generate("zsh", @fixture)]
    end

    test "has the #compdef directive", %{script: script} do
      assert script =~ "#compdef ado"
    end

    test "top-level _describe block lists all subcommands", %{script: script} do
      assert script =~ "'prs:Manage pull requests'"
      assert script =~ "'projects:Manage projects'"
    end

    test "nested case for prs dispatches to its subcommands", %{script: script} do
      assert script =~ "prs)"
      assert script =~ "case $words[2] in"
    end
  end

  describe "generate/2 — fish" do
    setup do
      [script: Completion.generate("fish", @fixture)]
    end

    test "uses __fish_use_subcommand for top-level", %{script: script} do
      assert script =~ "__fish_use_subcommand"
      assert script =~ ~s|-a "prs projects"|
    end

    test "uses __fish_seen_subcommand_from for nested", %{script: script} do
      assert script =~ "__fish_seen_subcommand_from prs"
      assert script =~ ~s|-a "diff comments"|
    end

    test "deeper nesting: 'prs comments' has its own complete entry", %{script: script} do
      assert script =~ "__fish_seen_subcommand_from prs comments"
      assert script =~ ~s|-a "add list"|
    end

    test "registers global options", %{script: script} do
      assert script =~ "complete -c ado -l org"
      assert script =~ "complete -c ado -l pat"
      assert script =~ "complete -c ado -l server"
    end
  end

  describe "generate/2 — powershell" do
    setup do
      [script: Completion.generate("powershell", @fixture)]
    end

    test "uses Register-ArgumentCompleter", %{script: script} do
      assert script =~ "Register-ArgumentCompleter -Native -CommandName 'ado'"
    end

    test "declares a top-level candidates array", %{script: script} do
      assert script =~ "$ado_top = @('prs', 'projects')"
    end

    test "declares nested path maps with full paths", %{script: script} do
      assert script =~ "@('prs') = @('diff', 'comments')"
      assert script =~ "@('prs', 'comments') = @('add', 'list')"
    end
  end

  describe "generate/2 — error cases" do
    test "raises ArgumentError for unknown shell" do
      assert_raise ArgumentError, ~r/Unknown shell/, fn ->
        Completion.generate("tcsh", @fixture)
      end
    end

    test "handles nil tree gracefully" do
      # nil tree means "no subcommands" — should still produce
      # a valid (if minimal) script.
      script = Completion.generate("bash", nil)
      assert script =~ "complete -F _ado_completion ado"
    end
  end
end

# ── CLI dispatch integration ───────────────────────────────────────

defmodule AdoCli.CLI.CompletionCLITest do
  use AdoCli.CLI.TestHelper
  import ExUnit.CaptureIO

  alias AdoCli.CLI.Completion

  describe "ado completion subcommand" do
    test "prints bash script by default" do
      output =
        capture_io(fn ->
          apply(Completion, :run, [
            %{options: %{}, arguments: %{}}
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output =~ "# bash completion for the ado CLI"
      assert output =~ "complete -F _ado_completion ado"
    end

    test "prints zsh script for shell=zsh" do
      output =
        capture_io(fn ->
          apply(Completion, :run, [
            %{options: %{}, arguments: %{shell: "zsh"}}
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output =~ "#compdef ado"
    end

    test "prints fish script for shell=fish" do
      output =
        capture_io(fn ->
          apply(Completion, :run, [
            %{options: %{}, arguments: %{shell: "fish"}}
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output =~ "__fish_use_subcommand"
    end

    test "prints powershell script for shell=powershell" do
      output =
        capture_io(fn ->
          apply(Completion, :run, [
            %{options: %{}, arguments: %{shell: "powershell"}}
          ])
        end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert output =~ "Register-ArgumentCompleter"
    end

    test "writes to file when -w is given" do
      path =
        Path.join(System.tmp_dir!(), "ado_complete_#{System.unique_integer([:positive])}.bash")

      on_exit(fn -> File.rm_rf(path) end)

      capture_io(fn ->
        apply(Completion, :run, [
          %{options: %{write_to_file: path}, arguments: %{}}
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 0}, 500
      assert File.read!(path) =~ "# bash completion for the ado CLI"
    end

    test "halts with clear error on unknown shell" do
      capture_io(fn ->
        apply(Completion, :run, [
          %{options: %{}, arguments: %{shell: "tcsh"}}
        ])
      end)

      assert_receive {:cli_mate_shell, :halt, 1}, 500
      assert_receive {:cli_mate_shell, :error, msg}, 500
      assert msg =~ "Unknown shell 'tcsh'"
    end
  end
end
