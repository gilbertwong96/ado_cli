---
name: ado-auth
description: "Authenticate ado: PAT (CI-friendly), browser OAuth (AAD + MSA), device code (headless), env vars, self-hosted server"
version: "0.4.10"
commands:
  - ado login
  - ado login --method device
  - ado login --org ORG
  - ado login --method pat --org ORG --pat TOKEN
  - ado logout
  - ado whoami
  - export ADO_ORG=org ADO_PAT=token
  - export ADO_SERVER=https://dev.azure.com
---

# Authentication

`ado` auto-resolves auth in this priority:

1. **CLI flags** (`--org`, `--pat`, `--server`) — one-off, never persisted
2. **Environment variables** (`ADO_ORG`, `ADO_PAT`, `ADO_SERVER`) — session-level
3. **Config file** (`~/.ado_cli/config.json`) — persistent, set via `ado login`

There is **no `az` CLI dependency**.

## Decision tree: which auth method?

```
Are you in CI/headless (no browser)?
  ├── Yes → Use PAT (method: pat, requires --org)
  │         ado login --method pat --org myorg --pat mytoken
  │
  └── No → Just type `ado login` — picks the right method automatically:
            ├── Browser OAuth (default, no flags needed)
            │   ado login                 # auto-detects org
            │   ado login --org myorg     # or hint a specific org
            │
            └── Device code (browser blocked by firewall/Zscaler)
                ado login --method device  # org also auto-detected
```

## Method details

### PAT (Personal Access Token) — most reliable, CI-friendly

```bash
# Generate at: https://dev.azure.com/{org}/_usersSettings/tokens
# Recommended scopes: vso.work, vso.code, vso.project, vso.build, vso.release
# Or use "Full access" for broadest coverage

# Save to config (persistent)
ado login --method pat --org myorg --pat mytoken

# One-off (never saved to disk)
export ADO_ORG=myorg ADO_PAT=mytoken
ado projects list
```

**Works for ALL org types** (AAD, MSA, self-hosted). No browser required.

### Browser OAuth — default, interactive

```bash
ado login                 # auto-detects org from token (recommended)
ado login --org myorg     # hint a specific org
```

Supports:
- AAD (work/school) accounts
- MSA (personal) accounts via ARM-first OAuth flow
- Prompt=select_account for multi-account Microsoft sessions

Prerequisites:
- Port 58585 must be free (localhost callback)
- Default browser must be installed

### Device Code — headless, no browser

```bash
ado login --method device         # org auto-detected, no --org needed
ado login --method device --org myorg  # or hint a specific org
# CLI prints a URL and code → visit https://login.microsoft.com/device
# Enter the code → CLI polls for completion
```

Use when:
- Browser is blocked by firewall/Zscaler
- Running on a headless server
- The default browser OAuth port is unavailable

### Self-hosted Server

```bash
ado login --method pat --server https://ado.example.com --org DefaultCollection --pat xxx
# Per-command:
ado --server https://ado.example.com --org Coll --pat xxx projects list
```

## MSA (personal) org support

`*.visualstudio.com` orgs are first-class:
1. Authenticates via Azure Resource Manager (MSA-compatible)
2. Exchanges ARM refresh token for DevOps access token
3. Auto-detects org from accounts API
4. Uses v1.0 token endpoint (`resource=` parameter)

No special flags required.

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
ado logout    # deletes ~/.ado_cli/config.json
```

## CI / headless servers

```bash
# Env-var approach (preferred — nothing written to disk)
export ADO_ORG=myorg
export ADO_PAT=$(cat /run/secrets/ado_pat)
ado projects list

# Or: login at job setup
ado login --method pat --org myorg --pat $ADO_PAT
ado <command>  # uses saved config
```

## Common pitfalls

| Error | Cause | Fix |
|-------|-------|-----|
| `Not authenticated` | No auth configured | `ado login` or set `ADO_ORG`/`ADO_PAT` |
| `API redirected to sign-in page` (302) | Token expired | Re-run `ado login` |
| 401 / 403 | Token invalid or wrong scopes | Check token at https://dev.azure.com/{org}/_usersSettings/tokens |
| `Identity not materialized` | New MSA org, user never visited in browser | Visit `https://dev.azure.com/{org}` once, then re-login |
| `Organization not found` | Wrong org name | Check spelling; `ado whoami` to verify |
| Browser OAuth times out | Firewall/Zscaler blocks | Use PAT or device code instead |
| Device code login crashes with `WithClauseError` | API response uses `verification_url` (not `_uri`) | Fixed in v0.4.2+ |
| Device code login prints garbled text | `CLI.color` returns IO list | Fixed in v0.4.2+ |
| `connectionData` returns 400 with `api-version=7.1` | Some orgs reject versioned calls | Fixed in v0.4.2+ — now calls without version |
| `Cannot record a vote for someone else` | PR created by someone else, user not in reviewer list | Fixed in v0.4.3 — CLI uses `current_user_id()` instead of `createdBy.id` |

## Security

- PATs are never written to config file (CLI flag or env var only)
- Config file stores bearer tokens and org name
- File permissions: 0600 (owner read/write only)
- `--pat` flag masked in error output

## See also

- [ado-cli skill](ado-cli) — full command reference
- [ado-ci skill](ado-ci) — CI/CD patterns
