---
name: ado-cli
description: Complete command reference for all 24 Azure DevOps service areas (projects, repos, workitems, pipelines, prs, releases, packages, and more)
version: "0.4.9"
commands:
  - ado --version
  - ado version
  - ado schema --json
  - ado completion bash
  - ado login
  - ado login --method device
  - ado login --org ORG
  - ado login --method pat --org ORG --pat TOKEN
  - ado logout
  - ado whoami
  - ado projects list
  - ado projects show PROJECT
  - ado projects create --name N --description D
  - ado projects update PROJECT --description D
  - ado projects delete PROJECT
  - ado repos list PROJECT
  - ado repos show PROJECT REPO
  - ado repos create PROJECT --name N
  - ado repos delete PROJECT REPO
  - ado repos branches PROJECT REPO
  - ado branch-policies list PROJECT REPO
  - ado workitems list PROJECT
  - ado workitems show PROJECT ID
  - ado workitems create PROJECT --type T --title T --tags t1,t2
  - ado workitems update ID --state S
  - ado workitems query PROJECT --wiql "SELECT ..."
  - ado pipelines list PROJECT
  - ado pipelines show PROJECT ID
  - ado pipelines run PROJECT ID --branch B --variables K=V
  - ado pipelines vars create PROJECT --name N --variables K=V --secret K
  - ado pipelines-builds queue PROJECT --definition D --branch B
  - ado pipelines-builds cancel PROJECT BUILD_ID
  - ado pipelines-artifacts list PROJECT BUILD_ID
  - ado prs list PROJECT REPO
  - ado prs show PROJECT REPO ID
  - ado prs create PROJECT REPO --title T --source S --target T
  - ado prs complete PROJECT REPO ID --merge-strategy squash
  - ado prs approve PROJECT REPO ID
  - ado prs comments list PROJECT REPO PR_ID
  - ado prs comments add PROJECT REPO PR_ID --content TEXT
  - ado prs comments update PROJECT REPO PR_ID THREAD COMMENT --content TEXT
  - ado prs diff PROJECT REPO PR_ID
  - ado prs diff PROJECT REPO PR_ID --file PATH
  - ado prs diff PROJECT REPO PR_ID --unified
  - ado prs reviewers list PROJECT REPO PR_ID
  - ado prs reviewers add PROJECT REPO PR_ID --reviewer USER_GUID
  - ado prs reviewers remove PROJECT REPO PR_ID --reviewer USER_GUID
  - ado releases list PROJECT
  - ado releases show PROJECT ID
  - ado iterations list PROJECT
  - ado areas list PROJECT
  - ado wikis list PROJECT
  - ado teams list PROJECT
  - ado teams show PROJECT TEAM
  - ado users list
  - ado users show USER
  - ado extensions list
  - ado agent-pools list
  - ado connections list PROJECT
  - ado connections create PROJECT --name N --type T --url URL --access-token TOKEN
  - ado connections update PROJECT ID --name N
  - ado connections delete PROJECT ID
  - ado security groups list PROJECT SCOPE
  - ado banners set --message TEXT --type warning
  - ado packages list PROJECT FEED
  - ado ci watch PROJECT BUILD_ID
  - ado skills list
  - ado skills describe ado-cli
  - ado skills read ado-cli
  - ado skills search "query"
  - ado skills install
  - ado test-results list PROJECT
  - ado test-results show PROJECT RUN_ID
  - ado test-results publish PROJECT --name N --file coverage.xml --build-id ID
  - ado test-coverage show PROJECT BUILD_ID
---

# ado — Azure DevOps CLI

A self-contained, cross-compiled CLI for managing every Azure DevOps service:
projects, repos, work items, pipelines, PRs, releases, packages, and more.
Single-file binaries for macOS, Linux, and Windows via Burrito. No `az` or
Node.js dependency.

## When to use this skill

- You need to automate Azure DevOps from a CI pipeline or script
- You are an LLM agent helping a user manage their DevOps org
- You want to script PR reviews, pipeline triggers, or work item workflows
- You are behind a firewall/offline and cannot use `az devops`

## Quick start (the 80% case)

