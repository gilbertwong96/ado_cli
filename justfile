# ado_cli build automation
# https://github.com/casey/just

default:
    @just --list

# ── Rust (the rewrite) ─────────────────────────────────────────────────

# Format Rust code
fmt:
    cargo fmt

# Verify Rust formatting is clean
fmt-check:
    cargo fmt --check

# Lint Rust code (clippy, warnings as errors)
lint:
    cargo clippy --all-targets -- -D warnings

# Run Rust tests
test:
    cargo test --workspace

# Build the Rust workspace
build:
    cargo build --workspace

# Build with locked dependencies and warnings as errors
build-strict:
    RUSTFLAGS="-Dwarnings" cargo build --locked

# Check for unused dependencies
machete:
    cargo machete

# Audit dependencies: advisories, licenses, bans, sources
deny:
    cargo deny check advisories licenses bans sources

# Run the Rust test suite on the CI runner
nextest:
    cargo nextest run --workspace

# Line-coverage floor (rewrite spec §9)
coverage:
    cargo llvm-cov --workspace --fail-under-lines 85

# Run the full Rust quality gate — `mix ci` parity per rewrite spec §10.
# Fail fast, cheap checks first: format, lint, build, deps, tests, coverage.
ci: fmt-check lint build-strict machete deny nextest coverage

# Build the stripped release binary the budgets below measure
build-release:
    cargo build --release --locked

# Enforce the rewrite spec §1 budgets: `ado --version` wall clock
# (regression guard 50ms; design target 10ms) and stripped release binary
# size (8MB). Measures the best of 3 runs and prints both measured values.
budget: build-release
    #!/usr/bin/env bash
    set -euo pipefail

    bin=target/release/ado
    if [[ ! -x "$bin" ]]; then
        echo "budget: $bin missing — run 'just build-release' first" >&2
        exit 1
    fi

    startup_limit_ms=50
    size_limit_bytes=$((8 * 1024 * 1024))

    # Best of 3: a cold first run pays page-fault costs that are not the
    # binary's startup cost.
    TIMEFORMAT='%R'
    best_seconds=""
    for run in 1 2 3; do
        seconds=$( { time "$bin" --version >/dev/null; } 2>&1 )
        echo "budget: run $run: ${seconds}s"
        if [[ -z "$best_seconds" ]] || awk -v a="$seconds" -v b="$best_seconds" 'BEGIN { exit !(a < b) }'; then
            best_seconds="$seconds"
        fi
    done

    startup_ms=$(awk -v s="$best_seconds" 'BEGIN { printf "%.1f", s * 1000 }')
    size_bytes=$(wc -c < "$bin" | tr -d '[:space:]')
    size_mib=$(awk -v b="$size_bytes" 'BEGIN { printf "%.2f", b / 1048576 }')
    limit_mib=$(awk -v b="$size_limit_bytes" 'BEGIN { printf "%.0f", b / 1048576 }')

    echo "budget: startup (best of 3) ${startup_ms}ms ≤ ${startup_limit_ms}ms"
    echo "budget: release binary ${size_bytes} bytes (${size_mib}MiB) ≤ ${size_limit_bytes} bytes (${limit_mib}MiB)"

    failed=0
    if awk -v ms="$startup_ms" -v limit="$startup_limit_ms" 'BEGIN { exit !(ms > limit) }'; then
        echo "budget: FAIL — startup ${startup_ms}ms exceeds ${startup_limit_ms}ms" >&2
        failed=1
    fi
    if (( size_bytes > size_limit_bytes )); then
        echo "budget: FAIL — release binary ${size_mib}MiB exceeds ${limit_mib}MiB" >&2
        failed=1
    fi
    exit "$failed"

# ── Development ────────────────────────────────────────────────────────

# Build the escript for local development
dev:
    mix escript.build
    @echo "→ ./ado ready"

# Run the full Elixir CI pipeline (frozen tree; `ci` is the Rust gate)
elixir-ci:
    mix ci

# Run the quality pipeline (ci + ex_dna + reach + tests)
quality:
    mix quality

# Run Elixir tests
elixir-test:
    mix test

# Run Elixir tests with coverage
elixir-test-cover:
    mix test --cover

# Format Elixir code
elixir-fmt:
    mix format

# Lint Elixir code (credo strict)
elixir-lint:
    mix credo --strict

# Generate docs
docs:
    mix docs

# Run with verbose output
run +args:
    mix escript.build
    ./ado {{args}} --verbose

# ── Burrito Release ────────────────────────────────────────────────────

