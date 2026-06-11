# CLI Usage Guide

All commands follow the pattern:

```bash
ado_cli [global-options] <command> [subcommand] [arguments] [options]
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
ado_cli projects list
ado_cli projects list --state wellFormed        # filter by state
ado_cli projects list --top 10                  # paginate
ado_cli projects list --json                    # JSON output
```

### Show project details
```bash
ado_cli projects show MyProject
ado_cli projects show MyProject --capabilities
```

### Create a project
```bash
ado_cli projects create MyNewProject
ado_cli projects create MyProj --description "My description" --visibility private --process agile
```

### Update a project
```bash
ado_cli projects update MyProject --name NewName
ado_cli projects update MyProject --description "Updated description"
```

### Delete a project
```bash
ado_cli projects delete MyProject               # prompts for confirmation
ado_cli projects delete MyProject --force       # skip confirmation
```

## Repositories

### List repositories
```bash
ado_cli repos list MyProject
```

### Show repository details
```bash
ado_cli repos show MyProject MyRepo
```

### Create a repository
```bash
ado_cli repos create MyProject MyNewRepo
ado_cli repos create MyProject MyRepo --default-branch develop
```

### Delete a repository
```bash
ado_cli repos delete MyProject MyRepo
ado_cli repos delete MyProject MyRepo --force   # skip confirmation
```

### List branches
```bash
ado_cli repos branches MyProject MyRepo
ado_cli repos branches MyProject MyRepo --filter "feature/"
```

## Work Items

### List work items
```bash
ado_cli workitems list MyProject
ado_cli workitems list MyProject --type Bug
ado_cli workitems list MyProject --state Active
ado_cli workitems list MyProject --assigned-to "John Doe"
```

### Show work item details
```bash
ado_cli workitems show 42
ado_cli workitems show 42 --expand all
```

### WIQL query
```bash
ado_cli workitems query MyProject --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.State] = 'Active'"
```

### Create a work item
```bash
ado_cli workitems create MyProject --type Bug --title "Fix login page"
ado_cli workitems create MyProject --type "User Story" --title "New feature" \
  --description "As a user..." --assigned-to "Jane" --priority 2 --tags "frontend,ux"
```

### Update a work item
```bash
ado_cli workitems update 42 --state Resolved
ado_cli workitems update 42 --title "Updated title" --assigned-to "Bob" --priority 1 --tags "bug,critical"
```

## Pipelines

### List pipelines
```bash
ado_cli pipelines list MyProject
ado_cli pipelines list MyProject --top 10
ado_cli pipelines list MyProject --folder "\\CI"
```

### Show pipeline definition
```bash
ado_cli pipelines show MyProject 1
```

### Trigger a pipeline run
```bash
ado_cli pipelines run MyProject 1
ado_cli pipelines run MyProject 1 --branch feature/login
ado_cli pipelines run MyProject 1 --variables "ENV=staging,DEBUG=true"
```

## Pull Requests

### List pull requests
```bash
ado_cli prs list MyProject MyRepo
ado_cli prs list MyProject MyRepo --status all
ado_cli prs list MyProject MyRepo --creator "John"
```

### Show PR details
```bash
ado_cli prs show MyProject MyRepo 42
```

### Create a pull request
```bash
ado_cli prs create MyProject MyRepo --title "New feature" \
  --source feature/new --target main
ado_cli prs create MyProject MyRepo --title "WIP" \
  --source dev --target main --description "Work in progress" --draft
```

### Complete (merge) a pull request
```bash
ado_cli prs complete MyProject MyRepo 42
ado_cli prs complete MyProject MyRepo 42 --delete-source
ado_cli prs complete MyProject MyRepo 42 --merge-strategy squash
```

### Abandon a pull request
```bash
ado_cli prs abandon MyProject MyRepo 42
```

## Releases

### List releases
```bash
ado_cli releases list MyProject
ado_cli releases list MyProject --status active
ado_cli releases list MyProject --definition-id 1
```

### Show release details
```bash
ado_cli releases show MyProject 42
```

## Authentication Commands

### Login
```bash
ado_cli login                          # browser OAuth (default)
ado_cli login --org myorg              # browser OAuth with org
ado_cli login --method pat --org myorg --pat xxxxx
ado_cli login --method device --org myorg
ado_cli login --method pat --server https://ado.example.com --org Coll --pat xxx
```

### Check status
```bash
ado_cli whoami
```

### Logout
```bash
ado_cli logout
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
| `ADO_OAUTH_CLIENT_ID` | Custom OAuth client ID |
| `ADO_API_VERSION` | API version (default: `7.1`) |
