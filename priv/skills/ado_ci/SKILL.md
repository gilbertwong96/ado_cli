---
description: Using ado in CI/CD — authentication, pipeline triggers, PR automation, package publishing
version: "0.2.0"
---

# CI/CD Usage

How to use `ado` in continuous integration and deployment workflows. The CLI
is self-contained (no `az` dependency) and cross-compiled to single-file
binaries for Linux, macOS, and Windows.

## CI Authentication

Use PAT with environment variables — no browser needed:

```bash
export ADO_ORG=myorg
export ADO_PAT=$(cat /run/secrets/ado_pat)

ado projects list
```

> Store the PAT in a CI secret manager (GitHub Actions Secrets, GitLab CI
> variables, Azure Key Vault, etc.). Never commit PATs to source control.

For PAT scopes:
- `vso.work` — work items
- `vso.code` — repos, PRs
- `vso.project` — projects, teams
- `vso.build` — pipelines
- `vso.release` — releases
- Or select "Full access" for everything

## Pipeline Triggers

```bash
# Trigger a YAML pipeline run
ado pipelines run MyProject 42 --branch main

# With per-run variables
ado pipelines run MyProject 42 --branch release --variables "ENV=staging,DEBUG=true"

# Trigger a classic build
ado pipelines-builds queue MyProject --definition 5 --branch main

# Cancel a running build
ado pipelines-builds cancel MyProject 99
```

## Variable Groups (CI Secrets)

```bash
# Create a group with a secret variable
ado pipelines vars create MyProject \
  --name prod-secrets \
  --description "Production credentials" \
  --variables "DB_HOST=prod-db,DB_USER=app,DB_PASS=hunter2" \
  --secret DB_PASS

# Show a group (secret values are hidden)
ado pipelines vars show MyProject 5
# Variables:
#   DB_HOST
#   DB_USER
#   DB_PASS [secret]

# Update
ado pipelines vars update MyProject 5 --variables "DB_HOST=new-prod-db"
```

## Per-Pipeline Variables

```bash
# Add a variable to a specific pipeline
ado pipelines variables create MyProject 42 \
  --key DEPLOY_TOKEN \
  --value "ghp_xxx" \
  --secret
```

## PR Automation

```bash
# Create a PR from CI
ado prs create MyProject MyRepo \
  --title "Release v1.2.3" \
  --source release/v1.2.3 \
  --target main \
  --description "Automated release PR"

# Complete (merge) with squash
ado prs complete MyProject MyRepo 99 --merge-strategy squash --delete-source

# Approve a PR
ado prs approve MyProject MyRepo 42
```

## Work Item Automation

```bash
# Create a work item from CI alerts
ado workitems create MyProject --type Bug \
  --title "CI failure: build #42" \
  --description "Pipeline failed at test stage. Check logs." \
  --tags "ci,automated"

# Update work item state
ado workitems update 123 --state Resolved
```

## Repo Migration

Import a Git repository (e.g. from GitHub) into Azure DevOps:

```bash
# Start an import
ado imports create MyProject my-new-repo \
  --url https://github.com/some-org/some-repo.git \
  --user $GITHUB_USER \
  --password $GITHUB_TOKEN

# Check progress
ado imports show MyProject {import_id}
```

## Universal Package Publishing

Publish a release artifact as a Universal Package:

```bash
# List existing packages in a feed
ado packages list MyProject MyFeed

# Show a specific version
ado packages show MyProject MyFeed my-package 1.0.0
```

(Upload via `twine` or the Azure Artifacts client; the CLI manages metadata only.)

## Organization Notifications

Set a banner for all users during a maintenance window:

```bash
ado banners set --message "Maintenance in progress. Builds may fail." --type warning
ado banners delete
```

## JSON Output for Scripting

```bash
# Machine-readable output (--json is global)
ado --json projects list | jq '.[].name'
ado --json workitems show 42 | jq '.fields."System.Title"'

# Suppress non-error output
ado projects list 2>/dev/null
```

## GitHub Actions Example

```yaml
- name: Set up ado
  run: |
    # Pre-built binary available from burrito_out/ in releases
    curl -L -o ado https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado_linux
    chmod +x ado
    sudo mv ado /usr/local/bin/

- name: Create Azure DevOps work item on failure
  if: failure()
  env:
    ADO_ORG: ${{ secrets.ADO_ORG }}
    ADO_PAT: ${{ secrets.ADO_PAT }}
  run: |
    ado workitems create MyProject --type Bug \
      --title "CI failed: ${{ github.workflow }}" \
      --description "Run: ${{ github.run_id }}"

- name: Trigger downstream pipeline
  env:
    ADO_ORG: ${{ secrets.ADO_ORG }}
    ADO_PAT: ${{ secrets.ADO_PAT }}
  run: |
    ado pipelines run MyProject 99 --branch main --variables "SHA=${{ github.sha }}"
```

## GitLab CI Example

```yaml
trigger_deploy:
  stage: deploy
  image: ghcr.io/gilbertwong96/ado_cli:latest
  script:
    - ado pipelines run MyProject 42 --branch $CI_COMMIT_REF_NAME
  variables:
    ADO_ORG: $ADO_ORG
    ADO_PAT: $ADO_PAT
```

## Exit Codes

- 0   Success
- 1   Generic / unexpected error
- 2   API error (4xx/5xx from DevOps)
- 3   Auth not configured

Use `set -euo pipefail` and check `$?` to fail fast on errors.
