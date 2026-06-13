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

# Build, sign, and notarize the macOS binary. See bin/sign.sh for env vars.
release-macos:
    @just release
    @bin/sign.sh

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
