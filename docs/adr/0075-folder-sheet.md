# Folder sheet

Status: Accepted, 2026-10-28. Amends
[ADR 0041](0041-cases-and-ai-response-restoration.md) (folder drop) and
[ADR 0028](0028-empty-page-and-file-drop.md) (folder drops open as a case).

## Problem

A dropped folder opened every supported file at once, README.md included, with
no limit, and asked for the Keychain before anything was chosen (2026-10-19
rc.1 run). A large folder floods the sidebar.

## Decision

- **Folder sheet.** A dropped folder shows a modal sheet before anything
  opens: the folder name, its supported top-level files sorted by name, each
  with a checkbox, all ticked, with no name rules, and "N subfolders, M hidden
  files skipped". Subfolders stay skipped. An empty folder posts the notice it
  does today and shows no sheet.
- **Buttons.** Primary **Open as case**, secondary **Open files**, and Cancel.
  If a loaded case already holds files from this folder, or the folder is
  dropped on a case header, the primary is **Add to “X”**. Locked cases are not
  checked, so a new case is offered.
- **Cap: 20 files per folder.** Only the first 20 by name start ticked. More
  ticks are refused with "Up to 20 files at a time". Files that are already
  open show "Already open" or "In “Y”", stay ticked and don't count toward the
  cap. They join the case only if they have no case yet; a tab moves between
  cases only through Move to case.
- **Key only for a case.** **Open files** never reads the case key. **Open as
  case** and **Add to “X”** read it after the click: if reading might ask, the
  [ADR 0043](0043-deferred-keychain-access.md) explainer shows first.
  Cancelling it returns to the sheet. If the key can't be had, the files open
  without a case and a notice says so.
- **Several folders in one drop.** Plain files in the drop open at once. Each
  folder gets its own sheet, in turn.
- **Keys.** ↑/↓ move between rows, Space toggles, Enter is the primary
  button, Esc cancels. Rows are checkboxes and the sheet is a dialog
  ([ADR 0064](0064-dialog-and-role-semantics.md)).
- Loose files (dropped or ⌘O) are not capped.

## Invariants

- No case-key read happens before the user picks a case action.
- One folder drop opens at most 20 new tabs.

## Consequences

A folder drop costs one extra click. A folder of more than 20 files takes
several drops, or opening the files directly.
