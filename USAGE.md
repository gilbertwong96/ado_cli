# CLI Usage Guide

All commands follow the pattern:

```bash
ado [global-options] <command> [subcommand] [arguments] [options]
```

## Global Options

```
-o, --org ORG       Azure DevOps organization name      [env: ADO_ORG]
-t, --pat TOKEN     Personal Access Token               [env: ADO_PAT]
-s, --server URL    Self-hosted server URL              [env: ADO_SERVER]
-v, --verbose       Enable verbose output
    --json          Output raw JSON instead of tables
    --help          Show help for any command
```

## Projects

### List projects
```bash
ado projects list
ado projects list --state wellFormed        # filter by state
ado projects list --top 10                  # paginate
ado projects list --json                    # JSON output
```

### Show project details
```bash
ado projects show MyProject
ado projects show MyProject --capabilities
```

### Create a project
```bash
ado projects create MyNewProject
ado projects create MyProj --description "My description" --visibility private --process agile
```

### Update a project
```bash
ado projects update MyProject --name NewName
ado projects update MyProject --description "Updated description"
```

### Delete a project
```bash
ado projects delete MyProject               # prompts for confirmation
ado projects delete MyProject --force       # skip confirmation
```

## Repositories

### List repositories
```bash
ado repos list MyProject
```

### Show repository details
```bash
ado repos show MyProject MyRepo
```

### Create a repository
```bash
ado repos create MyProject MyNewRepo
ado repos create MyProject MyRepo --default-branch develop
```

### Delete a repository
```bash
ado repos delete MyProject MyRepo
ado repos delete MyProject MyRepo --force   # skip confirmation
```

### List branches
```bash
ado repos branches MyProject MyRepo
ado repos branches MyProject MyRepo --filter "feature/"
```

## Work Items

### List work items
```bash
ado workitems list MyProject
ado workitems list MyProject --type Bug
ado workitems list MyProject --state Active
ado workitems list MyProject --assigned-to "John Doe"
```

### Show work item details
```bash
ado workitems show 42
ado workitems show 42 --expand all
```

### WIQL query
```bash
ado workitems query MyProject --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.State] = 'Active'"
```

### Create a work item
```bash
ado workitems create MyProject --type Bug --title "Fix login page"
ado workitems create MyProject --type "User Story" --title "New feature" \
  --description "As a user..." --assigned-to "Jane" --priority 2 --tags "frontend,ux"
```

### Update a work item
```bash
ado workitems update 42 --state Resolved
ado workitems update 42 --title "Updated title" --assigned-to "Bob" --priority 1 --tags "bug,critical"
```

## Pipelines

### Watch a build in real-time

```bash
# Stream live status + per-line log output for a running build.
# Like `tail -f` for CI. Exits when the build completes or on Ctrl+C.
ado ci watch MyProject 123
ado ci watch MyProject --latest --definition 42 --branch main
ado ci watch MyProject 123 --poll-interval 500
```

### List pipelines
```bash
ado pipelines list MyProject
ado pipelines list MyProject --top 10
ado pipelines list MyProject --folder "\\CI"
```

### Show pipeline definition
```bash
ado pipelines show MyProject 1
```

### Trigger a pipeline run
```bash
ado pipelines run MyProject 1
ado pipelines run MyProject 1 --branch feature/login
ado pipelines run MyProject 1 --variables "ENV=staging,DEBUG=true"
```

## Pull Requests

### List pull requests
```bash
ado prs list MyProject MyRepo
ado prs list MyProject MyRepo --status all
ado prs list MyProject MyRepo --creator "John"
```

### Show PR details
```bash
ado prs show MyProject MyRepo 42
```

### Create a pull request
```bash
ado prs create MyProject MyRepo --title "New feature" \
  --source feature/new --target main
ado prs create MyProject MyRepo --title "WIP" \
  --source dev --target main --description "Work in progress" --draft
```

### Complete (merge) a pull request
```bash
ado prs complete MyProject MyRepo 42
ado prs complete MyProject MyRepo 42 --delete-source
ado prs complete MyProject MyRepo 42 --merge-strategy squash
```

### Abandon a pull request
```bash
ado prs abandon MyProject MyRepo 42
```

## Releases

### List releases
```bash
ado releases list MyProject
ado releases list MyProject --status active
ado releases list MyProject --definition-id 1
```

### Show release details
```bash
ado releases show MyProject 42
```

## Authentication Commands

### Login
```bash
ado login                          # browser OAuth (default)
ado login --org myorg              # browser OAuth with org
ado login --method pat --org myorg --pat xxxxx
ado login --method device --org myorg
ado login --method pat --server https://ado.example.com --org Coll --pat xxx
```

### Check status
```bash
ado whoami
```

### Logout
```bash
ado logout
```

## Output Control

| Flag | Effect |
|------|--------|
| `--json` | Raw JSON output instead of formatted tables |
| `--verbose` | Detailed logging for troubleshooting |
| `--help` | Show help for any command or subcommand |

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `ADO_ORG` | Organization name |
| `ADO_PAT` | Personal Access Token |
| `ADO_SERVER` | Self-hosted server URL |
| `ADO_OAUTH_CLIENT_ID` | Override the OAuth client id the interactive login flows use |

`ADO_PAT` is read only from the environment and never persisted. A blank value
counts as unset. `ADO_API_VERSION` is **not** read: every request carries
`api-version=7.1`, and an endpoint that needs another version supplies it itself.
