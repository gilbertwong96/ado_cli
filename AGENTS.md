# AGENTS.md

## CI Quality Gate

Every change to this project **must** pass the full CI pipeline before merging.
Run the pipeline with:

```bash
mix ci
```

The CI alias runs all of the following checks in order, failing on the first failure:

| Step | Check | Tool |
|------|-------|------|
| 1 | Compile with all warnings as errors | `mix compile --all-warnings --warnings-as-errors` |
| 2 | Ensure code is formatted | `mix format --check-formatted` |
| 3 | Static code analysis | `mix credo --strict` |
| 4 | Check for unused dependencies | `mix deps.unlock --check-unused` |
| 5 | Audit dependencies for vulnerabilities | `mix deps.audit` |
| 6 | Cross-reference analysis (no orphans) | `mix xref graph --label compile-connected --fail-above 0` |
| 7 | Type checking (with Finch false-positive filtering) | `mix ci.dialyzer` |
| 8 | Test coverage ≥ 90% on testable modules | `MIX_ENV=test mix test --cover` |

## Additional Quality Commands

```bash
mix quality  # Compile + Credo + ex_dna + reach + tests
mix lint     # Credo strict only
mix inspect  # Project structure map (reach)
mix health   # Dead code & smell detection (reach)
mix test     # Unit tests
```

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
