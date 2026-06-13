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
  end
end
