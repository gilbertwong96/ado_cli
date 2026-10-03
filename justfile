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

# Run the full Rust quality gate — the seven stages below.
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

# Run the npm package's release-artifact suite (node --test): the postinstall
# downloader and the archive resolution that fetch the dist archives, plus
# the shell-completion install, which runs the release binary built here.
npm-test: build-release
    node --test npm/@gilbertwong1996-ado/test

# ── Helpers ────────────────────────────────────────────────────────────

# `check` is the full gate (`just ci` + `just npm-test`); `all` adds the
# release build. Both cover the Rust tree, which is the whole tree now.

# Show all checks pass
check: ci npm-test

# Full build + test + release
all: check build-release
    @echo "✅ All checks passed, release built"

# ── Version Bumping ────────────────────────────────────────────────────
# Bump the version across every live version source in the Rust tree.
# Usage: just bump 0.7.0
#
# Updates, asserting each target after the edit (a missing, unmatched or
# stale target fails the run — no step may report success while changing
# nothing):
#   * Cargo.toml               — [workspace.package] version (canonical)
#   * Cargo.lock               — every workspace member's version
#   * npm/@*/package.json      — all 6 npm package manifests: `version`,
#                                 and the main package's 5
#                                 `optionalDependencies`
#   * crates/ado-skills/assets/*/SKILL.md
#                              — the `version:` frontmatter, which tracks
#                                 the CLI version
#   * crates/ado/tests/cli_skills.rs
#     crates/ado-skills/src/lib.rs
#                              — the frontmatter-version pins
#   * crates/ado/tests/snapshots/
#       cli_schema__schema_version_target_shape_matches_oracle.snap
#                              — the schema snapshot's version line
#   * github-page/index.html   — guards the class of version-bearing
#                                 archive literals: an
#                                 `ado-<version>-macos-aarch64` literal is
#                                 rewritten when present, and the step fails
#                                 if any stale version-bearing literal remains
#   * README.md                — every occurrence of the old version
#
# Does NOT auto-update (needs human input):
#   * CHANGELOG.md             — needs a human-written entry
#
# Files intentionally left alone:
#   * npm/@*-{platform}/bin/ado{,.exe} — downloaded from the GitHub
#                                 Release by the publish script
#   * crates/ado/tests/cli_argv.rs, crates/ado/tests/cli_version.rs
#                              — read the version from the package via
#                                 env!("CARGO_PKG_VERSION")
#
# The schema snapshot's version line is rewritten in place; if it ever
# drifts from the generated schema, refresh it with
# `INSTA_UPDATE=always cargo test -p ado`.
#
# Running the recipe twice with the same argument is refused by the
# "same as the current version" guard.
bump new_version:
    #!/usr/bin/env bash
    set -euo pipefail
    export LC_ALL=C
    NEW="{{new_version}}"

    die() { echo "ERROR: $*" >&2; exit 1; }
    count() { grep -c -F -- "$1" "$2" || true; }
    esc() { printf '%s' "$1" | sed 's/\./\\./g'; }
    # Rewrite a file in place; refuse a missing file or a changed line count.
    rewrite() {
        local file="$1" script="$2" tmp
        [[ -f "$file" ]] || die "$file: missing"
        tmp=$(mktemp)
        sed -E "$script" "$file" > "$tmp" || die "$file: rewrite failed"
        [[ "$(wc -l < "$file")" == "$(wc -l < "$tmp")" ]] \
            || die "$file: the rewrite changed the line count"
        mv "$tmp" "$file"
    }
    read_manifest_version() {
        awk '
            /^\[workspace\.package\]/ { in_section = 1; next }
            /^\[/                     { in_section = 0 }
            in_section && /^version = "/ {
                sub(/^version = "/, ""); sub(/"$/, ""); print; exit
            }
        ' Cargo.toml
    }
    lock_version() {
        awk -v m="$1" '
            $0 == "[[package]]"      { name = "" }
            $0 == "name = \"" m "\"" { name = m; next }
            name == m && /^version = "/ {
                sub(/^version = "/, ""); sub(/"$/, ""); print; exit
            }
        ' Cargo.lock
    }

    if [[ -z "$NEW" ]]; then
        die "Usage: just bump <new-version>  (e.g. just bump 0.7.0)"
    fi
    # Loose semver check; it only needs to catch a typo before any edit.
    if ! [[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$ ]]; then
        die "'$NEW' doesn't look like a semver version (e.g. 0.5.0 or 1.0.0)"
    fi

    # 1. the current version: [workspace.package] version in Cargo.toml
    OLD=$(read_manifest_version)
    if [[ -z "$OLD" ]]; then
        die "Cargo.toml: no version under [workspace.package]"
    fi
    if [[ "$OLD" == "$NEW" ]]; then
        die "new version ($NEW) is the same as the current version ($OLD)"
    fi

    echo "Bumping $OLD → $NEW"
    echo ""

    # 2. Cargo.toml — the canonical version
    n=$(count "version = \"$OLD\"" Cargo.toml)
    [[ "$n" == 1 ]] || die "Cargo.toml: expected 1 'version = \"$OLD\"' line, found $n"
    rewrite Cargo.toml "s/^version = \"$(esc "$OLD")\"$/version = \"$NEW\"/"
    [[ "$(read_manifest_version)" == "$NEW" ]] || die "Cargo.toml: [workspace.package] version does not read $NEW"
    [[ "$(count "version = \"$OLD\"" Cargo.toml)" == 0 ]] || die "Cargo.toml: $OLD still present"
    echo "  ✓ Cargo.toml ([workspace.package] version)"

    # 3. Cargo.lock — every workspace member's version
    cargo update --workspace --offline || die "cargo update --workspace --offline failed"
    MEMBERS=$(sed -nE 's/^members = \[(.*)\]$/\1/p' Cargo.toml | tr -d '"' | tr ',' ' ')
    [[ -n "$MEMBERS" ]] || die "Cargo.toml: could not read [workspace] members"
    MEMBER_COUNT=0
    for dir in $MEMBERS; do
        member=$(awk '/^name = "/ { sub(/^name = "/, ""); sub(/"$/, ""); print; exit }' "$dir/Cargo.toml")
        [[ -n "$member" ]] || die "$dir/Cargo.toml: no [package] name"
        v=$(lock_version "$member")
        [[ -n "$v" ]] || die "Cargo.lock: no [[package]] entry for member '$member'"
        [[ "$v" == "$NEW" ]] || die "Cargo.lock: $member is at $v, expected $NEW"
        MEMBER_COUNT=$((MEMBER_COUNT + 1))
    done
    cargo metadata --locked --format-version 1 >/dev/null \
        || die "Cargo.lock is not in sync: 'cargo metadata --locked' failed"
    # The only lock lines cargo may touch are member versions.
    while IFS= read -r line; do
        case "$line" in
            -version\ =*|+version\ =*) ;;
            *) die "Cargo.lock: unexpected change: $line" ;;
        esac
    done < <(git diff -U0 -- Cargo.lock | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)')
    echo "  ✓ Cargo.lock ($MEMBER_COUNT workspace members at $NEW; cargo metadata --locked green)"

    # 4. npm manifests — version + the main package's optionalDependencies
    NPM_MANIFESTS=(
        npm/@gilbertwong1996-ado/package.json
        npm/@gilbertwong1996-ado-darwin-arm64/package.json
        npm/@gilbertwong1996-ado-darwin-x64/package.json
        npm/@gilbertwong1996-ado-linux-arm64/package.json
        npm/@gilbertwong1996-ado-linux-x64/package.json
        npm/@gilbertwong1996-ado-win32-x64/package.json
    )
    for pkg in "${NPM_MANIFESTS[@]}"; do
        [[ -f "$pkg" ]] || die "npm manifest missing: $pkg"
    done
    # The glob is the discovery side: a manifest it finds that the list does
    # not name is a target the bump would silently miss.
    for pkg in npm/@gilbertwong1996-ado/package.json npm/@gilbertwong1996-ado-*/package.json; do
        [[ -f "$pkg" ]] || die "npm manifest missing: $pkg"
        case " ${NPM_MANIFESTS[*]} " in
            *" $pkg "*) ;;
            *) die "npm manifest is not in the bump's target set: $pkg" ;;
        esac
    done
    for pkg in "${NPM_MANIFESTS[@]}"; do
        name_before=$(jq -r .name "$pkg") || die "$pkg: not valid JSON"
        tmp=$(mktemp)
        jq --arg v "$NEW" '
            .version = $v
            | if has("optionalDependencies")
                  then .optionalDependencies |= with_entries(.value = $v)
                  else . end
        ' "$pkg" > "$tmp" || die "$pkg: jq failed"
        mv "$tmp" "$pkg"
        [[ "$(jq -r .version "$pkg")" == "$NEW" ]] || die "$pkg: version does not read $NEW"
        [[ "$(jq -r .name "$pkg")" == "$name_before" ]] || die "$pkg: name changed"
    done
    main=npm/@gilbertwong1996-ado/package.json
    [[ "$(jq -r '.optionalDependencies | length' "$main")" == 5 ]] \
        || die "$main: expected 5 optionalDependencies"
    stale=$(jq -r --arg v "$NEW" '.optionalDependencies | to_entries[] | select(.value != $v) | "\(.key)=\(.value)"' "$main")
    [[ -z "$stale" ]] || die "$main: optionalDependencies not at $NEW: $stale"
    echo "  ✓ npm/@*/package.json (6 manifests; version + 5 optionalDependencies)"

    # 5. skills assets — the version frontmatter tracks the CLI version
    ASSETS=()
    for skill in crates/ado-skills/assets/*/SKILL.md; do
        [[ -f "$skill" ]] || die "skills asset missing: $skill"
        ASSETS+=("$skill")
    done
    SKILLS_OLD=""
    for skill in "${ASSETS[@]}"; do
        [[ "$(grep -c -E '^version: ' "$skill" || true)" == 1 ]] \
            || die "$skill: expected exactly 1 'version:' frontmatter line"
        v=$(sed -nE 's/^version: "(.*)"$/\1/p' "$skill")
        [[ -n "$v" ]] || die "$skill: 'version:' is not of the form version: \"...\""
        if [[ -z "$SKILLS_OLD" ]]; then
            SKILLS_OLD="$v"
        fi
        [[ "$v" == "$SKILLS_OLD" ]] || die "$skill: frontmatter version $v disagrees with $SKILLS_OLD"
        rewrite "$skill" "s/^version: \".*\"$/version: \"$NEW\"/"
        [[ "$(sed -nE 's/^version: "(.*)"$/\1/p' "$skill")" == "$NEW" ]] \
            || die "$skill: version does not read $NEW"
        echo "  ✓ $skill (version $v → $NEW)"
    done

    # 6. frontmatter-version pins in the test suites
    for f in crates/ado/tests/cli_skills.rs crates/ado-skills/src/lib.rs; do
        if [[ "$SKILLS_OLD" != "$NEW" ]]; then
            before=$(count "$SKILLS_OLD" "$f")
            [[ "$before" -ge 1 ]] || die "$f: no occurrence of the frontmatter version $SKILLS_OLD"
            rewrite "$f" "s/$(esc "$SKILLS_OLD")/$NEW/g"
            [[ "$(count "$SKILLS_OLD" "$f")" == 0 ]] || die "$f: $SKILLS_OLD still pinned"
        fi
        [[ "$(count "$NEW" "$f")" -ge 1 ]] || die "$f: does not read the frontmatter version $NEW"
        echo "  ✓ $f (frontmatter version pins)"
    done

    # 7. the schema snapshot's version line
    SNAP=crates/ado/tests/snapshots/cli_schema__schema_version_target_shape_matches_oracle.snap
    n=$(count "\"version\": \"$OLD\"" "$SNAP")
    [[ "$n" == 1 ]] || die "$SNAP: expected 1 '\"version\": \"$OLD\"' line, found $n"
    rewrite "$SNAP" "s/\"version\": \"$(esc "$OLD")\"/\"version\": \"$NEW\"/"
    [[ "$(count "\"version\": \"$NEW\"" "$SNAP")" == 1 ]] || die "$SNAP: version does not read $NEW"
    echo "  ✓ $(basename "$SNAP") (version line)"

    # 8. github-page/index.html — version-bearing archive literals
    PAGE=github-page/index.html
    [[ -f "$PAGE" ]] || die "$PAGE: missing"
    page_before=$( { grep -oE 'ado-[0-9]+\.[0-9]+\.[0-9]+[A-Za-z0-9.+-]*' "$PAGE" || true; } | wc -l | tr -d ' ')
    rewrite "$PAGE" "s/ado-[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?-macos-aarch64/ado-$(esc "$NEW")-macos-aarch64/g"
    page_stale=$(grep -oE 'ado-[0-9]+\.[0-9]+\.[0-9]+[A-Za-z0-9.+-]*' "$PAGE" | grep -v -F "ado-$NEW" || true)
    [[ -z "$page_stale" ]] || die "$PAGE: stale version-bearing literal(s) remain: $page_stale"
    echo "  ✓ $PAGE ($page_before version-bearing literal(s), none stale)"

    # 9. README.md — the old version in the publishing flow and examples
    README=README.md
    [[ -f "$README" ]] || die "$README: missing"
    n=$(count "$OLD" "$README")
    if [[ "$n" -ge 1 ]]; then
        rewrite "$README" "s/$(esc "$OLD")/$NEW/g"
    fi
    [[ "$(count "$OLD" "$README")" == 0 ]] || die "$README: still reads $OLD"
    echo "  ✓ $README ($n occurrence(s) of $OLD rewritten)"

    echo ""
    echo "Done. You still need to:"
    echo ""
    echo "  1. Add a CHANGELOG.md entry under '## [$NEW]'"
    echo "  2. Run \`cargo fmt\` and \`just check\` to confirm the gate is green"
    echo "  3. Commit and tag (the maintainer's step):"
    echo "       git add -u && git commit -m 'chore: bump to $NEW'"
    echo "       git tag -a v$NEW -m 'Release $NEW'"
    echo "       git push github main v$NEW"
    echo "       (the release workflow builds the five dist archives)"
    echo "  4. Publish the npm packages (the maintainer's step):"
    echo "       ./scripts/npm-publish.sh $NEW"
    echo ""
    echo "Diff (review before committing):"
    echo "─────────────────────────────────────────────────────────"
    git --no-pager diff --stat
    echo "─────────────────────────────────────────────────────────"
    git --no-pager diff Cargo.toml Cargo.lock npm/ crates/ado-skills crates/ado/tests github-page/index.html README.md | head -120
