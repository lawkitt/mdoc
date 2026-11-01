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

- **Shelf.** A menu bar item (macOS first), on by default, opens the shelf:
  the list of pseudonymized documents as small previews, plus clipboard
  actions. The rules here are written for both platforms; the Windows
  notification-area flyout is a Later item.
- **Staying in the menu bar.** One switch, Settings › General › "Show mdoc
  in the menu bar", with "mdoc keeps running after you close its windows."
  below. On by default for everyone who has not turned it off. ⌘Q or the
  shelf's Quit really quits. With it off, mdoc quits with its last window as
  it did before.
- **First-close note.** The first time the last window closes with the item
  on, a note anchored under the icon reads "mdoc is still in the menu bar.
  Your documents are here." with **Got it** and **Quit when windows close**
  (turns the switch off). It never shows again. The default, the note and
  the switch together are the consent to keep running.
- **No window.** With the item on, closing the last window hides it: tabs,
  undo, mappings and unsaved text stay in memory without a prompt, and
  reopening shows the same workspace. While no window shows, mdoc leaves the
  Dock (accessory activation policy) and returns when one opens.
- **Updates.** A downloaded update adds a "Restart to update" row to the
  shelf. It goes through the ordinary quit, prompts included; mdoc never
  restarts unasked.
- **What is listed.** At most 8 documents in one list, most recently edited
  first. Open tabs with applied aliases, then recently saved members of
  cases. A member's file is read only while the shelf is open; nothing about
  it is cached on disk. No handoff history: the shelf never stores document
  text in app data. No search field.
- **Ready.** A document is ready when it has at least one applied alias and
  no pending proposals. A document that is not ready is listed with
  "N left to review", has no copy or drag, and a click opens its review.
- **Closed members.** Saving a ready document records a fingerprint of its
  text in the (encrypted) case. A closed member is ready only while its file
  matches that fingerprint; otherwise it reads "Changed since review". As a
  backstop, a closed member whose file contains any of the case's known
  originals reads "N names not replaced". Neither state copies or drags.
- **Row.** The title without `.md`, cut in the middle; then the case as a
  small label tinted in the case colour (nothing without a case); readiness
  states use the same label style in amber. Then the preview, then a muted
  line: "5 min ago · 1,297 words · 24 replaced" (a date once older than
  yesterday). The count is of mentions, the same number as the editor's
  Share tab; hovering it lists the distinct names those mentions are, by
  category ("14 names: 6 people · 4 organizations · 4 IDs").
- **Preview.** The first real prose of the pseudonymized text: lines with
  fewer than about 3 words or 12 letters (page numbers, rules, "Page 1 of
  5") are skipped, and Markdown syntax is hidden through `mdoc-markdown`
  (bold stays bold, `**` never shows). One wrapped paragraph of at most two
  lines, ending in "…", with alias chips. Never originals or source pages:
  the menu bar shows in screen shares.
- **Copy for AI.** A small outlined secondary button on every ready row,
  always visible. A click (or ⌘C, or ⌘1–⌘8 for rows 1–8) copies, shows ✓
  "Copied", and closes the shelf after about 0.8 s. It copies the same text a
  drag carries; Copy Markdown stays in the editor.
- **Drag.** Dragging a ready preview carries one pasteboard item with both
  plain text and a `.md` file. The file is a fresh export of the current text
  (not the saved file) in a private temporary folder cleared at quit, and its
  name is the title with known originals pseudonymized. The cursor is an open
  hand over a preview; the drag image is a small card with the title and
  first line. After the first successful drag, a one-time hint "Drag a
  preview into any app" is gone for good.
- **Click.** Brings mdoc forward (opening a window if none shows) on that
  tab in its current view; a closed case member opens as a tab; a document
  that is not ready opens with its first pending proposal selected.
- **Clipboard section.** A "Clipboard" heading over two equal buttons,
  **Pseudonymize** and **Restore**, then one quiet source line: "Names from
  Ivanov ▾" (the case of the document used last; a tab without a case counts
  through its live mapping, named without `.md`). The ▾ picks another source.
  Cases are never mixed, since aliases repeat across them. After a run the
  source line shows the result for a few seconds ("7 names restored ·
  1 unrecognized"); unrecognized tokens cannot be highlighted in a
  clipboard. No Paste AI reply row: Restore covers it.
- **Footer.** One row: **Open mdoc** (opens Home) at the left, a Settings
  gear and **Quit** at the right.
- **Keyboard.** ↑↓ move, Enter opens, ⌘C copies for AI, ⌘1–⌘8 copy a row,
  Tab reaches the other controls, Esc closes. No global shortcuts yet.
- **Icon.** A monochrome template glyph with no badge or count: the app
  icon's M monogram alone (`build/menubar.svg`), rendered by `just icon`.
- **Empty and locked.** With nothing to list the shelf reads "Pseudonymized
  documents appear here" above the clipboard section. While cases are
  locked, open tabs still show, closed members are hidden, and a locked
  case's clipboard buttons read "Unlock cases in mdoc…", which opens the
  Keychain explainer in a window. The shelf never raises a system dialog
  itself.
- **Look.** 360 pt wide; as tall as its rows, never past the screen, then
  scrolling. A native blurred (vibrancy) background like macOS menus, with
  the system's window corner radius and shadow. Title 13 pt semibold, preview
  12 pt, meta 11 pt muted. Alias chips in the editor's colours with a little
  more padding. It follows mdoc's theme setting.
- **Accessibility.** A row reads "<title>, <case>, ready" (or "…, 3 left to
  review"); its button reads "Copy for AI". Focus starts on the first row
  with the one focus ring; ⌘C is the keyboard counterpart of a drag.
- **Rendering.** An `NSStatusItem` opens a borderless GPUI `PopUp` window
  anchored under it, closed by a click outside. The native patches in
  `vendor/gpui-pre-macos`, each listed in its `MDOC-PATCH.md`, are the status
  item, the anchoring, the text drag (with its card image and whether the
  drop landed), hiding and showing a window (so its tabs and undo survive),
  resizing the shelf to its content with its top edge kept, and the
  activation policy (Regular / Accessory).
  The app crate takes no Objective-C dependency.

## Not adopted

A second switch for keeping mdoc running; previews chosen for their aliases
(they read as chips); a search field; a Paste AI reply row; a progress bar
for not-ready documents (the scan gives no such precision); a source format
tag beside the title.

## Invariants

- Nothing leaves through the shelf that is not ready.
- The shelf shows and exports only pseudonymized text; originals appear only
  in the clipboard after Restore Clipboard.
- The shelf writes no document text to app data; its temporary exports live
  only until quit.

## Consequences

- 0.3.0 ships it, polish included, in rc.2; rc.1 gets an install and smoke
  check, and the full acceptance, with its section 12, runs on rc.2
  ([ADR 0083](0083-fewer-release-candidates.md)).
- Done when section 12 passes on rc.2, Windows is unchanged (no tray), and a
  test drags and copies ready, not-ready and changed-since-review documents
  and finds no original in any export.
- By default, closing the last window no longer quits on macOS; unsaved tabs
  and mappings stay in memory with no window open, and ADR 0071's prompts
  run at the real quit.
- Later: the Windows flyout, global shortcuts (with the system-wide
  Pseudonymize Clipboard hotkey) and Open at login (`SMAppService`).

References: [interview](../design/menu-bar-grill-2026-10-31.md).
