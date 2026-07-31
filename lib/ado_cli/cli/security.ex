defmodule AdoCli.CLI.Security do
  @moduledoc """
  Commands for granting and revoking Azure DevOps security permissions.

  Currently scoped to a single use case: toggling the `ViewSecrets` bit
  on the project's Library namespace for the **calling user only**.
  This is the workaround when the auto-elevation flow in
  `ado pipelines secure_files download` is blocked by missing token
  scopes (e.g. browser-OAuth tokens, MSA-backed accounts).

  ⚠️ Use with care. Granting `ViewSecrets` to a user lets anyone
  authenticating as that user download every Secure File in the
  project directly, bypassing per-pipeline approval. Prefer the
  ephemeral auto-elevation path when it works.

    # Permanent grant (must be explicitly typed --yes-this-mutates-secret-read)
    ado security grant --project MyProject --permission ViewSecrets --yes-this-mutates-secret-read

    # Revert when done
    ado security revoke --project MyProject --permission ViewSecrets --yes-this-mutates-secret-read
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.Client

  # Library security namespace GUID (per Microsoft.Security namespace reference)
  @library_namespace_id "b7e84409-6553-448a-bbb2-af228e07cbeb"

  # Bit values (per terraform-provider-azuredevops / Microsoft.Security reference)
  @bit_view_secrets 8

  @source_url "https://github.com/gilbertwong96/ado_cli"

  @impl true
  def command do
    [
      name: "ado security",
      doc:
        "Manage Azure DevOps security permissions on the caller identity. " <>
          "Currently supports toggling the Library 'ViewSecrets' bit for the calling user only. " <>
          "Use this as a workaround when the auto-elevation in 'ado pipelines secure_files download' is unavailable.",
      subcommands: [
        grant: [
          name: "ado security grant",
          doc:
            "Permanently grant the calling user the 'ViewSecrets' permission on the Library namespace for the given project. " <>
              "Allows downloading Secure Files without per-download elevation. " <>
              "Requires the confirmation flag --yes-this-mutates-secret-read.",
          arguments: [
            project_name_or_id: [type: :string, doc: "Project name or ID"]
          ],
          options: [
            permission: [
              type: :string,
              default: "ViewSecrets",
              doc: "Permission name (currently only ViewSecrets is supported)"
            ],
            yes_this_mutates_secret_read: [
              type: :boolean,
              default: false,
              doc:
                "Required safety flag. Without it, the command refuses to run. The verbose name is intentional — typing 'y' to a prompt should never be enough to grant permanent secret-read permission."
            ]
          ],
          execute: &grant/1
        ],
        revoke: [
          name: "ado security revoke",
          doc:
            "Revoke a permission previously granted with 'ado security grant'. " <>
              "Restores the Library namespace's viewSecrets bit to OFF for the calling user on the given project. " <>
              "Requires the confirmation flag --yes-this-mutates-secret-read.",
          arguments: [
            project_name_or_id: [type: :string, doc: "Project name or ID"]
          ],
          options: [
            permission: [
              type: :string,
              default: "ViewSecrets",
              doc: "Permission name (currently only ViewSecrets is supported)"
            ],
            yes_this_mutates_secret_read: [
              type: :boolean,
              default: false,
              doc: "Required safety flag. Without it, the command refuses to run."
            ]
          ],
          execute: &revoke/1
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.(parsed))

  # ─────────────────────────────────────────────────────────────────────
  # grant
  # ─────────────────────────────────────────────────────────────────────────

  def grant(parsed) do
    project = parsed.arguments.project_name_or_id
    permission = Map.fetch!(parsed.options, :permission)
    confirmed? = Map.fetch!(parsed.options, :yes_this_mutates_secret_read)

    with :ok <- validate_inputs(project, permission, confirmed?),
         {:ok, project_id} <- resolve_project_id(project),
         {:ok, descriptor} <- fetch_caller_descriptor(),
         :ok <- modify_library_bit(project_id, descriptor, @bit_view_secrets, true) do
      success(
        "Granted '#{permission}' on Library namespace for project '#{project}' (#{project_id}) " <>
          "to the calling user. You can now download Secure Files without elevation. " <>
          "Revoke later with 'ado security revoke --project #{project} --permission #{permission} --yes-this-mutates-secret-read'."
      )

      halt_success("")
    else
      {:error, reason} ->
        halt_error(reason)
    end
  end

  def revoke(parsed) do
    project = parsed.arguments.project_name_or_id
    permission = Map.fetch!(parsed.options, :permission)
    confirmed? = Map.fetch!(parsed.options, :yes_this_mutates_secret_read)

    with :ok <- validate_inputs(project, permission, confirmed?),
         {:ok, project_id} <- resolve_project_id(project),
         {:ok, descriptor} <- fetch_caller_descriptor(),
         :ok <- modify_library_bit(project_id, descriptor, @bit_view_secrets, false) do
      success(
        "Revoked '#{permission}' on Library namespace for project '#{project}' (#{project_id}) for the calling user."
      )

      halt_success("")
    else
      {:error, reason} ->
        halt_error(reason)
    end
  end

  # ─────────────────────────────────────────────────────────────────────
  # Validators
  # ─────────────────────────────────────────────────────────────────────

  defp validate_inputs(project, permission, confirmed?) do
    cond do
      not confirmed? ->
        {:error,
         "Refusing to run without the safety flag. " <>
           "Re-run with --yes-this-mutates-secret-read to confirm."}

      is_nil(project) or project == "" ->
        {:error, "Project argument is required."}

      permission != "ViewSecrets" ->
        {:error,
         "Unsupported permission '#{permission}'. Currently only 'ViewSecrets' is supported. " <>
           "Patches welcome at #{@source_url}."}

      true ->
        :ok
    end
  end

  defp resolve_project_id(project_name_or_id) do
    # If it looks like a valid UUID, use it directly. Otherwise, query
    # the projects list and find by name. The regex is strict (8-4-4-4-12)
    # so that 36 chars of dashes or random hex/dash strings don't slip through.
    uuid? =
      String.match?(
        project_name_or_id,
        ~r/^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/
      )

    if uuid? do
      {:ok, project_name_or_id}
    else
      case Client.get("/_apis/projects", %{"api-version" => "7.1"}) do
        {:ok, %{"value" => projects}} when is_list(projects) ->
          case Enum.find(projects, &(&1["name"] == project_name_or_id)) do
            %{"id" => id} -> {:ok, id}
            nil -> {:error, "Project '#{project_name_or_id}' not found."}
          end

        err ->
          {:error, "Failed to look up project ID: #{inspect(err)}"}
      end
    end
  end

  defp fetch_caller_descriptor do
    case Client.get("/_apis/connectionData", %{"api-version" => "7.1-preview.1"}) do
      {:ok, %{"authenticatedUser" => %{"subjectDescriptor" => d}}}
      when is_binary(d) and d != "" ->
        # The Azure DevOps Security API does not accept msa.* descriptors
        # (Newtonsoft.Json deserialization error: "Could not cast or convert
        # from System.String to Microsoft.VisualStudio.Services.Identity.IdentityDescriptor").
        # Detect this up-front so the user gets an actionable error instead
        # of a confusing 500 from the grant endpoint.
        if String.starts_with?(d, "msa.") do
          {:error,
           "Your identity is a personal Microsoft account (MSA). The Azure DevOps " <>
             "Security API rejects 'msa.*' descriptors for permission grants, " <>
             "so this command cannot elevate the caller. Use a work/school Entra ID " <>
             "(AAD) identity, or grant 'View library item secrets' on the Library via the web UI."}
        else
          {:ok, d}
        end

      {:ok, _} ->
        {:error,
         "Could not determine caller identity descriptor. The /_apis/connectionData endpoint " <>
           "did not return a subjectDescriptor. Re-run 'ado whoami' to verify auth is healthy."}

      err ->
        {:error, "Failed to fetch caller descriptor: #{inspect(err)}"}
    end
  end

  # Apply (set=true) or remove (set=false) the named bit on the caller's
  # Library ACL at the project root. Uses merge=true for addition (so other
  # bits are preserved) and merge=false with the original allow bit set to
  # the inverted value for removal.
  defp modify_library_bit(project_id, descriptor, bit, set?) do
    allow_bits = if(set?, do: bit, else: 0)

    body = %{
      "token" => project_id,
      "merge" => set?,
      "accessControlEntries" => [
        %{
          "descriptor" => descriptor,
          "allow" => allow_bits,
          "deny" => 0,
          "extendedInfo" => %{}
        }
      ]
    }

    case Client.post("/_apis/accesscontrolentries/#{@library_namespace_id}", body) do
      {:ok, _} ->
        :ok

      {:error, %{status: s, body: body}} when s in [400, 401, 403] ->
        action = if(set?, do: "grant", else: "revoke")
        body_preview = body |> inspect() |> String.slice(0, 200)

        {:error,
         "Azure DevOps rejected the #{action} (#{s}). " <>
           "Common causes: (a) your token lacks the 'vso.security_manage' scope " <>
           "(browser OAuth tokens don't have it; use a PAT with 'Project and Team'); " <>
           "(b) the caller's MSA-descriptor format isn't accepted by Azure DevOps " <>
           "for MSA-backed accounts; (c) you are not a Project Collection Administrator. " <>
           "Raw response: #{body_preview}"}

      err ->
        {:error, "API error: #{inspect(err)}"}
    end
  end
end
