#!/usr/bin/env bash
# npm-unpublish.sh — Unpublish a specific version of all 6 ado npm packages.
#
# Usage:
#   scripts/npm-unpublish.sh 0.2.0            # interactively confirm
#   scripts/npm-unpublish.sh 0.2.0 --yes      # no prompt
#   scripts/npm-unpublish.sh 0.2.0 --dry-run  # show what would happen
#   scripts/npm-unpublish.sh 0.2.0 --no-local # unpublish npm, leave local files alone
#
# What it does:
#   1. Verifies the version was published < 72h ago (npm's hard
#      cutoff for `npm unpublish <pkg>@<version>`). After 72h,
#      only `npm deprecate` is available, and the script bails.
#   2. Lists the 6 packages that would be unpublished, with
#      publish timestamps and sizes, and requires a typed
#      "yes" confirmation (or `--yes`).
#   3. Runs `npm unpublish @gilbertwong1996/<pkg>@<VERSION>` for
#      the main wrapper first, then the 5 platform packages.
#      Order matters: if we unpublish platform packages first
#      and the script dies, users on the in-flight version get
#      install errors instead of a clean "not found" error.
#   4. (Unless --no-local) Resets the local `npm/*/package.json`
#      files to a previous good version and removes the
#      downloaded binaries + postinstall.js, so the next
#      `./scripts/npm-publish.sh 0.2.0` run starts from a
#      clean slate.
#
# Scope note:
#   - npm: @gilbertwong1996 (the maintainer's npm username)
#   - GitHub: gilbertwong96 (the maintainer's GitHub handle)
#
# Requirements:
#   - npm (authenticated, with publish rights on @gilbertwong1996/*)
#   - jq (for JSON manipulation)
#   - bash 3.2+ (for `set -euo pipefail`, arrays, and `[[ ]]`)

set -euo pipefail

# ── args ─────────────────────────────────────────────────────────────
# If the first arg is --help/-h, show help and exit. Otherwise the
# first arg is the version and the rest are flags.
if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    sed -n '2,28p' "${BASH_SOURCE[0]}" | sed 's/^# \?//'
    exit 0
fi

VERSION="${1:-}"
ASSUME_YES=""
DRY_RUN=""
RESET_LOCAL="1"

shift || true
while [[ $# -gt 0 ]]; do
    case "$1" in
        --yes|-y)         ASSUME_YES="1"; shift ;;
        --dry-run)        DRY_RUN="1"; shift ;;
        --no-local)       RESET_LOCAL=""; shift ;;
        -h|--help)
            sed -n '2,28p' "${BASH_SOURCE[0]}" | sed 's/^# \?//'
            exit 0
            ;;
        *) echo "Unknown arg: $1" >&2; exit 1 ;;
    esac
done

if [[ -z "$VERSION" ]]; then
    echo "Usage: $0 VERSION [--yes] [--dry-run] [--no-local]" >&2
    echo "  e.g. $0 0.2.0 --yes" >&2
    exit 1
fi

# ── paths ────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NPM_DIR="$ROOT_DIR/npm"

# ── preflight: collect publish times across all 6 packages ──────────
# npm's "unpublish within 72h" rule is per-version, but the same
# 72h window applies to every package that shares that version.
# We collect the most recent publish time across all 6 packages
# (some may already be unpublished; the `time` map still caches
# the original publish timestamp for ~24h, and even after the
# map gets pruned the server enforces the same 72h cutoff on
# each `npm unpublish` call individually).
#
# We also tolerate the case where the main wrapper was already
# unpublished by the maintainer: as long as the platform packages
# still exist, the script can clean those up too.
PACKAGES=(
    "@gilbertwong1996/ado"
    "@gilbertwong1996/ado-darwin-arm64"
    "@gilbertwong1996/ado-darwin-x64"
    "@gilbertwong1996/ado-linux-arm64"
    "@gilbertwong1996/ado-linux-x64"
    "@gilbertwong1996/ado-win32-x64"
)

most_recent_epoch=0
most_recent_pkg=""
none_published=1
for pkg in "${PACKAGES[@]}"; do
    ts=$(npm view "$pkg" time --json 2>/dev/null \
         | jq -r --arg v "$VERSION" '.[$v] // empty')
    if [[ -n "$ts" ]]; then
        none_published=0
        # Parse ISO 8601 -> epoch. Strip fractional seconds + `Z`
        # so both GNU date and BSD date (macOS) accept it.
        iso_no_ms="${ts%.*}"
        iso_no_tz="${iso_no_ms%Z}"
        epoch=$(date -u -j -f '%Y-%m-%dT%H:%M:%S' "$iso_no_tz" +%s 2>/dev/null \
                || date -u -d "$iso_no_tz" +%s)
        if (( epoch > most_recent_epoch )); then
            most_recent_epoch=$epoch
            most_recent_pkg=$pkg
        fi
    fi
done

if (( none_published )); then
    echo "ERROR: no @gilbertwong1996 package has version $VERSION on npm." >&2
    echo "       Nothing to unpublish. Run \`npm view @gilbertwong1996/ado versions\`" >&2
    echo "       to see what's there." >&2
    exit 1
fi

NOW_EPOCH=$(date -u +%s)
HOURS_SINCE=$(( (NOW_EPOCH - most_recent_epoch) / 3600 ))
MINUTES_REMAINING=$(( (72 * 60) - ((NOW_EPOCH - most_recent_epoch) / 60) ))

