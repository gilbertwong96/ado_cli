---
description: How to authenticate ado — PAT, browser OAuth, device code, MSA support
version: "0.3.0"
commands:
  - ado login --method pat --org ORG --pat TOKEN                   # PAT, no browser, CI-friendly
  - ado login --org ORG                                          # browser OAuth (AAD + MSA)
  - ado login --method device --org ORG                         # device code, no browser
  - ado login                                                    # auto-detect org from token
  - ado logout
  - ado whoami
  - export ADO_ORG=org ADO_PAT=token                             # env-var auth (session-level)
  - export ADO_SERVER=https://dev.azure.com                      # self-hosted server URL
---

# Authentication

`ado` auto-resolves auth in this priority:

1. **CLI flags** (`--org`, `--pat`, `--server`) — one-off, never persisted
2. **Environment variables** (`ADO_ORG`, `ADO_PAT`, `ADO_SERVER`) — session-level
3. **Config file** (`~/.ado_cli/config.json`) — persistent, set via `ado login`

There is **no `az` CLI dependency**. The CLI is self-contained.

## Choosing a method

### PAT (Personal Access Token) — most reliable

```bash
# Create at: https://dev.azure.com/{org}/_usersSettings/tokens
# Recommended scopes: vso.work, vso.code, vso.project, vso.build, vso.release
# (or "Full access" for broadest coverage)

ado login --method pat --org myorg --pat mytoken
ado whoami     # verify
```

**Works for ALL org types** (AAD, MSA personal, self-hosted). No browser needed.
**Use this in CI/CD.**

For one-off use without saving to config:
```bash
export ADO_ORG=myorg ADO_PAT=mytoken
ado projects list
```

### Browser OAuth — interactive login

```bash
ado login --org myorg    # opens browser, sign in with Microsoft
# Or just:
ado login                # auto-detects the only org you have access to
```

Works for **AAD (work/school)** orgs. Also works for **MSA personal**
(`*.visualstudio.com`) orgs via the ARM-first OAuth flow. After successful
login, the org is auto-detected from the token.

### Device Code — no browser (slower than PAT but works headless)

```bash
ado login --method device --org myorg
# CLI prints a code and URL → visit the URL in any browser, enter the code
```

### Self-hosted Server

```bash
ado login --method pat --server https://ado.example.com --org DefaultCollection --pat xxx
# or per-command
ado --server https://ado.example.com --org Coll --pat xxx projects list
```

## MSA Personal Org Support

MSA-backed orgs (`*.visualstudio.com`) are first-class. The CLI:
1. Authenticates against Azure Resource Manager (ARM), which accepts MSA accounts
2. Exchanges the ARM refresh token for a DevOps access token
3. Auto-detects your org from the accounts API
4. Uses the v1.0 token endpoint (`resource=` parameter) — same path as the
   official `az devops` CLI

No `az login` or `az devops invoke` required.

## Checking status

```bash
ado whoami
# Organization: myorg
# Server:       dev.azure.com (cloud)
# Auth Method:  browser
# Config File:  ~/.ado_cli/config.json
```

## Logging out

```bash
ado logout    # removes ~/.ado_cli/config.json
```

Auth via CLI flags or env vars is unaffected.

## CI / Headless Servers

Use PAT with environment variables — never commit tokens to disk:

```bash
# In a CI job
export ADO_ORG=myorg
export ADO_PAT=$(cat /run/secrets/ado_pat)
ado projects list
```

Or use `ado login --method pat` at job setup (writes `~/.ado_cli/config.json`),
then `ado <command>` in subsequent steps.

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| `Not authenticated` | No auth configured | Set `ADO_ORG`+`ADO_PAT` or run `ado login` |
| `API redirected to sign-in page` (302) | Token expired or org requires interactive sign-in first | Re-run `ado login` to refresh |
| `401 / 403` | Token invalid or insufficient scopes | Check token at <https://dev.azure.com/_usersSettings/tokens> |
| `Identity not materialized` (first use of new MSA org) | Org exists but user hasn't visited it in browser | Visit `https://dev.azure.com/{org}` once, then re-login |
| `Organization not found` | Wrong org name (try the on-prem `DefaultCollection` form) | Check spelling; use `ado projects list` to see valid orgs |
| Browser opens but auth fails | Behind Zscaler or strict corp firewall | Use PAT instead of browser OAuth |

## Security

- PATs are never written to the config file. They are only accepted via
  CLI flag or env var (both transient).
- The config file stores browser OAuth bearer tokens and the org name.
- File permissions on `~/.ado_cli/config.json` are 0600 (owner read/write only).
- The `--pat` flag masks the value in error output.
