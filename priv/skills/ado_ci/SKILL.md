---
description: Using ado in CI/CD — authentication, pipeline triggers, PR automation
version: "0.1.0"
---

# CI/CD Usage

How to use `ado` in continuous integration and deployment workflows.

## CI Authentication

Use PAT with environment variables — no browser needed:

```bash
export ADO_ORG=myorg
export ADO_PAT=$(cat /run/secrets/ado_pat)

ado projects list
```

> Store the PAT in a CI secret manager (GitHub Secrets, Azure Key Vault, etc.).
> Never commit PATs to source control.

## Pipeline Triggers

```bash
# Trigger a pipeline run
ado pipelines run MyProject 42 --branch main

# With variables
ado pipelines run MyProject 42 --variables "ENV=staging,DEBUG=true"
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

## Release Management

```bash
# List recent releases
ado releases list MyProject --status active

# Show release details
ado releases show MyProject 5
```

## JSON Output for Scripting

```bash
# Machine-readable output
ado --json projects list | jq '.[].name'
ado --json workitems show 42 | jq '.fields."System.State"'

# Use with --only-show-errors equivalent (--verbose off by default)
ado projects list 2>/dev/null
```

## GitHub Actions Example

```yaml
- name: Create Azure DevOps work item on failure
  if: failure()
  env:
    ADO_ORG: ${{ secrets.ADO_ORG }}
    ADO_PAT: ${{ secrets.ADO_PAT }}
  run: |
    ado workitems create MyProject --type Bug \
      --title "CI failed: ${{ github.workflow }}" \
      --description "Run: ${{ github.run_id }}"
```
