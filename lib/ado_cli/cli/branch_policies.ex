defmodule AdoCli.CLI.BranchPolicies do
  @moduledoc false

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado repos policies",
      doc:
        "Manage branch policies that gate pull requests (build validation, required reviewers, status checks, etc.). A policy is a configuration object scoped to a specific branch and repository.",
      subcommands: [
        list: [
          name: "ado repos policies list",
          doc:
            "List branch policies in a repository as a table (ID, Type, Branch, Blocking, Enabled). Use --branch to filter to a single branch (e.g. main). Pass --json for the raw array.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [
            branch: [
              type: :string,
              doc:
                "Filter by branch name. Pass the full ref like 'refs/heads/main', or just 'main' (substring match)",
              doc_arg: "BRANCH"
            ]
          ],
          execute: &list_policies/1
        ],
        show: [
          name: "ado repos policies show",
          doc:
            "Show details of a single policy (ID, type, branch, repo, blocking, enabled, created date). Use `list` first to discover the policy ID.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            policy_id: [type: :integer, doc: "Numeric policy configuration ID (from `list`)"]
          ],
          execute: &show_policy/1
        ],
        create: [
          name: "ado repos policies create",
          doc:
            "Create a new branch policy. The policy type is identified by a UUID; common ones are: fa4e907d-c16b-4a4c-9dfa-4906e5d171dd (Build validation), fd2167ab-9d2a-4d8b-b2c9-1cdfbb6d4c34 (Required reviewers), 0609b952-1397-4640-95ec-e121a052fb4b (Status check).",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [
            type: [
              type: :string,
              required: true,
              doc:
                "Policy type UUID. Find these in the Azure DevOps UI under Project Settings > Repos > Policies > any policy > URL contains 'policyType='.",
              doc_arg: "TYPE_ID"
            ],
            branch: [
              type: :string,
              required: true,
              doc:
                "Target branch as a ref (e.g. 'refs/heads/main', 'refs/heads/feature/*' for wildcards)",
              doc_arg: "BRANCH"
            ],
            blocking: [
              type: :boolean,
              default: true,
              doc:
                "When true (default), PRs cannot be completed until the policy passes. When false, the policy is informational only."
            ]
          ],
          execute: &create_policy/1
        ],
        update: [
          name: "ado repos policies update",
          doc:
            "Modify an existing policy's blocking flag or enabled state. The policy type and scope are preserved from the existing policy.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            policy_id: [type: :integer, doc: "Numeric policy configuration ID"]
          ],
          options: [
            blocking: [
              type: :boolean,
              doc: "Set whether the policy blocks PR completion. Omit to keep current value."
            ],
            enabled: [
              type: :boolean,
              doc: "Set whether the policy is active. Omit to keep current value."
            ]
          ],
          execute: &update_policy/1
        ],
        delete: [
          name: "ado repos policies delete",
          doc:
            "Permanently remove a branch policy. The policy is removed from all branches it was scoped to (usually just one).",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            policy_id: [type: :integer, doc: "Numeric policy configuration ID"]
          ],
          execute: &delete_policy/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_policies(parsed) do
    %{project: project, repo_id: repo_id} = parsed.arguments
    params = %{}

    params =
      if branch = Map.get(parsed.options, :branch),
        do: Map.put(params, "branch", branch),
        else: params

    path =
      "/#{URI.encode(project)}/_apis/policy/configurations?repositoryId=#{URI.encode(repo_id)}"

    result = Client.list(path, params)

    Helpers.handle_api_result(result, parsed, fn policies ->
      Helpers.json_or_format(policies, parsed, &print_policies_table/1)
    end)
  end

  def show_policy(parsed) do
    %{project: project, policy_id: policy_id} = parsed.arguments
    path = "/#{URI.encode(project)}/_apis/policy/configurations/#{policy_id}"

    case Client.get(path) do
      {:ok, policy} -> Helpers.json_or_format(policy, parsed, &print_policy_detail/1)
      {:error, %{status: 404}} -> halt_error("Policy ##{policy_id} not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def create_policy(parsed) do
    %{project: project, repo_id: repo_id} = parsed.arguments
    type_id = Map.fetch!(parsed.options, :type)
    branch = Map.fetch!(parsed.options, :branch)
    blocking = Map.get(parsed.options, :blocking, true)

    body = %{
      "type" => %{"id" => type_id},
      "isBlocking" => blocking,
      "isEnabled" => true,
      "settings" => %{
        "scope" => [
          %{
            "repositoryId" => repo_id,
            "refName" => branch,
            "matchKind" => "Exact"
          }
        ]
      }
    }

    path = "/#{URI.encode(project)}/_apis/policy/configurations"

    case Client.post(path, body) do
      {:ok, policy} ->
        success("Policy ##{policy["id"]} created.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def update_policy(parsed) do
    %{project: project, policy_id: policy_id} = parsed.arguments
    path = "/#{URI.encode(project)}/_apis/policy/configurations/#{policy_id}"

    case Client.get(path) do
      {:ok, existing} ->
        body = %{
          "type" => existing["type"],
          "isBlocking" => Map.get(parsed.options, :blocking, existing["isBlocking"]),
          "isEnabled" => Map.get(parsed.options, :enabled, existing["isEnabled"]),
          "settings" => existing["settings"]
        }

        case Client.put(path, body) do
          {:ok, _policy} ->
            success("Policy ##{policy_id} updated.\n")
            halt_success("")

          error ->
            Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
        end

      {:error, %{status: 404}} ->
        halt_error("Policy ##{policy_id} not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_policy(parsed) do
    %{project: project, policy_id: policy_id} = parsed.arguments
    path = "/#{URI.encode(project)}/_apis/policy/configurations/#{policy_id}"

    case Client.delete(path) do
      :ok ->
        success("Policy ##{policy_id} deleted.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_policies_table(policies) do
    if Enum.empty?(policies) do
      writeln("No policies found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 6)}  #{String.pad_trailing("Type", 36)}  #{String.pad_trailing("Branch", 30)}  Blocking  Enabled"
      )

      writeln(String.duplicate("-", 100))

      Enum.each(policies, fn p ->
        type = p["type"]["displayName"] || p["type"]["id"] || ""
        scope = List.first(p["settings"]["scope"] || [])
        branch = (scope && scope["refName"]) || ""

        writeln(
          "#{String.pad_trailing(to_string(p["id"]), 6)}  #{String.pad_trailing(String.slice(type, 0, 34), 36)}  #{String.pad_trailing(branch, 30)}  #{p["isBlocking"]}         #{p["isEnabled"]}"
        )
      end)

      writeln("")
      writeln("#{length(policies)} policy(ies)")
    end
  end

  defp print_policy_detail(policy) do
    scope = List.first(policy["settings"]["scope"] || [])
    writeln("")
    success("Policy Details\n")
    writeln(String.duplicate("-", 60))
    writeln("  ID:        #{policy["id"]}")
    writeln("  Type:      #{policy["type"]["displayName"] || policy["type"]["id"]}")
    writeln("  Branch:    #{scope["refName"] || "(none)"}")
    writeln("  Repository:#{scope["repositoryId"] || "(none)"}")
    writeln("  Blocking:  #{policy["isBlocking"]}")
    writeln("  Enabled:   #{policy["isEnabled"]}")
    writeln("  Created:   #{policy["createdDate"]}")
    writeln("")
  end
end
