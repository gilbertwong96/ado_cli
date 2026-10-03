# ado

> **AI-native Azure DevOps CLI** — structured JSON output, embedded skills for
> LLM agents, and a self-discoverable command tree. Built for humans **and**
> AI agents (pi, Claude Code, Copilot, Cursor, etc.) to share.

A self-contained, cross-platform command-line tool for managing Azure DevOps —
projects, repositories, work items, pipelines, pull requests, releases, and
more. Works with both **cloud** (`dev.azure.com`) and **self-hosted** Azure
DevOps Server.

Built in Rust with [clap](https://crates.io/crates/clap),
[ureq](https://crates.io/crates/ureq), [serde_json](https://crates.io/crates/serde_json),
[comfy-table](https://crates.io/crates/comfy-table), and the OS credential store
([keyring](https://crates.io/crates/keyring)).

[![CI](https://github.com/gilbertwong96/ado_cli/actions/workflows/ci.yml/badge.svg)](https://github.com/gilbertwong96/ado_cli/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/gilbertwong96/ado_cli/graph/badge.svg)](https://codecov.io/gh/gilbertwong96/ado_cli)
[![Rust](https://img.shields.io/badge/rust-1.98+-orange.svg)](https://www.rust-lang.org)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Website](https://img.shields.io/badge/website-gilbertwong96.github.io%2Fado_cli-blue)](https://gilbertwong96.github.io/ado_cli/)

---

## Why "AI-native"?

Most CLIs are designed for humans and retrofitted for AI agents with `--json`
flags that nobody uses. `ado` is designed the other way around:

| Feature | Human-friendly | AI-agent-friendly |
|---|---|---|
| Output | Tables (bordered on a terminal, plain when piped) | Stable JSON envelopes on every command |
| Discovery | `ado --help` → 3 levels deep | `ado schema --json` — full command tree in 1 round trip |
| Documentation | Built-in `--help` at every level | `ado skills list` / `ado skills read NAME` |
| Error handling | Pretty stack traces | `{ok: false, error: {code, status, message, details}}` |
| Auth | Browser OAuth (interactive) | PAT, device code, browser — all machine-discoverable |
| Distribution | `npm install -g @gilbertwong1996/ado` | Single self-contained binary, no runtime deps |

Every command supports `--json` for machine consumption. Every error has a
stable `code` (e.g. `auth_required`, `not_found`, `validation_error`) so agents
can match on it instead of parsing English.

**For LLM agents:** run `ado skills install` to install the embedded skills to
your agent's skill directory (`~/.pi/agent/skills/`, `~/.claude/skills/`, etc.)
so they load natively on startup.

---

## Installation

### npm (recommended)

```bash
npm install -g @gilbertwong1996/ado --foreground-scripts
```

> **npm 11+ note**: newer npm versions gate lifecycle scripts behind explicit
> approval. The `--foreground-scripts` flag allows the postinstall to run, which
> downloads the platform binary and auto-installs shell completion.

### Pre-built binaries

The release ships one archive per target, named by target triple and **not** by
version. Download the one for your platform from the
[latest release](https://github.com/gilbertwong96/ado_cli/releases/latest) and put
it on your `$PATH`:

```bash
# Example for Apple Silicon Mac (pick the right archive for your platform)
curl -L -o ado.tar.gz https://github.com/gilbertwong96/ado_cli/releases/latest/download/ado-aarch64-apple-darwin.tar.gz
tar -xzf ado.tar.gz ado-aarch64-apple-darwin/ado
chmod +x ado-aarch64-apple-darwin/ado && sudo mv ado-aarch64-apple-darwin/ado /usr/local/bin/
```

The five archives are `ado-aarch64-apple-darwin.tar.gz`,
`ado-x86_64-apple-darwin.tar.gz`, `ado-x86_64-unknown-linux-musl.tar.gz`,
`ado-aarch64-unknown-linux-musl.tar.gz` and `ado-x86_64-pc-windows-msvc.zip`
(plus `ado-installer.sh` / `ado-installer.ps1`, checksums and a source archive).

### From source

Requires [Rust 1.98+](https://rustup.rs/) (the pin is `rust-toolchain.toml`):

```bash
git clone https://github.com/gilbertwong96/ado_cli.git
cd ado_cli
cargo build --release --locked
cp target/release/ado /usr/local/bin/
```

---

## Quick Start

```bash
# Set credentials
export ADO_ORG=myorg
export ADO_PAT=mypersonalaccesstoken

# List projects
ado projects list

# Create a work item
ado workitems create MyProject --type Bug --title "Fix login page"

# Open a pull request
ado prs create MyProject MyRepo --title "Add feature" --source dev --target main
```

## Shell Completion

`<TAB>` works out of the box once you source the generated script:

```bash
# bash (add to ~/.bashrc)
eval "$(ado completion bash)"

# zsh (add to ~/.zshrc; also works as a one-liner)
ado completion zsh > "${fpath[1]}/_ado"

# fish (add to ~/.config/fish/config.fish)
ado completion fish | source

# PowerShell (add to $PROFILE)
ado completion powershell | Out-String | Invoke-Expression
```

After installing, pressing `<TAB>` after `ado ` shows every top-level
subcommand; after `ado prs ` shows `abandon`, `approve`, `comments`,
`complete`, `create`, `diff`, `list`, `reviewers`, `show`, `vote`; and so on at
every nesting level.

Re-run the completion command after upgrading `ado` to pick up new
subcommands and options.

---

## For LLM Agents (pi, Claude Code, Copilot, Cursor, Codex)

Run `ado schema --json` to discover the full command tree, or install the
embedded skills so your agent loads them natively:

```bash
# Install skills to your agent's skill directory
ado skills install                          # pi + claude + cursor + codex
ado skills install --target pi              # ~/.pi/agent/skills/
ado skills install --target claude          # ~/.claude/skills/
ado skills install --target cursor          # ~/.cursor/skills/
ado skills install --target codex           # ~/.codex/skills/
ado skills install --target copilot --repo .  # ./.github/ado-cli/  (per-repo)

# Verify the install
ls ~/.pi/agent/skills/                       # you should see ado-cli/, ado-auth/, ado-ci/
```

Once installed, the agent can `ado skills list`, `ado skills read <name>`, and
`ado schema <name> --json` (quote a multi-word name: `ado schema "ci watch" --json`)
as native operations, with no shell-out overhead.

---

## Authentication

Multiple auth methods, auto-resolved in priority order. **No `az` CLI required.**

| Priority | Method | How |
|----------|--------|-----|
| 1 | CLI flags | `--org ORG --pat TOKEN` |
| 2 | Environment variables | `ADO_ORG` + `ADO_PAT` |
| 3 | OS credential store | persistent, set via `ado login` |
| 4 | Config file | `<config dir>/ado/config.toml` — org and method only, never a token |

Auth via `ado login` defaults to browser-based OAuth when no `--pat` is
given, and infers PAT login when `--pat` is present — no `az login` is
detected or required. The CLI is self-contained.

### Login (persistent)

Three login methods are supported. Pick the one that fits your situation.
The `--method` flag is optional — if `--pat` (or `ADO_PAT`) is present,
the CLI infers PAT login; otherwise it defaults to browser OAuth.

#### 1. Browser OAuth (default) — recommended for interactive use

Opens your default browser to the Microsoft sign-in page, then captures the
auth code on a local callback. Works with both work/school (AAD) and personal
(MSA) Microsoft accounts. Personal accounts (e.g. `me@outlook.com`) on
`*.visualstudio.com` orgs are supported via an ARM-first token-exchange flow.

```bash
# Open browser, sign in, get redirected back. Org auto-detected.
ado login

# Hint the org (avoids the auto-detect query)
ado login --org myorg
```

What happens:
1. `ado` opens `https://login.microsoftonline.com/...` in your browser
2. You sign in with AAD or MSA credentials
3. The CLI captures the auth code on a localhost callback
4. The CLI exchanges the code for an ARM token, then for an Azure DevOps
   access token, and stores the token in the OS credential store
5. Org is auto-detected from the token (or use `--org` to pin it)

#### 2. Device code — recommended for SSH / no-browser sessions

For headless terminals, remote boxes, or any machine without a browser
reachable from the same shell. The CLI prints a URL and a code; you visit
the URL in any browser (laptop, phone) and enter the code to authenticate.

```bash
ado login --method device --org myorg
```

Example output:

```
To sign in, use a web browser to open:
  https://microsoft.com/devicelogin
And enter the code: ABC123XYZ
```

The CLI polls the token endpoint every few seconds; once you complete the
sign-in on the other device, the CLI saves the token and you're logged in.

#### 3. Personal Access Token (PAT) — recommended for CI / scripts

Stores a PAT in the OS credential store for repeated use. Best for
automation, CI runners, and scripts. See
[How to create a PAT](https://learn.microsoft.com/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate).

```bash
# --method pat is inferred from --pat (both forms are equivalent)
ado login --org myorg --pat mytoken
ado login --method pat --org myorg --pat mytoken
```

Required scopes for full CLI coverage: `vso.work`, `vso.code`, `vso.project`,
`vso.build`, `vso.release` (or "Full access" if you prefer).

#### 4. Environment variables (no login) — recommended for ephemeral CI

Skip `ado login` entirely by setting env vars in the runner / shell:

```bash
export ADO_ORG=myorg
export ADO_PAT=mytoken
ado projects list
```

`ADO_PAT` is only read from the environment; it is never written to the
credential store or the config file. `ADO_ORG` is also accepted as a CLI flag
(`--org myorg`).

#### Check status / log out

```bash
ado whoami       # Show current auth method, org, server
ado logout       # Remove stored credentials
```

### Auth priority order

When you run a command, the CLI resolves credentials in this order:

1. `--pat` / `--org` / `--server` CLI flags (per-invocation)
2. `ADO_PAT` / `ADO_ORG` / `ADO_SERVER` env vars (per-session)
3. The OS credential store (persistent, set via `ado login`; macOS Keychain,
   Windows Credential Manager, Linux secret service — falling back to
   `<config dir>/ado/credentials.json`, mode 0600, where no store is reachable)
4. `<config dir>/ado/config.toml`, which records the organization and method a
   login stored — never the token

The first source that provides both an org and a token wins.

### Upgrading from the Elixir CLI

On first use, a legacy install is imported once: `~/.ado_cli/config.json`'s
organization, server and token are moved into the credential store and the config
file, so an existing install keeps working without re-authenticating. The legacy
file is not read again after that.

---

## Self-Hosted Azure DevOps Server

For on-premises / private cloud Azure DevOps Server:

```bash
# CLI flag
ado --server https://ado.internal.example.com --org DefaultCollection projects list

# Environment variable
export ADO_SERVER=https://ado.internal.example.com

# Login with server (--method pat inferred from --pat)
ado login --server https://ado.internal.example.com --org DefaultCollection --pat xxx
```

When `--server` is set, `--org` becomes the collection name (e.g. `DefaultCollection`).

---

## Commands

The CLI covers **24 service areas** of Azure DevOps. The canonical,
up-to-date reference is the `ado-cli` skill:

```bash
ado skills read ado-cli
# Or read on disk: cat crates/ado-skills/assets/ado-cli/SKILL.md
```

Quick examples:

```bash
# Projects
ado projects list
ado projects create MyProject --description "My project" --visibility private

# Repos & PRs
ado repos list MyProject
ado repos create MyProject MyRepo
ado prs create MyProject MyRepo --title "Add feature" --source dev --target main
ado prs complete MyProject MyRepo 42 --merge-strategy squash --delete-source
ado prs reviewers list MyProject MyRepo 42 --search alice   # fuzzy filter by name/email

# Work items
ado workitems list MyProject --type Bug --state Active
ado workitems query MyProject --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.AssignedTo] = @Me"
ado workitems create MyProject --type Bug --title "Fix login" --tags "bug,critical"

# Pipelines (YAML)
ado pipelines list MyProject --top 10
ado pipelines run MyProject 42 --branch main --variables "ENV=staging,DEBUG=true"

# Watch a build in real-time (live status + streaming logs)
ado ci watch MyProject 99
ado ci watch MyProject --latest --definition 42 --branch main

# Pipelines (Classic builds)
ado pipelines-builds queue MyProject --definition 5 --branch main
ado pipelines-builds cancel MyProject 99

# Variable groups + per-pipeline variables (secrets)
ado pipelines vars create MyProject --name prod-secrets --variables "DB_HOST=db,DB_PASS=secret" --secret DB_PASS
ado pipelines variables create MyProject 42 --key DEPLOY_TOKEN --value "ghp_xxx" --secret

# Iterations, areas, wikis
ado iterations list MyProject MyTeam
ado areas create MyProject --name Backend
ado wikis pages create MyProject Engineering --path /Home --content "# Engineering"

# Users, teams, security
ado users list
ado teams members list MyProject MyTeam
ado security grant MyProject --yes-this-mutates-secret-read

# Banners + packages
ado banners set --message "Maintenance in progress" --type warning
ado packages list MyProject MyFeed
ado imports create MyProject new-repo --url https://github.com/org/repo.git

# CI / scriptable
ado --json projects list | jq '.[].name'
```

For the complete command reference including all options, flags, and JSON
output schemas, see the embedded `ado-cli` skill (`ado skills read ado-cli`).
Every command also supports `--help` for inline reference:

```bash
ado --help
ado projects --help
ado pipelines vars create --help
```

---

## Global Options

```
-o, --org ORG       Azure DevOps organization / collection name [env: ADO_ORG]
-t, --pat TOKEN     Personal Access Token [env: ADO_PAT]
-s, --server URL    Self-hosted Azure DevOps Server URL [env: ADO_SERVER]
-v, --verbose       Enable verbose output
    --json          Output raw JSON instead of formatted tables
    --help          Show help for any command
```

---

## Development

### Setup

```bash
# Build the workspace (the toolchain is pinned by rust-toolchain.toml)
cargo build

# Run the test suite
cargo test --workspace

# Run the CLI from the workspace
cargo run -q -p ado -- projects list
```

### Build

```bash
# Dev build
cargo build
./target/debug/ado projects list

# The stripped release binary (what the budgets measure)
just build-release        # cargo build --release --locked → target/release/ado
```

### Quality

```bash
# The full gate, in order: fmt, clippy, locked build, machete, deny,
# nextest, the 85% coverage floor
just ci

# Individual checks (also usable on their own)
just fmt-check      # cargo fmt --check
just lint           # cargo clippy --all-targets -- -D warnings
just build-strict   # RUSTFLAGS="-Dwarnings" cargo build --locked
just machete        # unused dependencies
just deny           # advisories, licenses, bans, sources
just nextest        # the workspace suite on the CI runner
just coverage       # cargo llvm-cov --fail-under-lines 85

# Budgets: startup time and release binary size
just budget
```

`just test` is the plain-cargo form of the suite (`cargo test --workspace`).
`just npm-test` runs the npm package's tests.

---

## Quality Tools

| Tool | Purpose | Command |
|------|---------|---------|
| [rustfmt](https://github.com/rust-lang/rustfmt) | Formatting | `cargo fmt --check` |
| [Clippy](https://doc.rust-lang.org/clippy/) | Lints, warnings as errors | `cargo clippy --all-targets -- -D warnings` |
| [nextest](https://nexte.st/) | Test runner | `cargo nextest run --workspace` |
| [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) | Line coverage (85% floor) | `cargo llvm-cov --workspace --fail-under-lines 85` |
| [cargo-machete](https://github.com/bnjbvr/cargo-machete) | Unused dependencies | `cargo machete` |
| [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) | Advisories, licenses, bans, sources | `cargo deny check advisories licenses bans sources` |
| [insta](https://insta.rs/) | Snapshot tests | `cargo insta test` / `INSTA_UPDATE=always cargo test -p ado` |
| [cargo-dist](https://opensource.axo.dev/cargo-dist/) | Release archives and installers | `dist plan` |

---

## Project Structure

The workspace has four crates:

```
ado_cli/
├── crates/
│   ├── ado/                          # The `ado` binary
│   │   ├── src/main.rs               # Entry point, exit codes, output rendering
│   │   ├── src/cli.rs                # The clap surface
│   │   ├── src/context.rs            # Per-invocation state (config, store, client)
│   │   ├── src/commands/             # One module per command group
│   │   └── tests/cli_<area>.rs       # Per-area integration suites (mock-backed)
│   ├── ado-core/                     # Envelope, errors, the sync ureq client,
│   │                                 # auth, credential resolution, config,
│   │                                 # the one-time legacy import
│   ├── ado-skills/                   # The embedded `ado skills` assets
│   │   └── assets/                   # skills: ado-cli, ado-auth, ado-ci
│   │       ├── ado-cli/SKILL.md      # Main reference (+ references/*.md)
│   │       ├── ado-auth/SKILL.md     # Auth details
│   │       └── ado-ci/SKILL.md       # CI/CD patterns
│   └── ado-testkit/                  # Dev-only: the axum mock, TempHome and fixtures
├── docs/rust-rewrite/                # The contract record and the wave records
├── npm/                              # The six npm packages
├── github-page/                      # The project site
├── scripts/npm-publish.sh            # The npm publish flow
├── Cargo.toml                        # Workspace manifest (version source)
├── rust-toolchain.toml               # The toolchain pin
├── justfile                          # Every check, the budgets, the bump
├── dist-workspace.toml               # cargo-dist configuration
└── AGENTS.md                         # CI quality gate principles
```

## Command Reference

The CLI covers **24 service areas** of Azure DevOps. The canonical,
up-to-date reference is the **`ado-cli` skill** embedded in the binary:

```bash
ado skills read ado-cli
```

Or see the on-disk source at `crates/ado-skills/assets/ado-cli/SKILL.md`. The skill
includes the full command table, conventions, and quick-start examples.

---

## API Coverage

All commands use Azure DevOps REST API v7.1 (configurable via `ADO_API_VERSION`).
The CLI covers **24 service areas** of Azure DevOps. See the embedded
`ado-cli` skill (`ado skills read ado-cli`) for the full command reference
and per-endpoint coverage.

| Service area | Endpoint | Status |
|--------------|----------|--------|
| Core / Projects | `_apis/projects` | list, show, create, update, delete |
| Git / Repos | `{project}/_apis/git/repositories` | list, show, create, delete |
| Git / Pull Requests | `{project}/_apis/git/repositories/{id}/pullrequests` | list, show, create, complete, abandon, vote, comments |
| Git / Policies | `{project}/_apis/policy/configurations` | list, show, create, update, delete |
| Git / Imports | `{project}/_apis/git/importRequests` | list, show, create |
| Work Items | `{project}/_apis/wit/wiql`, `_apis/wit/workitems/{id}` | query, show, create, update, delete, comments, attachments |
| Sprints / Iterations | `{project}/_apis/work/teamsettings/iterations` | list, show, create, update, delete |
| Area Paths | `{project}/_apis/wit/classificationNodes/areas` | list, show, create, update, delete |
| Pipelines (YAML) | `{project}/_apis/pipelines`, `/runs` | list, show, run, create, update, delete |
| Pipelines (Variables) | `{project}/_apis/pipelines/{id}` (the definition's `variables`) | list, create, delete |
| Variable Groups | `{project}/_apis/distributedtask/variablegroups` | list, show, create, update, delete |
| Pipelines (Classic) | `{project}/_apis/build/builds` | list, show, queue, cancel, tags, definitions |
| Pipelines (Folders) | `{project}/_apis/pipelines/folders` | list, create, delete |
| Pipeline Artifacts | `{project}/_apis/build/builds/{id}/artifacts` | list, download |
| Releases | `{project}/_apis/release/releases` | list, show |
| Wikis | `{project}/_apis/wiki/wikis`, `pages` | list, show, pages (list/show/create/update) |
| Teams | `_apis/projects/{project}/teams` | list, show, create, update, delete, members |
| Users | `_apis/userentitlements` | list, show, add, remove |
| Extensions | `_apis/extensionmanagement/installedextensions` | list, show, install, uninstall, enable, disable |
| Agent Pools | `_apis/distributedtask/pools`, `/queues` | list, show, queues |
| Service Connections | `{project}/_apis/serviceendpoint/endpoints` | list, show |
| Security | `_apis/connectionData`, `_apis/accesscontrolentries` | grant, revoke (the caller's Library ViewSecrets bit) |
| Admin Banners | `_apis/settings/entries/banners` | show, set, delete |
| Universal Packages | `{project}/_apis/packaging/feeds/{id}/packages` | list, versions, show |

---

## CI Pipeline

Every commit must pass the full CI gate (`just ci`):

| Step | Check |
|------|-------|
| 1 | `cargo fmt --check` |
| 2 | `cargo clippy --all-targets -- -D warnings` |
| 3 | `RUSTFLAGS="-Dwarnings" cargo build --locked` |
| 4 | `cargo machete` |
| 5 | `cargo deny check advisories licenses bans sources` |
| 6 | `cargo nextest run --workspace` |
| 7 | `cargo llvm-cov --workspace --fail-under-lines 85` |

See `AGENTS.md` for the full quality gate specification, and
`docs/rust-rewrite/` for the contract record and the wave records.

---

## Watch a build in real-time

`ado ci watch` streams live status and per-line log output
for an Azure DevOps build — like `tail -f` for CI:

```bash
# Watch build #123 in MyProject
ado ci watch MyProject 123

# Watch the latest build on a specific pipeline + branch
ado ci watch MyProject --latest --definition 42 --branch main

# Tighter polling (default 2s)
ado ci watch MyProject 123 --poll-interval 500
```

The watcher polls the Azure DevOps Build API every 2s (configurable)
and prints:

- Build status (running / succeeded / failed / canceled)
- Job/step transitions from the timeline
- New log lines as they're written by the running step

The underlying streaming mechanism uses `?id=N` on the log endpoint,
which returns log content up to line N. The watcher increments N
each tick and only prints lines it hasn't seen before.

The watch exits automatically when the build reaches a terminal
state, or on Ctrl+C.

---

## Skills for AI Agents

`ado` ships with embedded skill files (YAML-frontmatter Markdown) for use by
AI coding agents. The skills teach agents how to use the CLI, not the
underlying REST API — agents should never call the Azure DevOps REST API
directly when the CLI is available.

### What's in the binary

Three skills are embedded at compile time and exposed via:

```bash
ado skills list                       # List all available skills
ado skills read ado-cli               # Main reference: setup, auth, all 24 command groups
ado skills read ado-auth              # Authentication details (PAT, browser, MSA, troubleshooting)
ado skills read ado-ci                # CI/CD patterns: GitHub Actions, GitLab CI, secrets, scripts
```

Skills are also on disk in `crates/ado-skills/assets/{name}/SKILL.md` for repository-level
inspection or to copy into an agent's `~/.claude/skills/` directory.

### For AI agents: how to use these skills

1. **Read `ado-cli` first** when you need to discover available commands. The
   reference table in that skill is the canonical index — it
   covers all 24 service areas (agent-pools, areas, connections, extensions,
   imports, banners, security, iterations, projects, repos, branch-policies,
   prs, workitems, teams, test-coverage, test-results, users, pipelines,
   pipelines-folders, pipelines-builds, pipelines-artifacts, packages,
   releases, wikis).

2. **Read `ado-auth`** before invoking any command, to confirm which auth
   method is in effect and how to handle the `Not authenticated` error. The
   skill explains PAT vs browser OAuth vs device code, MSA personal org
   support, and the priority order of CLI flags / env vars / credential store /
   config file.

3. **Read `ado-ci`** when running `ado` in a pipeline / CI script. The
   skill documents:
   - The recommended env-var-based auth (`ADO_ORG` + `ADO_PAT`)
   - JSON output (`ado --json ... | jq ...`) for scripting
   - Exit codes (`0` success, `1` for any error, `2` only for a cancelled
     `ado ci watch`)
   - GitHub Actions and GitLab CI examples

4. **Always prefer `--json` flag** when scripting. The CLI emits a single
   stable JSON document per command under `--json`, with no table formatting;
   human output is a table on a terminal and plain padded lines when piped.

5. **Always quote project and repo names** that contain spaces:
   ```bash
   ado repos list "Employee Management"
   ```

6. **Never log credentials.** The CLI keeps the token in the credential store and
   out of the config file; pass it via the `ADO_PAT` env var or `--pat` rather
   than hard-coding it, and never echo it yourself.

### Loading skills into an agent

If your agent supports the `skills` discovery protocol:

```bash
# Quick check
ado skills list | head

# Dump a skill to a file (e.g., for an agent's skill directory)
ado skills read ado-cli > ~/.claude/skills/ado-cli/SKILL.md
```

If your agent does **not** support the skills protocol, paste the output of
`ado skills read ado-cli` into its context — it contains the full command
reference (196 lines at this version).

### Skills vs REST API

The skills deliberately **do not** document the Azure DevOps REST API.
Agents reading the skills will call the CLI rather than constructing
HTTP requests by hand. This is intentional: the CLI handles auth, retries,
result pagination, and cross-org quirks that vary between AAD / MSA /
self-hosted deployments.

---

## Publishing

`ado` is distributed through two channels:

1. **GitHub Releases** — single self-contained binary per platform.
2. **npm** — 6 packages under the `@gilbertwong1996` scope, one of
   which (the main `@gilbertwong1996/ado`) is a thin Node.js wrapper
   that picks the right platform binary via `optionalDependencies`.

> **Note:** The npm scope is `@gilbertwong1996` (the maintainer's npm
> username), but the GitHub repo is `gilbertwong96/ado_cli` (the
> maintainer's GitHub handle). These are different accounts.

**All publishing happens from your local laptop, not from CI.** Tag a
release, push the tag, wait for the CI release job to attach the
binaries, then run the publish script. The CI release job produces the
GitHub Release artifacts; npm publishing is intentionally a manual
step so the maintainer (currently just you) can verify the artifacts
before they go public.

### Release flow

```bash
# 1. Bump every live version source (asserting each one)
just bump 1.0.0-rc.1
# 2. Write the CHANGELOG.md entry by hand (the bump does not touch it)
# 3. Confirm the gate
just ci && just npm-test
# 4. Commit and tag (the maintainer's step)
git add -u && git commit -m 'chore: bump to 1.0.0-rc.1'
git tag -a v1.0.0-rc.1 -m 'Release 1.0.0-rc.1'
git push github main v1.0.0-rc.1
```

Pushing the tag triggers the dist-generated `release` workflow, which builds the
five native archives on native runners and creates the GitHub Release with the
archives, installers and checksums attached. The tag also decides the version:
the archives are named by target triple and never carry the version, so
`releases/latest/download/<archive>` always resolves to the newest release.

Wait a few minutes for CI to finish. Verify the release is up:

```bash
gh release view v1.0.0-rc.1
```

Then publish to npm locally:

```bash
# Downloads the five archives from the v1.0.0-rc.1 release + publishes all 6 pkgs
./scripts/npm-publish.sh 1.0.0-rc.1
```

That's it. The script handles everything else.

### What the publish script does

`scripts/npm-publish.sh VERSION` runs four steps:

1. **`gh release download v${VERSION}`** — fetches the five cargo-dist archives
   (`ado-{aarch64-apple-darwin,x86_64-apple-darwin,x86_64-unknown-linux-musl,aarch64-unknown-linux-musl}.tar.gz`
   and `ado-x86_64-pc-windows-msvc.zip`) from the GitHub Release you just tagged.
2. **Unpacks each archive** and copies the binary into
   `npm/@gilbertwong1996-ado-<platform>-<arch>/bin/ado{,.exe}`.
3. **`jq` bumps the version** in all 6 `package.json` files — `version`, and the
   main package's five `optionalDependencies`.
4. **`npm publish --access public`** for the 5 platform packages first,
   then the main package last (so its `optionalDependencies` resolve
   cleanly).

The main package's `postinstall` downloads the platform archive itself when npm
skipped the optional dependency, and installs shell completion.

### Useful flags

```bash
./scripts/npm-publish.sh 1.0.0-rc.1 --dry-run       # show what would happen, no network
./scripts/npm-publish.sh 1.0.0-rc.1 --skip-download  # binaries already in place
```

### Verifying a publish

```bash
# Check the published package on npm
npm view @gilbertwong1996/ado
npm view @gilbertwong1996/ado-darwin-arm64 dist.integrity

# Install from npm and smoke-test
npm install -g @gilbertwong1996/ado
ado --help
ado projects list --json
ado skills install --target pi
```

### Prerequisites

| Tool | Why | How to get it |
|---|---|---|
| `gh` | Download release archives | `brew install gh && gh auth login` |
| `npm` | Publish packages | `brew install node` (bundles npm) |
| `jq` | Edit `package.json` files | `brew install jq` |
| `tar` | Unpack the `.tar.gz`/`.zip` archives | system `tar` |
| An `npm` token with publish rights on `@gilbertwong1996/*` | Auth | `npm login` then verify with `npm whoami` |

The script uses whatever `npm` authentication is in your environment
(usually `~/.npmrc`). No tokens are hard-coded.

### Why local-only publishing?

A few reasons:

- **You're the only maintainer.** There's no team to coordinate with,
  and no separation-of-duties that requires a CI service identity.
- **OIDC provenance doesn't work locally.** `npm publish --provenance`
  requires a GitHub Actions OIDC token, which only Actions has. Your
  local publish will show no provenance badge on npmjs.com — that's
  fine for a single-maintainer project.
- **You want to verify before going public.** The script's `--dry-run`
  flag lets you inspect exactly what would be published without
  actually publishing.
- **The `NPM_TOKEN` secret is fragile.** Long-lived npm tokens in CI
  are a security liability. A local publish uses your own short-lived
  login session.

If a second maintainer joins later, the script can still be invoked
from CI by exporting the same `gh` and `npm` auth that you'd use
locally. The script has no assumptions about where it runs.

### Rolling back a bad publish

npm allows unpublishing within 72 hours of release:

```bash
npm unpublish @gilbertwong1996/ado@1.0.0-rc.1
```

After 72 hours, you'll need to publish a new patch version. Prefer
fix-forward over rollback.

### Why the 6-package split?

| Package | Size | Who downloads |
|---|---|---|
| `@gilbertwong1996/ado` | ~3 KB (Node.js wrapper) | Everyone |
| `@gilbertwong1996/ado-darwin-arm64` | ~9 MB | Apple Silicon Mac users only |
| `@gilbertwong1996/ado-darwin-x64` | ~9 MB | Intel Mac users only |
| `@gilbertwong1996/ado-linux-arm64` | ~15 MB | Linux aarch64 users only |
| `@gilbertwong1996/ado-linux-x64` | ~16 MB | Linux x86_64 users only |
| `@gilbertwong1996/ado-win32-x64` | ~22 MB | Windows users only |

`os`, `cpu`, and `libc` fields in each `package.json` make npm pick
only the right one at install time. A Mac user never downloads the
22 MB Windows binary, and vice versa.

---

## License

Copyright © 2026 Gilbert Wong

This project is licensed under the **Apache License, Version 2.0** — see the
[LICENSE](LICENSE) file for the full text. You may obtain a copy of the
license at <https://www.apache.org/licenses/LICENSE-2.0>.

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
