# Changelog

All notable changes to `ado` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-06-16

### Added

- **`ado test-results`**: List, show, and publish test results.
    * `ado test-results list PROJECT [--build-id ID] [--top N]` —
      table of recent test runs with total/passed/failed counts
    * `ado test-results show PROJECT RUN_ID` — detailed view of
      a single test run with outcome breakdowns
    * `ado test-results publish PROJECT --name NAME --file PATH`
      [--build-id ID] — upload Cobertura XML or JUnit results
      to Azure DevOps; creates a test run and attaches the file
  Supports `--json` for machine-readable output on all three
  subcommands.
- **`ado test-coverage`**: Fetch code coverage data.
    * `ado test-coverage show PROJECT BUILD_ID` — visual bar
      chart of coverage percentages per metric (lines, branches)
      with color-coded percentage bars (green ≥80%, yellow ≥50%,
      red otherwise).

  Both commands are registered in the CLI dispatch
  (`lib/ado_cli/cli.ex`) and the `ado-cli` skill
  (`priv/skills/ado-cli/SKILL.md`). 8 new tests added.

## [0.2.4] - 2026-06-16

### Fixed

- **`ado ci watch` log streaming was silently broken.** The same
  API-response destructuring bug that was fixed for timeline
  diff rendering in v0.2.3 (commit `0be521c`) also affected the
  log-streaming path (`stream_active_logs/2`). The Azure DevOps
  timeline API returns `%{"records" => [...]}`, not a bare list,
  so the `Enum.filter/2` over records was iterating over a
  map's key/value pairs (tuples like `{"records", [...]}`)
  instead of the actual record maps — `r["state"]` and
  `r["log"]` were always nil, and no logs were ever streamed.
  Fix: destructure with `%{"records" => records}`.

  Both call sites of `fetch_timeline/3` are now correct:
    * `render_timeline_diff/2` (fixed in `0be521c`)
    * `stream_active_logs/2` (this fix)

## [0.2.3] - 2026-06-16

### Fixed

- **`ado ci watch` raised `BadArityError` on the first status
  update.** The default `:print_callback` was `&IO.write/2`
  (a 2-arity function expecting device + data), but every
  render function called it as `print.(line)` (1-arity). The
  watcher crashed with:

      (BadArityError) &IO.write/2 with arity 2 called with 1 argument
        ("? Build 9655 · status=inProgress result=nil · <1s\n")
          (ado_cli 0.2.2) lib/ado_cli/ci/watcher.ex:124: render_status/3

  on every invocation. Fix: change the default to a 1-arity
  wrapper `&IO.write(:stdio, &1)`. Also made `render_status/3`
  and `render_final/2` public (`def` instead of `defp`) so they
  can be unit-tested directly. Added 2 e2e tests in
  `test/ado_cli/ci/watcher_e2e_test.exs` (one for custom print
  callback, one for the default) that catch this exact bug.

## [0.2.2] - 2026-06-16

> **Known issue**: this release has a third `ado ci watch` bug
> (BadArityError on the default print callback) that is fixed
> in **v0.2.3**. Use v0.2.3 if you depend on `ado ci watch`.

### Changed

- **`ado skills install --target=all` now prints a hint about copilot**:
  copilot is per-repo, not per-user, so it can't be included in
  `--target=all`. The output now notes this and tells the user
  how to install to copilot (`cd <repo> && ado skills install
  --target copilot`). Help text also clarifies copilot's
  per-repo nature.
- **Code style cleanups addressing `mix ex_dna` and `mix reach`**
  warnings. Includes:
    * Extracted `confirm_delete/2` and `print_id_name_type_table/1`
      into `AdoCli.CLI.Helpers` (was duplicated in `projects.ex` +
      `repos.ex` and `connections.ex` + `wikis.ex` respectively)
    * `mix ci` now runs `ex_dna` as its 8th quality step
    * Replaced `String.split/2 |> hd/1` with `String.split/3, parts: 2`
      in 4 places (`auth.ex`, `skills.ex`)
    * Refactored `skills.ex` file listing to iterate the map as
      `{key, value}` pairs instead of `Map.keys/1 |> Enum.filter`
    * Replaced guard `when content == "-"` with pattern
      `def resolve_content("-")` in `pull_requests.ex`
    * Replaced `<>` inside `Enum.reduce/3` with `Enum.join/2` in
      `frontmatter.ex`
    * Replaced 5 bare `rescue _` clauses with explicit exception
      types in `schema.ex`, `auth.ex`, `version.ex`

  The remaining 11 reach findings (10 dead-code false positives on
  the `handle_api_result` test pattern + 1 already-explained bare
  rescue in `auth.ex` + 2 map-shape suggestions) are noted in
  the commit but intentionally left for a future refactor; the
  dead-code warnings are about CLI test helpers that halt, the
  map-shape suggestions would require introducing two new
  structs across `skills.ex` and `pull_requests.ex`.

### Fixed

