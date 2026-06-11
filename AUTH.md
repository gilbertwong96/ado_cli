# Authentication Guide

`ado_cli` supports multiple authentication methods for Azure DevOps,
auto-resolved in priority order.

## Priority order

| Priority | Method | How | Best for |
|----------|--------|-----|----------|
| 1 | CLI flags | `--org ORG --pat TOKEN` | One-off commands, CI/CD |
| 2 | Environment variables | `ADO_ORG` + `ADO_PAT` | Session-based, scripts |
| 3 | Azure CLI token | `az login` (auto-detected) | AAD + MSA personal orgs |
| 4 | Config file | `ado_cli login` (persistent) | Daily CLI usage |

## Methods in detail

### PAT (Personal Access Token)

Create a PAT at `https://dev.azure.com/{org}/_usersSettings/tokens` with
at minimum **Code (Read & Write)** and **Work Items (Read & Write)** scopes.

```bash
# One-off
ado_cli --org myorg --pat xxxxx projects list

# Persistent
ado_cli login --method pat --org myorg --pat xxxxx
ado_cli whoami
```

### Browser OAuth (default)

Opens your system browser for interactive sign-in via Microsoft Identity Platform.
Works for both AAD (work/school) and MSA (personal) accounts.

```bash
# Interactive browser sign-in
ado_cli login --org myorg

# Device code flow (no browser needed)
ado_cli login --method device --org myorg
```

> **MSA personal orgs** (`*.visualstudio.com`): after browser sign-in, the CLI
> delegates API calls to `az devops invoke` for reliable authentication across
> tenant boundaries. Install `az` and run `az login` for best results.

### Azure CLI (`az`)

If `az login` is active, the CLI auto-detects and uses its token. No additional
setup needed.

```bash
az login
export ADO_ORG=myorg
ado_cli projects list     # uses az token automatically
```

## Self-hosted Azure DevOps Server

```bash
ado_cli login --method pat --server https://ado.example.com --org DefaultCollection --pat xxx
# Or per-command:
ado_cli --server https://ado.example.com --org DefaultCollection --pat xxx projects list
```

## OAuth Client ID

The default OAuth client ID is the Azure CLI public client (`04b07795-…`).
Override via:

```bash
export ADO_OAUTH_CLIENT_ID=your-app-id
```

## Troubleshooting

| Error | Fix |
|---|---|
| "Identity not materialized" | Visit `https://dev.azure.com/{org}` in browser once |
| "You can't sign in with a personal account" | Use `az login` + auto-detection, or PAT |
| "The client does not exist or is not enabled for consumers" | Set `ADO_OAUTH_CLIENT_ID` to an MSA-enabled AAD app |
