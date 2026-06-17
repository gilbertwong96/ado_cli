defmodule AdoCli.CLITest do
  use ExUnit.Case, async: true

  # The public entry point is the same one used by escript/Burrito.
  # We test join_multivalue_opts indirectly by calling run/1 with
  # a list of args and observing the parsed execute function's
  # arguments. To keep the test focused, we test the
  # join_multivalue_opts/1 helper directly.
  describe "join_multivalue_opts/1" do
    test "joins unquoted multi-word content into --content=value" do
      args = ["--content", "Code", "review", "approved", "and", "merged."]

      assert AdoCli.CLI.join_multivalue_opts(args) == [
               "--content=Code review approved and merged."
             ]
    end

    test "leaves --content with --status after intact" do
      args = ["--content", "Code", "review", "--status", "active"]

      assert AdoCli.CLI.join_multivalue_opts(args) == [
               "--content=Code review",
               "--status",
               "active"
             ]
    end

    test "leaves single-word --content unchanged" do
      args = ["--content", "Approved"]
      assert AdoCli.CLI.join_multivalue_opts(args) == ["--content=Approved"]
    end

    test "handles --content=... pre-joined form" do
      args = ["--content=Code review", "--status", "active"]

      assert AdoCli.CLI.join_multivalue_opts(args) == [
               "--content=Code review",
               "--status",
               "active"
             ]
    end

    test "leaves --content - (stdin) alone" do
      args = ["--content", "-"]
      assert AdoCli.CLI.join_multivalue_opts(args) == ["--content=-"]
    end

    test "leaves --content @file alone" do
      args = ["--content", "@/tmp/note.md"]
      assert AdoCli.CLI.join_multivalue_opts(args) == ["--content=@/tmp/note.md"]
    end

    test "stops at the next flag even with quoted-looking words" do
      args = ["--content", "LGTM!", "--json"]
      assert AdoCli.CLI.join_multivalue_opts(args) == ["--content=LGTM!", "--json"]
    end

    test "preserves unrelated args around the multivalue" do
      args = ["--org", "myorg", "pos1", "pos2", "--content", "hello world", "--json"]

      assert AdoCli.CLI.join_multivalue_opts(args) == [
               "--org",
               "myorg",
               "pos1",
               "pos2",
               "--content=hello world",
               "--json"
             ]
    end

    test "applies to --description too" do
      args = ["--description", "This is a", "multi-line", "description"]

      assert AdoCli.CLI.join_multivalue_opts(args) == [
               "--description=This is a multi-line description"
             ]
    end

    test "joins empty list at end of args" do
      args = ["--content", "hello", "world"]
      assert AdoCli.CLI.join_multivalue_opts(args) == ["--content=hello world"]
    end

    test "handles multiple multivalue options" do
      args = ["--content", "foo bar", "--description", "baz qux"]

      assert AdoCli.CLI.join_multivalue_opts(args) == [
               "--content=foo bar",
               "--description=baz qux"
             ]
    end

    test "leaves other flags untouched" do
      args = ["--json", "--org", "myorg"]
      assert AdoCli.CLI.join_multivalue_opts(args) == ["--json", "--org", "myorg"]
    end
  end
end
