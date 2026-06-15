# Changelog

All notable changes to `ado` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-06-15

The first public release. `ado` is a self-contained, cross-platform Azure DevOps
CLI built in Elixir, designed for both humans and AI agents (pi, Claude Code,
Copilot, Cursor). Every command supports `--json` for machine consumption, and
the embedded skills can be installed into any LLM agent's skill directory.

### Highlights

- **AI-native design** — Structured JSON envelopes, stable error codes, and a
  self-discoverable command tree (`ado schema --json`). See `ado --help` /
  `ado schema --json`.
- **Embedded skills for LLM agents** — `ado skills list` / `read` / `describe`
  / `search` / `install`. Skills install into `~/.pi/agent/skills/`,
  `~/.claude/skills/`, `~/.cursor/skills/`, or a custom path.
- **Cross-platform single binary** — Built with [Burrito]. One self-contained
  binary for macOS (arm64/x86_64), Linux (x86_64/aarch64), and Windows x86_64.
  No runtime dependencies.
- **Multiple auth methods** — PAT, browser OAuth (PKCE), and device code. MSA
  personal accounts (`*.visualstudio.com`) supported. No `az` CLI dependency.
- **24 service areas covered** — projects, repos, work items, pipelines,
  builds, pull requests, packages, security, agent pools, branches/policies,
  wikis, service connections, etc.
- **CI/CD friendly** — `ado ci watch PROJECT BUILD_ID` streams build logs in
  real time. Per-run variables, variable groups, PR automation.

### Added

- `ado schema` / `ado schema NAME --json` — Dump the full CLI command tree as
  a structured JSON object for LLM discovery. Single round trip, no `--help`
  parsing.
- `ado whoami --json` / `ado logout --json` / `ado login --json` — Structured
  output for all auth-related commands.
- `AdoCli.CLI.Output` module — Single source of truth for success and error
  envelopes. Stable error codes: `auth_required`, `not_found`,
  `validation_error`, `forbidden`, `conflict`, `api_error`, `network_error`,
  `cancelled`. Exit code is always 1 on error (LLMs should match on
  `error.code`, not exit code).
- `AdoCli.Skills.search/1` / `describe/1` / `list_skills_info/0` — Small-
  payload API for LLM agents to decide which skill to load.
- `AdoCli.Frontmatter.parse_commands/1` — Pure-Elixir frontmatter parser now
  supports a `commands:` YAML block list.
- `ado ci watch` — Real-time streaming of Azure DevOps pipeline build status
  and logs. Polls every 2s (configurable). Supports `--latest [--definition ID]
  [--branch REF]`. Renders build status, job/step transitions, live log lines.
  Normalizes CRLF to LF. Exits on terminal state or Ctrl+C.
- `ado skills install [--target pi|claude|cursor|/path] [--skill NAME]
  [--force] [--json]` — Install the embedded skills to an LLM agent's skill
  directory. Lets agents discover `ado` as a native skill on startup, instead
  of shelling out to `ado skills read`.
- Cross-agent skill compatibility — All `SKILL.md` frontmatters now include
  `name:` (matches pi / Claude Code / Cursor format).
- Apache-2.0 LICENSE file.

### Changed

- `ado` is now the canonical binary name (was `ado_cli`). Existing references
  in docs/skills updated.
- `AdoCli.Frontmatter` is pure-Elixir (no `yamerl` NIF dependency for project
  code).
- Skills bumped to v0.4.0 with new frontmatter fields (`name:`, expanded
  `commands:` lists including `ado schema` and `ado ci watch`).
- GitHub Actions upgraded to Node.js 24-compatible versions:
  `actions/checkout@v6`, `actions/cache@v5`, `actions/upload-artifact@v7`,
  `actions/download-artifact@v8`, `mlugg/setup-zig@v2.2.1`.
- macOS code signing removed in favor of npm + Homebrew distribution
  (sidesteps Gatekeeper by not setting the `com.apple.quarantine` xattr).
- `mix ci` runs 8 quality gates: compile (warnings-as-errors), format check,
  Credo `--strict`, deps unused check, deps audit, xref, Dialyzer (with Finch
  false-positive filtering), test coverage ≥ 90% on testable modules.
- Burrito upgraded to a Zig 0.16.0 fork (`gilbertwong96/burrito`,
  `zig-0.16.0` branch) with a Windows `toMode()` archiver fix.

### Fixed

- `Client.do_request` method/url parameter swap bug.
- Org injection into API URLs.
- TCP listener binding to `127.0.0.1` and `response_mode=query` for OAuth.
- `writeln(success(...))` ANSI color code leak.
- Burrito Windows `archiver.zig` `toMode()` crash (upstream fix).
- Various Credo complexity/readability warnings.

### Infrastructure

- GitHub Actions CI (`.github/workflows/ci.yml`):
  - `linux` job: full 8-step quality gate + excoveralls + Codecov.
  - `macos` job: escript build + smoke tests.
  - `ci-status` job: gates merge on both green.
  - `release` job: 5-way matrix (macOS arm64/x86_64, Linux x86_64/aarch64,
    Windows x86_64) cross-compiles with Burrito + Zig 0.16.0.
  - `release-attach` job: tag-push-only; downloads artifacts and creates a
    GitHub Release with markdown table.
- `mix test_coverage.ignore_modules` reduced to 4 modules (was the whole
  `AdoCli.Auth` and 27 CLI error-handling branches).
- `test/support/test_server.ex` — Bandit-based HTTP test server (replaces
  Bypass/Cowboy which were out of date).

### Notes

- The release matrix builds without code signing (npm + Homebrew distribution
  sidesteps Gatekeeper). See `bin/` removal in the commit history.
- Dialyzer reports zero unexpected type errors. Finch-related false positives
  are filtered by the `mix ci.dialyzer` task.
- ExUnit test count: 234 (across 30+ test files). All pass.

[Unreleased]: https://github.com/gilbertwong96/ado_cli/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/gilbertwong96/ado_cli/releases/tag/v0.1.0
