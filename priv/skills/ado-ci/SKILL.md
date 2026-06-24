---
name: ado-ci
description: "Use ado in CI/CD: auth setup, pipeline triggers, PR automation, package publishing, work item creation on failure"
version: "0.4.8"
commands:
  - ado projects list
  - ado pipelines list PROJECT
  - ado pipelines run PROJECT ID --branch BRANCH --variables KEY=VALUE
  - ado pipelines-builds queue PROJECT --definition ID --branch BRANCH
  - ado pipelines-builds cancel PROJECT BUILD_ID
  - ado pipelines-builds show PROJECT BUILD_ID
  - ado pipelines vars create PROJECT --name N --variables K=V --secret K
  - ado prs create PROJECT REPO --title TEXT --source BRANCH --target BRANCH
  - ado prs complete PROJECT REPO ID --merge-strategy squash --delete-source
  - ado prs approve PROJECT REPO ID
  - ado workitems create PROJECT --type Bug --title TEXT
  - ado workitems update ID --state Resolved
  - ado imports create PROJECT REPO --url URL --user U --password P
  - ado ci watch PROJECT BUILD_ID
  - export ADO_ORG=org ADO_PAT=token
---

# CI/CD Usage

How to use `ado` in continuous integration and deployment workflows. The CLI
is self-contained (no `az` dependency) and cross-compiled to single-file
binaries for Linux, macOS, and Windows.

## When to use this skill

- Setting up a CI pipeline that interacts with Azure DevOps
- Automating PR creation, approval, and merging from a bot
- Creating work items on pipeline failure
- Triggering downstream pipelines from a parent pipeline
- Publishing test results / coverage to Azure DevOps from CI

For authentication methods and troubleshooting, see the [ado-auth skill](ado-auth).
For the full command reference, see the [ado-cli skill](ado-cli).

## CI Authentication

Use PAT with environment variables — no browser needed, nothing persisted to disk:

```bash
export ADO_ORG=myorg
export ADO_PAT=$(cat /run/secrets/ado_pat)

ado projects list
```

> Store the PAT in a CI secret manager (GitHub Secrets, GitLab CI variables,
> Azure Key Vault, etc.). Never commit PATs to source control.

### Required PAT scopes

| Operation | Minimum scope |
|-----------|---------------|
| Read projects, repos, work items | `vso.work_read`, `vso.code_read` |
| Create/update work items | `vso.work` |
| Manage pipelines (run, cancel) | `vso.build` |
| Create/complete/approve PRs | `vso.code` |
| Publish packages and releases | `vso.release` |
| User administration | `vso.project` |
| Everything | Full access |

## Downloading the binary

```bash
# macOS arm64 (Apple Silicon)
curl -L -o ado https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado-0.4.3-macos-aarch64
chmod +x ado && sudo mv ado /usr/local/bin/

# Linux x86_64
curl -L -o ado https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado-0.4.3-linux-x86_64
chmod +x ado && sudo mv ado /usr/local/bin/

# Linux aarch64 (ARM servers, Raspberry Pi 4/5)
curl -L -o ado https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado-0.4.3-linux-aarch64
chmod +x ado && sudo mv ado /usr/local/bin/

# macOS x86_64 (Intel)
curl -L -o ado https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado-0.4.3-macos-x86_64

# Windows x86_64
# Download from https://github.com/gilbertwong96/ado_cli/releases/latest
# ado-0.4.3-windows-x86_64.exe
```

Or install via npm: `npm install -g @gilbertwong1996/ado`

## Pipeline triggers

```bash
# Trigger a YAML pipeline
ado pipelines run MyProject 42 --branch main

# With per-run variables
ado pipelines run MyProject 42 --branch release --variables "ENV=staging,DEBUG=true"

# Trigger a classic build
ado pipelines-builds queue MyProject --definition 5 --branch main

# Cancel a running build
ado pipelines-builds cancel MyProject 99

# Watch live logs (streaming)
ado ci watch MyProject 99
ado ci watch MyProject --latest --definition 42 --branch main
```

## Variable groups (CI secrets)

```bash
# Create a group with a secret variable
ado pipelines vars create MyProject \
  --name prod-secrets \
  --description "Production credentials" \
  --variables "DB_HOST=prod-db,DB_USER=app,DB_PASS=hunter2" \
  --secret DB_PASS

# Show (secret values are hidden in output)
ado pipelines vars show MyProject 5

# Update
ado pipelines vars update MyProject 5 --variables "DB_HOST=new-prod-db"
```

## Per-pipeline variables

```bash
ado pipelines variables create MyProject 42 \
  --key DEPLOY_TOKEN \
  --value "ghp_xxx" \
  --secret
```

## PR automation

