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
cp ado_cli /usr/local/bin/
```

Or build a standalone binary with Burrito:

```bash
MIX_ENV=prod mix release
# Binaries in burrito_out/
```

---

## Quick Start

```bash
# Set credentials
export ADO_ORG=myorg
export ADO_PAT=mypersonalaccesstoken

# List projects
ado_cli projects list

# Create a work item
ado_cli workitems create MyProject --type Bug --title "Fix login page"

# Open a pull request
ado_cli prs create MyProject MyRepo --title "Add feature" --source dev --target main
```

---

## Authentication

Multiple auth methods, auto-resolved in priority order:

| Priority | Method | How |
|----------|--------|-----|
| 1 | CLI flags | `--org ORG --pat TOKEN` |
| 2 | Environment variables | `ADO_ORG` + `ADO_PAT` |
| 3 | Azure CLI | If `az login` is active (bearer token) |
| 4 | Config file | `~/.ado_cli/config.json` (persistent) |

### Login (persistent)

```bash
# Personal Access Token
ado_cli login --method pat --org myorg --pat mytoken

# Interactive browser-based OAuth (Microsoft identity platform)
ado_cli login --method device --org myorg

# Check status
ado_cli whoami

# Remove stored credentials
ado_cli logout
```

---

## Self-Hosted Azure DevOps Server

For on-premises / private cloud Azure DevOps Server:

```bash
# CLI flag
ado_cli --server https://ado.internal.example.com --org DefaultCollection projects list

# Environment variable
export ADO_SERVER=https://ado.internal.example.com

# Login with server
ado_cli login --method pat --server https://ado.internal.example.com --org DefaultCollection --pat xxx
```

When `--server` is set, `--org` becomes the collection name (e.g. `DefaultCollection`).

---

## Commands

### Projects

```bash
ado_cli projects list                     # List all projects
ado_cli projects list --state wellFormed   # Filter by state
ado_cli projects list --top 10             # Paginate

ado_cli projects show MyProject           # Show details
ado_cli projects show MyProject --capabilities

ado_cli projects create MyNewProject       # Create
ado_cli projects create MyProj --description "My project" --visibility private --process agile

ado_cli projects update MyProject --name NewName   # Rename
ado_cli projects update MyProject --description "Updated" --name "New Name"

ado_cli projects delete MyProject          # Delete (with confirmation)
ado_cli projects delete MyProject --force  # Skip confirmation
```

### Repositories

```bash
ado_cli repos list MyProject              # List repos
ado_cli repos show MyProject MyRepo       # Show details

ado_cli repos create MyProject MyNewRepo                   # Create
ado_cli repos create MyProject MyRepo --default-branch develop

ado_cli repos delete MyProject MyRepo                       # Delete (with confirmation)
ado_cli repos delete MyProject MyRepo --force               # Skip confirmation

ado_cli repos branches MyProject MyRepo                    # List branches
ado_cli repos branches MyProject MyRepo --filter "feature/" # Filter branches
```

### Work Items

```bash
ado_cli workitems list MyProject                          # List all
ado_cli workitems list MyProject --type Bug               # Filter by type
ado_cli workitems list MyProject --state Active            # Filter by state
ado_cli workitems list MyProject --assigned-to "John Doe"  # Filter by assignee

ado_cli workitems show 42                                  # Show details
ado_cli workitems show 42 --expand all

ado_cli workitems query MyProject --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.State] = 'Active'"

ado_cli workitems create MyProject --type Bug --title "Fix login"       # Create
ado_cli workitems create MyProject --type "User Story" --title "New feature" \
  --description "As a user..." --assigned-to "Jane" --priority 2 --tags "frontend,ux"

ado_cli workitems update 42 --state Resolved               # Update state
ado_cli workitems update 42 --title "Updated title" --assigned-to "Bob" --priority 1 --tags "bug,critical"
```

### Pipelines

```bash
ado_cli pipelines list MyProject           # List pipelines
ado_cli pipelines list MyProject --top 10
ado_cli pipelines list MyProject --folder "\\CI"

