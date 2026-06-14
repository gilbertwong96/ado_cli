---
description: Main ado skill — setup, authentication, and complete command reference for all 24 service areas
version: "0.3.0"
commands:
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
  - ado branch-policies show PROJECT REPO POLICY_ID
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
  - ado extensions install EXTENSION
  - ado agent-pools list
  - ado agent-pools show POOL_ID
  - ado connections list PROJECT
  - ado security groups list PROJECT SCOPE
  - ado banners set --message TEXT --type warning
  - ado banners delete
  - ado packages list PROJECT FEED
  - ado ci watch PROJECT BUILD_ID                            # stream live build logs
  - ado skills list                                            # list available skills
  - ado skills describe NAME                                   # skill frontmatter + command index
  - ado skills read NAME                                       # full skill content
  - ado skills search "QUERY"                                  # find skill by topic
---

# ado

A self-contained command-line tool for managing Azure DevOps — projects, repos,
work items, pipelines, PRs, releases, artifacts, packages, and more. Cross-compiled
to single-file executables for macOS, Linux, and Windows via Burrito.

## First Time Setup

```bash
# Build from source
mix escript.build
cp ado /usr/local/bin/

# Authenticate (pick one)
ado login --org {your-org}                              # browser OAuth
ado login --method pat --org {org} --pat {token}       # Personal Access Token
ado login --method device --org {org}                  # no browser needed
```

The CLI auto-detects the org from the token if `--org` is omitted.

## Global Options

```
--org, -o ORG       Organization (or ADO_ORG env var; auto-detected on login)
--pat, -t TOKEN     Personal Access Token (or ADO_PAT env var)
--server, -s URL    Self-hosted server (or ADO_SERVER env var)
--json              Output raw JSON instead of formatted tables
--verbose, -v       Verbose output
```

## Command Groups

| Group | Subcommands |
|-------|-------------|
| `login`, `logout`, `whoami` | Authentication & status |
| `projects` | list, show, create, update, delete |
| `repos` | list, show, create, delete |
| `branch-policies` | list, show, create, update, delete |
| `workitems` | list, show, create, update, delete, comments, attachments |
| `pipelines` | list, show, run, create, update, delete |
| `pipelines vars` | Variable groups: list, show, create, update, delete |
| `pipelines variables` | Per-pipeline variables: list, create, delete |
| `pipelines-builds` | Classic builds: list, show, queue, cancel, tags, definitions |
| `pipelines-folders` | Folders: list, create, delete |
| `pipelines-artifacts` | Run artifacts: list, download |
| `prs` | list, show, create, complete, abandon, approve, vote, comments |
| `releases` | list, show, create, update |
| `iterations` | Sprints: list, show, create, update, delete |
| `areas` | Area paths: list, show, create, update, delete |
| `wikis` | Wikis and pages: list, show, create, update, delete |
| `teams` | Teams: list, show, create, update, delete, members |
| `users` | User entitlements: list, show, add, remove |
| `extensions` | Marketplace: list, show, install, uninstall, enable, disable |
| `agent-pools` | Pools and queues: list, show, queues |
| `connections` | Service connections: list, show |
| `security groups` | Groups: list, show, create, delete, members |
| `security permissions` | ACLs: list, namespaces |
| `banners` | Org-wide notifications: show, set, delete |
| `packages` | Universal Packages: list, versions, show |
| `skills` | list, read |

## Conventions

1. **Project and repo names with spaces**: Quote them.
   ```bash
   ado repos list "Employee Management"
   ```

2. **Org types**: Works with AAD (work/school), MSA (personal `*.visualstudio.com`),
   and self-hosted DevOps Server orgs. No `az` CLI dependency.

3. **Error handling**: Non-zero exit code + stderr message on failure. Use `--verbose`
   for stack traces.

4. **Secrets**: Never log or store PATs in plain text. Use `--pat` flag or `ADO_PAT`
   env var (both are transient — never written to disk by the CLI).

5. **Output**: Default is formatted tables. Use `--json` for machine-readable output.

## Quick Examples

```bash
# List projects (with PAT, headless)
ado --org myorg --pat mytoken projects list

# Authenticate interactively (browser)
ado login --org myorg

# Auto-detect org from token
ado login

# Create a work item
ado workitems create MyProject --type Bug --title "Fix login page"

# Create a PR
ado prs create MyProject MyRepo --title "Add feature" --source dev --target main

# Trigger a pipeline with variables
ado pipelines run MyProject 42 --branch main --variables "ENV=staging,DEBUG=true"

# Watch a build in real-time (live status + streaming logs)
ado ci watch MyProject 99
ado ci watch MyProject --latest --definition 42 --branch main

# Add a secret to a variable group
ado pipelines vars create MyProject --name CI --variables "DB_PASS=hunter2" --secret DB_PASS

# Run on Linux server without browser
export ADO_ORG=myorg ADO_PAT=mytoken
ado projects list

# Check auth status
ado whoami
```

## Help

Every command and subcommand supports `--help`:
```bash
ado --help
ado projects --help
ado projects create --help
```

## Exit Codes

- 0   Success
- 1   Generic error
- 2   API error (4xx/5xx response)
- 3   Auth not configured

Use `set -e` in shell scripts; non-zero always means failure.