- **`ado ci watch` was completely non-functional in v0.2.1** due
  to two bugs that prevented any invocation from working:
    * **URLs missing the project segment.** Both
      `AdoCli.CLI.CI.build_path/3` and the Watcher helpers
      (`fetch_build/3`, `fetch_timeline/3`, the `stream_log`
      path construction) dropped the project from the URL,
      producing `/_apis/build/builds/{id}` instead of
      `/{project}/_apis/build/builds/{id}`. Azure DevOps
      rejected every request with `VS800075: The project with
      id 'No project was specified.'`
    * **KeyError on `--latest`.** `resolve_build_id/3` used
      dot-access (`parsed.arguments.build_id`) which raised
      `KeyError` when the optional `build_id` argument was
      absent (i.e. when using `--latest`). CliMate omits absent
      optional arguments from the map entirely. Fix: use
      `Map.get/2` with the same key.

  Both bugs are covered by 2 new regression tests in
  `test/ado_cli/cli/ci_test.exs` that pin the project-scoped
  URL and the no-crash behavior on `--latest`. Each test
  FAILS if either bug is reintroduced (verified by temporarily
  breaking the helpers and re-running).

## [0.2.1] - 2026-06-15

Re-publish of v0.2.0 with the npm packaging bugs fixed. The Elixir
code, GitHub release binaries, and Burrito build artifacts are
**identical** to v0.2.0 — only the npm distribution was rebuilt.

### Fixed

- **npm `optionalDependencies` referenced the wrong version.** The
  original v0.2.0 `@gilbertwong1996/ado@0.2.0` npm package
  shipped with `optionalDependencies` pointing at `0.1.0` of the
  platform packages, so `npm install -g @gilbertwong1996/ado`
  pulled the v0.1.0 binary. The new `optionalDependencies` points
  at `0.2.1` (matching this release). `scripts/npm-publish.sh`
  was missing the `optionalDependencies` bump — only `.version`
  was being updated. Fixed.
- **npm `scripts/postinstall.js` was missing from the v0.2.0
  tarball.** The main package's `package.json` declared a
  `postinstall` hook that auto-installs shell completion, and
  listed `scripts/postinstall.js` in `files`, but the publish
  script never actually copied the file into the package dir.
  npm silently omits missing files from the tarball, so users
  who installed v0.2.0 got no shell-completion auto-install.
  `scripts/npm-publish.sh` now has a step that copies the
  postinstall hook into the main package and hard-errors if the
  source file is missing.

### Notes

- v0.2.0 of the `@gilbertwong1996/ado` npm package was unpublished
  immediately after discovery of the bugs, but npm's policy
  forbids republishing an unpublished version. Hence v0.2.1.
- The v0.2.0 GitHub release (with all 5 binaries) is preserved.
  The v0.2.1 release on GitHub is a re-tag with the same
  binaries.
- The release also adds `scripts/npm-unpublish.sh` — a new script
  for unpublishing a specific version of all 6 packages within
  npm's 72h window, with `--dry-run` / `--yes` / `--no-local`
  flags and tolerant handling of partial state.

## [0.2.0] - 2026-06-15