# Build Burrito release for all targets (clears cache first)
# Output: burrito_out/ado_<target>{,.exe}, then renames to
#         burrito_out/ado-<version>-<os>-<arch>{,.exe} for stable naming.
release:
    rm -rf ~/Library/Application\ Support/.burrito/ado*
    rm -rf _build/prod
    MIX_ENV=prod mix release --overwrite
    @just release-rename
    @echo "→ burrito_out/"

# Build Burrito release without clearing cache (faster, for minor changes)
release-fast:
    rm -rf _build/prod
    MIX_ENV=prod mix release --overwrite
    @just release-rename
    @echo "→ burrito_out/"

# Clear Burrito cache only
release-clean:
    rm -rf ~/Library/Application\ Support/.burrito/ado*
    @# Burrito leaves staged build dirs and unpacked ERTS in $TMPDIR
    @-rm -rf ${TMPDIR:-/tmp}/burrito_build_* ${TMPDIR:-/tmp}/unpacked_erts_*

# List built binaries
release-list:
    @ls -lh burrito_out/

# Rename Burrito's ado_<target>{,.exe} binaries to the versioned,
# platform-tagged naming convention used by the CI release workflow:
#   ado-<version>-linux-x86_64
#   ado-<version>-linux-aarch64
#   ado-<version>-macos-aarch64
#   ado-<version>-macos-x86_64
#   ado-<version>-windows-x86_64.exe
# Original Burrito outputs are removed.
release-rename:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION=$(grep -E '^\s*version:\s*"' mix.exs | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
    for src in burrito_out/ado_*; do
      [[ -f "$src" ]] || continue
      base=$(basename "$src")
      ext=""
      [[ "$base" == *.exe ]] && ext=".exe"
      key="${base%.exe}"
      key="${key#ado_}"
      case "$key" in
        linux)     SUFFIX="linux-x86_64" ;;
        linux_arm) SUFFIX="linux-aarch64" ;;
        macos)     SUFFIX="macos-aarch64" ;;
        macos_x86) SUFFIX="macos-x86_64" ;;
        windows)   SUFFIX="windows-x86_64" ;;
        *) echo "::warn::Unknown Burrito target: $key (no rename rule)"; continue ;;
      esac
      dest="burrito_out/ado-${VERSION}-${SUFFIX}${ext}"
      mv "$src" "$dest"
      echo "renamed $src -> $dest"
    done

# (macOS code signing is intentionally not provided here. We
#  distribute via package managers — npm, Homebrew — which
#  sidestep macOS Gatekeeper entirely. See README for details.)

# ── Skills ─────────────────────────────────────────────────────────────

# List embedded skills
skills-list:
    mix escript.build
    ./ado skills list

# Read a skill (usage: just skill-read ado_cli)
skill-read name:
    mix escript.build
    ./ado skills read {{name}}

# ── Demo / Smoke Test ──────────────────────────────────────────────────

# Quick smoke test using saved browser auth (usage: just smoke-test gilbertscode)
smoke-test org:
    @echo "=== whoami ===" && ./ado whoami
    @echo "=== projects ===" && ./ado projects list --org {{org}} || true
    @echo "=== skills ===" && ./ado skills list

# Headless smoke test using PAT (no browser needed) — for CI / Linux servers.
# usage: just smoke-test-pat myorg xxxxxxxxxxxxx
smoke-test-pat org pat:
    @echo "=== whoami ===" && ./ado whoami --org {{org}} --pat {{pat}}
    @echo "=== projects ===" && ./ado projects list --org {{org}} --pat {{pat}} || true
    @echo "=== skills ===" && ./ado skills list

# Set up PAT-based login (writes config, no browser)
# usage: just login-pat myorg xxxxxxxxxxxxx
login-pat org pat:
    ./ado login --method pat --org {{org}} --pat {{pat}}
    @echo "✓ saved to ~/.ado_cli/config.json"

# ── Helpers ────────────────────────────────────────────────────────────

# `check` and `all` cover both toolchains until Wave 4 deletes the Elixir
# tree: `ci` is the Rust gate, `elixir-ci` still verifies the frozen fallback.

# Show all checks pass
check: ci elixir-ci

# Full build + test + release
all: ci elixir-ci release
    @echo "✅ All checks passed, release built"

