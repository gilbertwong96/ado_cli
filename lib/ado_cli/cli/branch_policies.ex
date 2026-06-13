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
      doc: "Manage branch policies (pull request, build, status, etc.).",
      subcommands: [
        list: [
          name: "ado repos policies list",
          doc: "List branch policies in a repository.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [branch: [type: :string, doc: "Filter by branch name", doc_arg: "BRANCH"]],
          execute: &list_policies/1
        ],
        show: [
          name: "ado repos policies show",
          doc: "Show details of a policy.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            policy_id: [type: :integer, doc: "Policy configuration ID"]
          ],
          execute: &show_policy/1
        ],
        create: [
          name: "ado repos policies create",
          doc: "Create a branch policy.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"]
          ],
          options: [
            type: [
              type: :string,
              required: true,
              doc: "Policy type id (UUID)",
              doc_arg: "TYPE_ID"
            ],
            branch: [
              type: :string,
              required: true,
              doc: "Target branch (e.g. refs/heads/main)",
              doc_arg: "BRANCH"
            ],
            blocking: [type: :boolean, default: true, doc: "Block pull request on policy failure"]
          ],
          execute: &create_policy/1
        ],
        update: [
          name: "ado repos policies update",
          doc: "Update a branch policy.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            policy_id: [type: :integer, doc: "Policy configuration ID"]
          ],
          options: [
            blocking: [type: :boolean, doc: "Block pull request on policy failure"],
            enabled: [type: :boolean, doc: "Enable or disable the policy"]
          ],
          execute: &update_policy/1
        ],
        delete: [
          name: "ado repos policies delete",
          doc: "Delete a branch policy.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            repo_id: [type: :string, doc: "Repository name or ID"],
            policy_id: [type: :integer, doc: "Policy configuration ID"]
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
