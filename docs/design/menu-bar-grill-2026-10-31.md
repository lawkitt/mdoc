# Grill: menu bar shelf of pseudonymized documents (2026-10-31)

Interview record for a new feature: a macOS menu bar item, like Anoni's, but
listing pseudonymized documents as small previews that open in mdoc on click,
drag out to any app, and copy with a button. ADRs written from it hold the
rules; this file keeps the reasoning and the evidence.

## The idea as asked

- Anoni's menu bar item (screenshot, 2026-10-31) is a plain menu:
  "Anonymiser le presse-papiers ⇧⌘A", "Ouvrir Anoni", "Quitter Anoni".
- mdoc's version: a list of pseudonymized documents as mini previews.
  Click a preview → the document opens in mdoc. Drag a preview → drop it
  anywhere (a chat box, Finder, Mail). A copy button beside each preview →
  copy as Markdown without dragging.

## State at the start

- **Wording.** mdoc never says "anonymize" (ADR 0022); the result is
  *pseudonymized* and stays personal data (ADR 0041, Consequences).
- **Lifecycle.** mdoc quits when its last window closes (`src/main.rs`,
  `on_window_closed`), and ADR 0071 hangs the quit prompts on that. A menu
  bar item that should work with no window open needs mdoc to keep running.
- **What exists to list.** Open tabs hold live text and mappings. Recent
  files are paths only, never content (glossary). A case knows its member
  paths, and the case page never reads member files or the key (ADR 0077).
  No app data holds pseudonymized text today; ADR 0041 forbids originals in
  plaintext app data.
- **Saved files.** Apply edits the Markdown, so a saved `.md` holds aliases
  where replacements were applied, and originals wherever a proposal is still
  pending or a name was missed.
- **Copy actions.** Copy Markdown copies the source exactly, pending
  proposals (originals) included. Copy for AI adds the placeholder note and is
  the primary action once aliases are applied.
- **Pseudonymize Clipboard** (⌥⇧⌘C, mdoc focused) swaps known originals of
  the active case for aliases; a system-wide hotkey is a Later item
  (`global-hotkey` crate, as in Panoply).
- **GPUI.** No status item or tray API in gpui-pre 0.3.2 or the vendored
  platform crates. `WindowKind::PopUp` and `AnchoredPopup` exist. The vendored
  macOS platform can start an outbound drag, but only of files on disk
  (`ExternalDragPayload::Files`); mdoc does not use it yet. A text drag would
  need a patch in `vendor/gpui-pre-macos`.
- **Platforms.** Apple Silicon macOS and Windows x64 ship; Windows has a
  notification-area icon with a flyout as its counterpart.

## Rounds

### Round 1 (asked 2026-10-31)

All ten recommendations accepted except the milestone. Recorded in
[ADR 0084](../adr/0084-menu-bar-shelf.md).

1. **Purpose:** shelf plus Pseudonymize Clipboard and Restore Clipboard. No
   drop zone: review belongs in the editor.
2. **Lifecycle:** an optional item, off by default; turning it on keeps mdoc
   running after its last window closes.
3. **List:** open tabs with applied aliases, then recently saved case
   members, read only while the shelf is open. No handoff history: it would
   keep missed names in plaintext app data, the reason version history is
   Not planned.
4. **Readiness:** not-ready documents listed with "N left to review", no
   copy or drag, click opens the review.
5. **Drag:** text and a `.md` file on one item; the file is a fresh export
   with a pseudonymized name ("Ivanov settlement.md" would leak).
6. **Copy:** Copy for AI, same text as a drag.
7. **Preview:** pseudonymized title, case colour, three lines with alias
   chips, edit time, word count; never originals (screen shares).
8. **Platforms:** macOS first; Windows flyout in Later; rules written for
   both.
9. **Rendering:** `NSStatusItem` plus a GPUI `PopUp` window.
10. **Milestone:** the user chose **0.3.0-rc.2**, not 0.4.0. ADR 0083 allows
    another RC only for must-fix fixes to the installer, signing, updates or
    the keychain; a feature after rc.1 is an exception, taken up in round 2.

### Round 2 (asked 2026-10-31)

All ten recommendations accepted; ADR 0084 updated, ADR 0083 amended.

11. **rc.2:** skip rc.1's full acceptance; rc.2 gets the one full pass and
    0.3.0 stable waits for the shelf. ADR 0083 gains: a feature after rc.1
    makes a new RC, and acceptance runs only on the last one.
12. **List:** one list, most recent first, case colour dots, at most 8, then
    "Show all in mdoc".
13. **Click:** bring mdoc forward on that tab in its view; closed members
    open as tabs; not-ready opens at the first pending proposal.
14. **Keyboard:** ↑↓, Enter, ⌘C, Esc inside the shelf; global shortcuts stay
    in Later with the system-wide clipboard hotkey.
15. **Setting:** General › "Show mdoc in the menu bar", off by default.
16. **Icon:** monochrome template glyph, no badge.
17. **Clipboard case:** the current case (of the document used last), with a
    picker on the row; never mixed. Result summary on the row.
18. **No window:** closing the last window hides it with no prompt; mdoc
    leaves the Dock until a window shows.
19. **Updates:** "Restart to update" row; ordinary quit; never unasked.
20. **Open at login:** Later.

### Round 3 (asked 2026-10-31)

All seven recommendations accepted; ADR 0084 accepted. The frontier is
empty.

21. **Closed members:** pending proposals are memory-only, so a saved file
    cannot say whether it was fully reviewed. The case records a fingerprint
    of the text saved while ready; a closed member is ready only while it
    matches ("Changed since review" otherwise). Backstop: any known original
    in the file reads "N names not replaced".
22. **Empty and locked:** "Pseudonymized documents appear here"; locked
    cases hide closed members and offer "Unlock cases in mdoc…"; no system
    dialog from the shelf.
23. **Look:** 360 pt, scrolls past the screen, middle truncation, mdoc's
    theme.
24. **Accessibility:** row and button labels, focus on the first row, ⌘C as
    the drag's counterpart; full focus tracking stays in Later.
25. **Acceptance:** new section 12 (M1–M10) in `docs/platform-acceptance.md`.
26. **Roadmap:** 0.3.0 item with the Done-when line; status line says rc.1
    is out and rc.2 follows the shelf.
27. **rc.1:** its release run (started 2026-10-31) publishes as a
    pre-release with an install and smoke check only.