```bash
# Build from source or download a binary
mix escript.build && cp ado /usr/local/bin/
# Or: curl -L -o ado https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado_linux

# Authenticate
ado login                                                   # browser OAuth, auto-detects org
ado login --method device                                   # device code, no --org needed
ado login --method pat --org myorg --pat mytoken            # PAT (CI-friendly)

# Verify
ado whoami

# Top 3 commands
ado projects list
ado workitems list MyProject --state Active
ado pipelines run MyProject 42 --branch main
```

## Global options

```
--org, -o ORG       Organization (or ADO_ORG env var; auto-detected on login)
--pat, -t TOKEN     Personal Access Token (or ADO_PAT env var)
--server, -s URL    Self-hosted server (or ADO_SERVER env var)
--json              Output raw JSON instead of formatted tables
--verbose, -v       Verbose output (includes stack traces on error)
--version           Print the version and exit
```

All commands support `--help`.

## Decision tree: which command for my task?

1. **Listing/viewing something?** → `<area> list` or `<area> show`
2. **Creating new content?** → `<area> create` (always needs a project)
3. **Updating existing content?** → `<area> update` + the item's ID
4. **Deleting?** → `<area> delete` (add `--force` to skip confirmation)
5. **Need JSON for scripting?** → add `--json` to any command

### Project and repo names with spaces

Quote them:
```bash
ado repos list "Employee Management"
ado prs list "Employee Management" "My Repo"
```

### `--org`, `--pat`, and `--server`

These can appear anywhere (before or after the subcommand):
```bash
ado --org myorg projects list
ado projects list --org myorg               # same thing
export ADO_ORG=myorg                        # or set env var once
ado projects list                            # no --org needed
```

## Full command reference

### Projects

```bash
# List all projects (table: Name, ID, State, Visibility)
ado projects list
ado projects list --state wellFormed --top 20

# Show a single project
ado projects show MyProject

# Create a project
ado projects create MyNewProject --description "My new project" --visibility private
# Visibility: private (default) or public. Process: Agile, Scrum, CMMI, Basic.

# Update (rename or change description)
ado projects update MyProject --name "Renamed Project"

# Delete (--force skips confirmation)
ado projects delete OldProject --force
```

### Repositories

```bash
# List repos in a project
ado repos list MyProject

# Show a single repo
ado repos show MyProject MyRepo

# Create a repo
ado repos create MyProject --name "new-repo" --default_branch main

# List branches
ado repos branches MyProject MyRepo
ado repos branches MyProject MyRepo --filter feature

# Delete
ado repos delete MyProject MyRepo --force
```

### Branch policies

```bash
ado branch-policies list MyProject MyRepo
ado branch-policies show MyProject MyRepo POLICY_ID
ado branch-policies create MyProject MyRepo --type UUID --branch refs/heads/main --blocking
ado branch-policies update MyProject MyRepo POLICY_ID --enabled false
ado branch-policies delete MyProject MyRepo POLICY_ID
```

### Work Items

```bash
# List work items (table: ID, Title, Type, State, Assigned To)
ado workitems list MyProject --state Active --type Bug --top 20

# Show details
ado workitems show 42

# WIQL query
ado workitems query MyProject --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.State] = 'Active'"

# Create
ado workitems create MyProject --type Bug --title "Login fails" --description "Steps to reproduce: ..." --tags "ui,critical" --priority 1

# Update state or fields
ado workitems update 42 --state Resolved --assigned_to "Jane Smith"

# Delete
ado workitems delete 42
```

### Pipelines

```bash
# List pipelines in a project
ado pipelines list MyProject

# Trigger a run
ado pipelines run MyProject 42 --branch main
ado pipelines run MyProject 42 --branch release --variables "ENV=staging,DEBUG=true"

# Variable groups
ado pipelines vars list MyProject
ado pipelines vars show MyProject 5
ado pipelines vars create MyProject --name "prod-secrets" --variables "DB_HOST=prod" --secret NPM_TOKEN
```

### Classic Builds

```bash
ado pipelines-builds queue MyProject --definition 5 --branch main
ado pipelines-builds cancel MyProject 99
ado pipelines-builds show MyProject 99
```

### Pull Requests

