---
description: Azure DevOps authentication methods (PAT, OAuth, az CLI)
version: "1.0"
---

# Azure DevOps Authentication

This skill describes the authentication methods supported by `ado_cli`.

## Priority Order (auto-resolved)

1. **CLI flags** `--org ORG --pat TOKEN` (per-invocation)
2. **Environment** `ADO_ORG` + `ADO_PAT` (session-based)
3. **Azure CLI** `az login` (auto-detected, MSAL-based)
4. **Config file** `~/.ado_cli/config.json` (persistent, set via `ado_cli login`)

## PAT (Personal Access Token)

Create at: `https://dev.azure.com/{org}/_usersSettings/tokens`

Minimum scopes:
- **Code** (Read & Write) — for repos, PRs
- **Work Items** (Read & Write) — for work items

```bash
ado_cli login --method pat --org myorg --pat mytoken
```

## Browser OAuth (default)

```bash
ado_cli login --org myorg
```

Uses ARM-first authentication (`organizations` tenant) then exchanges for
a DevOps token. For MSA personal orgs, delegates to `az devops invoke`.

## Device Code OAuth

```bash
ado_cli login --method device --org myorg
```

## Azure CLI Token

If `az login` is active, the CLI auto-detects the token:

```bash
az login
export ADO_ORG=myorg
ado_cli projects list  # uses az token
```

## Self-hosted Server

```bash
ado_cli login --method pat --server https://ado.example.com --org Coll --pat xxx
export ADO_SERVER=https://ado.example.com
```

## Troubleshooting

| Error | Fix |
|-------|-----|
| "Identity not materialized" | Visit `https://dev.azure.com/{org}` in browser |
| "personal account not allowed" | Use `az login` + auto-detection or PAT |
| 401 Unauthorized | Check `ado_cli whoami` — PAT may be expired |
