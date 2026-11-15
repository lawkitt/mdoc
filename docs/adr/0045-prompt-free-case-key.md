# Prompt-free case key on macOS

Status: Accepted, 2026-10-14.

## Problem

A login-keychain item trusts only the code signature that created it, so a key
made by one build prompts every build signed differently, and the prompts
look suspicious in a privacy app.

## Decision

- **Data protection keychain** (`kSecUseDataProtectionKeychain`) for builds
  signed with a Developer ID provisioning profile that grants
  `keychain-access-groups` (`F33DLM8K53.io.github.lawkitt.mdoc`). Access is by
  access group, not a per-item list, so it never asks and survives updates.
- **Build identity, decided once at runtime** (`key::keychain()`): signed by
  team F33DLM8K53 with `Contents/embedded.provisionprofile` is Protected;
  signed by the team without a profile is Release (login keychain, service
  `io.github.lawkitt.mdoc`); anything else is Dev (login keychain under
  `io.github.lawkitt.mdoc.dev`, cases in `mdoc/cases-dev/`). A Protected
  build whose profile lacks the entitlement behaves as Release.
- **Quiet means quiet.** The key is read with dialogs off
  (`SecKeychainSetUserInteractionAllowed(false)`); -25308, -25293 and -128
  mean "may ask", never "denied". The per-query UI flag does not prevent the
  dialog.
- **Migration on Unlock.** A Protected build with no protected key copies the
  login-keychain key after one dialog, then deletes the login item with
  dialogs off, or leaves it unread if deleting would ask.
- **Signing** embeds the profile only when `APPLE_PROVISIONING_PROFILE` is set,
  adding the application-identifier, team-identifier and access-group
  entitlements; it refuses a profile that doesn't allow the access group.

## Invariants

- Launch never reads the key with dialogs allowed.
- A dev build can never create a key that later prompts the release.
- No plaintext key file, not even for dev or as an opt-in default.
- No case data is lost by migration.

## Verification

`scripts/keychain-launch-check.sh` runs on a clean macOS user account with the
signed release before every release that touches the case key. CI cannot see
system dialogs.

References: [owner checklist for the data protection keychain](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/anoni-comparison.md#owner-checklist-data-protection-keychain).
