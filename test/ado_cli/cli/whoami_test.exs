defmodule AdoCli.CLI.WhoamiTest do
  use AdoCli.CLI.TestHelper
  alias AdoCli.CLI.Whoami

  describe "run" do
    test "halts 0 on success" do
      apply(AdoCli.CLI.Logout, :run, [%{}])
      assert_receive {:cli_mate_shell, :halt, 0}, 500
    end
  end
end