```bash
# List PRs in a repo (table: ID, Title, Source, Target, Status)
ado prs list MyProject MyRepo --status active --creator "alice@example.com"

# Show details
ado prs show MyProject MyRepo 42

# Create
ado prs create MyProject MyRepo --title "Add feature" --source dev --target main --draft

# View diff (3 modes)
ado prs diff MyProject MyRepo 42                                    # file list with +/- counts
ado prs diff MyProject MyRepo 42 --file src/app.ex                   # per-file unified diff
ado prs diff MyProject MyRepo 42 --unified | delta                  # full unified diff stream

# Diff handles new, edited, and deleted files correctly:
ado prs diff MyProject MyRepo 42 --file new_file.ex                  # new file: shows all + lines
ado prs diff MyProject MyRepo 42 --file deleted.ex                   # deleted: shows all - lines

# Review and merge
ado prs approve MyProject MyRepo 42                                 # vote +10
ado prs complete MyProject MyRepo 42 --merge-strategy squash --delete-source
ado prs abandon MyProject MyRepo 42

# Comments (multi-word content does NOT need quoting)
ado prs comments add MyProject MyRepo 42 --content LGTM ship it
ado prs comments add MyProject MyRepo 42 --content @notes.md        # from file
echo "review" | ado prs comments add MyProject MyRepo 42 --content - # from stdin
ado prs comments list MyProject MyRepo 42 --all                     # full content view
ado prs comments update MyProject MyRepo 42 THREAD_ID COMMENT_ID --content "updated text"
ado prs comments update MyProject MyRepo 42 THREAD_ID COMMENT_ID --status closed --resolved-by-me

# Reviewers
ado prs reviewers list MyProject MyRepo 42
ado prs reviewers add MyProject MyRepo 42 --reviewer USER_GUID
ado prs reviewers remove MyProject MyRepo 42 --reviewer USER_GUID
```

### Releases

```bash
ado releases list MyProject --definition_id 5 --status active
ado releases show MyProject 42
```

### Iterations (Sprints)

```bash
ado iterations list MyProject
ado iterations show MyProject MyTeam "Sprint 23"
ado iterations create MyProject MyTeam --name "Sprint 24" --start-date 2026-01-15 --finish-date 2026-01-29
```

### Areas

```bash
ado areas list MyProject
ado areas show MyProject "MyArea"
ado areas create MyProject --name "NewArea"
```

### Wikis

```bash
ado wikis list MyProject
ado wikis pages list MyProject MyWiki
ado wikis pages show MyProject MyWiki --path /Home
ado wikis pages create MyProject MyWiki --path /Design --content "# Design Doc"
```

### Teams

```bash
ado teams list MyProject
ado teams members list MyProject "My Team"
```

### Users (Entitlements)

```bash
ado users list
ado users show "alice@example.com"
ado users add --email "newuser@example.com" --license professional
ado users remove "user_id_or_email"
```

### Extensions

```bash
ado extensions list
ado extensions install ms.azure-devops-utilities --publisher ms
ado extensions uninstall "ms.azure-devops-utilities"
```

### Agent Pools

```bash
ado agent-pools list
ado agent-pools show POOL_ID
ado agent-pools queues POOL_ID
```

### Service Connections

```bash
# List connections (table: ID, Name, Type)
ado connections list MyProject
ado connections list MyProject --type github

# Show details (ID, name, type, url, isReady). Secrets are never returned.
ado connections show MyProject <connection-id>

# Create a GitHub PAT connection (literal token — appears in shell history)
ado connections create MyProject GitHubPat github https://github.com \
    --access-token gh_xxxxx --description "CI bot PAT"

# Create with token from stdin (secure — no shell history)
echo "$GITHUB_PAT" | ado connections create MyProject GitHubPat github https://github.com \
    --access-token -

# Create with token from a file
ado connections create MyProject GitHubPat github https://github.com \
    --access-token @~/.github-pat --description "CI bot PAT"

# Create with type-specific --data (e.g. Azure RM subscription)
ado connections create MyProject AzureProd azure "" \
    --data '{"subscriptionId":"11111111-2222-3333-4444-555555555555","subscriptionName":"Prod"}' \
    --scheme UsernamePassword --access-token secret-password

# Update (rename, change description, or rotate credentials)
ado connections update MyProject <id> --name "Renamed"
ado connections update MyProject <id> --access-token new-token
ado connections update MyProject <id> --data '{"subscriptionId":"new-sub-id"}'
# No fields supplied → usage error

# Delete (--force skips y/N confirmation)
ado connections delete MyProject <id> --force
```

