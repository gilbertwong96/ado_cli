# Changelog

All notable changes to `ado` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`ado prs diff`** for viewing the diff of a pull request. Three modes:
    * Default: list of changed files (path, change type, +/- counts)
    * `--file PATH`: full unified diff for one path
    * `--unified`: a single concatenated unified diff stream (pipe to
      delta/less/vimdiff for pretty viewing)
  Also supports `--iteration N` to inspect an earlier iteration
  (default: latest) and `--json` for LLM-friendly structured output.
- **`ado prs comments add`** for adding/replying to PR review comments.
  Three modes:
    * General thread: `ado prs comments add PROJ REPO PR --content "LGTM!"`
    * Inline (file/line): `ado prs comments add PROJ REPO PR --content "use a guard clause" --file-path src/foo.ex --line 42`
    * Reply to existing thread: `ado prs comments add PROJ REPO PR --content "fixed in abc123" --thread-id 5`
  Supports `--json` for structured output (returns `{ok, thread_id, comment_id}`).
  See https://learn.microsoft.com/en-us/rest/azure/devops/git/pull-request-threads
- **`ado prs comments add --status`** to set the new thread's status.
  Valid values: `active` (default), `fixed`, `wontFix`, `closed`, `byDesign`.
  Invalid values produce a clear error listing the allowed set.
- **`ado prs comments add --content @<file>`** to read comment text from a file
  (useful for multi-line comments). Trailing newlines are stripped.
- **`ado prs comments add --content -`** to read comment text from stdin
  (also for multi-line). Example: `echo 'first line\nsecond' | ado prs comments add ... --content -`
- **`ado prs comments list --all`** to expand the listing to show full comment
  content (no 80-char truncation), file path for inline threads, and
  reply markers (e.g. `[11] (reply to 10) bob:`). Default view still shows
  thread headers with a preview of each comment.
- **`ado prs comments update`** now supports both content and status
  changes. At least one of `--content` or `--status` is required.
    * `--content "new text"` edits the comment (legacy behavior)
    * `--status fixed` changes the thread's resolution state
      (active, fixed, wontFix, closed, byDesign)
    * Pass both to update in one call
    * `--content @<file>` reads from a file (multi-line friendly)
    * `--content -` reads from stdin
    * `--resolved-by-me` auto-sets the thread's `resolvedBy` field
      to the authenticated user's GUID (fetches it from
      `/_apis/connectionData`, cached for the command's lifetime)
    * `--dry-run` prints the would-be PATCH request(s) as JSON
      (method, path, body) and exits without making any network calls
    * `--json` emits a structured envelope
- **`ado version` subcommand and `--version` flag** for the next release.
  - `ado version` — prints `ado 0.2.0` (plain text) or `{"ok": true, "version": "0.2.0"}` (with `--json`)
  - `ado --version` — same output, exits immediately
  - `ado -v` still means `--verbose` (no breaking change to existing behavior)
  - Resolves the version from `Application.spec/2` (works in dev, escript, and Burrito binaries)
- **New `AdoCli.Version` module** (lib/ado_cli/version.ex) — shared helper for resolving the current version across dev, escript, and Burrito build contexts. Replaces the duplicated `current_version/0` that was previously in `AdoCli.CLI.Schema`.
- **Fixed bug**: `ado schema --json` returned an empty `"version": ""` field when run from the escript. The `AdoCli.Version` module now calls `Application.ensure_loaded/1` to load the bundled `.app` file, so `Application.spec/2` works correctly in escript/Burrito mode (where the app isn't auto-loaded).

### Breaking changes

- **Skill names renamed**: `ado_cli` → `ado-cli`, `ado_auth` → `ado-auth`,
  `ado_ci` → `ado-ci`. pi (the AI agent) requires skill names to use only
  lowercase `a-z`, `0-9`, and hyphens — underscores are rejected with a
  "name contains invalid characters" warning. Updated:
  - Source dirs in `priv/skills/`
  - `name:` field in each `SKILL.md` frontmatter
  - `@skills` embedded map (skill keys)
  - All test assertions
  - README + GitHub Pages references
  - Embedded error messages

  **Action required**: delete the old install dirs and reinstall:
  ```bash
  rm -rf ~/.pi/agent/skills/ado_{cli,auth,ci}
  rm -rf ~/.claude/skills/ado_{cli,auth,ci}
  rm -rf ~/.cursor/skills/ado_{cli,auth,ci}
  rm -rf ~/.codex/skills/ado_{cli,auth,ci}
  ado skills install
  ```

  Or with one command per target:
  ```bash
  ado skills install --target pi       # creates ~/.pi/agent/skills/ado-{cli,auth,ci}/
  ado skills install --target claude   # creates ~/.claude/skills/ado-{cli,auth,ci}/
  ```

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
