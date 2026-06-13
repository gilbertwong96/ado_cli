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
      doc: "Manage security groups and permissions.",
      subcommands: [
        groups: [
          name: "ado security groups",
          doc: "Manage security groups.",
          subcommands: [
            list: [
              name: "ado security groups list",
              doc: "List security groups in a project.",
              arguments: [project: [type: :string, doc: "Project name or ID"]],
              execute: &list_groups/1
            ],
            show: [
              name: "ado security groups show",
              doc: "Show details of a security group.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                group_id: [type: :string, doc: "Group ID"]
              ],
              execute: &show_group/1
            ],
            create: [
              name: "ado security groups create",
              doc: "Create a security group.",
              arguments: [project: [type: :string, doc: "Project name or ID"]],
              options: [
                name: [type: :string, required: true, doc: "Group name", doc_arg: "NAME"],
                description: [type: :string, doc: "Group description", doc_arg: "DESC"]
              ],
              execute: &create_group/1
            ],
            delete: [
              name: "ado security groups delete",
              doc: "Delete a security group.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                group_id: [type: :string, doc: "Group ID"]
              ],
              execute: &delete_group/1
            ],
            members: [
              name: "ado security groups members",
              doc: "Manage group memberships.",
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
end