ado_cli pipelines show MyProject 1         # Show pipeline definition

ado_cli pipelines run MyProject 1          # Trigger a run
ado_cli pipelines run MyProject 1 --branch feature/login
ado_cli pipelines run MyProject 1 --variables "ENV=staging,DEBUG=true"
```

### Pull Requests

```bash
ado_cli prs list MyProject MyRepo                        # List active PRs
ado_cli prs list MyProject MyRepo --status all            # All PRs
ado_cli prs list MyProject MyRepo --creator "John"        # By creator

ado_cli prs show MyProject MyRepo 42                      # Show PR details

ado_cli prs create MyProject MyRepo --title "New feature" \  # Create PR
  --source feature/new --target main
ado_cli prs create MyProject MyRepo --title "WIP" \
  --source dev --target main --description "Work in progress" --draft

ado_cli prs complete MyProject MyRepo 42                   # Complete (merge)
ado_cli prs complete MyProject MyRepo 42 --delete-source    # + delete branch
ado_cli prs complete MyProject MyRepo 42 --merge-strategy squash  # Squash merge

ado_cli prs abandon MyProject MyRepo 42                    # Abandon PR
```

### Releases

```bash
ado_cli releases list MyProject                         # List releases
ado_cli releases list MyProject --status active          # Filter by status
ado_cli releases list MyProject --definition-id 1        # By definition

ado_cli releases show MyProject 42                       # Show release + environments
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
# Escript (local binary)
mix escript.build
./ado_cli projects list

# Cross-platform binaries (macOS, Linux, Windows)
MIX_ENV=prod mix release
# Binaries in burrito_out/
```

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
│   ├── ado_cli.ex                    # Main module
│   ├── ado_cli/
│   │   ├── application.ex            # Burrito entry point
│   │   ├── auth.ex                   # Multi-provider authentication
│   │   ├── client.ex                 # Finch HTTP client
│   │   ├── config_file.ex            # ~/.ado_cli/config.json persistence
│   │   └── cli/
│   │       ├── cli.ex                # CLI dispatch & global options
│   │       ├── helpers.ex            # Shared output/error helpers
│   │       ├── projects.ex           # projects list|show|create|update|delete
│   │       ├── repos.ex              # repos list|show|create|delete|branches
│   │       ├── work_items.ex         # workitems list|show|query|create|update
│   │       ├── pipelines.ex          # pipelines list|show|run
│   │       ├── pull_requests.ex      # prs list|show|create|complete|abandon
│   │       ├── releases.ex           # releases list|show
│   │       ├── auth_commands.ex      # login command
│   │       ├── logout.ex             # logout command
│   │       └── whoami.ex             # whoami command
│   └── mix/tasks/ci/
│       └── dialyzer.ex              # CI dialyzer with Finch false-positive filter
├── config/
│   └── config.exs                    # Application configuration
├── test/
├── .credo.exs                        # Credo configuration (strict mode)
├── AGENTS.md                         # CI quality gate principles
└── mix.exs                           # Project definition & aliases
```

---

## API Coverage

All commands use Azure DevOps REST API v7.1 (configurable via `ADO_API_VERSION`).

| Area | Endpoint | Operations |
|------|----------|------------|
| Core | `_apis/projects` | list, show, create, update, delete |
| Git | `{project}/_apis/git/repositories` | list, show, create, delete, branches |
| Git | `{project}/_apis/git/repositories/{id}/pullrequests` | list, show, create, update |
| Work Items | `{project}/_apis/wit/wiql` | query |
| Work Items | `_apis/wit/workitems/{id}` | show, update |
| Work Items | `{project}/_apis/wit/workitems/${type}` | create |
| Pipelines | `{project}/_apis/pipelines` | list, show |
| Pipelines | `{project}/_apis/pipelines/{id}/runs` | run |
| Releases | `{project}/_apis/release/releases` | list, show |

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

## License

MIT