# ── Version Bumping ────────────────────────────────────────────────────
# Bump the version across every source file that references it.
# Usage: just bump 0.2.2
#
# Updates:
#   * mix.exs                  — the canonical version
#   * npm/@*/package.json      — all 6 npm package manifests
#   * priv/skills/*/SKILL.md   — version frontmatter in every skill
#   * github-page/index.html   — the "Download binary" curl example
#   * README.md                — the Publishing section's release flow
#                                 (tag, push, npm-publish.sh, etc.)
#
# Does NOT auto-update (needs human input):
#   * CHANGELOG.md             — needs a human-written entry
#
# Must be regenerated after every bump (it fails loudly until then):
#   * crates/ado/tests/snapshots/
#       cli_schema__schema_version_target_shape_matches_oracle.snap
#     — the schema snapshot pins VERSION literally; refresh it with
#       `INSTA_UPDATE=always cargo test -p ado` (or `cargo insta test --accept`)
#
# Files intentionally left alone:
#   * npm/@*-{platform}/bin/ado{,.exe} — downloaded from the GitHub
#                                 Release by the publish script
#   * lib/ado_cli/version.ex   — reads the version dynamically from mix.exs;
#                                 there's no hard-coded string
#
# The task is idempotent: running it twice with the same arg is a
# no-op (the second pass sees nothing to change).
bump new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    NEW="{{new_version}}"

    # Current version from mix.exs
    OLD=$(grep -E '^\s*version:\s*"' mix.exs | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
    if [[ -z "$OLD" ]]; then
        echo "ERROR: couldn't read current version from mix.exs" >&2
        exit 1
    fi
    if [[ -z "$NEW" ]]; then
        echo "Usage: just bump <new-version>  (e.g. just bump 0.2.2)" >&2
        exit 1
    fi
    if [[ "$OLD" == "$NEW" ]]; then
        echo "ERROR: new version ($NEW) is the same as current version ($OLD)" >&2
        exit 1
    fi
    # Sanity-check the new version looks like a semver string. We
    # only need to be loose here — `mix version` would do a stricter
    # check, but we don't depend on Mix at the just layer.
    if ! [[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$ ]]; then
        echo "ERROR: '$NEW' doesn't look like a semver version (e.g. 0.2.2 or 1.0.0-rc.1)" >&2
        exit 1
    fi

    echo "Bumping $OLD → $NEW"
    echo ""

    # 1. mix.exs — the canonical version string
    sed -i '' "s/version: \"$OLD\"/version: \"$NEW\"/" mix.exs
    echo "  ✓ mix.exs"

    # 2. npm package manifests — all 6 package.json files
    for pkg in npm/@gilbertwong1996-ado{,-darwin-arm64,-darwin-x64,-linux-arm64,-linux-x64,-win32-x64}/package.json; do
        if [[ -f "$pkg" ]]; then
            sed -i '' "s/\"$OLD\"/\"$NEW\"/" "$pkg"
        fi
    done
    echo "  ✓ npm/@*/package.json (6 files)"

    # 3. priv/skills/*/SKILL.md — version frontmatter in YAML header
    for skill in priv/skills/*/SKILL.md; do
        if [[ -f "$skill" ]]; then
            sed -i '' "s/^version: \"$OLD\"/version: \"$NEW\"/" "$skill"
        fi
    done
    echo "  ✓ priv/skills/*/SKILL.md (version frontmatter)"

    # 4. github-page/index.html — the curl example
    sed -i '' -E "s/ado-[0-9]+\.[0-9]+\.[0-9]+-macos-aarch64/ado-${NEW}-macos-aarch64/g" github-page/index.html
    echo "  ✓ github-page/index.html (Download binary curl example)"

    # 5. README.md — the Publishing section's release flow + examples
    #    (lines ~688–788, the publishing cheat-sheet). We replace
    #    $OLD with $NEW; the rest of README shouldn't reference the
    #    version, but if it does, the diff at the end will show it.
    if [[ -f README.md ]]; then
        sed -i '' "s/$OLD/$NEW/g" README.md
        echo "  ✓ README.md (Publishing section)"
    fi

    echo ""
    echo "Done. You still need to:"
    echo ""
    echo "  1. Add a CHANGELOG.md entry under '## [$NEW]'"
    echo "  2. Run \`mix format\` to normalize the diff"
    echo "  3. Run \`just check\` to confirm CI is still green"
    echo "  4. Commit and tag:"
    echo "       git add -u && git commit -m 'release: v$NEW'"
    echo "       git tag -a v$NEW -m 'Release $NEW'"
    echo "       git push github main v$NEW"
    echo "       (CI will build the binaries and create the GitHub Release)"
    echo "  5. Run \`./scripts/npm-publish.sh $NEW\` locally to publish the npm packages"
    echo ""
    echo "Diff (review before committing):"
    echo "─────────────────────────────────────────────────────────"
    git --no-pager diff --stat
    echo "─────────────────────────────────────────────────────────"
    git --no-pager diff mix.exs npm/ github-page/index.html README.md | head -80
