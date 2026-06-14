defmodule AdoCli.CLI.LogoutTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Logout

  describe "run" do
    test "halts 0 on success" do
      # The new --json path uses Output.ok_message/2 which calls
      # halt_success('') internally. Without --json, the old path
      # calls halt_success('') directly. Either way, exit 0.
      Logout.run(%{options: %{}, arguments: %{}})
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end
end
