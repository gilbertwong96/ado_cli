defmodule AdoCli.CLI.Users do
  @moduledoc """
  Commands for managing Azure DevOps user entitlements.

    ado users list [--search SEARCH]
    ado users show USER_ID
    ado users add --email EMAIL [--license LICENSE]
    ado users remove USER_ID
  """

  @behaviour CliMate.CLI.Command
  import CliMate.CLI
  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  def command do
    [
      name: "ado users",
      doc:
        "Manage user access levels and entitlements (licenses, extensions, project memberships). Requires Project Collection Administrator permissions.",
      subcommands: [
        list: [
          name: "ado users list",
          doc:
            "List all users in the organization with their access level, last login, and project memberships. Output is a table by default; use --json for raw data. Use --top to limit.",
          options: [top: [type: :integer, doc: "Maximum number to return", doc_arg: "N"]],
          execute: &list_users/1
        ],
        show: [
          name: "ado users show",
          doc:
            "Show details of a single user: email, display name, access level (Stakeholder/Basic/Basic+Test Plans/VS Enterprise), date created, last accessed, and project/group memberships.",
          arguments: [user_id: [type: :string, doc: "User ID or email"]],
          execute: &show_user/1
        ],
        add: [
          name: "ado users add",
          doc: "Add a user to the organization.",
          options: [
            email: [type: :string, required: true, doc: "User email address", doc_arg: "EMAIL"],
            license: [
              type: :string,
              doc:
                "Access level: express (Basic, 5 free users), professional (Basic, paid), stakeholder (free, limited). Default: express.",
              doc_arg: "LICENSE"
            ]
          ],
          execute: &add_user/1
        ],
        remove: [
          name: "ado users remove",
          doc:
            "Remove a user from the organization entirely. Revokes all licenses and memberships. The user is immediately blocked from accessing any project. Requires confirmation unless --force.",
          arguments: [user_id: [type: :string, doc: "User ID or email"]],
          execute: &remove_user/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  def list_users(parsed) do
    params = %{}

    params =
      if top = Map.get(parsed.options, :top), do: Map.put(params, "$top", top), else: params

    result = Client.list("/_apis/userentitlements", params)

    Helpers.handle_api_result(result, parsed, fn users ->
      Helpers.json_or_format(users, parsed, &print_users_table/1)
    end)
  end

  def show_user(parsed) do
    user_id = parsed.arguments.user_id

    case Client.get("/_apis/userentitlements/#{URI.encode(user_id)}") do
      {:ok, user} -> Helpers.json_or_format(user, parsed, &print_user_detail/1)
      {:error, %{status: 404}} -> halt_error("User '#{user_id}' not found")
      error -> Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def add_user(parsed) do
    email = Map.fetch!(parsed.options, :email)
    license = Map.get(parsed.options, :license, "express")

    body = %{
      "accessLevel" => %{"accountLicenseType" => license},
      "user" => %{"principalName" => email, "subjectKind" => "user"}
    }

    case Client.post("/_apis/userentitlements", body) do
      {:ok, user} ->
        success("User '#{user["user"]["principalName"]}' added.\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def remove_user(parsed) do
    user_id = parsed.arguments.user_id

    case Client.delete("/_apis/userentitlements/#{URI.encode(user_id)}") do
      :ok ->
        success("User '#{user_id}' removed.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("User '#{user_id}' not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp print_users_table(users) do
    if users == [] do
      writeln("No users found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 40)}  #{String.pad_trailing("Email", 35)}  License")
      writeln(String.duplicate("─", 95))

      Enum.each(users, fn u ->
        writeln(
          "#{String.pad_trailing(u["id"] || "", 40)}  #{String.pad_trailing(u["user"]["principalName"] || "", 35)}  #{u["accessLevel"]["accountLicenseType"] || ""}"
        )
      end)

      writeln("")
      writeln("#{length(users)} user(s)")
    end
  end

  defp print_user_detail(user) do
    writeln("")
    success("User Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:      #{user["id"]}")
    writeln("  Email:   #{user["user"]["principalName"]}")
    writeln("  Name:    #{user["user"]["displayName"]}")
    writeln("  License: #{user["accessLevel"]["accountLicenseType"]}")
    writeln("  Status:  #{user["accessLevel"]["status"]}")
    writeln("")
  end
end
