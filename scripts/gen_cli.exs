#!/usr/bin/env elixir
# Simple test generator that produces working tests.
# Run: mix run scripts/gen_cli.exs

defmodule GenCli do
  @moduledoc """
  Generates test files for all CLI command modules. Uses a simpler
  approach than the previous generator — just direct string templating.
  """
  @test_dir "test/ado_cli/cli"

  # Per-module specs: list of {function_name, http_method, path_template, args_template}
  @specs %{
    "AdoCli.CLI.Areas" => [
      {"list_areas", :get, "/test/_apis/wit/classificationnodes", "%{options: %{json: true}}"},
      {"show_area", :get, "/test/_apis/wit/classificationnodes/1", "%{options: %{json: true}, arguments: %{id: 1, project: \"test\"}}"},
      {"create_area", :post, "/test/_apis/wit/classificationnodes", "%{options: %{json: true, name: \"test\"}, arguments: %{project: \"test\"}}"},
      {"update_area", :patch, "/test/_apis/wit/classificationnodes/1", "%{options: %{json: true, name: \"new\"}, arguments: %{id: 1, project: \"test\"}}"},
      {"delete_area", :delete, "/test/_apis/wit/classificationnodes/1", "%{options: %{json: true, force: false}, arguments: %{id: 1, project: \"test\"}}"}
    ],
    "AdoCli.CLI.Banners" => [
      {"show_banner", :get, "/_apis/notification/banners", "%{options: %{json: true}}"},
      {"set_banner", :put, "/_apis/notification/banners", "%{options: %{json: true, message: \"test\"}}"},
      {"delete_banner", :delete, "/_apis/notification/banners", "%{options: %{json: true, force: false}}"}
    ],
    "AdoCli.CLI.BranchPolicies" => [
      {"list_policies", :get, "/test/_apis/policy/configurations", "%{options: %{json: true, ref_name: nil}, arguments: %{project: \"test\", repo_id: \"repo\"}}"},
      {"show_policy", :get, "/test/_apis/policy/configurations/1", "%{options: %{json: true}, arguments: %{project: \"test\", repo_id: \"repo\", policy_id: 1}}"},
      {"create_policy", :post, "/test/_apis/policy/configurations", "%{options: %{json: true, type: \"required reviewers\", settings: %{}, ref_name: \"main\"}, arguments: %{project: \"test\", repo_id: \"repo\"}}"},
      {"update_policy", :put, "/test/_apis/policy/configurations/1", "%{options: %{json: true, is_enabled: true, is_blocking: false, settings: %{}}, arguments: %{project: \"test\", repo_id: \"repo\", policy_id: 1}}"},
      {"delete_policy", :delete, "/test/_apis/policy/configurations/1", "%{options: %{json: true, force: false}, arguments: %{project: \"test\", repo_id: \"repo\", policy_id: 1}}"}
    ],
    "AdoCli.CLI.Builds" => [
      {"list_builds", :get, "/test/_apis/build/builds", "%{options: %{json: true, top: nil, definitions: nil, branch: nil}, arguments: %{project: \"test\"}}"},
      {"show_build", :get, "/test/_apis/build/builds/1", "%{options: %{json: true}, arguments: %{project: \"test\", build_id: 1}}"},
      {"queue_build", :post, "/test/_apis/build/builds", "%{options: %{json: true, definition_id: 1, source_branch: nil, parameters: nil}, arguments: %{project: \"test\"}}"},
      {"cancel_build", :patch, "/test/_apis/build/builds/1", "%{options: %{json: true}, arguments: %{project: \"test\", build_id: 1}}"},
      {"list_tags", :get, "/test/_apis/build/builds/1/tags", "%{options: %{json: true}, arguments: %{project: \"test\", build_id: 1}}"},
      {"add_tags", :put, "/test/_apis/build/builds/1/tags", "%{options: %{json: true, tags: \"tag1,tag2\"}, arguments: %{project: \"test\", build_id: 1}}"},
      {"list_definitions", :get, "/test/_apis/build/definitions", "%{options: %{json: true, top: nil, name: nil, path: nil}, arguments: %{project: \"test\"}}"}
    ],
    "AdoCli.CLI.Connections" => [
      {"list_connections", :get, "/test/_apis/serviceendpoint/endpoints", "%{options: %{json: true, top: nil, type: nil, include_details: false}, arguments: %{project: \"test\"}}"},
      {"show_connection", :get, "/test/_apis/serviceendpoint/endpoints/1", "%{options: %{json: true, include_details: false}, arguments: %{project: \"test\", connection_id: 1}}"}
    ],
    "AdoCli.CLI.Extensions" => [
      {"list_extensions", :get, "/_apis/extensionmanagement/installedextensions", "%{options: %{json: true, top: nil, include_disabled: false}}"},
      {"show_extension", :get, "/_apis/extensionmanagement/installedextensions/1", "%{options: %{json: true}, arguments: %{ext: \"1\"}}"},
      {"install_extension", :post, "/_apis/extensionmanagement/installedextensions", "%{options: %{json: true, publisher: \"ms\", extension: \"vss-services\"}}"},
      {"uninstall_extension", :delete, "/_apis/extensionmanagement/installedextensions/1", "%{options: %{json: true, force: false, reason: \"test\"}, arguments: %{publisher: \"ms\", name: \"vss-services\"}}"},
      {"enable_extension", :patch, "/_apis/extensionmanagement/installedextensions/1", "%{options: %{json: true}, arguments: %{publisher: \"ms\", name: \"vss-services\"}}"},
      {"disable_extension", :patch, "/_apis/extensionmanagement/installedextensions/1", "%{options: %{json: true, reason: \"test\"}, arguments: %{publisher: \"ms\", name: \"vss-services\"}}"}
    ],
    "AdoCli.CLI.Folders" => [
      {"list_folders", :get, "/test/_apis/pipelines/folders", "%{options: %{json: true, top: nil, query: nil}, arguments: %{project: \"test\"}}"},
      {"create_folder", :post, "/test/_apis/pipelines/folders", "%{options: %{json: true, path: \"/test\"}, arguments: %{project: \"test\"}}"},
      {"delete_folder", :delete, "/test/_apis/pipelines/folders/test", "%{options: %{json: true, force: false}, arguments: %{project: \"test\", path: \"test\"}}"}
    ],
    "AdoCli.CLI.Imports" => [
      {"list_imports", :get, "/test/_apis/git/importRequests", "%{options: %{json: true, top: nil, include_abandoned: false}, arguments: %{project: \"test\"}}"},
      {"show_import", :get, "/test/_apis/git/importRequests/1", "%{options: %{json: true}, arguments: %{project: \"test\", import_id: 1}}"},
      {"create_import", :post, "/test/_apis/git/importRequests", "%{options: %{json: true, source: \"github\", endpoint: \"https://api.github.com\", repository: \"repo\"}, arguments: %{project: \"test\"}}"}
    ],
    "AdoCli.CLI.Iterations" => [
      {"list_iterations", :get, "/test/_apis/work/teamsettings/iterations", "%{options: %{json: true, top: nil}, arguments: %{project: \"test\", team: \"Default Team\"}}"},
      {"show_iteration", :get, "/test/_apis/work/teamsettings/iterations/1", "%{options: %{json: true}, arguments: %{project: \"test\", team: \"Default Team\", iteration_id: \"1\"}}"},
      {"create_iteration", :post, "/test/_apis/work/teamsettings/iterations", "%{options: %{json: true, name: \"Sprint 1\", start_date: \"2024-01-01\", finish_date: \"2024-01-14\"}, arguments: %{project: \"test\", team: \"Default Team\"}}"},
      {"update_iteration", :patch, "/test/_apis/work/teamsettings/iterations/1", "%{options: %{json: true, name: \"Sprint 1 Updated\"}, arguments: %{project: \"test\", team: \"Default Team\", iteration_id: \"1\"}}"},
      {"delete_iteration", :delete, "/test/_apis/work/teamsettings/iterations/1", "%{options: %{json: true, force: false}, arguments: %{project: \"test\", team: \"Default Team\", iteration_id: \"1\"}}"}
    ],
    "AdoCli.CLI.Packages" => [
      {"list_packages", :get, "/test/_apis/packaging/feeds", "%{options: %{json: true, top: nil}, arguments: %{project: \"test\"}}"},
      {"list_versions", :get, "/test/_apis/packaging/feeds/1/packages", "%{options: %{json: true, top: nil}, arguments: %{project: \"test\", feed_id: 1}}"},
      {"show_package", :get, "/test/_apis/packaging/feeds/1/packages/1", "%{options: %{json: true}, arguments: %{project: \"test\", feed_id: 1, package_id: \"1\"}}"}
    ],
    "AdoCli.CLI.PullRequests" => [
      {"list_prs", :get, "/test/_apis/git/repositories/test/pullrequests", "%{options: %{json: true, top: nil, status: nil, creator: nil, reviewer: nil, source: nil, target: nil}, arguments: %{project: \"test\", repo_id: \"test\"}}"},
      {"show_pr", :get, "/test/_apis/git/repositories/test/pullrequests/1", "%{options: %{json: true, include_commits: false, include_work_item_refs: false}, arguments: %{project: \"test\", repo_id: \"test\", pr_id: 1}}"},
      {"create_pr", :post, "/test/_apis/git/repositories/test/pullrequests", "%{options: %{json: true, title: \"Test\", description: nil, source: \"refs/heads/feature\", target: \"refs/heads/main\", draft: false, work_items: nil, reviewers: nil, labels: nil}, arguments: %{project: \"test\", repo_id: \"test\"}}"},
      {"complete_pr", :patch, "/test/_apis/git/repositories/test/pullrequests/1", "%{options: %{json: true, delete_source: false, merge_strategy: \"noFastForward\", merge_message: nil, squashed: false, bypass_policy: false, transition_work_items: false}, arguments: %{project: \"test\", repo_id: \"test\", pr_id: 1, completion_options: %{}}}"}
    ],
    "AdoCli.CLI.Releases" => [
      {"list_releases", :get, "/test/_apis/release/releases", "%{options: %{json: true, top: nil, definition_id: nil}, arguments: %{project: \"test\"}}"},
      {"show_release", :get, "/test/_apis/release/releases/1", "%{options: %{json: true, include_artifacts: false, expand: \"none\"}, arguments: %{project: \"test\", release_id: 1}}"}
    ],
    "AdoCli.CLI.Repos" => [
      {"list_repos", :get, "/test/_apis/git/repositories", "%{options: %{json: true, top: nil, include_links: false, include_all_urls: false}, arguments: %{project: \"test\"}}"},
      {"show_repo", :get, "/test/_apis/git/repositories/test", "%{options: %{json: true, include_links: false}, arguments: %{project: \"test\", repo_id: \"test\"}}"},
      {"list_branches", :get, "/test/_apis/git/repositories/test/refs", "%{options: %{json: true, top: nil, filter: nil}, arguments: %{project: \"test\", repo_id: \"test\"}}"},
      {"create_repo", :post, "/test/_apis/git/repositories", "%{options: %{json: true, name: \"new-repo\", default_branch: nil, parent_repo: nil}, arguments: %{project: \"test\"}}"},
      {"delete_repo", :delete, "/test/_apis/git/repositories/test", "%{options: %{json: true, force: false}, arguments: %{project: \"test\", repo_id: \"test\"}}"}
    ],
    "AdoCli.CLI.Security" => [
      {"list_groups", :get, "/_apis/graph/groups", "%{options: %{json: true, top: nil, scope_descriptor: nil, subject_types: nil}}"},
      {"show_group", :get, "/_apis/graph/groups/1", "%{options: %{json: true, expand: false}, arguments: %{group_id: \"1\"}}"},
      {"create_group", :post, "/_apis/graph/groups", "%{options: %{json: true, display_name: \"New Group\", description: nil, scope_descriptor: \"scp\"}}"},
      {"delete_group", :delete, "/_apis/graph/groups/1", "%{options: %{json: true, force: false}, arguments: %{group_id: \"1\"}}"},
      {"list_members", :get, "/_apis/graph/groups/1/memberships", "%{options: %{json: true, top: nil}, arguments: %{group_id: \"1\"}}"},
      {"list_namespaces", :get, "/_apis/securitynamespaces", "%{options: %{json: true, top: nil, local_only: false}}"},
      {"list_permissions", :get, "/_apis/securitynamespaces/2/permissions", "%{options: %{json: true, top: nil}, arguments: %{namespace_id: \"2\"}}"}
    ],
    "AdoCli.CLI.Teams" => [
      {"list_teams", :get, "/test/_apis/teams", "%{options: %{json: true, top: nil, mine: false, expand_identity: false, subject_types: nil}, arguments: %{project: \"test\"}}"},
      {"show_team", :get, "/test/_apis/teams/1", "%{options: %{json: true, expand_identity: false}, arguments: %{project: \"test\", team_id: \"1\"}}"},
      {"create_team", :post, "/test/_apis/teams", "%{options: %{json: true, name: \"New Team\", description: nil}, arguments: %{project: \"test\"}}"},
      {"update_team", :patch, "/test/_apis/teams/1", "%{options: %{json: true, name: \"Updated Team\", description: nil}, arguments: %{project: \"test\", team_id: \"1\"}}"},
      {"delete_team", :delete, "/test/_apis/teams/1", "%{options: %{json: true, force: false}, arguments: %{project: \"test\", team_id: \"1\"}}"},
      {"list_team_members", :get, "/test/_apis/teams/1/members", "%{options: %{json: true, top: nil}, arguments: %{project: \"test\", team_id: \"1\"}}"}
    ],
    "AdoCli.CLI.Users" => [
      {"list_users", :get, "/_apis/identities", "%{options: %{json: true, top: nil, filter: nil, subject_types: nil}}"},
      {"show_user", :get, "/_apis/identities/1", "%{options: %{json: true}, arguments: %{user_id: \"1\"}}"},
      {"add_user", :post, "/_apis/identities", "%{options: %{json: true, descriptor: \"vssgp.Uy0xLTkt\"}}"},
      {"remove_user", :delete, "/_apis/identities/1", "%{options: %{json: true, force: false}, arguments: %{user_id: \"1\"}}"}
    ],
    "AdoCli.CLI.Wikis" => [
      {"list_wikis", :get, "/test/_apis/wiki/wikis", "%{options: %{json: true, top: nil}, arguments: %{project: \"test\"}}"},
      {"show_wiki", :get, "/test/_apis/wiki/wikis/1", "%{options: %{json: true}, arguments: %{project: \"test\", wiki_id: \"1\"}}"},
      {"list_pages", :get, "/test/_apis/wiki/wikis/1/pages", "%{options: %{json: true, top: nil, path: nil, recursion_level: nil}, arguments: %{project: \"test\", wiki_id: \"1\"}}"},
      {"show_page", :get, "/test/_apis/wiki/wikis/1/pages/test", "%{options: %{json: true, include_content: true, recursion_level: \"full\"}, arguments: %{project: \"test\", wiki_id: \"1\", path: \"test\"}}"},
      {"create_page", :put, "/test/_apis/wiki/wikis/1/pages", "%{options: %{json: true, path: \"/test\", content: \"test\"}, arguments: %{project: \"test\", wiki_id: \"1\"}}"},
      {"update_page", :patch, "/test/_apis/wiki/wikis/1/pages", "%{options: %{json: true, path: \"/test\", content: \"updated\"}, arguments: %{project: \"test\", wiki_id: \"1\"}}"}
    ],
    "AdoCli.CLI.Logout" => [
      {"run", :noop, "", "%{}"}
    ],
    "AdoCli.CLI.Whoami" => [
      {"run", :noop, "", "%{}"}
    ]
  }

  def run do
    File.mkdir_p!(@test_dir)

    for {module, specs} <- @specs do
      if File.exists?(test_path(module)) do
        IO.puts("  Skipping #{Path.basename(test_path(module))} (exists)")
      else
        generate(module, specs)
      end
    end
  end

  defp test_path(module) do
    base = module |> String.split(".") |> List.last() |> Macro.underscore()
    "#{@test_dir}/#{base}_test.exs"
  end

  defp generate(module, specs) do
    content = render(module, specs)
    File.write!(test_path(module), content)
    IO.puts("  Generated #{Path.basename(test_path(module))} (#{length(specs)} subcommands)")
  end

  defp render(module, specs) do
    tests_block = Enum.map_join(specs, "\n\n", fn spec -> render_spec(module, spec) end)

    """
    defmodule #{module}Test do
      use AdoCli.CLI.TestHelper
      alias #{module}

    #{tests_block}
    end
    """
  end

  defp render_spec(_module, {fn_name, method, path, args}) when method == :noop do
    """
        describe "#{fn_name}" do
          test "halts 0 on success" do
            apply(#{noop_module(fn_name)}, :#{fn_name}, [#{args}])
            assert_receive {:cli_mate_shell, :halt, 0}, 500
          end
        end
    """
  end

  defp noop_module("run"), do: "AdoCli.CLI.Logout"
  defp noop_module(_), do: "AdoCli.CLI.Whoami"

  defp render_spec(module, {fn_name, method, path, args}) do
    method_str =
      case method do
        :get -> "expect_success_json"
        :post -> "expect_post_success"
        :put -> "expect_put_success"
        :patch -> "expect_patch_success"
        :delete -> "expect_delete_success"
      end

    body_arg =
      case method do
        :get -> ", ~s({\"value\":[]})"
        m when m in [:post, :put, :patch] -> ~s(, "", "{\\"id\\":1}")
        :delete -> ""
      end

    """
        describe "#{fn_name}" do
          test "halts 0 on successful #{method}", %{server: server} do
            #{method_str}(server, "#{path}"#{body_arg}, fn ->
              apply(#{module}, :#{fn_name}, [#{args}])
            end)
          end

          test "halts 1 on API error", %{server: server} do
            expect_api_error(server, "#{path}", 500, "{}", fn ->
              apply(#{module}, :#{fn_name}, [#{args}])
            end)
          end
        end
    """
  end
end

GenCli.run()
