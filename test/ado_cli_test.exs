defmodule AdoCliTest do
  use ExUnit.Case
  doctest AdoCli

  alias AdoCli.CLI

  describe "CLI parsing" do
    test "parses projects list" do
      {:ok, result} = CliMate.CLI.parse(~w(projects list), CLI.command_definition())
      assert result.path == [:projects, :list]
    end

    test "parses projects show" do
      {:ok, result} =
        CliMate.CLI.parse(~w(projects show my-project), CLI.command_definition())

      assert result.path == [:projects, :show]
      assert result.arguments.project_id == "my-project"
    end

    test "parses projects create with options" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(projects create NewProject --description MyDesc --visibility private),
          CLI.command_definition()
        )

      assert result.path == [:projects, :create]
      assert result.arguments.name == "NewProject"
      assert result.options.description == "MyDesc"
      assert result.options.visibility == "private"
    end

    test "parses projects update with --name" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(projects update OldProject --name NewProject),
          CLI.command_definition()
        )

      assert result.arguments.project_id == "OldProject"
      assert result.options.name == "NewProject"
    end

    test "parses projects delete with --force" do
      {:ok, result} =
        CliMate.CLI.parse(~w(projects delete MyProject --force), CLI.command_definition())

      assert result.arguments.project_id == "MyProject"
      assert result.options.force == true
    end

    test "parses repos list with project" do
      {:ok, result} =
        CliMate.CLI.parse(~w(repos list MyProject), CLI.command_definition())

      assert result.path == [:repos, :list]
      assert result.arguments.project == "MyProject"
    end

    test "parses repos create" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(repos create MyProject NewRepo --default-branch develop),
          CLI.command_definition()
        )

      assert result.path == [:repos, :create]
      assert result.arguments.project == "MyProject"
      assert result.arguments.name == "NewRepo"
      assert result.options.default_branch == "develop"
    end

    test "parses repos branches with filter" do
      {:ok, result} =
        CliMate.CLI.parse(
          ["repos", "branches", "MyProject", "MyRepo", "--filter", "feature/"],
          CLI.command_definition()
        )

      assert result.arguments.project == "MyProject"
      assert result.arguments.repo_id == "MyRepo"
      assert result.options.filter == "feature/"
    end

    test "parses workitems show with id" do
      {:ok, result} =
        CliMate.CLI.parse(~w(workitems show 42), CLI.command_definition())

      assert result.path == [:workitems, :show]
      assert result.arguments.id == 42
    end

    test "parses workitems create with type and title" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(workitems create MyProject --type Bug --title Fix-bug),
          CLI.command_definition()
        )

      assert result.arguments.project == "MyProject"
      assert result.options.type == "Bug"
      assert result.options.title == "Fix-bug"
    end

    test "parses workitems create with multiple options" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(workitems create MyProject --type Task --title "Task" --priority 2 --assigned-to Jane --tags frontend,ux),
          CLI.command_definition()
        )

      assert result.options.priority == 2
      assert result.options.assigned_to == "Jane"
      assert result.options.tags == "frontend,ux"
    end

    test "parses workitems update" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(workitems update 42 --state Resolved --priority 1),
          CLI.command_definition()
        )

      assert result.arguments.id == 42
      assert result.options.state == "Resolved"
      assert result.options.priority == 1
    end

    test "parses workitems query" do
      {:ok, result} =
        CliMate.CLI.parse(
          ["workitems", "query", "MyProject", "--wiql", "SELECT * FROM WorkItems"],
          CLI.command_definition()
        )

      assert result.arguments.project == "MyProject"
      assert result.options.wiql == "SELECT * FROM WorkItems"
    end

    test "parses pipelines list with top" do
      {:ok, result} =
        CliMate.CLI.parse(~w(pipelines list MyProject --top 10), CLI.command_definition())

      assert result.path == [:pipelines, :list]
      assert result.arguments.project == "MyProject"
      assert result.options.top == 10
    end

    test "parses pipelines run with branch and variables" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(pipelines run MyProject 5 --branch feature/x --variables ENV=staging),
          CLI.command_definition()
        )

      assert result.arguments.project == "MyProject"
      assert result.arguments.pipeline_id == 5
      assert result.options.branch == "feature/x"
      assert result.options.variables == "ENV=staging"
    end

    test "parses prs list with status filter" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(prs list MyProject MyRepo --status all),
          CLI.command_definition()
        )

      assert result.path == [:prs, :list]
      assert result.options.status == "all"
    end

    test "parses prs create" do
      {:ok, result} =
        CliMate.CLI.parse(
          [
            "prs",
            "create",
            "MyProject",
            "MyRepo",
            "--title",
            "New feature",
            "--source",
            "dev",
            "--target",
            "main",
            "--draft"
          ],
          CLI.command_definition()
        )

      assert result.path == [:prs, :create]
      assert result.arguments.project == "MyProject"
      assert result.arguments.repo_id == "MyRepo"
      assert result.options.title == "New feature"
      assert result.options.source == "dev"
      assert result.options.target == "main"
      assert result.options.draft == true
    end

    test "parses prs complete with merge strategy" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(prs complete MyProject MyRepo 42 --delete-source --merge-strategy squash),
          CLI.command_definition()
        )

      assert result.arguments.pr_id == 42
      assert result.options.delete_source == true
      assert result.options.merge_strategy == "squash"
    end

    test "parses prs abandon" do
      {:ok, result} =
        CliMate.CLI.parse(~w(prs abandon MyProject MyRepo 42), CLI.command_definition())

      assert result.path == [:prs, :abandon]
      assert result.arguments.pr_id == 42
    end

    test "parses releases list with definition-id" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(releases list MyProject --definition-id 1),
          CLI.command_definition()
        )

      assert result.options.definition_id == 1
    end

    test "parses releases show" do
      {:ok, result} =
        CliMate.CLI.parse(~w(releases show MyProject 42), CLI.command_definition())

      assert result.arguments.release_id == 42
    end

    test "parses login command (default browser method)" do
      {:ok, result} =
        CliMate.CLI.parse(~w(login --org myorg), CLI.command_definition())

      assert result.path == [:login]
      assert result.options.org == "myorg"
    end

    test "parses login with explicit pat method" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(login --method pat --org myorg --pat mytoken),
          CLI.command_definition()
        )

      assert result.options.method == "pat"
      assert result.options.pat == "mytoken"
    end

    test "parses login with device method" do
      {:ok, result} =
        CliMate.CLI.parse(~w(login --method device --org myorg), CLI.command_definition())

      assert result.options.method == "device"
    end

    test "parses login with self-hosted server" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(login --method pat --server https://ado.example.com --org Coll --pat xxx),
          CLI.command_definition()
        )

      assert result.options.server == "https://ado.example.com"
      assert result.options.org == "Coll"
    end

    test "parses logout" do
      {:ok, result} = CliMate.CLI.parse(~w(logout), CLI.command_definition())
      assert result.path == [:logout]
    end

    test "parses whoami" do
      {:ok, result} = CliMate.CLI.parse(~w(whoami), CLI.command_definition())
      assert result.path == [:whoami]
    end
  end

  describe "global options" do
    test "parses --org and --pat" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(--org myorg --pat mytoken projects list),
          CLI.command_definition()
        )

      assert result.options.org == "myorg"
      assert result.options.pat == "mytoken"
      assert result.path == [:projects, :list]
    end

    test "parses --server" do
      {:ok, result} =
        CliMate.CLI.parse(
          ~w(--server https://ado.example.com --org Coll projects list),
          CLI.command_definition()
        )

      assert result.options.server == "https://ado.example.com"
    end

    test "parses --json flag" do
      {:ok, result} =
        CliMate.CLI.parse(~w(--json projects list), CLI.command_definition())

      assert result.options.json == true
    end

    test "parses --verbose flag" do
      {:ok, result} =
        CliMate.CLI.parse(~w(--verbose projects list), CLI.command_definition())

      assert result.options.verbose == true
    end
  end
end
