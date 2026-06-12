# Code Signing (macOS only)

Burrito does **not** sign the macOS binary it produces. Signing and
notarization must be done post-build using Apple's `codesign` and
`notarytool`.

## Prerequisites

1. **Apple Developer Program** membership ($99/year)
2. A **Developer ID Application** certificate in your keychain
   - Create one at <https://developer.apple.com/account/resources/certificates>
   - Verify with: `security find-identity -p codesigning`
3. A **notarytool** keychain profile (recommended) **OR** Apple ID credentials:
   - **Profile (preferred)** — set up once:
     ```sh
     xcrun notarytool store-credentials --apple-id you@example.com \
         --team-id ABCDE12345 --password <app-specific-password>
     ```
   - **Or** set env vars each time (less secure, expires more often)

## Run

```sh
# Build + sign + notarize
export MACOS_SIGN_IDENTITY="Developer ID Application: ACME Inc. (TEAMID)"
export MACOS_KEYCHAIN_PROFILE=ado-notary     # created with store-credentials
just release-macos

# Or with raw credentials (no profile)
export MACOS_NOTARY_APPLE_ID=you@example.com
export MACOS_NOTARY_TEAM_ID=ABCDE12345
export MACOS_NOTARY_PASSWORD=abcd-efgh-ijkl-mnop
just release-macos
```

The script:
1. Runs `codesign --options runtime --timestamp --sign`
2. Verifies with `codesign --verify --verbose=2`
3. Submits to Apple's notary service and waits for approval
4. Stitches the notarization ticket onto the binary with `xcrun stapler staple`

## CI integration

Example GitLab CI job:

```yaml
build:sign:
  stage: release
  script:
    - just release-macos
  artifacts:
    paths:
      - burrito_out/ado_macos
    expire_in: 30 days
```

Store credentials in CI variables (masked, protected):
- `MACOS_SIGN_IDENTITY`
- `MACOS_KEYCHAIN_PROFILE` (recommended) **OR** the three raw notary vars
  (`MACOS_NOTARY_APPLE_ID`, `MACOS_NOTARY_TEAM_ID`, `MACOS_NOTARY_PASSWORD`)

## Verifying signatures

```sh
codesign --verify --verbose=2 burrito_out/ado_macos
codesign -d --entitlements - burrito_out/ado_macos
spctl --assess --verbose burrito_out/ado_macos
```

## Reference

- <https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution>
- <https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution#Use-the-xcrun-notarytool-command>
