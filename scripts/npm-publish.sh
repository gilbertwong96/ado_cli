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

# ── step 1: download binaries from GitHub Release ───────────────────
if [[ -n "$SKIP_DOWNLOAD" ]]; then
    echo "==> Skipping download (--skip-download)"
else
    echo "==> Downloading ado v${VERSION} binaries from GitHub Release..."
    gh release download "v${VERSION}" \
        --repo gilbertwong96/ado_cli \
        --pattern "ado-${VERSION}-*" \
        --dir "$TMP_DIR"
fi

# ── step 2: copy binaries into platform packages ─────────────────────
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
    src="$TMP_DIR/${artifact_name}"

    if [[ ! -f "$src" ]]; then
        echo "ERROR: $src not found" >&2
        echo "       Make sure the release v${VERSION} has all 5 binaries." >&2
        exit 1
    fi

    mkdir -p "$pkg_dir/bin"
    if [[ "$platform_arch" == "win32-x64" ]]; then
        cp "$src" "$pkg_dir/bin/ado.exe"
    else
        cp "$src" "$pkg_dir/bin/ado"
        chmod +x "$pkg_dir/bin/ado"
    fi

    echo "    copied $artifact_name → npm/@gilbertwong1996-ado-${platform_arch}/bin/"
done

# ── step 3: update version in all package.json files ────────────────
echo "==> Updating version in all 6 package.json files..."
for pkg_json in "$NPM_DIR"/@gilbertwong1996-ado/package.json "$NPM_DIR"/@gilbertwong1996-ado-*/package.json; do
    tmp=$(mktemp)
    jq --arg v "$VERSION" '.version = $v' "$pkg_json" > "$tmp"
    mv "$tmp" "$pkg_json"
    pkg_name=$(jq -r '.name' "$pkg_json")
    echo "    $pkg_name: set version to $VERSION"
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
