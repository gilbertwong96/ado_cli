defmodule AdoCli.CLI.Security do
  @moduledoc """
  Commands for managing Azure DevOps security groups and memberships.

    ado security groups list PROJECT
    ado security groups show PROJECT GROUP_ID
    ado security groups create PROJECT --name NAME [--description DESC]
    ado security groups delete PROJECT GROUP_ID
    ado security groups members list PROJECT GROUP_ID
  """

  @behaviour CliMate.CLI.Command
  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado security",
      doc:
        "Manage Azure DevOps security groups and access control lists (ACLs). Groups control who has access; permissions control what they can do. Requires Project Administrator permissions.",
      subcommands: [
        groups: [
          name: "ado security groups",
          doc:
            "Create, list, show, and delete security groups. Groups are collections of users and nested groups. Use members subcommand to add/remove members.",
          subcommands: [
            list: [
              name: "ado security groups list",
              doc:
                "List all security groups in a project. Output is a table (Name, Descriptor). Includes built-in groups (Readers, Contributors, Build Administrators) and custom groups. Use --json for raw data.",
              arguments: [project: [type: :string, doc: "Project name or ID"]],
              execute: &list_groups/1
            ],
            show: [
              name: "ado security groups show",
              doc:
                "Show a single security group: name, descriptor, and domain. The descriptor is the canonical identifier used by permissions APIs.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                group_id: [type: :string, doc: "Group ID"]
              ],
              execute: &show_group/1
            ],
            create: [
              name: "ado security groups create",
              doc:
                "Create a new project-scoped security group. Returns the group descriptor, which you use for membership and permission operations. Groups are visible in the web UI under Project Settings > Permissions.",
              arguments: [project: [type: :string, doc: "Project name or ID"]],
              options: [
                name: [
                  type: :string,
                  required: true,
                  doc:
                    "Display name for the group. Must be unique within the project. Use descriptive names like Release Managers or Code Reviewers.",
                  doc_arg: "NAME"
                ],
                description: [
                  type: :string,
                  doc: "Optional description shown in the group list and settings.",
                  doc_arg: "DESC"
                ]
              ],
              execute: &create_group/1
            ],
            delete: [
              name: "ado security groups delete",
              doc:
                "Permanently delete a security group. Members are not removed from the org, just the group is deleted. Requires confirmation.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                group_id: [type: :string, doc: "Group ID"]
              ],
              execute: &delete_group/1
            ],
            members: [
              name: "ado security groups members",
              doc: "Manage group memberships (who is in a security group).",
              subcommands: [
                list: [
                  name: "ado security groups members list",
                  doc: "List members of a security group.",
                  arguments: [
                    project: [type: :string, doc: "Project name or ID"],
                    group_id: [type: :string, doc: "Group ID"]
                  ],
                  execute: &list_members/1
                ]
              ]
            ]
          ]
        ],
        permissions: [
          name: "ado security permissions",
          doc:
            "List permissions (ACL entries) for a security namespace. Permissions map identity descriptors to granted/denied permission bits. Editing ACLs is advanced; prefer the web UI.",
          subcommands: [
            list: [
              name: "ado security permissions list",
              doc:
                "Show all ACL entries for a given namespace and optional token. Output is a table (Identity descriptor, Allow bits, Deny bits). Use --token to scope to a specific resource (e.g. a repo).",
              arguments: [
                namespace_id: [
                  type: :string,
                  doc:
                    "Security namespace GUID. Find these with the namespaces command. Common: Git Repositories namespace."
                ]
              ],
              options: [
                token: [
                  type: :string,
                  doc:
                    "Resource-specific path for the ACL. For repo permissions: repoV2/<projectId>/<repoId>. Omit to list namespace-level permissions.",
                  doc_arg: "TOKEN"
                ]
              ],
              execute: &list_permissions/1
            ],
            namespaces: [
              name: "ado security permissions namespaces",
              doc:
                "List all security namespaces in the organization (e.g. Git Repositories, Build, Release, Analytics). Each namespace has a GUID; use it with permissions list.",
              execute: &list_namespaces/1
            ]
          ]
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_groups(parsed) do
    project = parsed.arguments.project
    result = Client.list("/_apis/graph/groups", %{"scopeDescriptor" => "scp.#{project}"})

    Helpers.handle_api_result(result, parsed, fn groups ->
      Helpers.json_or_format(groups, parsed, &print_groups_table/1)
    end)
  end

  def show_group(parsed) do
    project = parsed.arguments.project
    group_id = parsed.arguments.group_id

    case Client.get("/_apis/graph/groups/#{URI.encode(group_id)}", %{
           "scopeDescriptor" => "scp.#{project}"
         }) do
      {:ok, group} -> Helpers.json_or_format(group, parsed, &print_group_detail/1)
      {:error, %{status: 404}} -> halt_error("Group '#{group_id}' not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def create_group(parsed) do
    project = parsed.arguments.project
    name = Map.fetch!(parsed.options, :name)
    body = %{"displayName" => name}

    body =
      if desc = Map.get(parsed.options, :description),
        do: Map.put(body, "description", desc),
        else: body

    case Client.post("/_apis/graph/groups", body, %{"scopeDescriptor" => "scp.#{project}"}) do
      {:ok, group} ->
        success("Group '#{group["displayName"]}' created.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_group(parsed) do
    project = parsed.arguments.project
    group_id = parsed.arguments.group_id

    case Client.delete("/_apis/graph/groups/#{URI.encode(group_id)}", %{
           "scopeDescriptor" => "scp.#{project}"
         }) do
      :ok ->
        success("Group deleted.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Group '#{group_id}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def list_members(parsed) do
    project = parsed.arguments.project
    group_id = parsed.arguments.group_id

    result =
      Client.list("/_apis/graph/groups/#{URI.encode(group_id)}/memberships", %{
        "scopeDescriptor" => "scp.#{project}"
      })

    Helpers.handle_api_result(result, parsed, fn members ->
      Helpers.json_or_format(members, parsed, &print_members_table/1)
    end)
  end

  defp print_groups_table(groups) do
    if Enum.empty?(groups) do
      writeln("No groups found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 40)} #{String.pad_trailing("Display Name", 30)} Description"
      )

      writeln(String.duplicate("─", 95))

      Enum.each(groups, fn g ->
        writeln(
          "#{String.pad_trailing(g["descriptor"] || "", 40)} #{String.pad_trailing(g["displayName"] || "", 30)} #{String.slice(g["description"] || "", 0, 30)}"
        )
      end)

      writeln("")
      writeln("#{length(groups)} group(s)")
    end
  end

  defp print_group_detail(group) do
    writeln("")
    success("Security Group Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  Descriptor:  #{group["descriptor"]}")
    writeln("  Name:        #{group["displayName"]}")
    writeln("  Description: #{group["description"] || "(none)"}")
    writeln("  Domain:      #{group["domain"]}")
    writeln("")
  end

  defp print_members_table(members) do
    if Enum.empty?(members) do
      writeln("No members found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 40)} #{String.pad_trailing("Display Name", 30)} Type")
      writeln(String.duplicate("─", 90))

      Enum.each(members, fn m ->
        mi = m["memberDescriptor"] || m["id"] || ""
        dn = m["displayName"] || ""
        st = m["subjectType"] || m["memberType"] || ""
        writeln("#{String.pad_trailing(mi, 40)} #{String.pad_trailing(dn, 30)} #{st}")
      end)

      writeln("")
      writeln("#{length(members)} member(s)")
    end
  end

  # ── Permissions ────────────────────────────────────────────────────

  def list_namespaces(parsed) do
    result = Client.list("/_apis/securitynamespaces")

    Helpers.handle_api_result(result, parsed, fn ns ->
      Helpers.json_or_format(ns, parsed, fn namespaces ->
        writeln("")
        writeln("#{String.pad_trailing("ID", 40)}  Name")
        writeln(String.duplicate("─", 80))

        Enum.each(namespaces, fn n ->
          writeln("#{String.pad_trailing(n["namespaceId"] || "", 40)}  #{n["name"]}")
        end)

        writeln("")
        writeln("#{length(namespaces)} namespace(s)")
        halt_success("")
      end)
    end)
  end

  def list_permissions(parsed) do
    ns_id = parsed.arguments.namespace_id
    token = Map.get(parsed.options, :token, "")
    path = "/_apis/permissions/#{URI.encode(ns_id)}/#{URI.encode(token)}"

    case Client.get(path, %{"api-version" => "7.1"}) do
      {:ok, aces} ->
        Helpers.json_or_format(aces, parsed, fn perm ->
          writeln("")
          writeln("#{String.pad_trailing("Identity", 36)}  Allow  Deny")
          writeln(String.duplicate("─", 60))
          acl = perm["acesDictionary"] || %{}

          Enum.each(acl, fn {_k, ace} ->
            id = ace["descriptor"] || ""
            allow = ace["allow"] || 0
            deny = ace["deny"] || 0
            writeln("#{String.pad_trailing(String.slice(id, 0, 34), 36)}  #{allow}     #{deny}")
          end)

          writeln("")
          halt_success("")
        end)

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end
end
