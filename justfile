# ado_cli build automation
# https://github.com/casey/just

default:
    @just --list

# ── Development ────────────────────────────────────────────────────────

# Build the escript for local development
dev:
    mix escript.build
    @echo "→ ./ado ready"

# Run the full CI pipeline
ci:
    mix ci

# Run the quality pipeline (ci + ex_dna + reach + tests)
quality:
    mix quality

# Run tests
test:
    mix test

# Run tests with coverage
test-cover:
    mix test --cover

# Format code
fmt:
    mix format

# Lint only (credo strict)
lint:
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

# Show all checks pass
check: ci test

# Full build + test + release
all: ci test release
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