JSON response shape for create/update (with `--json`):
```json
{
  "ok": true,
  "result": {
    "id": "uuid",
    "name": "string",
    "type": "string",
    "url": "string",
    "isReady": true
  }
}
```

### Security

```bash
ado security groups list MyProject
ado security groups create MyProject --name "Reviewers"
ado security groups members list MyProject "vssgp.xxxxx"
ado security permissions list "2e9eb7ed-..." --token "repoV2/projectId/repoId"
```

### Banners (Org Notifications)

```bash
ado banners show
ado banners set --message "Maintenance window: Sat 2-4am" --type warning
ado banners delete
```

### Packages (Universal)

```bash
ado packages list MyProject MyFeed
ado packages versions MyProject MyFeed my-package
ado packages show MyProject MyFeed my-package 1.0.0
```

### CI Watch (Live Pipeline Logs)

```bash
ado ci watch MyProject 99                                         # specific build
ado ci watch MyProject --latest --definition 42 --branch main    # latest matching build
```

### Test Results and Coverage

```bash
ado test-results list MyProject
ado test-results show MyProject 42
ado test-results publish MyProject --name "CI Suite" --file coverage.cobertura.xml --build-id 99
ado test-coverage show MyProject 99
```

### Skills (for AI agents)

```bash
ado skills list
ado skills describe ado-cli
ado skills read ado-cli
ado skills search "pipeline"
ado skills install                                   # install to all known agent dirs
ado skills install --target pi --skill ado-cli
```

## Non-obvious behaviors

### Multi-word option values

`--content`, `--description`, `--message`, `--body`, `--text`, `--summary`, `--reason`, and similar text options do NOT need shell quoting:

```bash
ado prs comments add MyProject MyRepo 42 --content This is fine without quotes
ado prs comments add MyProject MyRepo 42 --content and works with multiple words
```

All words after the flag are joined into a single value. The joining stops at the next `--flag`.

### MSA (personal) orgs

Works with `*.visualstudio.com` orgs. No special flags needed. Use browser OAuth (default) or PAT. Device code also works.

### Self-hosted Azure DevOps Server

```bash
ado login --method pat --server https://ado.example.com --org DefaultCollection --pat xxx
ado --server https://ado.example.com --org Coll projects list
```

### Output formats

- Default: formatted tables (human-readable)
- `--json`: raw JSON envelope (machine-readable)
- `ado schema --json`: the full command tree for LLM agent discovery

### Exit codes

| Code | Meaning |
|------|---------|
| 0    | Success |
| 1    | Generic error |
| 2    | API error (4xx/5xx) |
| 3    | Auth not configured |

## Common pitfalls

1. **"Not authenticated"** → Run `ado login` or set `ADO_ORG`/`ADO_PAT` env vars
2. **"API redirected to sign-in page" (302)** → Token expired. Re-run `ado login`
3. **"You cannot record a vote for someone else"** → If approving a PR you did not create, you need to be added as a reviewer first. Use `ado prs approve` anyway — the CLI auto-adds you via `PUT /reviewers/{user-guid}`
4. **"Organization not found"** → Check spelling. Use `ado whoami` to see what org is configured
5. **401/403** → Token invalid or scope too narrow. Check your PAT at https://dev.azure.com/{org}/_usersSettings/tokens
6. **Project/repo with spaces** → Always quote in the shell: `ado repos list "My Project"`
7. **`connectionData` returns 400** → This is a known Azure DevOps API version issue for some orgs. The CLI now calls the endpoint without `api-version` to avoid this
8. **PR diff shows no changes** → Use `--iteration` to inspect a specific iteration. The default is the latest

## Help

```bash
ado --help                     # top-level
ado projects --help            # command group
ado projects create --help     # specific subcommand
```

## See also

- [ado-auth skill](ado-auth) — authentication methods, PAT vs OAuth, troubleshooting
- [ado-ci skill](ado-ci) — CI/CD patterns, GitHub/GitLab examples, headless auth
- [Azure DevOps REST API docs](https://learn.microsoft.com/en-us/rest/azure/devops)
- [Project homepage](https://gilbertwong96.github.io/ado_cli/)
