defmodule AdoCli.FuzzyTest do
  use ExUnit.Case, async: true

  alias AdoCli.Fuzzy

  describe "match/2" do
    test "substring match (case-insensitive)" do
      [{winner, score} | _] = Fuzzy.match(["John Doe", "Jane Smith", "Bob Jones"], "john")
      assert winner == "John Doe"
      assert score >= 300
    end

    test "exact match scores higher than substring match" do
      [{exact, s1}, {_substring, s2} | _] =
        Fuzzy.match(["Alice Smith", "alice", "Bob"], "alice")

      assert exact == "alice"
      assert s1 > s2
    end

    test "prefix match ranks above infix match" do
      [{prefix, _} | [{_infix, _} | _]] =
        Fuzzy.match(["Al Jones", "Bob Al Green"], "al")

      assert prefix == "Al Jones"
    end

    test "subsequence match (fzf-style, non-adjacent)" do
      results = Fuzzy.match(["John Smith", "Jane Smith", "Bob Jones"], "jsmith")
      assert {winner, _} = hd(results)
      assert winner == "John Smith"
    end

    test "returns empty list when nothing matches" do
      assert Fuzzy.match(["Alice", "Bob"], "zzz") == []
    end

    test "empty query returns no matches" do
      assert Fuzzy.match(["Alice", "Bob"], "") == []
    end

    test "nil query returns no matches" do
      assert Fuzzy.match(["Alice", "Bob"], nil) == []
    end

    test "results sorted best-first" do
      results = Fuzzy.match(["Bob Jones", "John Doe", "Johnny Doe"], "john")
      names = Enum.map(results, &elem(&1, 0))
      assert hd(names) == "John Doe"
    end

    test "matches email addresses" do
      results =
        Fuzzy.match(["John Doe <john@example.com>", "Bob <bob@x.com>"], "john@example.com")

      assert String.contains?(elem(hd(results), 0), "john@example.com")
    end
  end

  describe "match_fields/3" do
    @items [
      %{name: "Alice Smith", email: "alice@example.com", id: "1"},
      %{name: "Bob Jones", email: "bob@example.com", id: "2"},
      %{name: "Alicia Keys", email: "alicia@example.com", id: "3"}
    ]

    test "filters by name field (substring)" do
      result = Fuzzy.match_fields(@items, "alice", [:name, :email])
      names = Enum.map(result, & &1.name)
      assert "Alice Smith" in names
      assert "Alicia Keys" in names
      refute "Bob Jones" in names
    end

    test "filters by email field" do
      result = Fuzzy.match_fields(@items, "bob@example.com", [:name, :email])
      [_ | _] = result
      assert hd(result).name == "Bob Jones"
    end

    test "subsequence matches across fields" do
      # 'asmith' = subsequence of 'Alice Smith'
      result = Fuzzy.match_fields(@items, "asmith", [:name, :email])
      names = Enum.map(result, & &1.name)
      assert "Alice Smith" in names
      refute "Bob Jones" in names
    end

    test "no matches returns empty list" do
      assert Fuzzy.match_fields(@items, "nobody", [:name, :email]) == []
    end

    test "nil query returns all items unchanged" do
      assert Fuzzy.match_fields(@items, nil, [:name, :email]) == @items
    end

    test "empty query returns all items unchanged" do
      assert Fuzzy.match_fields(@items, "", [:name, :email]) == @items
    end

    test "ignores non-binary field values" do
      items = [%{name: "Alice", count: 42}]
      assert Fuzzy.match_fields(items, "alice", [:name, :count]) == items
    end
  end
end
