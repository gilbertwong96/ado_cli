#!/usr/bin/env bash
# publish-npm.sh — Publish the ado npm packages.
#
# Usage:
#   scripts/npm-publish.sh 0.1.0
#   scripts/npm-publish.sh 0.1.0 --skip-download
#   scripts/npm-publish.sh 0.1.0 --dry-run
#
# What it does:
#   1. Downloads the 5 platform binaries from GitHub Releases (tagged v<version>)
#      and copies them into npm/@gilbertwong1996-ado-<platform>-<arch>/bin/.
#   2. Updates the version field in all 6 package.json files to <version>.
#   3. Publishes the 5 platform packages first, then the main
#      @gilbertwong1996/ado.
#
# Requirements:
#   - gh (GitHub CLI, authenticated)
#   - npm (authenticated, with publish rights on @gilbertwong1996/*)
#   - jq (for JSON manipulation)
#
# Scope note:
#   - npm: @gilbertwong1996 (the maintainer's npm username)
#   - GitHub: gilbertwong96 (the maintainer's GitHub handle)

set -euo pipefail

# ── args ─────────────────────────────────────────────────────────────
VERSION="${1:-}"
DRY_RUN=""
SKIP_DOWNLOAD=""

shift || true
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) DRY_RUN="--dry-run"; shift ;;
        --skip-download) SKIP_DOWNLOAD="1"; shift ;;
        *) echo "Unknown arg: $1" >&2; exit 1 ;;
    esac
done

if [[ -z "$VERSION" ]]; then
    echo "Usage: $0 VERSION [--dry-run] [--skip-download]" >&2
    echo "  e.g. $0 0.1.0" >&2
    exit 1
fi

# ── paths ────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NPM_DIR="$ROOT_DIR/npm"
TMP_DIR="$(mktemp -d)"

trap "rm -rf '$TMP_DIR'" EXIT

# ── step 0: validate all package.json files are valid JSON ────────
# Catches the kind of bug we hit before (unquoted @scope/name) early
# instead of failing partway through with a cryptic jq error.
echo "==> Validating package.json files..."
for pkg_json in "$NPM_DIR"/@gilbertwong1996-ado/package.json "$NPM_DIR"/@gilbertwong1996-ado-*/package.json; do
    if ! jq -e . "$pkg_json" > /dev/null 2>&1; then
        echo "ERROR: $pkg_json is not valid JSON" >&2
        jq -e . "$pkg_json" 2>&1 | head -2 >&2
        exit 1
    fi
done
echo "    all $(ls "$NPM_DIR"/@gilbertwong1996-ado*/package.json | wc -l | tr -d ' ') files valid"

# ── step 1: download binaries from GitHub Release ───────────────────
if [[ -n "$SKIP_DOWNLOAD" ]]; then
    echo "==> Skipping download (--skip-download); using binaries already in place"
else
    echo "==> Downloading ado v${VERSION} binaries from GitHub Release..."
    gh release download "v${VERSION}" \
        --repo gilbertwong96/ado_cli \
        --pattern "ado-${VERSION}-*" \
        --dir "$TMP_DIR"
fi

# ── step 2: copy binaries into platform packages ─────────────────────
# Default behavior: overwrite any existing binary in the target dir
# (so a previous run's binary doesn't linger). With --skip-download,
# the binary is already in place from a previous run — just verify it.
PLATFORM_MAP=(
    "darwin-arm64:ado-${VERSION}-macos-aarch64"
    "darwin-x64:ado-${VERSION}-macos-x86_64"
    "linux-arm64:ado-${VERSION}-linux-aarch64"
    "linux-x64:ado-${VERSION}-linux-x86_64"
    "win32-x64:ado-${VERSION}-windows-x86_64.exe"
)

for entry in "${PLATFORM_MAP[@]}"; do
    platform_arch="${entry%%:*}"
    artifact_name="${entry##*:}"

    pkg_dir="$NPM_DIR/@gilbertwong1996-ado-${platform_arch}"
    if [[ "$platform_arch" == "win32-x64" ]]; then
        dest="$pkg_dir/bin/ado.exe"
    else
        dest="$pkg_dir/bin/ado"
    fi

    mkdir -p "$pkg_dir/bin"

    if [[ -n "$SKIP_DOWNLOAD" ]]; then
        # No source from the release. The binary should already be in
        # place from a previous run; just verify it exists.
        if [[ ! -f "$dest" ]]; then
            echo "ERROR: $dest not found" >&2
            echo "       --skip-download was set but the binary is not in place." >&2
            echo "       Run without --skip-download first, or run" >&2
            echo "       'gh release download v${VERSION} --pattern \"ado-${VERSION}-*\"' manually." >&2
            exit 1
        fi
        echo "    kept $artifact_name → $dest (--skip-download)"
    else
        # Copy from the downloaded release artifact to the package bin.
        src="$TMP_DIR/${artifact_name}"
        if [[ ! -f "$src" ]]; then
            echo "ERROR: $src not found" >&2
            echo "       Make sure the release v${VERSION} has all 5 binaries." >&2
            exit 1
        fi
        cp "$src" "$dest"
        chmod +x "$dest"
        echo "    copied $artifact_name → $dest"
    fi
