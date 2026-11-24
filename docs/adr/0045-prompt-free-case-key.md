# Prompt-free Vault key on macOS

Status: Accepted, 2026-10-14.

## Why

A login-keychain item trusts only the code signature that created it, so a
key one build made prompts every build signed differently, and the prompts
look suspicious in a privacy app. Builds signed with a Developer ID
provisioning profile granting `keychain-access-groups` keep the key in the
data protection keychain, where access is by group, so it never asks and
survives updates. Each build decides its identity once at runtime; a dev
build keeps its own key and Vault, so it can never create a key that later
prompts the release. Launch reads the key with dialogs off, and a protected
build migrates the login-keychain key on the next Unlock without changing
it. CI can't see system dialogs, so a release that touches the key is
checked by hand on a clean user account.

## Rejected

- **A plaintext key file**, even for dev builds or as an opt-in default.
  The one exception is a debug build's `MDOC_DEV_PROFILE` scratch folder
  (#1153), which release builds ignore.
- **The per-query `kSecUseAuthenticationUIFail` flag**: it doesn't prevent
  the dialog; only the process-wide switch does.

## Revisit when

Apple changes how Developer ID apps get the data protection keychain.
