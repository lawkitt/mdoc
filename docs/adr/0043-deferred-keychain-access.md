# Deferred Keychain access with an explainer

Status: Accepted, 2026-10-13. Amends the storage section of
[ADR 0041](0041-cases-and-ai-response-restoration.md). Amended by
[ADR 0045](0045-prompt-free-case-key.md) (what counts as "may ask"; data
protection keychain) and on 2026-10-18 (decision 5: the preview is a captioned
figure below the text).

## Problem

Users saw one or two unexplained macOS password dialogs ("mdoc wants to use
your confidential information stored in “io.github.lawkitt.mdoc”…"), which look
suspicious in a privacy app. Any document load at launch read the case key once
`mdoc/cases/` existed, and a login-keychain item trusts only the code signature
that created it, so ad-hoc and dev builds prompt each other.

## Decision

1. **Touch the key only for an action that needs it**: Save mapping as case
   (including the save-on-close question), folder drop as a case, File → Open
   Case…, and Unlock. Moving a tab into a case needs a loaded case, so the key
   is already open. Sidebar case names come from the session manifest.
2. **Ask the Keychain whether it may ask.** The key is read with user
   interaction disabled (`SecKeychainSetUserInteractionAllowed`). Success, or a
   new key created without a dialog, opens the store silently. Any quiet refusal
   means "may ask" (ADR 0045). Windows and Linux never see the explainer.
3. **Nothing asks at launch.** Launch reads the key only when it will not ask.
   Otherwise cases stay locked: members keep their case link, and a bar says
   "Saved cases are locked. Unlock them to restore replacements in their
   documents." with **Unlock…**. The bar shows whenever cases are locked,
   because a document opened by hand is matched to its case by path.
4. **Explainer before the system dialog** (`src/workspace_keychain.rs`), in the
   standard dialog style (ADR 0015). Title "mdoc will ask for Keychain access":
   saved cases are encrypted with a key in the Mac's Keychain; macOS will ask for
   the login password, which mdoc never sees; choose **Always Allow**.
   **Continue** (focused, Enter) proceeds; **Not now** (Esc) cancels the action.
   Tab stays inside the dialog; window shortcuts and clicks behind it are
   stopped; closing returns focus.
5. **Animated preview** of the dialog, drawn as GPUI vectors: padlock with the
   mdoc badge, the real wording, the password field filling, a pointer pressing
   **Always Allow**. Loops every 4.2 s at up to 30 fps; under Reduce Motion a
   single frame (password typed, Always Allow pressed); follows system
   light/dark. It sits **below the text**, captioned "After Continue, macOS
   shows this:", flat (no window shadow), and is announced as an image
   ("Preview of the macOS Keychain dialog"). At the top of the card, uncaptioned,
   the 2026-10-18 dry run on beta.13 took it for the real dialog showing before
   Continue: no SecurityAgent window existed until Continue, and a traced build
   made only quiet reads (`errSecAuthFailed`, no dialog).
6. **Denial** (`errSecUserCanceled`, `errSecAuthFailed` after a real dialog)
   keeps members locked and keeps the requested action; the bar offers
   **Try again…**, which finishes it once the key opens. A failed read never
   drops members' case links.
7. **Waiting actions queue**: another gated action while one waits runs after
   it; a repeated Save as case for a saved tab does nothing.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0043-deferred-keychain-access.md).
