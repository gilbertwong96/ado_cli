defmodule AdoCli.FrontmatterTest do
  use ExUnit.Case, async: true

  alias AdoCli.Frontmatter

  describe "parse/1" do
    test "parses simple key-value pairs" do
      content = "---\nkey: value\nother: data\n---\nbody"
      assert Frontmatter.parse(content) == %{"key" => "value", "other" => "data"}
    end

    test "returns empty map when no frontmatter" do
      assert Frontmatter.parse("no frontmatter here") == %{}
    end

    test "returns empty map when content is nil-like" do
      assert Frontmatter.parse("") == %{}
    end

    test "strips double quotes from values" do
      content = ~s(---\nname: "John Doe"\nage: "30"\n---\n)
      assert Frontmatter.parse(content) == %{"name" => "John Doe", "age" => "30"}
    end

    test "strips single quotes from values" do
      content = "---\nname: 'John Doe'\nage: '30'\n---\n"
      assert Frontmatter.parse(content) == %{"name" => "John Doe", "age" => "30"}
    end

    test "handles empty values" do
      content = "---\nkey:\nother: value\n---\n"
      assert Frontmatter.parse(content) == %{"key" => "", "other" => "value"}
    end

    test "trims surrounding whitespace" do
      content = "---\n  key:    value with spaces  \n---\n"
      assert Frontmatter.parse(content) == %{"key" => "value with spaces"}
    end

    test "ignores lines without colon" do
      content = "---\nno_colon_here\nkey: value\n---\n"
      assert Frontmatter.parse(content) == %{"key" => "value"}
    end

    test "stops at second --- delimiter" do
      content = "---\nkey: value\n---\nbody\n---\nshould not appear"
      assert Frontmatter.parse(content) == %{"key" => "value"}
    end

    test "values with colons are preserved (only first colon splits)" do
      content = "---\nurl: http://example.com:8080\n---\n"
      assert Frontmatter.parse(content) == %{"url" => "http://example.com:8080"}
    end

    test "realistic skill frontmatter" do
      content = """
      ---
      description: Main ado skill — setup, auth, and commands
      version: "0.2.0"
      ---

      # ado

      Content here.
      """

      fm = Frontmatter.parse(content)
      assert fm["description"] =~ "Main ado skill"
      assert fm["version"] == "0.2.0"
    end

    test "parses commands list (YAML block list syntax)" do
      content = """
      ---
      description: x
      version: "0.1.0"
      commands:
        - ado projects list
        - ado prs create PROJECT REPO --title TEXT
        - ado pipelines run PROJECT ID --branch main
      ---
      body
      """

      cmds = Frontmatter.parse_commands(content)
      assert length(cmds) == 3
      assert Enum.at(cmds, 0) == "ado projects list"
      assert Enum.at(cmds, 1) == "ado prs create PROJECT REPO --title TEXT"
      assert Enum.at(cmds, 2) == "ado pipelines run PROJECT ID --branch main"
    end

    test "parse_commands preserves trailing # comments on each item" do
      content = """
      ---
      commands:
        - ado projects list ORG          # ORG here is the org name
        - ado prs create                # opens a PR
      ---
      """

      [c1, c2] = Frontmatter.parse_commands(content)
      assert c1 =~ "ORG here is the org name"
      assert c2 =~ "opens a PR"
    end

    test "parse_commands returns empty list when no commands field" do
      content = "---\ndescription: x\n---\n"
      assert Frontmatter.parse_commands(content) == []
    end
  end
end