```bash
# Create a release PR
ado prs create MyProject MyRepo \
  --title "Release v1.2.3" \
  --source release/v1.2.3 \
  --target main \
  --description "Automated release PR"

# Complete (merge) with squash
ado prs complete MyProject MyRepo 99 --merge-strategy squash --delete-source

# Approve a PR (the CLI auto-adds the authenticated user as reviewer)
ado prs approve MyProject MyRepo 42

# Add review comments
ado prs comments add MyProject MyRepo 42 \
  --content "Automated code review passed. All tests green. LGTM!"

# List reviewers
ado prs reviewers list MyProject MyRepo 42
```

## Work item automation

```bash
# Create a work item on CI failure
ado workitems create MyProject --type Bug \
  --title "CI failure: build #42" \
  --description "Pipeline failed at test stage. See logs for details." \
  --tags "ci,automated" \
  --priority 1

# Update state on fix deployment
ado workitems update 123 --state Resolved
```

## Repo migration

```bash
ado imports create MyProject my-new-repo \
  --url https://github.com/some-org/some-repo.git \
  --user $GITHUB_USER \
  --password $GITHUB_TOKEN

# Check progress
ado imports show MyProject {import_id}
```

## Publish test results

```bash
# Publish coverage from CI
ado test-results publish MyProject \
  --name "CI Suite" \
  --file coverage.cobertura.xml \
  --build-id $BUILD_ID

# Show coverage
ado test-coverage show MyProject $BUILD_ID
```

## Org notifications

```bash
# Set a maintenance banner
ado banners set --message "CI pipeline maintenance: builds paused until 3pm UTC" --type warning
ado banners delete
```

## JSON output for scripting

```bash
# Machine-readable output
ado --json projects list | jq '.[].name'
ado --json workitems show 42 | jq '.fields."System.Title"'

# Use in scripts
result=$(ado --json projects list)
echo "$result" | jq -r '.[].name'
```

## GitHub Actions example

```yaml
- name: Download ado binary
  run: |
    curl -L -o ado https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado_linux
    chmod +x ado && sudo mv ado /usr/local/bin/

- name: Create work item on failure
  if: failure()
  env:
    ADO_ORG: ${{ secrets.ADO_ORG }}
    ADO_PAT: ${{ secrets.ADO_PAT }}
  run: |
    ado workitems create MyProject --type Bug \
      --title "CI failed: ${{ github.workflow }}" \
      --description "Run: ${{ github.run_id }}" \
      --tags "ci,automated"

- name: Trigger downstream pipeline
  env:
    ADO_ORG: ${{ secrets.ADO_ORG }}
    ADO_PAT: ${{ secrets.ADO_PAT }}
  run: |
    ado pipelines run MyProject 99 --branch main --variables "SHA=${{ github.sha }}"
```

## GitLab CI example

```yaml
trigger_deploy:
  stage: deploy
  image: alpine:latest
  before_script:
    - curl -L -o ado https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado_linux
    - chmod +x ado && mv ado /usr/local/bin/
  script:
    - ado pipelines run MyProject 42 --branch $CI_COMMIT_REF_NAME
  variables:
    ADO_ORG: $ADO_ORG
    ADO_PAT: $ADO_PAT
```

## Common pitfalls

1. **"Not authenticated" on CI** → `ADO_ORG`/`ADO_PAT` env vars not set. Check your CI secrets.
2. **"API redirected to sign-in page" (302)** → Token expired or wrong org. Generate a new PAT.
3. **401/403 on pipeline operations** → PAT missing `vso.build` scope. Add it or use Full access.
4. **"Cannot record a vote for someone else"** → Fixed in v0.4.3. Upgrade the binary.
5. **Long pipeline logs time out** → `ado ci watch` streams indefinitely. If it hangs, the build may be stuck.
6. **Work item creation fails with "invalid field"** → The `--type` name must match the process template (Bug, User Story, Task vs Issue, Epic). Check the project's process.
7. **Multi-line content gets truncated** → Use `--content @file` to read from a file, or `--content -` to read from stdin.
8. **Race conditions on shared state** → Each `ado` invocation is stateless (no persistent connections). Safe to parallelize. Two commands editing the same PR simultaneously may conflict; the second caller gets a 409.

## Exit codes

| Code | Meaning |
|------|---------|
| 0    | Success |
| 1    | Generic error |
| 2    | API error (4xx/5xx) |
| 3    | Auth not configured |

Use `set -euo pipefail` in shell scripts. Non-zero always means failure.

## See also

- [ado-auth skill](ado-auth) — authentication methods and troubleshooting
- [ado-cli skill](ado-cli) — full command reference
- [Project homepage](https://gilbertwong96.github.io/ado_cli/)