done

# ── step 2.5: copy the postinstall hook into the main package ────────
# The main @gilbertwong1996/ado package's package.json declares
# `"scripts": { "postinstall": "node scripts/postinstall.js" }` and
# lists `scripts/postinstall.js` in its `files` array. If the file
# isn't actually present in the package dir at publish time, npm
# silently omits it from the tarball (it doesn't error — it just
# packs what's there), and users on `npm install -g` get no shell
# completion auto-install. This step was the second bug fixed in
# v0.2.1 (the v0.2.0 main-package tarball was published without
# scripts/postinstall.js inside).
main_pkg_dir="$NPM_DIR/@gilbertwong1996-ado"
if [[ -d "$main_pkg_dir" ]]; then
    mkdir -p "$main_pkg_dir/scripts"
    if [[ -f "$ROOT_DIR/scripts/postinstall.js" ]]; then
        cp "$ROOT_DIR/scripts/postinstall.js" "$main_pkg_dir/scripts/postinstall.js"
        chmod +x "$main_pkg_dir/scripts/postinstall.js"
        echo "    copied scripts/postinstall.js → $main_pkg_dir/scripts/postinstall.js"
    else
        echo "ERROR: $ROOT_DIR/scripts/postinstall.js not found in the repo" >&2
        echo "       The main package's postinstall hook can't be installed without it." >&2
        exit 1
    fi
fi

# ── step 3: update version in all package.json files ────────────────
# We bump TWO fields, not one:
#   * `version` (the package's own version)
#   * `optionalDependencies` (in the main @gilbertwong1996/ado
#     package) — every entry there needs to be re-pointed at the
#     new platform-package version, otherwise npm pulls the old
#     platform binary at install time and the new subcommands are
#     missing. (This was the bug fixed in v0.2.0: the script
#     previously only updated `version`, leaving the optionalDeps
#     stuck at the previous release's version string.)
echo "==> Updating version (and optionalDependencies) in all 6 package.json files..."
for pkg_json in "$NPM_DIR"/@gilbertwong1996-ado/package.json "$NPM_DIR"/@gilbertwong1996-ado-*/package.json; do
    tmp=$(mktemp)
    jq --arg v "$VERSION" '
        .version = $v
        | if has("optionalDependencies")
              then .optionalDependencies |= with_entries(.value = $v)
              else .
          end
    ' "$pkg_json" > "$tmp"
    mv "$tmp" "$pkg_json"
    pkg_name=$(jq -r '.name' "$pkg_json")
    echo "    $pkg_name: set version to $VERSION (and optionalDependencies, if any)"
done

# ── step 4: publish (platform packages first, then main) ────────────
PUBLISH_FLAGS=(--access public)
if [[ -n "$DRY_RUN" ]]; then
    PUBLISH_FLAGS+=(--dry-run)
    echo "==> Dry run: would publish the following packages..."
fi

# Platform packages
for pkg_dir in \
    "$NPM_DIR/@gilbertwong1996-ado-darwin-arm64" \
    "$NPM_DIR/@gilbertwong1996-ado-darwin-x64" \
    "$NPM_DIR/@gilbertwong1996-ado-linux-arm64" \
    "$NPM_DIR/@gilbertwong1996-ado-linux-x64" \
    "$NPM_DIR/@gilbertwong1996-ado-win32-x64"; do
    pkg_name=$(jq -r '.name' "$pkg_dir/package.json")
    echo "==> Publishing $pkg_name@$VERSION..."
    (cd "$pkg_dir" && npm publish "${PUBLISH_FLAGS[@]}")
done

# Main package last (its optionalDependencies point to the platform packages)
echo "==> Publishing @gilbertwong1996/ado@$VERSION..."
(cd "$NPM_DIR/@gilbertwong1996-ado" && npm publish "${PUBLISH_FLAGS[@]}")

echo "==> Done."
