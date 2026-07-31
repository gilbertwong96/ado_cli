---
name: ado-cli
description: Complete command reference for all 24 Azure DevOps service areas (projects, repos, workitems, pipelines, prs, releases, packages, and more)
version: "0.5.0"
commands:
  - ado --version
  - ado version
  - ado schema --json
  - ado completion bash
  - ado login
  - ado login --method device
  - ado login --org ORG
  - ado login --org ORG --pat TOKEN
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
  - ado pipelines secure_files list PROJECT
  - ado pipelines secure_files show PROJECT ID
  - ado pipelines secure_files upload PROJECT NAME --file PATH [--allow-exists]
  - ado pipelines secure_files delete PROJECT ID [--force]
  # NOTE: 'download' was removed in the current branch. Microsoft
  # secure-files API does not issue a downloadTicket to bearer tokens
  # for personal Microsoft accounts, even with vso.securefiles_read,
  # the right scope, and Library/ViewSecrets granted. Re-introduce
  # the subcommand only when (a) Microsoft fixes the platform gap
  # or (b) a work/school AAD identity is used to authenticate.
  - ado security grant --project X --permission ViewSecrets --yes-this-mutates-secret-read
  - ado security revoke --project X --permission ViewSecrets --yes-this-mutates-secret-read
  - ado pipelines-builds queue PROJECT --definition D --branch B
  - ado pipelines-builds cancel PROJECT BUILD_ID
  - ado pipelines-artifacts list PROJECT BUILD_ID
  - ado prs list PROJECT REPO
  - ado prs show PROJECT REPO ID
  - ado prs create PROJECT REPO --title T --source S --target T
  - ado prs complete PROJECT REPO ID --merge-strategy squash
  - ado prs approve PROJECT REPO ID
  - ado prs comments list PROJECT REPO PR_ID
  - ado prs comments add PROJECT REPO PR_ID --content TEXT --file-path PATH --line N
  - ado prs comments add PROJECT REPO PR_ID --content TEXT --file-path PATH --line N --end-line N
  - ado prs comments update PROJECT REPO PR_ID THREAD COMMENT --content TEXT
  - ado prs comments delete PROJECT REPO PR_ID THREAD
  - ado prs comments delete PROJECT REPO PR_ID THREAD --comment-id ID
  - ado prs comments resolve PROJECT REPO PR_ID THREAD
  - ado prs diff PROJECT REPO PR_ID
  - ado prs diff PROJECT REPO PR_ID --file PATH
  - ado prs diff PROJECT REPO PR_ID --unified
  - ado prs reviewers list PROJECT REPO PR_ID
  - ado prs reviewers list PROJECT REPO PR_ID --search QUERY  # fuzzy filter
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

## Quick start

```bash
# Build from source or download a binary
mix escript.build && cp ado /usr/local/bin/
# Or: curl -L -o ado https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado_linux

# Authenticate
ado login                                                   # browser OAuth, auto-detects org
ado login --org myorg --pat mytoken                        # PAT (--method pat inferred from --pat)

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

## Command reference

The full command reference is split into topic-focused files. Each file
contains copy-paste-ready examples for every subcommand in that area.

```bash
# Read a reference file:
ado skills read ado-cli references/prs.md

# Or from disk:
cat priv/skills/ado-cli/references/prs.md
```

| Area | Reference file | Commands |
|------|---------------|----------|
| Projects, Teams, Users, Extensions | `references/projects-teams-users.md` | `projects`, `teams`, `users`, `extensions` |
| Repos & Branch Policies | `references/repos.md` | `repos`, `branch-policies` |
| Work Items | `references/workitems.md` | `workitems` |
| Pull Requests | `references/prs.md` | `prs list/show/create/complete/approve/vote/abandon`, `prs comments`, `prs reviewers`, `prs diff` |
| Pipelines, Builds & CI Watch | `references/pipelines.md` | `pipelines`, `pipelines-builds`, `pipelines-artifacts`, `ci watch`, `pipelines vars`, `pipelines variables` |
| Releases, Packages & Test Results | `references/artifacts.md` | `releases`, `packages`, `test-results`, `test-coverage` |
| Administration | `references/admin.md` | `connections`, `security`, `agent-pools`, `banners`, `wikis`, `iterations`, `areas`, `skills` |

Quick examples for the most common tasks:

```bash
# Projects
ado projects list
ado projects create MyProject --description "My project" --visibility private

# Repos & PRs
ado repos list MyProject
ado prs create MyProject MyRepo --title "Add feature" --source dev --target main
ado prs complete MyProject MyRepo 42 --merge-strategy squash --delete-source
ado prs reviewers list MyProject MyRepo 42 --search alice   # fuzzy filter by name/email

# Work items
ado workitems list MyProject --type Bug --state Active
ado workitems query MyProject --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.AssignedTo] = @Me"
ado workitems create MyProject --type Bug --title "Fix login" --tags "bug,critical"

# Pipelines (YAML)
ado pipelines list MyProject --top 10
ado pipelines run MyProject 42 --branch main --variables "ENV=staging,DEBUG=true"

# Watch a build in real-time (live status + streaming logs)
ado ci watch MyProject 99
ado ci watch MyProject --latest --definition 42 --branch main

# Users, teams, security
ado users list
ado teams members list MyProject MyTeam
ado security groups create MyProject --name "Deployers"
ado security permissions namespaces

# Banners + packages
ado banners set --message "Maintenance in progress" --type warning
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
ado login --server https://ado.example.com --org DefaultCollection --pat xxx
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
