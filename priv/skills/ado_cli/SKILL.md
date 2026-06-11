---
description: Main ado_cli skill — how to use the CLI, setup, auth, and command groups
version: "0.1.0"
---

# ado_cli

A command-line tool for managing Azure DevOps — projects, repositories,
work items, pipelines, pull requests, and releases.

## First Time Setup

```bash
# Install
mix escript.build
cp ado_cli /usr/local/bin/

# Authenticate (pick one)
ado_cli login --org {your-org}                    # browser OAuth
ado_cli login --method pat --org {your-org} --pat {token}
ado_cli login --method device --org {your-org}    # no browser needed
```

## Global Options (apply to any command)

```
--org, -o ORG     Organization name     (or ADO_ORG env var)
--pat, -t TOKEN   Personal Access Token (or ADO_PAT env var)
--server, -s URL  Self-hosted server    (or ADO_SERVER env var)
--json            Output raw JSON instead of tables
--verbose, -v     Verbose output
```

## Command Groups

| Group | Commands |
|-------|----------|
| `projects` | list, show, create, update, delete |
| `repos` | list, show, create, delete, branches |
| `workitems` | list, show, query, create, update, comments, attachments |
| `pipelines` | list, show, run |
| `prs` | list, show, create, complete, abandon, approve, vote, comments |
| `releases` | list, show |
| `skills` | list, read |

## Conventions

1. **Project and repo names with spaces**: Quote them.
   ```bash
   ado_cli repos list "Employee Management"
   ```

2. **Error handling**: Non-zero exit code + stderr message. Use `--verbose` for details.

3. **Secrets**: Never log or store PATs in plain text. Use `--pat` flag or `ADO_PAT` env var (both are transient).

4. **Org types**: For personal orgs (`*.visualstudio.com`), the CLI delegates API calls
   to `az devops invoke`. Ensure `az` is installed and `az login` has been run.

5. **Output**: Default is formatted tables. Use `--json` for machine-readable output.

## Quick Examples

```bash
# List projects
ado_cli --org myorg --pat mytoken projects list

# Create a work item
ado_cli workitems create MyProject --type Bug --title "Fix login page"

# Create a PR
ado_cli prs create MyProject MyRepo --title "Add feature" --source dev --target main

# Check auth status
ado_cli whoami
```

## Help

Every command and subcommand supports `--help`:
```bash
ado_cli --help
ado_cli projects --help
ado_cli projects create --help
```
