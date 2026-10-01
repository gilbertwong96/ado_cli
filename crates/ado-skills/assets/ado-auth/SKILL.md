---
name: ado-auth
description: "Authenticate ado: PAT (CI-friendly), browser OAuth (AAD + MSA), device code (headless), env vars, self-hosted server"
version: "1.0.0-rc.1"
commands:
  - ado login
  - ado login --method device
  - ado login --org ORG
  - ado login --org ORG --pat TOKEN             # --pat infers method=pat, no --method needed
  - ado login --method pat --org ORG --pat TOKEN   # explicit form (same result)
  - ado logout
  - ado whoami
  - export ADO_ORG=org ADO_PAT=token
  - export ADO_SERVER=https://dev.azure.com
---

# Authentication

`ado` auto-resolves auth in this priority:

1. **CLI flags** (`--org`, `--pat`, `--server`) — one-off, never persisted
2. **Environment variables** (`ADO_ORG`, `ADO_PAT`, `ADO_SERVER`) — session-level
3. **OS credential store** — persistent, set via `ado login`; macOS Keychain, Windows
   Credential Manager or the Linux secret service, keyed by organization. Where no
   store is reachable the credential falls back to
   `<config dir>/ado/credentials.json` (mode 0600)
4. **Config file** (`<config dir>/ado/config.toml`) — the organization and method
   `ado login` recorded; the token is never written there

There is **no `az` CLI dependency** — no Azure CLI token is detected or used.

On first use a legacy Elixir install is imported once: `~/.ado_cli/config.json`'s
organization, server and token are moved into the store and the config file, and the
file is never read again.

## Decision tree: which auth method?

```
Are you in CI/headless (no browser)?
  ├── Yes → Use PAT (--pat infers method=pat automatically; --org required)
  │         ado login --org myorg --pat mytoken
  │
  └── No → Just type `ado login` — picks the right method automatically:
            ├── Browser OAuth (default when no --pat given)
            │   ado login                 # auto-detects org
            │   ado login --org myorg     # or hint a specific org
            │
            └── Device code (browser blocked by firewall/Zscaler)
                ado login --method device  # org also auto-detected
```

> **Method inference:** If `--pat` (or `ADO_PAT`) is present without
> `--method`, the CLI uses PAT login — no browser. Pass `--method` only
> to override the inference (e.g. `--method browser` to force OAuth even
> when `--pat` is set).

## Method details

### PAT (Personal Access Token) — most reliable, CI-friendly

```bash
# Generate at: https://dev.azure.com/{org}/_usersSettings/tokens
# Recommended scopes: vso.work, vso.code, vso.project, vso.build, vso.release
# Or use "Full access" for broadest coverage

# Save to config (persistent) — --method pat is inferred from --pat
ado login --org myorg --pat mytoken
# Explicit form is equivalent:
# ado login --method pat --org myorg --pat mytoken

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
- The OS assigns the localhost callback port; nothing fixed must be free
- Default browser must be installed

### Device Code — headless, no browser

```bash
ado login --method device         # org auto-detected, no --org needed
ado login --method device --org myorg  # or hint a specific org
# CLI prints a URL and code → visit https://microsoft.com/devicelogin
# Enter the code → CLI polls for completion
```

Use when:
- Browser is blocked by firewall/Zscaler
- Running on a headless server
- A browser cannot reach the machine running `ado`

### Self-hosted Server

```bash
ado login --server https://ado.example.com --org DefaultCollection --pat xxx
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
# Config File:  <config dir>/ado/config.toml
```

(The path shown is the OS config directory: `~/Library/Application Support` on
macOS, `%APPDATA%` on Windows, `$XDG_CONFIG_HOME` or `~/.config` on Linux.)

## Logging out

```bash
ado logout    # removes the stored credential and clears the config entry
```

## Switching auth methods

Switching between PAT and browser/device login is safe — the CLI clears
stale credentials (`pat`, `token`) when the `method` changes:

```bash
ado login --org myorg --pat mytoken   # saves PAT to config
ado login                              # switch to browser OAuth
# config now has: {"method":"browser","token":"..."} — no stale pat
```

Non-credential fields (`org`, `server`) are preserved across switches.

## CI / headless servers

```bash
# Env-var approach (preferred — nothing written to disk)
export ADO_ORG=myorg
export ADO_PAT=$(cat /run/secrets/ado_pat)
ado projects list

# Or: login at job setup (--method pat inferred from --pat)
ado login --org myorg --pat $ADO_PAT
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

## Security

- PATs from env vars (`ADO_PAT`) and per-command `--pat` flags are transient — never written to disk
- PATs from `ado login --pat` (or `ado login --method pat`) ARE saved to the OS credential store for reuse
- Browser/device OAuth bearer tokens are stored the same way; the config file records only the org and method
- The `credentials.json` fallback is written mode 0600 (owner read/write only)
- `--pat` flag masked in error output

## See also

- [ado-cli skill](ado-cli) — full command reference
- [ado-ci skill](ado-ci) — CI/CD patterns
