#!/usr/bin/env bash
# Sign and notarize the Burrito-generated macOS binary.
#
# Burrito itself does NOT sign or notarize the binary — it produces an
# unsigned executable wrapper. You must sign + notarize post-build using
# Apple's `codesign` and `notarytool`.
#
# USAGE:
#   bin/sign.sh
#
# ENVIRONMENT VARIABLES:
#   MACOS_SIGN_IDENTITY    Developer ID Application identity (e.g. "Developer ID
#                          Application: ACME Inc. (TEAMID1234)")
#                          Find yours with: security find-identity -p codesigning
#
#   Either:
#     MACOS_KEYCHAIN_PROFILE   notarytool keychain profile (set up with
#                              `xcrun notarytool store-credentials`)
#   Or all three of:
#     MACOS_NOTARY_APPLE_ID    Apple ID email
#     MACOS_NOTARY_TEAM_ID     Team ID (10-char alphanumeric)
#     MACOS_NOTARY_PASSWORD    App-specific password
#                              (https://appleid.apple.com → App-Specific Passwords)
#
# EXAMPLES:
#   # CI — sign + notarize
#   MACOS_SIGN_IDENTITY="Developer ID Application: ACME (TEAMID)" \
#   MACOS_KEYCHAIN_PROFILE=notary-profile \
#     just release-macos
#
# REFERENCE:
#   https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution
set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────────

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BURRITO_OUT="$ROOT/burrito_out"
# Use the renamed binary (versioned, e.g. ado-0.1.0-macos-aarch64).
# If only the legacy Burrito name is present (ado_macos), use that as
# a fallback so the script keeps working on older release artifacts.
VERSION=$(grep -E '^\s*version:\s*"' "$ROOT/mix.exs" | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
NEW_BINARY="$BURRITO_OUT/ado-${VERSION}-macos-aarch64"
LEGACY_BINARY="$BURRITO_OUT/ado_macos"
if [[ -f "$NEW_BINARY" ]]; then
  BINARY="$NEW_BINARY"
elif [[ -f "$LEGACY_BINARY" ]]; then
  BINARY="$LEGACY_BINARY"
else
  printf '\033[1;31mxx  macOS binary not found: tried %s and %s\033[0m\n' \
    "$NEW_BINARY" "$LEGACY_BINARY" >&2
  printf '    Run '\''just release'\'' first.\n' >&2
  exit 1
fi

MACOS_SIGN_IDENTITY="${MACOS_SIGN_IDENTITY:-}"
MACOS_KEYCHAIN_PROFILE="${MACOS_KEYCHAIN_PROFILE:-}"
MACOS_NOTARY_APPLE_ID="${MACOS_NOTARY_APPLE_ID:-}"
MACOS_NOTARY_TEAM_ID="${MACOS_NOTARY_TEAM_ID:-}"
MACOS_NOTARY_PASSWORD="${MACOS_NOTARY_PASSWORD:-}"

# ── Helpers ──────────────────────────────────────────────────────────────

log()  { printf '\033[1;34m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m!!  %s\033[0m\n' "$*" >&2; }
err()  { printf '\033[1;31mxx  %s\033[0m\n' "$*" >&2; exit 1; }

# ── Validation ───────────────────────────────────────────────────────────

[[ -f "$BINARY" ]] \
  || err "macOS binary not found: $BINARY (run 'just release' first)"

[[ -n "$MACOS_SIGN_IDENTITY" ]] \
  || err "MACOS_SIGN_IDENTITY is not set. Find yours with: security find-identity -p codesigning"

# ── Sign ────────────────────────────────────────────────────────────────

log "Signing $BINARY with: $MACOS_SIGN_IDENTITY"
codesign \
  --force \
  --options runtime \
  --timestamp \
  --sign "$MACOS_SIGN_IDENTITY" \
  "$BINARY"

log "Verifying signature"
codesign --verify --verbose=2 "$BINARY"

# ── Notarize ─────────────────────────────────────────────────────────────

if [[ -n "$MACOS_KEYCHAIN_PROFILE" ]]; then
  log "Submitting for notarization (keychain profile: $MACOS_KEYCHAIN_PROFILE)"
  xcrun notarytool submit "$BINARY" \
    --keychain-profile "$MACOS_KEYCHAIN_PROFILE" \
    --wait
elif [[ -n "$MACOS_NOTARY_APPLE_ID" && -n "$MACOS_NOTARY_TEAM_ID" && -n "$MACOS_NOTARY_PASSWORD" ]]; then
  log "Submitting for notarization (credentials)"
  xcrun notarytool submit "$BINARY" \
    --apple-id "$MACOS_NOTARY_APPLE_ID" \
    --team-id "$MACOS_NOTARY_TEAM_ID" \
    --password "$MACOS_NOTARY_PASSWORD" \
    --wait
else
  warn "Skipping notarization (no MACOS_KEYCHAIN_PROFILE or credentials set)"
  warn "The binary will run on macOS but Gatekeeper may show a warning."
  exit 0
fi

log "Stapling notarization ticket"
xcrun stapler staple "$BINARY"
xcrun stapler validate "$BINARY"

log "Done."
