# Menu bar shelf

Status: Accepted, 2026-10-31. Amends [ADR 0071](0071-one-close-and-quit-prompt.md)
(when quit happens), [ADR 0041](0041-cases-and-ai-response-restoration.md)
(clipboard actions outside the window), [ADR 0040](0040-in-app-updates-on-macos.md)
(restart to update) and [ADR 0083](0083-fewer-release-candidates.md) (a
feature after rc.1).

## Problem

A prepared document is reached only through mdoc's window: switch to mdoc,
find the tab, Copy for AI, switch back. Anoni keeps a menu bar item, but it
holds one action (anonymize the clipboard). mdoc already knows which
documents are pseudonymized and holds the mappings to restore replies, so a
menu bar item can hand off documents and restore replies without opening the
window.

## Decision

- **Shelf.** An optional menu bar item (macOS first) opens the shelf: the
  list of pseudonymized documents as small previews, plus clipboard actions.
  The rules here are written for both platforms; the Windows
  notification-area flyout is a Later item.
- **Clipboard actions.** **Pseudonymize Clipboard** and **Restore
  Clipboard** (aliases in the clipboard text back to primary wordings, from a
  case) are on the shelf. No file drop or conversion on the shelf: review
  stays in the editor.
- **Current case.** The clipboard actions use the case of the document used
  last (a tab without a case counts through its live mapping), shown on the
  row with a picker: "Restore Clipboard · “Ivanov” ▾". Cases are never mixed,
  since aliases repeat across them. Afterwards the row reads "7 names restored
  · 1 unrecognised"; unrecognised tokens cannot be highlighted in a
  clipboard.
- **Staying in the menu bar.** Settings › General › "Show mdoc in the menu
  bar", with "mdoc keeps running after you close its windows." below. Off by
  default; turning it on is the consent to keep running. ⌘Q or the shelf's
  Quit really quits. With it off, mdoc quits with its last window as today.
- **No window.** With the item on, closing the last window hides it: tabs,
  undo, mappings and unsaved text stay in memory without a prompt, and
  reopening shows the same workspace. While no window shows, mdoc leaves the
  Dock (accessory activation policy) and returns when one opens.
- **Updates.** A downloaded update adds a "Restart to update" row to the
  shelf. It goes through the ordinary quit, prompts included; mdoc never
  restarts unasked.
- **What is listed.** At most 8 documents in one list, most recently edited
  first, each with its case colour, then "Show all in mdoc" (opens Home).
  Open tabs with applied aliases, then recently saved members of cases. A
  member's file is read only while the shelf is open; nothing about it is
  cached on disk. No handoff history: the shelf never stores document text in
  app data.
- **Click.** Brings mdoc forward (opening a window if none shows) on that
  tab in its current view; a closed case member opens as a tab; a document
  that is not ready opens with its first pending proposal selected.
- **Keyboard.** ↑↓ move, Enter opens, ⌘C copies for AI, Esc closes. No
  global shortcuts yet.
- **Icon.** A monochrome template glyph with no badge or count.
- **Ready.** A document is ready when it has at least one applied alias and
  no pending proposals. A document that is not ready is listed with
  "N left to review", has no copy or drag, and a click opens its review.
- **Closed members.** Saving a ready document records a fingerprint of its
  text in the (encrypted) case. A closed member is ready only while its file
  matches that fingerprint; otherwise it reads "Changed since review". As a
  backstop, a closed member whose file contains any of the case's known
  originals reads "N names not replaced". Neither state copies or drags.
- **Empty and locked.** With nothing to list the shelf reads "Pseudonymized
  documents appear here" above the clipboard rows. While cases are locked,
  open tabs still show, closed members are hidden, and a locked case's
  clipboard rows read "Unlock cases in mdoc…", which opens the Keychain
  explainer in a window. The shelf never raises a system dialog itself.
- **Look.** 360 pt wide; as tall as its rows, never past the screen, then
  scrolling. Titles truncate in the middle. It follows mdoc's theme setting
  and the editor's alias chip colours.
- **Accessibility.** A row reads "<title>, <case>, ready" (or "…, 3 left to
  review"); its button reads "Copy for AI". Focus starts on the first row
  with the one focus ring; ⌘C is the keyboard counterpart of a drag.
- **Drag.** Dragging a ready preview carries one pasteboard item with both
  plain text and a `.md` file. The file is a fresh export of the current text
  (not the saved file) in a private temporary folder cleared at quit, and its
  name is the title with known originals pseudonymized.
- **Copy for AI.** The preview's button is Copy for AI, the same text a drag
  carries. Copy Markdown stays in the editor.
- **Preview.** Pseudonymized title, case colour, three lines of
  pseudonymized text with alias chips, last edit time and word count. Never
  originals or source pages: the menu bar shows in screen shares.
- **Rendering.** An `NSStatusItem` opens a borderless GPUI `PopUp` window
  anchored under it, closed by a click outside. Only the status item, the
  anchoring and the text drag are native patches in `vendor/gpui-pre-macos`.

## Invariants

- Nothing leaves through the shelf that is not ready.
- The shelf shows and exports only pseudonymized text; originals appear only
  in the clipboard after Restore Clipboard.
- The shelf writes no document text to app data; its temporary exports live
  only until quit.

## Consequences

- 0.3.0 ships it in rc.2; rc.1 gets an install and smoke check, and the full
  acceptance, with its new section 12, runs on rc.2
  ([ADR 0083](0083-fewer-release-candidates.md)).
- Done when section 12 passes on rc.2, Windows is unchanged (no tray), and a
  test drags and copies ready, not-ready and changed-since-review documents
  and finds no original in any export.
- With the item on, unsaved tabs and mappings stay in memory with no window
  open, and ADR 0071's prompts run at the real quit.
- Later: the Windows flyout, global shortcuts (with the system-wide
  Pseudonymize Clipboard hotkey) and Open at login (`SMAppService`).

References: [interview](../design/menu-bar-grill-2026-10-31.md).
