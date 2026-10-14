# Prompt-free case key on macOS

Status: Accepted, 2026-10-14. Amends
[ADR 0043](0043-deferred-keychain-access.md).

## Problem

beta.7 still shows two macOS dialogs at launch, before the user clicks
anything: "mdoc wants to use your confidential information stored in
“io.github.lawkitt.mdoc”…" and then "mdoc wants to access key
“io.github.lawkitt.mdoc”…".

Seen on the owner's Mac: Developer ID beta.7, an existing `cases/` folder, and
a `case-key` login-keychain item created 2026-10-11 by an earlier build with a
different signature, so its access list does not trust the release.

**Cause (reproduced).** The quiet read (`SecKeychainSetUserInteractionAllowed(false)`)
does stay silent. For an item whose access list does not trust the app, though,
it returns `errSecAuthFailed` (-25293), not `errSecInteractionNotAllowed`
(-25308). `load` mapped -25293 to "user denied", so `key_may_ask` answered
"will not ask" (`Err(_) => false`). `load_cases` then fell through to
`case_store()`, a read with dialogs allowed, which produced both dialogs. A
probe binary showed that `kSecUseAuthenticationUIFail` on the query does
**not** prevent the dialog: it blocked on it.

A new user without a `cases/` folder sees no prompt at launch. Only users with a
key made under another signature are affected.

## Decision

1. **Quiet means quiet (beta.8).** With dialogs off, -25308, -25293 and -128
   all mean "may ask", never "denied". Launch never falls back to a read with
   dialogs allowed: any quiet error leaves cases locked, and an unsure answer
   shows the explainer before a gated action. ADR 0043's process-wide switch
   stays; the per-query UI flag does not work. On Windows and Linux the quiet
   read is the real one, so its errors are reported as before, with no
   Keychain explainer.
2. **Keep the launch probe.** Cases that open without a dialog unlock on their own.
3. **Data protection keychain** (`kSecUseDataProtectionKeychain`, the "Later"
   step of ADR 0043) for builds signed with a Developer ID provisioning
   profile that grants `keychain-access-groups` (`F33DLM8K53.io.github.lawkitt.mdoc`).
   Access is by access group, not a per-item list, so it never asks and
   survives updates.
4. **Migration on the next Unlock.** In a profile build with no protected key,
   the login-keychain key is read quietly. If that would ask, cases stay locked;
   **Unlock…** shows the ADR 0043 explainer, and after that one dialog the key
   is copied to the data protection keychain. The login item is then deleted
   with dialogs off. If macOS would ask even to delete it, it stays (the same
   key, still protected) and is never read again. No case data is lost.
5. **Build identity, decided once at runtime** (`key::keychain()`):
   - signed by team F33DLM8K53 and has `Contents/embedded.provisionprofile` →
     **Protected** (data protection keychain);
   - signed by the team but has no profile → **Release** (login keychain,
     service `io.github.lawkitt.mdoc`): beta.8 and every build until the
     profile exists;
   - anything else (unsigned, ad-hoc, debug, another team) → **Dev**: login
     keychain under `io.github.lawkitt.mdoc.dev` and cases in
     `mdoc/cases-dev/`. A dev build can no longer create a key that later
     prompts the release, which is how the 2026-10-11 key came about.
   A Protected build whose profile lacks the entitlement
   (`errSecMissingEntitlement`) behaves as Release. Signing refuses a
   profile that does not allow the access group (exactly, or `F33DLM8K53.*`).
6. **Only cases are kept apart for dev builds.** Settings, the session and the
   large downloaded models stay shared. A dev build that restores a session
   with case members finds no such case in `cases-dev/` and treats them as
   missing, not locked. No plaintext key file, not even for dev.
7. **Rollout.** Fix 1 ships in beta.8. Signing embeds the profile only when
   `APPLE_PROVISIONING_PROFILE` is set (CI: secret
   `APPLE_PROVISIONING_PROFILE_BASE64`) and then adds
   `com.apple.application-identifier`, `com.apple.developer.team-identifier`
   and `keychain-access-groups`. Without it, nothing changes. The owner
   registers the App ID with Keychain Sharing and creates the profile;
   Protected turns on with the first release signed with it.

## Verification

`scripts/keychain-launch-check.sh`, run on a clean macOS user account with the
signed release installed, before every release that touches the case key. It
seeds a login-keychain `case-key` with `security add-generic-password` (its
access list trusts only `/usr/bin/security`, like a key from another build) and
a `cases/` folder, launches the app, and checks for no SecurityAgent dialog and
the locked bar. It removes the seeded key and folder afterwards. CI cannot see
system dialogs. With a profile build, also check: Unlock → one dialog →
relaunch → no locked bar.

## Amended 2026-10-14

A plaintext key file beside the cases, as Anoni does, was considered as the
default with the Keychain as an opt-in, and rejected: it avoids prompts only by
giving up the protection. The data protection keychain (decision 3) is a
release blocker for beta.9; the owner's Apple setup is listed in the
[Anoni comparison](../design/anoni-comparison.md#owner-checklist-data-protection-keychain).
An optional case passphrase and case sharing stay deferred (ROADMAP).
