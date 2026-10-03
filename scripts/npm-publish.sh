#!/usr/bin/env bash
# publish-npm.sh — Publish the ado npm packages.
#
# Usage:
#   scripts/npm-publish.sh 0.1.0
#   scripts/npm-publish.sh 0.1.0 --skip-download
#   scripts/npm-publish.sh 0.1.0 --dry-run
#
# What it does:
#   1. Downloads the 5 platform archives from GitHub Releases (tagged v<version>),
#      unpacks each one and copies the binary into
#      npm/@gilbertwong1996-ado-<platform>-<arch>/bin/.
#   2. Updates the version field in all 6 package.json files to <version>.
#   3. Publishes the 5 platform packages first, then the main
#      @gilbertwong1996/ado, with a dist-tag derived from the version:
#      a prerelease (e.g. `1.0.0-rc.1`) goes to `next`, a stable version
#      (e.g. `0.6.0`) keeps npm's default `latest`. A version shape the
#      derivation does not recognise fails before any pack — a prerelease must
#      never be published to `latest` by accident.
#
# Requirements:
#   - gh (GitHub CLI, authenticated)
#   - npm (authenticated, with publish rights on @gilbertwong1996/*)
#   - jq (for JSON manipulation)
#   - tar (system tar; bsdtar on macOS and Windows reads the .tar.gz and .zip
#     artifacts the release ships)
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

# ── dist-tag derivation ───────────────────────────────────────────────
# npm refuses to publish a semver prerelease without an explicit --tag.
# A prerelease goes to `next` (the ecosystem's convention), a stable
# version keeps npm's default `latest`. Anything else fails here: a
# prerelease must never fall through to `latest` by accident.
if [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    DIST_TAG=""
elif [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+-[0-9A-Za-z.-]+$ ]]; then
    DIST_TAG="next"
else
    echo "ERROR: cannot derive an npm dist-tag for version '$VERSION'" >&2
    echo "       expected <major>.<minor>.<patch> or <major>.<minor>.<patch>-<prerelease>" >&2
    exit 1
fi

if [[ -n "$DIST_TAG" ]]; then
    echo "==> Prerelease $VERSION → dist-tag '$DIST_TAG'"
else
    echo "==> Stable $VERSION → npm's default dist-tag 'latest'"
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
    echo "==> Downloading ado v${VERSION} archives from GitHub Release..."
    gh release download "v${VERSION}" \
        --repo gilbertwong96/ado_cli \
        --pattern "ado-*.tar.gz" \
        --pattern "ado-*.zip" \
        --dir "$TMP_DIR"
fi

# ── step 2: unpack the archives into the platform packages ──────────
# Default behavior: overwrite any existing binary in the target dir
# (so a previous run's binary doesn't linger). With --skip-download,
# the binary is already in place from a previous run — just verify it.
#
# Each entry is platform-arch:release-archive:directory-inside-the-archive.
# The archive names are the cargo-dist target archives (five targets, one
# archive each); the tar archives carry a directory named after the archive,
# while cargo-dist's Windows zip is flat (its entries sit at the root).
PLATFORM_MAP=(
    "darwin-arm64:ado-aarch64-apple-darwin.tar.gz:ado-aarch64-apple-darwin"
    "darwin-x64:ado-x86_64-apple-darwin.tar.gz:ado-x86_64-apple-darwin"
    "linux-arm64:ado-aarch64-unknown-linux-musl.tar.gz:ado-aarch64-unknown-linux-musl"
    "linux-x64:ado-x86_64-unknown-linux-musl.tar.gz:ado-x86_64-unknown-linux-musl"
    "win32-x64:ado-x86_64-pc-windows-msvc.zip:ado-x86_64-pc-windows-msvc"
)

for entry in "${PLATFORM_MAP[@]}"; do
    IFS=':' read -r platform_arch archive target <<< "$entry"

    pkg_dir="$NPM_DIR/@gilbertwong1996-ado-${platform_arch}"
    if [[ "$platform_arch" == "win32-x64" ]]; then
        binary="ado.exe"
    else
        binary="ado"
    fi
    dest="$pkg_dir/bin/$binary"

    mkdir -p "$pkg_dir/bin"

    if [[ -n "$SKIP_DOWNLOAD" ]]; then
        # No source from the release. The binary should already be in
        # place from a previous run; just verify it exists.
        if [[ ! -f "$dest" ]]; then
            echo "ERROR: $dest not found" >&2
            echo "       --skip-download was set but the binary is not in place." >&2
            echo "       Run without --skip-download first, or run" >&2
            echo "       'gh release download v${VERSION} --pattern \"ado-*\"' manually." >&2
            exit 1
        fi
        echo "    kept $archive → $dest (--skip-download)"
    else
        # Unpack the release archive, then copy the binary out of it.
        src="$TMP_DIR/${archive}"
        if [[ ! -f "$src" ]]; then
            echo "ERROR: $src not found" >&2
            echo "       Make sure the release v${VERSION} has all 5 archives." >&2
            exit 1
        fi
        unpack="$TMP_DIR/unpacked-${platform_arch}"
        mkdir -p "$unpack"
        case "$archive" in
            *.zip) unzip -q -o "$src" -d "$unpack" ;;
            *)     tar -xf "$src" -C "$unpack" ;;
        esac
        # The tar archives carry the directory named after them; the zip is flat.
        from="$unpack/${target}/${binary}"
        [[ -f "$from" ]] || from="$unpack/${binary}"
        cp "$from" "$dest"
        chmod +x "$dest"
        echo "    unpacked $archive → $dest"
    fi
done

# ── step 2.5: assert the shipped postinstall hook is present ───────────
# The main @gilbertwong1996/ado package's package.json declares
# `"scripts": { "postinstall": "node scripts/postinstall.js" }` and
# lists `scripts/postinstall.js` in its `files` array. If the file
# isn't actually present in the package dir at publish time, npm
# silently omits it from the tarball (it doesn't error — it just
# packs what's there), and users on `npm install -g` get no shell
# completion auto-install. This was the second bug fixed in
# v0.2.1 (the v0.2.0 main-package tarball was published without
# scripts/postinstall.js inside). The hook lives in the package
# itself now, so there is nothing to copy — fail loudly if it is
# missing or empty.
main_pkg_dir="$NPM_DIR/@gilbertwong1996-ado"
postinstall="$main_pkg_dir/scripts/postinstall.js"
if [[ ! -s "$postinstall" ]]; then
    echo "ERROR: $postinstall is missing or empty" >&2
    echo "       The main package's postinstall hook can't ship without it." >&2
    exit 1
fi
echo "    found $postinstall ($(wc -c < "$postinstall" | tr -d ' ') bytes)"

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
if [[ -n "$DIST_TAG" ]]; then
    PUBLISH_FLAGS+=(--tag "$DIST_TAG")
fi
if [[ -n "$DRY_RUN" ]]; then
    PUBLISH_FLAGS+=(--dry-run)
    echo "==> Dry run: would publish the following packages (dist-tag ${DIST_TAG:-latest})..."
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
