# AdoCli

A command-line tool for managing Azure DevOps — projects, repositories, work items,
pipelines, pull requests, and releases. Works with both **cloud** (`dev.azure.com`)
and **self-hosted** Azure DevOps Server.

Built with [Finch](https://hex.pm/packages/finch),
[CLI Mate](https://hex.pm/packages/cli_mate), and
[Burrito](https://hex.pm/packages/burrito).

[![CI](https://img.shields.io/badge/ci-passing-brightgreen)]()

---

## Installation

```bash
git clone https://github.com/your-org/ado_cli.git
cd ado_cli
mix deps.get
mix escript.build
```

Move the escript into your PATH:

```bash
cp ado /usr/local/bin/
```

Or build standalone binaries with Burrito:

```bash
MIX_ENV=prod mix release
# Binaries in burrito_out/ (macOS aarch64, Linux x86_64/aarch64, Windows x86_64/aarch64)
```

> **Note**: `mix release` in dev produces an OTP system service (`start`, `stop`, `eval`)
> — not a CLI tool. Use `mix escript.build` for dev iteration.

---

## Quick Start

```bash
# Set credentials
export ADO_ORG=myorg
export ADO_PAT=mypersonalaccesstoken

# List projects
ado projects list

# Create a work item
ado workitems create MyProject --type Bug --title "Fix login page"

# Open a pull request
ado prs create MyProject MyRepo --title "Add feature" --source dev --target main
```

---

## Authentication

Multiple auth methods, auto-resolved in priority order. **No `az` CLI required.**

| Priority | Method | How |
|----------|--------|-----|
| 1 | CLI flags | `--org ORG --pat TOKEN` |
| 2 | Environment variables | `ADO_ORG` + `ADO_PAT` |
| 3 | Config file | `~/.ado_cli/config.json` (persistent) |

Auth via `ado login` is browser-based OAuth — no `az login` is detected or
required. The CLI is self-contained.

### Login (persistent)

```bash
# Personal Access Token (recommended for CI / headless)
ado login --method pat --org myorg --pat mytoken

# Interactive browser-based OAuth (auto-detects org on success)
ado login --org myorg
# Or just:
ado login

# Device code (no browser on this machine)
ado login --method device --org myorg

# Check status
ado whoami

# Remove stored credentials
ado logout
```

---

## Self-Hosted Azure DevOps Server

For on-premises / private cloud Azure DevOps Server:

```bash
# CLI flag
ado --server https://ado.internal.example.com --org DefaultCollection projects list

# Environment variable
export ADO_SERVER=https://ado.internal.example.com

# Login with server
ado login --method pat --server https://ado.internal.example.com --org DefaultCollection --pat xxx
```

When `--server` is set, `--org` becomes the collection name (e.g. `DefaultCollection`).

---

## Commands

The CLI covers **24 service areas** of Azure DevOps. The canonical,
up-to-date reference is the `ado_cli` skill:

```bash
ado skills read ado_cli
# Or read on disk: cat priv/skills/ado_cli/SKILL.md
```

Quick examples:

```bash
# Projects
ado projects list
ado projects create MyProject --description "My project" --visibility private

# Repos & PRs
ado repos list MyProject
ado repos create MyProject MyRepo
ado prs create MyProject MyRepo --title "Add feature" --source dev --target main
ado prs complete MyProject MyRepo 42 --merge-strategy squash --delete-source

# Work items
ado workitems list MyProject --type Bug --state Active
ado workitems query MyProject --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.AssignedTo] = @Me"
ado workitems create MyProject --type Bug --title "Fix login" --tags "bug,critical"

# Pipelines (YAML)
ado pipelines list MyProject --top 10
ado pipelines run MyProject 42 --branch main --variables "ENV=staging,DEBUG=true"

# Pipelines (Classic builds)
ado pipelines-builds queue MyProject --definition 5 --branch main
ado pipelines-builds cancel MyProject 99

# Variable groups + per-pipeline variables (secrets)
ado pipelines vars create MyProject --name prod-secrets --variables "DB_HOST=db,DB_PASS=secret" --secret DB_PASS
ado pipelines variables create MyProject 42 --key DEPLOY_TOKEN --value "ghp_xxx" --secret

# Iterations, areas, wikis
ado iterations list MyProject MyTeam
ado areas create MyProject --name Backend
ado wikis create MyProject --name Engineering

# Users, teams, security
ado users list
ado teams members list MyProject MyTeam
ado security groups create MyProject --name "Deployers"
ado security permissions namespaces

# Banners + packages
ado banners set --message "Maintenance in progress" --type warning
ado packages list MyProject MyFeed
ado imports create MyProject new-repo --url https://github.com/org/repo.git

# CI / scriptable
ado --json projects list | jq '.[].name'
```

For the complete command reference including all options, flags, and JSON
output schemas, see the embedded `ado_cli` skill (`ado skills read ado_cli`).
Every command also supports `--help` for inline reference:

```bash
ado --help
ado projects --help
ado pipelines vars create --help
```

---

## Global Options

```
-o, --org ORG       Azure DevOps organization / collection name [env: ADO_ORG]
-t, --pat TOKEN     Personal Access Token [env: ADO_PAT]
-s, --server URL    Self-hosted Azure DevOps Server URL [env: ADO_SERVER]
-v, --verbose       Enable verbose output
    --json          Output raw JSON instead of formatted tables
    --help          Show help for any command
```

---

## Development

### Setup

```bash
# Install dependencies
mix deps.get

# Compile
mix compile

# Run tests
mix test

# Run in dev mode
mix run -e 'AdoCli.CLI.run(System.argv())' -- projects list
```

### Build

```bash
# Escript (fast CLI binary for dev)
mix escript.build
./ado projects list

# Burrito native binaries (for distribution)
MIX_ENV=prod mix release
# → burrito_out/  (macOS aarch64, Linux x86_64/aarch64, Windows x86_64/aarch64)
```

> `mix release` in dev produces an OTP system service (`start`/`stop`/`eval`).
> Use `mix escript.build` for CLI iteration.

### Documentation

```bash
# Generate HTML docs
mix docs
# Open doc/index.html

# Also outputs: doc/llms.txt (AI context), doc/ado_cli.epub
```

### Quality

```bash
# Full CI pipeline (compile, format, credo, deps check, xref, dialyzer)
mix ci

# Quick checks
mix lint         # Credo static analysis
mix inspect      # Project structure map (reach)
mix health       # Dead code & smell detection
mix test         # Unit tests
```

---

## Quality Tools

| Tool | Purpose | Mix Task |
|------|---------|----------|
| [Credo](https://hex.pm/packages/credo) | Static code analysis | `mix credo --strict` |
| [Dialyxir](https://hex.pm/packages/dialyxir) | Type checking | `mix dialyzer` |
| [ex_dna](https://hex.pm/packages/ex_dna) | Code duplication detection | `mix ex_dna` |
| [ex_slop](https://hex.pm/packages/ex_slop) | AI-generated code slop checks | (loaded by Credo) |
| [Reach](https://hex.pm/packages/reach) | Program dependence graph | `mix reach.map` |
| [mix_audit](https://hex.pm/packages/mix_audit) | Dependency vulnerability audit | `mix deps.audit` |
| [ExDoc](https://hex.pm/packages/ex_doc) | Documentation generation | `mix docs` |

---

## Project Structure

```
ado_cli/
├── lib/
│   ├── ado_cli.ex                    # Main module (escript entry point)
│   ├── ado_cli/
│   │   ├── application.ex            # Burrito entry point
│   │   ├── auth.ex                   # Multi-provider authentication (PAT, browser, device)
│   │   ├── client.ex                 # Finch HTTP client with redirect handling
│   │   ├── config_file.ex            # ~/.ado_cli/config.json persistence
│   │   ├── skills.ex                 # Embedded skill file reader
│   │   └── cli/
│   │       ├── cli.ex                # CLI dispatch & global options
│   │       ├── helpers.ex            # Shared output/error helpers
│   │       ├── projects.ex           # projects list|show|create|update|delete
│   │       ├── repos.ex              # repos list|show|create|delete
│   │       ├── branch_policies.ex    # branch-policies list|show|create|update|delete
│   │       ├── work_items.ex         # workitems list|show|query|create|update|delete|comments
│   │       ├── pipelines.ex          # pipelines list|show|run|create|update|delete|vars|variables
│   │       ├── builds.ex             # pipelines-builds (classic) list|show|queue|cancel|tags|definitions
│   │       ├── folders.ex            # pipelines-folders list|create|delete
│   │       ├── run_artifacts.ex      # pipelines-artifacts list|download
│   │       ├── pull_requests.ex      # prs list|show|create|complete|abandon|approve|vote|comments
│   │       ├── releases.ex           # releases list|show
│   │       ├── iterations.ex         # iterations list|show|create|update|delete
│   │       ├── areas.ex              # areas list|show|create|update|delete
│   │       ├── wikis.ex              # wikis and pages list|show|create|update
│   │       ├── teams.ex              # teams list|show|create|update|delete|members
│   │       ├── users.ex              # users list|show|add|remove
│   │       ├── extensions.ex         # extensions list|show|install|uninstall|enable|disable
│   │       ├── agent_pools.ex        # agent-pools list|show|queues
│   │       ├── connections.ex        # connections list|show
│   │       ├── security.ex           # security groups + permissions
│   │       ├── banners.ex            # banners show|set|delete
│   │       ├── packages.ex           # packages list|versions|show
│   │       ├── imports.ex            # imports list|show|create (GitHub→AzDo migration)
│   │       ├── auth_commands.ex      # login command
│   │       ├── logout.ex             # logout command
│   │       ├── whoami.ex             # whoami command
│   │       └── skills.ex             # skills list|read command
│   └── mix/tasks/ci/
│       └── dialyzer.ex              # CI dialyzer with Finch false-positive filter
├── priv/skills/                      # Embedded skill content (loaded at compile time)
│   ├── ado_cli/                      # Main reference
│   ├── ado_auth/                     # Auth details
│   └── ado_ci/                       # CI/CD patterns
├── config/
│   └── config.exs                    # Application configuration
├── test/
├── .credo.exs                        # Credo configuration (strict mode)
├── AGENTS.md                         # CI quality gate principles
├── SIGNING.md                        # macOS code signing + notarization guide
└── mix.exs                           # Project definition & aliases
```

## Command Reference

The CLI covers **24 service areas** of Azure DevOps. The canonical,
up-to-date reference is the **`ado_cli` skill** embedded in the binary:

```bash
ado skills read ado_cli
```

Or see the on-disk source at `priv/skills/ado_cli/SKILL.md`. The skill
includes the full command table, conventions, and quick-start examples.

---

## API Coverage

All commands use Azure DevOps REST API v7.1 (configurable via `ADO_API_VERSION`).
The CLI covers **24 service areas** of Azure DevOps. See the embedded
`ado_cli` skill (`ado skills read ado_cli`) for the full command reference
and per-endpoint coverage.

| Service area | Endpoint | Status |
|--------------|----------|--------|
| Core / Projects | `_apis/projects` | list, show, create, update, delete |
| Git / Repos | `{project}/_apis/git/repositories` | list, show, create, delete |
| Git / Pull Requests | `{project}/_apis/git/repositories/{id}/pullrequests` | list, show, create, complete, abandon, vote, comments |
| Git / Policies | `{project}/_apis/policy/configurations` | list, show, create, update, delete |
| Git / Imports | `{project}/_apis/git/importRequests` | list, show, create |
| Work Items | `{project}/_apis/wit/wiql`, `_apis/wit/workitems/{id}` | query, show, create, update, delete, comments, attachments |
| Sprints / Iterations | `{project}/_apis/work/teamsettings/iterations` | list, show, create, update, delete |
| Area Paths | `{project}/_apis/wit/classificationNodes/areas` | list, show, create, update, delete |
| Pipelines (YAML) | `{project}/_apis/pipelines`, `/runs` | list, show, run, create, update, delete |
| Pipelines (Variables) | `{project}/_apis/pipelines/{id}/variables` | list, create, delete |
| Variable Groups | `{project}/_apis/distributedtask/variablegroups` | list, show, create, update, delete |
| Pipelines (Classic) | `{project}/_apis/build/builds` | list, show, queue, cancel, tags, definitions |
| Pipelines (Folders) | `{project}/_apis/pipelines/folders` | list, create, delete |
| Pipeline Artifacts | `{project}/_apis/build/builds/{id}/artifacts` | list, download |
| Releases | `{project}/_apis/release/releases` | list, show |
| Wikis | `{project}/_apis/wiki/wikis`, `pages` | list, show, create, update, delete |
| Teams | `_apis/teams` | list, show, create, update, delete, members |
| Users | `_apis/accesscontrolentries` | list, show, add, remove |
| Extensions | `_apis/extensionmanagement/installedextensions` | list, show, install, uninstall, enable, disable |
| Agent Pools | `_apis/distributedtask/pools`, `/queues` | list, show, queues |
| Service Connections | `{project}/_apis/serviceendpoint/endpoints` | list, show |
| Security Groups | `_apis/graph/groups` | list, show, create, delete, members |
| Security Permissions | `_apis/securitynamespaces`, `_apis/permissions` | namespaces, list |
| Admin Banners | `_apis/settings/entries/banners` | show, set, delete |
| Universal Packages | `{project}/_apis/packaging/feeds/{id}/packages` | list, versions, show |

---

## CI Pipeline

Every commit must pass the full CI gate (`mix ci`):

| Step | Check |
|------|-------|
| 1 | `compile --all-warnings --warnings-as-errors` |
| 2 | `format --check-formatted` |
| 3 | `credo --strict` |
| 4 | `deps.unlock --check-unused` |
| 5 | `deps.audit` |
| 6 | `xref graph --label compile-connected --fail-above 0` |
| 7 | `dialyzer` (with Finch false-positive filtering) |

See `AGENTS.md` for the full quality gate specification.

---

## Skills for AI Agents

`ado` ships with embedded skill files (YAML-frontmatter Markdown) for use by
AI coding agents. The skills teach agents how to use the CLI, not the
underlying REST API — agents should never call the Azure DevOps REST API
directly when the CLI is available.

### What's in the binary

Three skills are embedded at compile time and exposed via:

```bash
ado skills list                       # List all available skills
ado skills read ado_cli               # Main reference: setup, auth, all 24 command groups
ado skills read ado_auth              # Authentication details (PAT, browser, MSA, troubleshooting)
ado skills read ado_ci                # CI/CD patterns: GitHub Actions, GitLab CI, secrets, scripts
```

Skills are also on disk in `priv/skills/{name}/SKILL.md` for repository-level
inspection or to copy into an agent's `~/.claude/skills/` directory.

### For AI agents: how to use these skills

1. **Read `ado_cli` first** when you need to discover available commands. The
   `Command Groups` table in that skill is the canonical reference — it
   covers all 24 service areas (projects, repos, workitems, pipelines,
   vars, builds, artifacts, folders, prs, releases, iterations, areas,
   wikis, teams, users, extensions, agent-pools, connections, security
   groups, security permissions, banners, packages, imports, branch-policies).

2. **Read `ado_auth`** before invoking any command, to confirm which auth
   method is in effect and how to handle the `Not authenticated` error. The
   skill explains PAT vs browser OAuth vs device code, MSA personal org
   support, and the priority order of CLI flags / env vars / config file.

3. **Read `ado_ci`** when running `ado` in a pipeline / CI script. The
   skill documents:
   - The recommended env-var-based auth (`ADO_ORG` + `ADO_PAT`)
   - JSON output (`ado --json ... | jq ...`) for scripting
   - Exit codes (`0` success, `1` generic, `2` API error, `3` auth)
   - GitHub Actions and GitLab CI examples

4. **Always prefer `--json` flag** when scripting. The CLI emits stable
   JSON in non-tabular mode; `--json` also skips table-formatting errors
   that can appear in CI environments that don't handle ANSI escapes well.

5. **Always quote project and repo names** that contain spaces:
   ```bash
   ado repos list "Employee Management"
   ```

6. **Never log credentials.** The CLI masks PATs in error output, but
   don't echo them yourself. Pass via `ADO_PAT` env var or `--pat` flag
   only.

### Loading skills into an agent

If your agent supports the `skills` discovery protocol:

```bash
# Quick check
ado skills list | head

# Dump a skill to a file (e.g., for an agent's skill directory)
ado skills read ado_cli > ~/.claude/skills/ado_cli/SKILL.md
```

If your agent does **not** support the skills protocol, paste the output of
`ado skills read ado_cli` into its context — it contains the full command
reference (~130 lines).

### Skills vs REST API

The skills deliberately **do not** document the Azure DevOps REST API.
Agents reading the skills will call the CLI rather than constructing
HTTP requests by hand. This is intentional: the CLI handles auth, retries,
result pagination, and cross-org quirks that vary between AAD / MSA /
self-hosted deployments.

---

## License

MIT