if (( HOURS_SINCE >= 72 )); then
    echo "ERROR: $VERSION was published $HOURS_SINCE hours ago" >&2
    echo "       (oldest of the 6 packages, last seen on $most_recent_pkg)." >&2
    echo "       npm unpublish is only allowed within 72 hours of publish." >&2
    echo "       Use \`npm deprecate\` instead:" >&2
    echo "         npm deprecate @gilbertwong1996/ado@$VERSION 'broken release, use 0.1.0'" >&2
    exit 1
fi

if (( HOURS_SINCE >= 72 )); then
    echo "ERROR: $VERSION was published $HOURS_SINCE hours ago." >&2
    echo "       npm unpublish is only allowed within 72 hours of publish." >&2
    echo "       Use \`npm deprecate\` instead:" >&2
    echo "         npm deprecate @gilbertwong1996/ado@$VERSION 'broken release, use 0.1.0'" >&2
    exit 1
fi

# ── show the plan ───────────────────────────────────────────────────
echo "==> Will unpublish the following @gilbertwong1996 packages at version $VERSION:"
echo ""
for pkg in "${PACKAGES[@]}"; do
    if npm view "$pkg@$VERSION" version >/dev/null 2>&1; then
        size=$(npm view "$pkg@$VERSION" dist.unpackedSize 2>/dev/null || echo "?")
        size_h=$(numfmt --to=iec "$size" 2>/dev/null || echo "${size} bytes")
        ts=$(npm view "$pkg" time --json 2>/dev/null \
             | jq -r --arg v "$VERSION" '.[$v] // empty')
        printf "    %-40s  %8s   published %s\n" "$pkg@$VERSION" "$size_h" "$ts"
    else
        printf "    %-40s  (not on npm — will skip)\n" "$pkg@$VERSION"
    fi
done

echo ""
echo "    $VERSION was published $HOURS_SINCE hours ago."
echo "    72h unpublish window: OK ($MINUTES_REMAINING minutes remaining)"
echo ""

if [[ -n "$RESET_LOCAL" ]]; then
    echo "    Also resets local state:"
    echo "      - npm/*/package.json → version 0.1.0 + optionalDependencies 0.1.0"
    echo "      - npm/*/bin/ado and bin/ado.exe → removed (will re-download on next publish)"
    echo "      - npm/@gilbertwong1996-ado/scripts/postinstall.js → removed (will re-copy on next publish)"
    echo ""
fi

if [[ -n "$DRY_RUN" ]]; then
    echo "==> Dry run. Nothing changed."
    exit 0
fi

# ── confirm ─────────────────────────────────────────────────────────
if [[ -z "$ASSUME_YES" ]]; then
    read -r -p "Type 'yes' to confirm unpublishing all of the above: " confirm
    if [[ "$confirm" != "yes" ]]; then
        echo "Aborted."
        exit 0
    fi
fi

# ── unpublish ───────────────────────────────────────────────────────
echo ""
for pkg in "${PACKAGES[@]}"; do
    if npm view "$pkg@$VERSION" version >/dev/null 2>&1; then
        echo "==> Unpublishing $pkg@$VERSION..."
        if npm unpublish "$pkg@$VERSION" 2>&1; then
            echo "    OK"
        else
            echo "    FAILED (continuing so we can try the rest)" >&2
        fi
    else
        echo "    $pkg@$VERSION not on npm, skipping"
    fi
done

# ── local cleanup ───────────────────────────────────────────────────
if [[ -z "$RESET_LOCAL" ]]; then
    echo ""
    echo "==> Done. Skipped local state reset (--no-local)."
    echo "    Run \`scripts/npm-unpublish.sh $VERSION\` again without --no-local to clean up,"
    echo "    or restore the local files manually with \`git checkout npm/\`."
    exit 0
fi

echo ""
echo "==> Resetting local state to last known good (v0.1.0)..."

for pkg_json in "$NPM_DIR"/@gilbertwong1996-ado/package.json "$NPM_DIR"/@gilbertwong1996-ado-*/package.json; do
    tmp=$(mktemp)
    jq --arg v "0.1.0" '
        .version = $v
        | if has("optionalDependencies")
              then .optionalDependencies |= with_entries(.value = $v)
              else .
          end
    ' "$pkg_json" > "$tmp"
    mv "$tmp" "$pkg_json"
    pkg_name=$(jq -r '.name' "$pkg_json")
    echo "    $pkg_name: reset version + optionalDependencies to 0.1.0"
done

# Platform binaries are gitignored and large; remove so the next
# publish re-downloads them from the GitHub Release.
removed=0
for bin in "$NPM_DIR"/@gilbertwong1996-ado-*/bin/ado "$NPM_DIR"/@gilbertwong1996-ado-*/bin/ado.exe; do
    if [[ -f "$bin" ]]; then
        rm "$bin"
        removed=$((removed + 1))
    fi
done
echo "    removed $removed local binary file(s)"

# The main package's scripts/ dir holds the postinstall hook that
# the publish script copies in. Wipe it so the next publish
# re-copies cleanly (this is also the bug that bit v0.2.0: the
# script never copied postinstall.js in, so the tarball was
# published without it).
if [[ -d "$NPM_DIR/@gilbertwong1996-ado/scripts" ]]; then
    rm -rf "$NPM_DIR/@gilbertwong1996-ado/scripts"
    echo "    removed npm/@gilbertwong1996-ado/scripts/"
fi

echo ""
echo "==> Done. To republish a clean $VERSION, fix the script first, then:"
echo "    scripts/npm-publish.sh $VERSION"
