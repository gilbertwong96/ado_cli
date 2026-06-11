---
description: How to authenticate ado — PAT, browser OAuth, az CLI, device code
version: "0.1.0"
---

# Authentication

`ado` auto-resolves auth in this priority:

1. CLI flags (`--org`, `--pat`)
2. Environment (`ADO_ORG`, `ADO_PAT`)
3. Azure CLI (`az login` — auto-detected)
4. Config file (`~/.ado_cli/config.json`)

## Choosing a method

### PAT (Personal Access Token) — most reliable

```bash
# Create at: https://dev.azure.com/{org}/_usersSettings/tokens
# Scopes: Code (Read & Write) + Work Items (Read & Write)

ado login --method pat --org myorg --pat mytoken
ado whoami     # verify
```

Works for ALL org types. No browser needed. Use this in CI/CD.

### Browser OAuth — interactive login

```bash
ado login --org myorg    # opens browser, sign in with Microsoft
```

Works for AAD (work/school) orgs. For MSA personal orgs (`*.visualstudio.com`),
the CLI delegates API calls to `az devops invoke`. Keep `az` installed for MSA orgs.

### Azure CLI (az) — zero config

```bash
az login                     # sign in with Microsoft
ado --org myorg projects list   # CLI auto-detects az token
```

No `ado login` needed. Best for MSA-backed personal orgs.

### Device Code — no browser

```bash
ado login --method device --org myorg
# prints a code → enter at https://microsoft.com/devicelogin
```

### Self-hosted Server

```bash
ado login --method pat --server https://ado.example.com --org DefaultCollection --pat xxx
# or per-command
ado --server https://ado.example.com --org Coll --pat xxx projects list
```

## Checking status

```bash
ado whoami
# Shows: organization, auth method, server, config file location, az availability
```

## Logging out

```bash
ado logout    # removes ~/.ado_cli/config.json
```

## Troubleshooting

| Error | Likely cause | Fix |
|-------|-------------|-----|
| "Identity not materialized" | First-time access to personal org | Visit `https://dev.azure.com/{org}` in browser once |
| "personal account not allowed" | OAuth client doesn't support MSA | Use `az login` + auto-detection, or PAT |
| 401 / 403 | Expired or invalid credential | `ado whoami`, re-login if needed |
| "not_configured" | No auth provided | Set `ADO_ORG` + `ADO_PAT`, or run `ado login` |
