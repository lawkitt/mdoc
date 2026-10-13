# Deferred Keychain access with an explainer

Status: Accepted, 2026-10-13. Amends the storage section of
[ADR 0041](0041-cases-and-ai-response-restoration.md).

## Problem

Users see one or two macOS password dialogs ("mdoc wants to use your
confidential information stored in “io.github.lawkitt.mdoc”…", then "mdoc wants
to access key…") with no context. They look suspicious in an app whose pitch is
privacy. Two causes:

- **Timing.** Once `mdoc/cases/` exists, *any* document load at launch reads the
  case key (`attach_case` → `load_cases` → `case_store`), even when no open tab
  belongs to a case.
- **Signature.** A login-keychain item's access list trusts the code signature
  of the app that created it. Developer ID builds share a stable designated
  requirement; ad-hoc builds (dev, unsigned test builds) do not, so an item
  created or touched by one prompts every other build. Likely, but not yet
  reproduced on a clean account (see Verification).

## Decision

1. **Touch the key only for an action that needs it**: Save mapping as case,
   Move to case, File → Open Case…, and restoring a tab that is a case member.
   Never at launch otherwise. Sidebar case names come from the session manifest,
   which needs no key.
2. **Explainer before the system dialog**, shown before the first key access
   and again only after the user denied or cancelled the macOS dialog (not
   before every access). A setting remembers the first successful access.
   - Modal in the standard dialog style (ADR 0015). Title: "mdoc will ask for
     Keychain access". Lines: saved cases are encrypted and the key lives in
     your Mac's Keychain; macOS will ask for your Mac login password, which mdoc
     never sees; choose **Always Allow** so it isn't asked again.
   - **Continue** (default) proceeds to the key access. **Not now** cancels the
     action that needed it.
3. **Animated preview** of the upcoming dialog, drawn in GPUI as vectors, not a
   screenshot: the padlock with the mdoc badge, the real wording with
   `io.github.lawkitt.mdoc`, the password field filling with dots, a pointer
   moving to **Always Allow** which highlights on press. Only the
   "confidential information" dialog is shown. Loops about every 4 s; settles
   on a still frame under Reduce Motion; follows system light/dark dialog
   colours.
4. **Nothing asks at launch.** Restored case members open *locked*: no case
   mapping yet, a notice "Case ‘X’ is locked" with **Unlock**, and a dimmed
   case dot. Unlock (or any case action above) is what shows the explainer.
5. **Denial** (user cancelled / auth failed) shows a notice "Saved cases need
   Keychain access" with **Try again**, which shows the explainer again.

## Later

Move the key to the data protection keychain (`kSecUseDataProtectionKeychain`),
which never asks for the login password and survives updates. It needs a
Developer ID provisioning profile with `keychain-access-groups`, so it changes
signing (ADR 0039) and gets its own ADR. Ad-hoc builds cannot use it.

## Verification

Before shipping, on a clean macOS user account: install the signed release,
save a case, relaunch, update in-app, and count prompts. Expected: one prompt
at most, none after **Always Allow**. If signed builds prompt again, the cause
is not only the signature and the "Later" step moves forward.

## As implemented

- **"May ask"** (`cases::Store::may_ask`, macOS only): the Keychain item
  exists and this installation has not read it yet (no `.keychain-allowed`
  marker in the cases folder). The lookup reads attributes only, which never
  shows a dialog. A brand-new key is created without a dialog, so first-time
  users never see the explainer; Windows and Linux never do.
- **Launch** reads the key only when it may not ask. While it may, members
  restored at launch stay locked. Their case link is kept, and a bar says
  "Some documents belong to saved cases. Unlock them to restore their
  replacements." with **Unlock…**. The session manifest has no case names, so
  the bar does not name the case and there is no dimmed dot.
- **Gated actions:** Save mapping as case (including the save-on-close
  question), folder drop as a case, and File → Open Case…. Moving a tab into a
  case needs a loaded case, so the key is already open.
- **Denial** (`errSecUserCanceled`, `errSecAuthFailed`) removes the marker,
  keeps members locked, and the bar offers **Try again…**. Before this change,
  a failed key read at launch marked cases as loaded and dropped members'
  case links ("could not be found"); now they stay linked.
- **Explainer:** `src/workspace_keychain.rs`. Esc means Not now; Continue has
  focus, so Enter continues. The preview loops every 4.2 s at up to 30 fps and,
  under Reduce Motion, shows a single frame: password typed, Always Allow
  pressed.
