defmodule AdoCli.CLI.Pipelines do
  @moduledoc """
  Commands for managing Azure DevOps Pipelines.

    ado pipelines list PROJECT                   [--top N] [--folder PATH]
    ado pipelines show PROJECT ID
    ado pipelines run PROJECT ID                  [--branch BRANCH] [--variables KEY=VALUE,...]
    ado pipelines vars list PROJECT               [--top N]
    ado pipelines vars show PROJECT GROUP_ID
    ado pipelines vars create PROJECT             --name NAME [--description DESC] [--variables KEY=VALUE,...] [--secret KEY,...]
    ado pipelines vars update PROJECT GROUP_ID    [--name NAME] [--description DESC] [--variables KEY=VALUE,...] [--secret KEY,...]
    ado pipelines vars delete PROJECT GROUP_ID
    ado pipelines secure-files list PROJECT      [--top N]
    ado pipelines secure-files show PROJECT ID
    ado pipelines secure-files upload PROJECT NAME --file PATH [--allow-exists]
    ado pipelines secure-files delete PROJECT ID [--force]
    # NOTE: 'secure-files download' was removed. The Microsoft secure-files
    # API does not issue a downloadTicket to bearer tokens for personal
    # Microsoft accounts, even with vso.securefiles_read, the right scope,
    # and Library/ViewSecrets granted. Re-introduce only when (a) Microsoft
    # fixes the platform gap or (b) a work/school AAD identity is used.
  """

  @behaviour CliMate.CLI.Command

  import CliMate.CLI

  alias AdoCli.CLI.Helpers
  alias AdoCli.Client

  @impl true
  # credo:disable-for-next-line Credo.Check.Refactor.ABCSize
  def command do
    [
      name: "ado pipelines",
      doc:
        "Manage Azure DevOps YAML pipelines and variable groups. Pipelines define CI/CD workflows as code in azure-pipelines.yml; variable groups are shared sets of KEY=VALUE pairs that pipelines can reference.",
      subcommands: [
        list: [
          name: "ado pipelines list",
          doc:
            "List all pipelines in a project. Output is a table (ID, Name, Folder). Use --folder to scope to a subtree (e.g. 'MyTeam/Frontend'); use --top to limit. Pass --json for raw data.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            top: [
              type: :integer,
              doc: "Maximum number of pipelines to return. Default 100, max 1000.",
              doc_arg: "N"
            ],
            folder: [
              type: :string,
              doc:
                "Filter to pipelines in a specific folder (e.g. 'MyTeam/Frontend' or '/' for root)",
              doc_arg: "PATH"
            ]
          ],
          execute: &list_pipelines/1
        ],
        show: [
          name: "ado pipelines show",
          doc:
            "Show details of a single pipeline: ID, name, folder, YAML path, repository, and web URL. The pipeline ID is a stable integer — use it with `run`.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            pipeline_id: [
              type: :integer,
              doc:
                "Numeric pipeline ID (from `list`). The ID is project-scoped — the same number may refer to a different pipeline in another project."
            ]
          ],
          execute: &show_pipeline/1
        ],
        run: [
          name: "ado pipelines run",
          doc:
            "Trigger a new run of a pipeline. Returns the run ID and a link to monitor it (use `ado ci watch` for live streaming). Variables are scoped to this run only; use variable groups for shared config.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            pipeline_id: [type: :integer, doc: "Numeric pipeline ID to run"]
          ],
          options: [
            branch: [
              type: :string,
              doc:
                "Branch to run on (short name, e.g. 'main' or 'feature/foo'; 'refs/heads/' is added automatically). Default: the pipeline's default branch.",
              doc_arg: "BRANCH"
            ],
            variables: [
              type: :string,
              doc:
                "Run-time variables as comma-separated KEY=VALUE pairs (e.g. 'ENV=staging,DEBUG=true'). These override pipeline-defined variables for this run only.",
              doc_arg: "VARS"
            ]
          ],
          execute: &run_pipeline/1
        ],
        create: [
          name: "ado pipelines create",
          doc:
            "Create a new YAML pipeline that points to an existing azure-pipelines.yml file in a repository. The YAML file must already exist; this command registers the pipeline definition.",
          arguments: [project: [type: :string, doc: "Project name or ID"]],
          options: [
            name: [
              type: :string,
              required: true,
              doc: "Display name for the pipeline (e.g. 'MyApp CI')",
              doc_arg: "NAME"
            ],
            repo: [
              type: :string,
              required: true,
              doc: "Repository name or ID containing the YAML file (must be in the same project)",
              doc_arg: "REPO"
            ],
            path: [
              type: :string,
              required: true,
              doc:
                "Path to the YAML file in the repo, relative to the repo root (e.g. 'pipelines/ci.yml' or 'azure-pipelines.yml')",
              doc_arg: "PATH"
            ],
            folder: [
              type: :string,
              doc:
                "Folder to place the pipeline in (e.g. 'MyTeam/Frontend'). Use '/' for the root. The folder is auto-created if it doesn't exist.",
              doc_arg: "FOLDER"
            ]
          ],
          execute: &create_pipeline/1
        ],
        update: [
          name: "ado pipelines update",
          doc:
            "Update a pipeline's name or YAML path. Cannot change the repository; delete and recreate to switch repos. Pass at least one of --name or --path.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            pipeline_id: [type: :integer, doc: "Numeric pipeline ID"]
          ],
          options: [
            name: [type: :string, doc: "New display name for the pipeline", doc_arg: "NAME"],
            path: [
              type: :string,
              doc:
                "New YAML file path in the repo. Doesn't move the file on disk; just changes what the pipeline definition points to.",
              doc_arg: "PATH"
            ]
          ],
          execute: &update_pipeline/1
        ],
        delete: [
          name: "ado pipelines delete",
          doc:
            "Delete a pipeline definition. This is irreversible: the run history is preserved (Azure DevOps retains it), but the pipeline can no longer be triggered and new runs cannot be created from this definition.",
          arguments: [
            project: [type: :string, doc: "Project name or ID"],
            pipeline_id: [type: :integer, doc: "Numeric pipeline ID"]
          ],
          execute: &delete_pipeline/1
        ],
        vars: [
          name: "ado pipelines vars",
          doc:
            "Manage variable groups (a.k.a. library variable groups). Variable groups are shared KEY=VALUE sets that multiple pipelines can reference — perfect for environment-specific config (DB URLs, API keys).",
          subcommands: [
            list: [
              name: "ado pipelines vars list",
              doc:
                "List all variable groups in a project. Output is a table (ID, Name, Description, variable count). Use --top to limit.",
              arguments: [project: [type: :string, doc: "Project name or ID"]],
              options: [
                top: [type: :integer, doc: "Maximum number to return. Default 50.", doc_arg: "N"]
              ],
              execute: &list_var_groups/1
            ],
            show: [
              name: "ado pipelines vars show",
              doc:
                "Show details of a variable group: ID, name, description, type, and the list of variable names. SECRET VALUES ARE NEVER DISPLAYED (they show as ' [secret]'). Use --json to confirm a variable's key/value is being read correctly without exposing secrets.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                group_id: [type: :integer, doc: "Numeric variable group ID (from `list`)"]
              ],
              execute: &show_var_group/1
            ],
            create: [
              name: "ado pipelines vars create",
              doc:
                "Create a new variable group. Use --secret to mark specific keys as secret (the API stores them encrypted; they cannot be retrieved later).",
              arguments: [project: [type: :string, doc: "Project name or ID"]],
              options: [
                name: [
                  type: :string,
                  required: true,
                  doc: "Display name for the variable group (e.g. 'prod-secrets', 'ci-shared')",
                  doc_arg: "NAME"
                ],
                description: [
                  type: :string,
                  doc: "Human-readable description of the group's purpose",
                  doc_arg: "DESC"
                ],
                variables: [
                  type: :string,
                  doc:
                    "Initial variables as comma-separated KEY=VALUE pairs (e.g. 'DB_HOST=db.example.com,DB_USER=app')",
                  doc_arg: "VARS"
                ],
                secret: [
                  type: :string,
                  doc:
                    "Comma-separated list of variable names to mark as secret (e.g. 'DB_PASS,API_KEY'). The values are stored encrypted and cannot be retrieved via the API after creation.",
                  doc_arg: "KEYS"
                ]
              ],
              execute: &create_var_group/1
            ],
            update: [
              name: "ado pipelines vars update",
              doc:
                "Update an existing variable group. Pass --variables to merge new values (existing variables not in the list are kept). Pass --secret to change which keys are secret. Note: changing a secret's value requires re-passing the value; the original encrypted value cannot be read back.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                group_id: [type: :integer, doc: "Numeric variable group ID"]
              ],
              options: [
                name: [type: :string, doc: "New display name", doc_arg: "NAME"],
                description: [type: :string, doc: "New description", doc_arg: "DESC"],
                variables: [
                  type: :string,
                  doc: "Variables to merge, as comma-separated KEY=VALUE pairs",
                  doc_arg: "VARS"
                ],
                secret: [
                  type: :string,
                  doc:
                    "Comma-separated list of keys to mark as secret. Replaces the previous secret list.",
                  doc_arg: "KEYS"
                ]
              ],
              execute: &update_var_group/1
            ],
            delete: [
              name: "ado pipelines vars delete",
              doc:
                "Delete a variable group. Pipelines that reference it will fail at runtime until their YAML is updated to remove the reference.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                group_id: [type: :integer, doc: "Numeric variable group ID"]
              ],
              execute: &delete_var_group/1
            ]
          ]
        ],
        variables: [
          name: "ado pipelines variables",
          doc:
            "Manage per-pipeline variables (user-defined variables stored on a single pipeline definition, not shared). Prefer variable groups for shared config — these are scoped to one pipeline.",
          subcommands: [
            list: [
              name: "ado pipelines variables list",
              doc:
                "List all variables defined directly on a pipeline (not those in referenced variable groups). Output is a table (Key, Value, Secret).",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                pipeline_id: [type: :integer, doc: "Numeric pipeline ID"]
              ],
              execute: &list_pipeline_vars/1
            ],
            create: [
              name: "ado pipelines variables create",
              doc:
                "Add a single variable to a pipeline. For multiple variables, prefer `ado pipelines vars create` (variable groups) which can be shared across pipelines.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                pipeline_id: [type: :integer, doc: "Numeric pipeline ID"]
              ],
              options: [
                key: [
                  type: :string,
                  required: true,
                  doc: "Variable name (env-var friendly: uppercase, no spaces)",
                  doc_arg: "KEY"
                ],
                value: [
                  type: :string,
                  required: true,
                  doc:
                    "Variable value. Marked secret if --secret is passed (value is then stored encrypted and cannot be retrieved later).",
                  doc_arg: "VALUE"
                ],
                secret: [
                  type: :boolean,
                  default: false,
                  doc:
                    "Mark the variable as secret. Once set, the value cannot be retrieved via the API — only overwritten with a new value."
                ]
              ],
              execute: &create_pipeline_var/1
            ],
            delete: [
              name: "ado pipelines variables delete",
              doc:
                "Remove a variable from a pipeline. Pipelines referencing this variable will fail; update the pipeline YAML or use a default value.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                pipeline_id: [type: :integer, doc: "Numeric pipeline ID"]
              ],
              options: [
                key: [
                  type: :string,
                  required: true,
                  doc: "Variable name to remove (must match exactly, case-sensitive)",
                  doc_arg: "KEY"
                ]
              ],
              execute: &delete_pipeline_var/1
            ]
          ]
        ],
        secure_files: [
          name: "ado pipelines secure-files",
          doc:
            "Manage Secure Files in the Pipeline Library. Secure files are binary blobs (certificates, kubeconfigs, signing keys) referenced by pipelines via the DownloadSecureFile@1 task. (The 'download' subcommand is intentionally absent — see CHANGELOG for the Azure DevOps platform gap that prevents ticket issuance for personal Microsoft accounts.)",
          subcommands: [
            list: [
              name: "ado pipelines secure-files list",
              doc:
                "List all Secure Files in a project. Output is a table (ID, Name, Size, Modified). Use --top to limit. Pass --json for raw data.",
              arguments: [project: [type: :string, doc: "Project name or ID"]],
              options: [
                top: [
                  type: :integer,
                  doc: "Maximum number to return. Default 50.",
                  doc_arg: "N"
                ]
              ],
              execute: &secure_files_list/1
            ],
            show: [
              name: "ado pipelines secure-files show",
              doc:
                "Show details of a single Secure File: ID, name, size, created/modified by and on. The ID is a GUID string (from `list`).",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                secure_file_id: [
                  type: :string,
                  doc: "Secure File ID (GUID string, from `list`)"
                ]
              ],
              execute: &secure_files_show/1
            ],
            upload: [
              name: "ado pipelines secure-files upload",
              doc:
                "Upload a local file as a Secure File to the Library. The file content is sent as raw bytes (application/octet-stream). If a Secure File with the same name already exists and --allow-exists is passed, the existing one is deleted first; otherwise the command fails with a 409 error.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                name: [
                  type: :string,
                  doc:
                    "Name for the Secure File (e.g. 'prod-cert.pem'). Must be unique within the project."
                ]
              ],
              options: [
                file: [
                  type: :string,
                  required: true,
                  doc: "Path to the local file to upload",
                  doc_arg: "PATH"
                ],
                allow_exists: [
                  type: :boolean,
                  default: false,
                  doc:
                    "If a Secure File with this name already exists, delete it first before uploading. Without this flag, the command fails on name conflict."
                ]
              ],
              execute: &secure_files_upload/1
            ],
            # NOTE: 'download' was removed in this branch. The Microsoft
            # secure-files API does not issue a downloadTicket to bearer
            # tokens for personal Microsoft accounts (e.g. outlook.com /
            # hotmail.com / live.com), even with vso.securefiles_read,
            # the right scope, and the Library/ViewSecrets permission
            # explicitly granted. Live-tested against GilbertsCode/Employee
            # Management with a full-access PAT: metadata endpoint
            # returned 200 but the 'downloadTicket' field was absent
            # from the JSON body. Re-introduce this subcommand only when
            # either (a) Microsoft fixes the platform gap, or (b) a
            # work/school AAD identity is used to authenticate.
            delete: [
              name: "ado pipelines secure-files delete",
              doc:
                "Permanently delete a Secure File. Pipelines referencing it will fail until updated. Use --force to skip the confirmation prompt.",
              arguments: [
                project: [type: :string, doc: "Project name or ID"],
                secure_file_id: [
                  type: :string,
                  doc: "Secure File ID (GUID string, from `list`)"
                ]
              ],
              options: [
                force: [
                  type: :boolean,
                  default: false,
                  doc: "Skip the interactive confirmation prompt."
                ]
              ],
              execute: &secure_files_delete/1
            ]
          ]
        ]
      ]
    ]
  end

  @impl true
  def execute(parsed), do: if(parsed.execute, do: parsed.execute.())

  # ── Pipelines: Read ───────────────────────────────────────────────────

  @doc """
  Lists pipelines in a project.

  Supports `--top` for pagination and `--folder` for path filtering.
  """
  def list_pipelines(parsed) do
    project = parsed.arguments.project

    params =
      %{}
      |> put_if(Map.get(parsed.options, :top), "$top")
      |> put_if(Map.get(parsed.options, :folder), "folder")

    result = Client.list("/#{URI.encode(project)}/_apis/pipelines", params)

    Helpers.handle_api_result(result, parsed, fn pipelines ->
      Helpers.json_or_format(pipelines, parsed, &print_pipelines_table/1)
    end)
  end

  @doc """
  Shows details of a specific pipeline definition.
  """
  def show_pipeline(parsed) do
    project = parsed.arguments.project
    pipeline_id = parsed.arguments.pipeline_id

    case Client.get("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}") do
      {:ok, pipeline} ->
        Helpers.json_or_format(pipeline, parsed, &print_pipeline_detail/1)

      {:error, %{status: 404}} ->
        halt_error("Pipeline ##{pipeline_id} not found in project '#{project}'")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Pipelines: Write ──────────────────────────────────────────────────

  @doc """
  Triggers a pipeline run.

  Supports `--branch` to run on a specific branch,
  and `--variables KEY=VALUE,...` for pipeline variables.
  """
  def run_pipeline(parsed) do
    project = parsed.arguments.project
    pipeline_id = parsed.arguments.pipeline_id

    body = %{
      "resources" => %{
        "repositories" => %{
          "self" => %{"refName" => "refs/heads/#{Map.get(parsed.options, :branch, "main")}"}
        }
      }
    }

    body =
      if vars = Map.get(parsed.options, :variables) do
        vars = parse_variables(vars)
        Map.put(body, "variables", vars)
      else
        body
      end

    case Client.post("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}/runs", body) do
      {:ok, run} ->
        success("Pipeline run triggered!\n")
        writeln("  Run ID:   #{run["id"]}")
        writeln("  State:    #{run["state"]}")
        writeln("  Pipeline: #{run["pipeline"]["name"]}")
        writeln("  URL:      #{run["_links"]["web"]["href"]}")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Variable Groups: Read ─────────────────────────────────────────────

  def list_var_groups(parsed) do
    project = parsed.arguments.project
    params = put_if(%{}, Map.get(parsed.options, :top), "$top")

    result = Client.list("/#{URI.encode(project)}/_apis/distributedtask/variablegroups", params)

    Helpers.handle_api_result(result, parsed, fn groups ->
      Helpers.json_or_format(groups, parsed, &print_var_groups_table/1)
    end)
  end

  def show_var_group(parsed) do
    project = parsed.arguments.project
    group_id = parsed.arguments.group_id

    case Client.get("/#{URI.encode(project)}/_apis/distributedtask/variablegroups/#{group_id}") do
      {:ok, group} ->
        Helpers.json_or_format(group, parsed, &print_var_group_detail/1)

      {:error, %{status: 404}} ->
        halt_error("Variable group ##{group_id} not found in project '#{project}'")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Variable Groups: Write ────────────────────────────────────────────

  def create_var_group(parsed) do
    project = parsed.arguments.project
    body = build_var_group_body(parsed, project)

    case Client.post("/#{URI.encode(project)}/_apis/distributedtask/variablegroups", body) do
      {:ok, group} ->
        success("Variable group '#{group["name"]}' created (ID: #{group["id"]}).\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def update_var_group(parsed) do
    project = parsed.arguments.project
    group_id = parsed.arguments.group_id

    # Fetch existing group first to merge
    case Client.get("/#{URI.encode(project)}/_apis/distributedtask/variablegroups/#{group_id}") do
      {:ok, existing} ->
        body = merge_var_group_body(existing, parsed)

        case Client.put(
               "/#{URI.encode(project)}/_apis/distributedtask/variablegroups/#{group_id}",
               body
             ) do
          {:ok, group} ->
            success("Variable group '#{group["name"]}' updated.\n")
            halt_success("")

          error ->
            Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
        end

      {:error, %{status: 404}} ->
        halt_error("Variable group ##{group_id} not found in project '#{project}'")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_var_group(parsed) do
    project = parsed.arguments.project
    group_id = parsed.arguments.group_id

    # Resolve project ID for the projectIds query parameter
    project_id =
      case Client.list("/_apis/projects") do
        {:ok, projects} ->
          found = Enum.find(projects, &(&1["name"] == project))
          if found, do: found["id"], else: nil

        _ ->
          nil
      end

    params = if project_id, do: %{"projectIds" => project_id}, else: %{}

    case Client.delete(
           "/#{URI.encode(project)}/_apis/distributedtask/variablegroups/#{group_id}",
           params
         ) do
      :ok ->
        success("Variable group ##{group_id} deleted.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Variable group ##{group_id} not found in project '#{project}'")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Variable Group Helpers ────────────────────────────────────────────

  defp build_var_group_body(parsed, project_name) do
    name = Map.fetch!(parsed.options, :name)
    desc = Map.get(parsed.options, :description)
    variables = Map.get(parsed.options, :variables)
    secret_keys = Map.get(parsed.options, :secret)

    body = %{
      "name" => name,
      "type" => "Vsts",
      "variableGroupProjectReferences" => [
        %{
          "name" => name,
          "projectReference" => %{
            "name" => project_name
          }
        }
      ]
    }

    body = if desc, do: Map.put(body, "description", desc), else: body

    if variables do
      vars_map = parse_var_group_variables(variables, secret_keys)
      Map.put(body, "variables", vars_map)
    else
      body
    end
  end

  defp merge_var_group_body(existing, parsed) do
    body = base_group_body(existing, parsed)
    add_variables_to_body(body, existing, parsed)
  end

  defp base_group_body(existing, parsed) do
    name = Map.get(parsed.options, :name) || existing["name"]
    desc = Map.get(parsed.options, :description) || existing["description"]

    body = %{
      "name" => name,
      "type" => existing["type"] || "Vsts",
      "variableGroupProjectReferences" => existing["variableGroupProjectReferences"] || []
    }

    if desc, do: Map.put(body, "description", desc), else: body
  end

  defp add_variables_to_body(body, existing, parsed) do
    vars_map = merged_variables(existing, parsed)
    if vars_map == %{}, do: body, else: Map.put(body, "variables", vars_map)
  end

  defp merged_variables(existing, parsed) do
    existing_vars = existing["variables"] || %{}
    variables = Map.get(parsed.options, :variables)

    if variables do
      secret_keys = Map.get(parsed.options, :secret)
      parsed_vars = parse_var_group_variables(variables, secret_keys)
      Map.merge(existing_vars, parsed_vars)
    else
      existing_vars
    end
  end

  defp parse_var_group_variables(vars_string, secret_keys) do
    secret_set = parse_secret_keys(secret_keys)

    vars_string
    |> String.split(",")
    |> Enum.map(&String.trim/1)
    |> Enum.reduce(%{}, fn pair, acc ->
      case String.split(pair, "=", parts: 2) do
        [key, value] ->
          Map.put(acc, key, %{"value" => value, "isSecret" => MapSet.member?(secret_set, key)})

        _ ->
          acc
      end
    end)
  end

  defp parse_secret_keys(nil), do: MapSet.new()
  defp parse_secret_keys(""), do: MapSet.new()

  defp parse_secret_keys(str) do
    str |> String.split(",") |> Enum.map(&String.trim/1) |> MapSet.new()
  end

  # ── Pipeline Definitions: Create/Update/Delete ──────────────────────

  def create_pipeline(parsed) do
    project = parsed.arguments.project
    name = Map.fetch!(parsed.options, :name)
    repo = Map.fetch!(parsed.options, :repo)
    yaml_path = Map.fetch!(parsed.options, :path)
    folder = Map.get(parsed.options, :folder, "/")

    body = %{
      "name" => name,
      "folder" => folder,
      "configuration" => %{
        "type" => "yaml",
        "path" => yaml_path,
        "repository" => %{"id" => repo, "name" => repo, "type" => "azureReposGit"}
      }
    }

    case Client.post("/#{URI.encode(project)}/_apis/pipelines", body) do
      {:ok, pipeline} ->
        success("Pipeline '#{pipeline["name"]}' created (ID: #{pipeline["id"]}).\n")
        halt_success("")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def update_pipeline(parsed) do
    project = parsed.arguments.project
    pipeline_id = parsed.arguments.pipeline_id
    body = %{}
    body = if name = Map.get(parsed.options, :name), do: Map.put(body, "name", name), else: body

    body =
      if path = Map.get(parsed.options, :path),
        do: Map.put(body, :configuration, Map.put(body[:configuration] || %{}, :path, path)),
        else: body

    if body == %{}, do: halt_error("At least one of --name or --path is required.")

    case Client.patch("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}", body) do
      {:ok, _} ->
        success("Pipeline updated.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Pipeline ##{pipeline_id} not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_pipeline(parsed) do
    project = parsed.arguments.project
    pipeline_id = parsed.arguments.pipeline_id

    case Client.delete("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}") do
      :ok ->
        success("Pipeline deleted.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Pipeline ##{pipeline_id} not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Pipeline Variables (per-definition) ────────────────────────────────

  def list_pipeline_vars(parsed) do
    project = parsed.arguments.project
    pipeline_id = parsed.arguments.pipeline_id

    case Client.get("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}") do
      {:ok, pipeline} ->
        vars = get_in(pipeline, ["configuration", "variables"]) || %{}

        var_list =
          Enum.map(vars, fn {k, v} ->
            %{"key" => k, "value" => v["value"], "isSecret" => v["isSecret"]}
          end)

        Helpers.json_or_format(var_list, parsed, &print_pipeline_vars_table/1)

      {:error, %{status: 404}} ->
        halt_error("Pipeline ##{pipeline_id} not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def create_pipeline_var(parsed) do
    project = parsed.arguments.project
    pipeline_id = parsed.arguments.pipeline_id
    key = Map.fetch!(parsed.options, :key)
    value = Map.fetch!(parsed.options, :value)
    is_secret = Map.get(parsed.options, :secret, false)

    case Client.get("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}") do
      {:ok, pipeline} ->
        existing_vars = get_in(pipeline, ["configuration", "variables"]) || %{}
        new_var = %{"value" => value, "isSecret" => is_secret}
        updated_vars = Map.put(existing_vars, key, new_var)
        update_body = put_in(pipeline, ["configuration", "variables"], updated_vars)

        case Client.patch("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}", update_body) do
          {:ok, _} ->
            success("Variable '#{key}' added.\n")
            halt_success("")

          error ->
            Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
        end

      {:error, %{status: 404}} ->
        halt_error("Pipeline ##{pipeline_id} not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def delete_pipeline_var(parsed) do
    project = parsed.arguments.project
    pipeline_id = parsed.arguments.pipeline_id
    key = Map.fetch!(parsed.options, :key)

    case Client.get("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}") do
      {:ok, pipeline} ->
        existing_vars = get_in(pipeline, ["configuration", "variables"]) || %{}
        updated_vars = Map.delete(existing_vars, key)
        update_body = put_in(pipeline, ["configuration", "variables"], updated_vars)

        case Client.patch("/#{URI.encode(project)}/_apis/pipelines/#{pipeline_id}", update_body) do
          {:ok, _} ->
            success("Variable '#{key}' removed.\n")
            halt_success("")

          error ->
            Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
        end

      {:error, %{status: 404}} ->
        halt_error("Pipeline ##{pipeline_id} not found")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  # ── Secure Files ──────────────────────────────────────────────────────

  @doc """
  Lists Secure Files in a project.

  Supports `--top` for pagination.
  """
  def secure_files_list(parsed) do
    project = parsed.arguments.project
    params = put_if(%{}, Map.get(parsed.options, :top), "$top")
    result = Client.list("/#{URI.encode(project)}/_apis/distributedtask/securefiles", params)

    Helpers.handle_api_result(result, parsed, fn files ->
      Helpers.json_or_format(files, parsed, &print_secure_files_table/1)
    end)
  end

  @doc """
  Shows details of a specific Secure File.
  """
  def secure_files_show(parsed) do
    project = parsed.arguments.project
    secure_file_id = parsed.arguments.secure_file_id

    case Client.get("/#{URI.encode(project)}/_apis/distributedtask/securefiles/#{secure_file_id}") do
      {:ok, file} ->
        Helpers.json_or_format(file, parsed, &print_secure_file_detail/1)

      {:error, %{status: 404}} ->
        halt_error("Secure file #{secure_file_id} not found in project '#{project}'")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  @doc """
  Uploads a local file as a new Secure File.

  If `allow_exists` is true and a Secure File with the same name exists,
  the existing one is deleted first.
  """
  def secure_files_upload(parsed) do
    project = parsed.arguments.project
    name = parsed.arguments.name
    file_path = Map.fetch!(parsed.options, :file)
    allow_exists = Map.get(parsed.options, :allow_exists, false)

    unless File.regular?(file_path) do
      halt_error("File not found: #{file_path}")
    end

    if allow_exists do
      case find_secure_file_id_by_name(project, name) do
        {:ok, existing_id} ->
          case Client.delete(
                 "/#{URI.encode(project)}/_apis/distributedtask/securefiles/#{existing_id}"
               ) do
            :ok ->
              success("Deleted existing '#{name}' (id: #{existing_id}).\n")

            {:error, reason} ->
              Helpers.bail(reason, parsed)
          end

        :not_found ->
          :ok
      end
    end

    upload_bytes(project, name, file_path, parsed)
  end

  defp upload_bytes(project, name, file_path, parsed) do
    bytes = File.read!(file_path)
    path = "/#{URI.encode(project)}/_apis/distributedtask/securefiles"

    case Client.post_binary(path, bytes, %{"name" => name}) do
      {:ok, file} ->
        success(
          "Secure file '#{file["name"]}' uploaded (ID: #{file["id"]}, #{byte_size(bytes)} bytes).\n"
        )

        halt_success("")

      {:error, %{status: 409, body: body}} ->
        msg = Helpers.extract_error_message(body) || "name conflict"

        halt_error(
          "A secure file named '#{name}' already exists. " <>
            "Re-run with --allow-exists to replace it, or use a different name. (#{msg})"
        )

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  def secure_files_delete(parsed) do
    project = parsed.arguments.project
    secure_file_id = parsed.arguments.secure_file_id
    force = Map.get(parsed.options, :force, false)

    unless force do
      writeln("This will permanently delete the Secure File. Pass --force to confirm.")
      halt_success("")
    end

    case Client.delete(
           "/#{URI.encode(project)}/_apis/distributedtask/securefiles/#{secure_file_id}"
         ) do
      :ok ->
        success("Secure file #{secure_file_id} deleted.\n")
        halt_success("")

      {:error, %{status: 404}} ->
        halt_error("Secure file #{secure_file_id} not found in project '#{project}'")

      error ->
        Helpers.handle_api_result(error, parsed, fn _ -> :ok end)
    end
  end

  defp find_secure_file_id_by_name(project, name) do
    # Use the server-side namePattern filter (ADO supports wildcards) so
    # we don't fetch the whole library just to look up one file.
    case Client.list(
           "/#{URI.encode(project)}/_apis/distributedtask/securefiles",
           %{"namePattern" => name}
         ) do
      {:ok, files} ->
        case Enum.find(files, &(&1["name"] == name)) do
          %{"id" => id} -> {:ok, id}
          nil -> :not_found
        end

      _ ->
        :not_found
    end
  end

  defp print_secure_files_table(files) do
    if Enum.empty?(files) do
      writeln("No secure files found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 36)}  #{String.pad_trailing("Name", 30)}  #{String.pad_trailing("Size", 10)}  Modified"
      )

      writeln(String.duplicate("─", 100))

      Enum.each(files, fn f ->
        size =
          case f["contentLength"] do
            nil -> "?"
            n -> to_string(n)
          end

        writeln(
          "#{String.pad_trailing(to_string(f["id"] || ""), 36)}  #{String.pad_trailing(f["name"] || "", 30)}  #{String.pad_trailing(size, 10)}  #{f["modifiedOn"] || "?"}"
        )
      end)

      writeln("")
      writeln("#{length(files)} secure file(s)")
    end
  end

  defp print_secure_file_detail(file) do
    writeln("")
    success("Secure File Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:        #{file["id"]}")
    writeln("  Name:      #{file["name"]}")
    writeln("  Size:      #{file["contentLength"]} bytes")

    writeln(
      "  Created:   #{file["createdOn"]} by #{get_in(file, ["createdBy", "displayName"]) || "?"}"
    )

    writeln(
      "  Modified:  #{file["modifiedOn"]} by #{get_in(file, ["modifiedBy", "displayName"]) || "?"}"
    )

    writeln("")
  end

  # ── Generic Helpers ───────────────────────────────────────────────────

  defp put_if(map, nil, _key), do: map
  defp put_if(map, value, key), do: Map.put(map, key, value)

  defp parse_variables(vars_string) do
    vars_string
    |> String.split(",")
    |> Enum.map(&String.trim/1)
    |> Enum.reduce(%{}, fn pair, acc ->
      case String.split(pair, "=", parts: 2) do
        [key, value] -> Map.put(acc, key, %{"value" => value})
        _ -> acc
      end
    end)
  end

  # ── Formatting ────────────────────────────────────────────────────────

  defp print_pipelines_table(pipelines) do
    if Enum.empty?(pipelines) do
      writeln("No pipelines found.")
    else
      writeln("")
      writeln("#{String.pad_trailing("ID", 6)}  #{String.pad_trailing("Name", 40)}  Folder")
      writeln(String.duplicate("─", 80))

      Enum.each(pipelines, fn p ->
        writeln(
          "#{String.pad_trailing(to_string(p["id"] || ""), 6)}  #{String.pad_trailing(p["name"] || "", 40)}  #{p["folder"] || "/"}"
        )
      end)

      writeln("")
      writeln("#{length(pipelines)} pipeline(s)")
    end
  end

  defp print_pipeline_detail(pipeline) do
    writeln("")
    success("Pipeline Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:     #{pipeline["id"]}")
    writeln("  Name:   #{pipeline["name"]}")
    writeln("  Folder: #{pipeline["folder"] || "/"}")
    writeln("  URL:    #{pipeline["url"]}")

    if configuration = pipeline["configuration"] do
      writeln("  Type:   #{configuration["type"] || "?"}")
      if path = configuration["path"], do: writeln("  Path:   #{path}")
    end

    if pipeline["_links"]["web"], do: writeln("  Web:    #{pipeline["_links"]["web"]["href"]}")
    writeln("")
  end

  defp print_var_groups_table(groups) do
    if Enum.empty?(groups) do
      writeln("No variable groups found.")
    else
      writeln("")

      writeln(
        "#{String.pad_trailing("ID", 6)}  #{String.pad_trailing("Name", 30)}  #{String.pad_trailing("Description", 40)}  Variables"
      )

      writeln(String.duplicate("─", 100))

      Enum.each(groups, fn g ->
        var_count = g["variables"] |> Map.keys() |> length()

        desc = String.slice(g["description"] || "", 0, 38)

        writeln(
          "#{String.pad_trailing(to_string(g["id"] || ""), 6)}  #{String.pad_trailing(g["name"] || "", 30)}  #{String.pad_trailing(desc, 40)}  #{var_count}"
        )
      end)

      writeln("")
      writeln("#{length(groups)} variable group(s)")
    end
  end

  defp print_var_group_detail(group) do
    writeln("")
    success("Variable Group Details\n")
    writeln(String.duplicate("─", 60))
    writeln("  ID:          #{group["id"]}")
    writeln("  Name:        #{group["name"]}")
    writeln("  Description: #{group["description"] || "(none)"}")
    writeln("  Type:        #{group["type"]}")

    if variables = group["variables"] do
      writeln("")
      writeln("  Variables:")

      Enum.each(variables, fn {key, var} ->
        secret = if var["isSecret"], do: " [secret]", else: ""
        writeln("    #{key}#{secret}")
      end)
    end

    writeln("")
  end

  defp print_pipeline_vars_table(vars) do
    if Enum.empty?(vars) do
      writeln("No pipeline variables defined.")
    else
      writeln("")
      writeln("#{String.pad_trailing("Key", 25)}  #{String.pad_trailing("Value", 30)}  Secret")
      writeln(String.duplicate("─", 70))

      Enum.each(vars, fn v ->
        secret_label = if v["isSecret"], do: "yes", else: "no"

        writeln(
          "#{String.pad_trailing(v["key"] || "", 25)}  #{String.pad_trailing(v["value"] || "", 30)}  #{secret_label}"
        )
      end)

      writeln("")
      writeln("#{length(vars)} variable(s)")
    end
  end
end
