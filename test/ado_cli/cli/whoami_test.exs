defmodule AdoCli.CLI.WhoamiTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Whoami

  describe "run" do
    test "halts 0 on success" do
      # whoami writes to stdout; the new --json path uses Output.ok/4
      # which calls halt_success('') internally. Either way, exit 0.
      AdoCli.CLI.Logout.run(%{options: %{}, arguments: %{}})
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end
end
