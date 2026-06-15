# @gilbertwong1996/ado

**AI-native Azure DevOps CLI** — structured JSON output, embedded skills for
LLM agents, single-file cross-platform binary.

This is the npm distribution of [`ado`](https://github.com/gilbertwong96/ado_cli).
The main package picks the right platform binary (via `optionalDependencies`)
and execs it.

## Install

```bash
npm install -g @gilbertwong1996/ado
```

Then:

```bash
ado --help
ado login --org myorg
ado projects list --json
ado schema --json
ado skills install --target pi
```

## Features

- **AI-native** — `--json` on every command, stable error codes, self-discoverable
  command tree (`ado schema --json`).
- **Embedded skills for LLM agents** — `ado skills list` / `read` / `search`
  expose the skill catalog. `ado skills install` copies the skills into
  `~/.pi/agent/skills/`, `~/.claude/skills/`, or `~/.cursor/skills/` so the
  agent loads them natively.
- **24 service areas** — projects, repos, work items, pipelines, builds, pull
  requests, packages, security, agent pools, branches, wikis, service
  connections, etc.
- **3 auth methods** — PAT, browser OAuth (PKCE), device code. MSA personal
  accounts supported. No `az` CLI dependency.
- **CI/CD friendly** — `ado ci watch PROJECT BUILD_ID` streams build logs
  in real time.
- **Cross-platform** — single self-contained binary for macOS (arm64/x86_64),
  Linux (x86_64/aarch64), and Windows x86_64. No runtime dependencies.

## Documentation

See the full [README on GitHub](https://github.com/gilbertwong96/ado_cli#readme)
for the complete command reference.

## License

Apache-2.0 — see [LICENSE](./LICENSE).
