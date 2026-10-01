# Authentication Guide

`ado` supports three authentication methods for Azure DevOps, auto-resolved in
priority order. No Azure CLI (`az`) is detected or required.

## Priority order

| Priority | Method | How | Best for |
|----------|--------|-----|----------|
| 1 | CLI flags | `--org ORG --pat TOKEN` | One-off commands, CI/CD |
| 2 | Environment variables | `ADO_ORG` + `ADO_PAT` | Session-based, scripts |
| 3 | OS credential store | `ado login` (persistent) | Daily CLI usage |
| 4 | Config file | `<config dir>/ado/config.toml` | The org and method a login recorded |

The first source that provides both an organization and a token wins.

## Where credentials live

`ado login` stores the token in the **OS credential store** — macOS Keychain,
Windows Credential Manager or the Linux secret service — keyed by organization.
Where no store is reachable (a headless CI runner, a Linux session with no
secret service) the token falls back to
`<config dir>/ado/credentials.json`, written mode `0600`.

The **config file** `<config dir>/ado/config.toml` records only the organization
and the method; the token is never written there. The config directory is the
OS's own: `~/Library/Application Support` on macOS, `%APPDATA%` on Windows, and
`$XDG_CONFIG_HOME` (or `~/.config`) on Linux.

```bash
ado whoami    # which org, method and config file this run resolves
ado logout    # removes the stored credential and clears the config entry
```

## Methods in detail

### PAT (Personal Access Token)

Create a PAT at `https://dev.azure.com/{org}/_usersSettings/tokens` with at
minimum `vso.work`, `vso.code`, `vso.project`, `vso.build` and `vso.release`
(or "Full access" if you prefer).

```bash
# One-off (never persisted)
ado --org myorg --pat xxxxx projects list

# Persistent: --method pat is inferred from --pat
ado login --org myorg --pat xxxxx
# The explicit form is equivalent:
ado login --method pat --org myorg --pat xxxxx

ado whoami
```

### Browser OAuth (default) — interactive

`ado login` with no `--pat` opens your system browser for sign-in via Microsoft
Identity Platform. Works for both AAD (work/school) and MSA (personal) accounts,
including `*.visualstudio.com` personal orgs, which use an ARM-first
token-exchange flow.

```bash
ado login                 # browser OAuth, org auto-detected from the token
ado login --org myorg     # hint the org (skips the auto-detect query)

# Device code flow (no browser needed)
ado login --method device --org myorg
```

The browser flow listens on `http://localhost:<port>` for the redirect. The port
is assigned by the OS, so there is no fixed port to reserve; the browser needs to
be reachable from the same machine.

> **Not verified live in this repository.** The browser flow is shipped and
> covered by the test suite against a mock identity service, but a real sign-in
> has never been run end to end. One real `ado login` followed by `ado whoami`
> is the maintainer's release gate for `1.0.0`.

### Device code — headless, no browser

For SSH sessions, remote boxes, or any machine whose browser is blocked by a
firewall/Zscaler. The CLI prints a URL and a code; you visit the URL in any
browser and enter the code.

```bash
ado login --method device --org myorg
```

```
To sign in, use a web browser to open:
  https://microsoft.com/devicelogin
And enter the code: ABC123XYZ
```

The CLI polls the token endpoint until you complete the sign-in, then stores the
credential the same way the browser flow does.

### Environment variables (no login)

Skip `ado login` entirely by setting env vars in the runner or shell:

```bash
export ADO_ORG=myorg
export ADO_PAT=mytoken
ado projects list
```

`ADO_PAT` is read only from the environment and is never written to the
credential store or the config file. A blank value counts as unset.

## Self-hosted Azure DevOps Server

```bash
ado login --method pat --server https://ado.example.com --org DefaultCollection --pat xxx
# Or per-command:
ado --server https://ado.example.com --org DefaultCollection --pat xxx projects list
```

When `--server` is set, `--org` is the collection name.

## Upgrading from the Elixir CLI

On first use, a legacy install is imported once: `~/.ado_cli/config.json`'s
organization, server and token are moved into the credential store and the config
file, so an existing install keeps working without re-authenticating. The legacy
file is not read again after that.

## OAuth Client ID

The interactive flows use the Azure CLI public client
(`04b07795-8ddb-461a-bbee-02f9e1bf7b46`) by default. Override it per invocation:

```bash
export ADO_OAUTH_CLIENT_ID=your-app-id
```

## Troubleshooting

| Error | Fix |
|---|---|
| `Not authenticated` | `ado login`, or set `ADO_ORG` + `ADO_PAT` |
| "Identity not materialized" | Visit `https://dev.azure.com/{org}` in a browser once, then re-run `ado login` |
| "You can't sign in with a personal account" | Use browser OAuth (MSA personal orgs are supported), device code, or a PAT |
| "The client does not exist or is not enabled for consumers" | Set `ADO_OAUTH_CLIENT_ID` to an MSA-enabled AAD app |
| 401 / 403 | Token invalid or scope too narrow; check the PAT at `https://dev.azure.com/{org}/_usersSettings/tokens` |
