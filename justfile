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
release:
    rm -rf ~/Library/Application\ Support/.burrito/ado*
    rm -rf _build/prod
    MIX_ENV=prod mix release --overwrite
    @echo "→ burrito_out/"

# Build Burrito release without clearing cache (faster, for minor changes)
release-fast:
    rm -rf _build/prod
    MIX_ENV=prod mix release --overwrite

# Clear Burrito cache only
release-clean:
    rm -rf ~/Library/Application\ Support/.burrito/ado*

# List built binaries
release-list:
    @ls -lh burrito_out/

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

# Quick smoke test (usage: just smoke-test gilbertscode)
smoke-test org:
    @echo "=== whoami ===" && ./ado whoami
    @echo "=== projects ===" && ./ado projects list --org {{org}} || true
    @echo "=== skills ===" && ./ado skills list

# ── Helpers ────────────────────────────────────────────────────────────

# Show all checks pass
check: ci test

# Full build + test + release
all: ci test release
    @echo "✅ All checks passed, release built"
