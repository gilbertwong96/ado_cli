# AGENTS.md

## CI Quality Gate

Every change to this project **must** pass the full CI pipeline before merging.
Run the pipeline locally with:

```bash
mix ci
```

The CI alias runs all of the following checks in order, failing on the first failure:

| Step | Check | Tool |
|------|-------|------|
| 1 | Compile with all warnings as errors (our code only) | `mix compile --warnings-as-errors` |
| 2 | Ensure code is formatted | `mix format --check-formatted` |
| 3 | Static code analysis | `mix credo --strict` |
| 4 | Check for unused dependencies | `mix deps.unlock --check-unused` |
| 5 | Audit dependencies for vulnerabilities | `mix deps.audit` |
| 6 | Cross-reference analysis (no orphans) | `mix xref graph --label compile-connected --fail-above 0` |
| 7 | Type checking (with Finch false-positive filtering) | `mix ci.dialyzer` |
| 8 | Run unit tests with coverage | `mix test --cover` |

## GitHub Actions CI

In addition to the local pipeline, every push and PR runs the same checks
in `.github/workflows/ci.yml` on Linux + macOS runners:

- **Linux (Ubuntu)** — full quality gate (steps 1–8 above) + coverage
  uploaded to Codecov via `ex_coveralls`
- **macOS** — build the escript and run the unit test suite as a smoke test
  (Burrito cross-compilation is exercised in a separate workflow)

Coverage is tracked by Codecov. The badge in the README points to the
Codecov dashboard; configuration lives in the `coveralls:` section of
`mix.exs`.

The local `mix ci` command is the source of truth — if it passes locally
it will pass on CI. Never skip a check before pushing.

## Coverage reporting (Codecov)

Total project coverage is **7.9%** as of v0.2.0. This is honest: the 27
CLI command modules have 0% coverage because they call
`CliMate.halt_success` / `halt_error` (which exit the BEAM), making them
hard to unit-test. The `test_coverage.ignore_modules` list only excludes
4 modules that genuinely can't be tested (`AdoCli.Application`,
`AdoCli.TestServer`, `AdoCli.TestServer.Plug`, `Mix.Tasks.Ci.Dialyzer`).

The `mix test --cover` threshold check is set to `0` in `mix.exs`
(no enforced floor) until CLI integration tests bring the number up.
The Codecov badge shows the raw total.

**The right path forward** is integration tests for the CLI command
modules, not a bigger ignore list. Tests should run in a subprocess
and capture stdout/exit code — CliMate's `halt_*` pattern makes
this straightforward. Adding these would push coverage into the
70-90% range, at which point a 70% threshold becomes meaningful.

Coverage is reported to Codecov via the official bash uploader. To enable
it on CI, the user must add a `CODECOV_TOKEN` secret to the repo:

  1. Visit https://codecov.io/gh/gilbertwong96/ado_cli
  2. Sign in with the same GitHub account
  3. Go to Settings -> Upload Token
  4. Copy the token
  5. In the GitHub repo: Settings -> Secrets and variables -> Actions
     -> New repository secret
  6. Name: `CODECOV_TOKEN`, Value: <paste token>

The CI step that posts to Codecov is conditional on the secret being
set, so it's safe to leave the repo in this state until the user is
ready. When the secret is present, the codecov.io dashboard will start
showing coverage data within a few minutes of a CI run.

Note: `mix coveralls.post` is NOT the right path here — it posts to
coveralls.io, not codecov.io. For Codecov, the canonical path is
`mix test --cover` (with `tool: ExCoveralls`) + the codecov bash
uploader.

## Additional Quality Commands

```bash
mix quality  # Compile + Credo + ex_dna + reach + tests
mix lint     # Credo strict only
mix inspect  # Project structure map (reach)
mix health   # Dead code & smell detection (reach)
mix test     # Unit tests
```

## Testing without a browser (CI / Linux servers)

Use a Personal Access Token (PAT). No browser required.

```bash
# 1. Generate a PAT in Azure DevOps:
#    User Settings -> Personal Access Tokens -> New Token
#    Required scopes: vso.work, vso.code, vso.project, vso.build, vso.release
#    Or use a "Full access" token for broadest coverage.

# 2. One-off (don't save to disk) — env vars
export ADO_ORG=myorg
export ADO_PAT=xxxxxxxxxxxxxxxxxxxxxxxxxxxx
./ado projects list

# 3. Or save to config (preferred for repeated use)
just login-pat myorg xxxxxxxxxxxxxxxxxxxxxxxxxxxx
./ado projects list

# 4. Headless smoke test (any CI runner)
just smoke-test-pat myorg xxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

For a quick test on a Linux server without a desktop:
1. Build the escript: `mix escript.build` (or use a pre-built `ado_linux` from burrito_out)
2. Copy to the server along with `~/.ado_cli/config.json` (or set env vars)
3. Or use the Burrito-built `ado_linux` directly: `./burrito_out/ado_linux projects list --org X --pat Y`

## Development Principles

1. **No warnings**: The project must compile with zero warnings (`--all-warnings --warnings-as-errors`)
2. **Formatted code**: All code must pass `mix format --check-formatted`
3. **Strict linting**: Credo runs in `--strict` mode with zero tolerance
4. **Clean dependencies**: No unused deps, and all deps are audited for known CVEs
5. **No orphan modules**: Every module must be reachable from the compile-connected graph
6. **Type-safe**: Dialyzer must report zero unexpected type errors (Finch-related false positives are filtered)
7. **Full CI on every change**: Every code change must pass `mix ci` (all 8 stages) AND `mix test` before declaring the change complete. Never skip any stage.
8. **Run the full quality pipeline before committing**: At minimum, run `mix quality && mix ci` to validate compile, format, credo, ex_dna, reach, tests, deps, xref, and dialyzer all pass.
9. **90% test coverage on testable modules**: The following modules are excluded from coverage enforcement (tightly coupled to CliMate's halt_*): `AdoCli.Application`, `AdoCli.Auth`, `AdoCli.CLI.*`. All other modules must maintain ≥90% coverage. Run `MIX_ENV=test mix test --cover` to check.

## Dialyzer & Finch

Dialyzer cannot trace through Finch's HTTP client calls because Finch returns
dynamic types. This produces ~20 false positive `:pattern_match`, `:unused_fun`,
and `:call` warnings. The `mix ci.dialyzer` task filters these known false
positives and only fails on genuinely unexpected warnings.

## Building Releases

```bash
# Escript (dev)
mix escript.build

# Burrito cross-platform binary (prod)
MIX_ENV=prod mix release
```