The v0.2.0 release adds end-to-end PR review workflows (`prs comments`
add/list/update with file & thread context, plus `prs diff` in three
modes), shell completion for bash/zsh/fish/powershell with automatic
install via npm postinstall, and the `ado version` / `--version`
command. The release also cleans up all 25 outstanding Credo strict
issues.

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
  Supports `--status` (`active` (default) | `fixed` | `wontFix` |
  `closed` | `byDesign`) to set the new thread's status, `--content
  @<file>` to read text from a file, and `--content -` to read text
  from stdin. Emits `--json` with `{ok, thread_id, comment_id}` on
  success. See
  <https://learn.microsoft.com/en-us/rest/azure/devops/git/pull-request-threads>.
- **`ado prs comments list --all`** to expand the listing to show
  full comment content (no 80-char truncation), file path for inline
  threads, and reply markers (e.g. `[11] (reply to 10) bob:`). The
  default view still shows thread headers with a preview of each
  comment.
- **`ado prs comments update`** with both content and status changes
  (at least one of `--content` or `--status` is required):
    * `--content "new text"` edits the comment (legacy behavior)
    * `--status fixed` changes the thread's resolution state
      (`active`, `fixed`, `wontFix`, `closed`, `byDesign`)
    * `--content @<file>` reads from a file (multi-line friendly)
    * `--content -` reads from stdin
    * `--resolved-by-me` auto-sets the thread's `resolvedBy` field
      to the authenticated user's GUID (fetches it from
      `/_apis/connectionData`, cached for the command's lifetime)
    * `--dry-run` prints the would-be PATCH request(s) as JSON
      (method, path, body) and exits without making any network calls
    * `--json` emits a structured envelope
- **`ado completion`** for generating shell completion scripts.
  Supports bash, zsh, fish, and PowerShell. The generated script
  is static (regenerate it when ado upgrades) and always in sync
  with the CliMate command tree.

      eval "$(ado completion bash)"   # bash
      ado completion zsh > "${fpath[1]}/_ado"   # zsh
      ado completion fish | source    # fish
      ado completion powershell | Out-String | Invoke-Expression  # pwsh

  The shell is a positional argument (not `-s`, which is the
  global short for `--server`). Defaults to `bash` when no
  argument is given. Use `-w PATH` to write the script to a
  file instead of stdout (e.g. for system fpath installation).
- **npm postinstall auto-installs shell completion.** When you
  `npm install -g @gilbertwong1996/ado`, the postinstall hook
  generates the right completion script for your shell and wires
  it up (e.g. appends `fpath=($HOME/.zsh/completions $fpath) +
  autoload -U compinit && compinit` to `~/.zshrc`, installs to
  `~/.local/share/bash-completion/completions/ado` for bash, and
  adds a source line to `$PROFILE` for PowerShell). The hook is
  idempotent (checks for a marker line) and respects
  `ADO_NO_COMPLETION=1` to opt out. Set `ADO_BIN` to override the
  binary used during testing. Implementation lives in
  `scripts/postinstall.js` (336 LOC, Node.js, cross-platform).
- **`ado version` subcommand and `--version` flag.** `ado version`
  prints `ado 0.2.0` (plain text) or `{"ok": true, "version":
  "0.2.0"}` with `--json`. `ado --version` prints the same and
  exits immediately. `ado -v` still means `--verbose` (no breaking
  change to existing behavior). Version resolves from
  `Application.spec/2` and works in dev, escript, and Burrito
  binaries.
- **`AdoCli.Version` module** (`lib/ado_cli/version.ex`) — shared
  helper for resolving the current version across dev, escript,
  and Burrito build contexts. Replaces the duplicated
  `current_version/0` that was previously in `AdoCli.CLI.Schema`.
- **`AdoCli.Auth.current_user_id/0`** public helper that fetches
  and caches the authenticated user's GUID from
  `/_apis/connectionData`. Returns `{:ok, guid} | {:error, msg}`.
  Used by `ado prs comments update --resolved-by-me` to set the
  thread's `resolvedBy.id` without forcing the caller to look it
  up first.

### Fixed

- **`ado schema --json` empty `version` field** when run from the
  escript. `AdoCli.Version` now calls `Application.ensure_loaded/1`
  to load the bundled `.app` file, so `Application.spec/2` works
  correctly in escript/Burrito mode (where the app isn't
  auto-loaded).
- **npm `package.json` `os`/`cpu` filters** were missing on a
  couple of platform packages, so npm would warn on every install.
  `scripts/npm-publish.sh` now validates each `package.json` against
  the binary inside it before publishing, and refuses to publish
  a tarball that doesn't match the manifest.
- **`scripts/npm-publish.sh` JSON parse failure** when the maintainer
  ran it without arguments (the script would silently no-op).
  Added `set -euo pipefail`, an explicit `VERSION` positional arg,
  and a clearer error if `gh release view` can't find the release.

### Changed

- **Skill names renamed**: `ado_cli` → `ado-cli`, `ado_auth` →
  `ado-auth`, `ado_ci` → `ado-ci`. pi (the AI agent) requires skill
  names to use only lowercase `a-z`, `0-9`, and hyphens — underscores
  are rejected with a "name contains invalid characters" warning.
  Updated:
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
- **`AdoCli.CLI.Skills` refactored.** `resolve_target_dirs/3` is now
  public so tests can assert on the install layout without mocking
  `File.cwd/0`. `--target copilot` and `--target codex` now share a
  common path resolver, eliminating ~40 lines of duplication.
- **Code style cleanup: 25 outstanding Credo `--strict` issues
  fixed** across `lib/ado_cli/auth.ex`,
  `lib/ado_cli/cli/completion.ex`, `lib/ado_cli/cli/pull_requests.ex`,
  and `test/ado_cli/cli/pull_requests_test.exs`. Notable refactors:
    * `Completion.generate/2` split into a `dispatch/2` clause
      group keyed on shell name (cyclomatic complexity 9 → 5).
    * `PullRequests.update_comment/1` extracted `update_flags/1`
      and `resolve_inputs/2` (cyclomatic complexity 9 → 4).
    * `PullRequests.do_real_update/6` extracted `patch_only/4`
      (cyclomatic complexity 9 → 5).
    * `length(list) == 1` → `match?([_], list)` in tests
      (LengthComparison warning).
  Result: `786 mods/funs, 0 credo issues`.

### Notes

- All 311 Elixir tests pass. `mix format --check-formatted` is clean.
- ExUnit test count grew from 234 (v0.1.0) to 311 (v0.2.0) — the
  +77 tests cover the new `prs comments`, `prs diff`, `completion`,
  and `version` subcommands.
- The npm postinstall hook ships 5 unit tests in
  `scripts/test_postinstall.js`, run with `node --test`.

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

[Unreleased]: https://github.com/gilbertwong96/ado_cli/compare/v0.2.4...HEAD
[0.2.4]: https://github.com/gilbertwong96/ado_cli/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/gilbertwong96/ado_cli/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/gilbertwong96/ado_cli/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/gilbertwong96/ado_cli/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/gilbertwong96/ado_cli/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/gilbertwong96/ado_cli/releases/tag/v0.1.0
