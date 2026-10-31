# Keyboard shortcuts panel

Status: Accepted, 2026-10-28.

## Problem

mdoc binds about 150 keys in seven `bind_keys` functions, and the command
palette showed shortcuts typed in by hand. The two lists could drift apart,
and no screen listed what the keys are. Anoni 0.86.1 shows every shortcut by
context.

## Decision

- **One table.** `src/shortcuts.rs` holds every user-facing binding as a row:
  label, context group (Everywhere, Editor, Popup, Panel) and, per binding,
  the action, its key context, the keys per platform (`mod-` is ⌘ or Ctrl)
  and the module that registers it. Each `bind_keys` registers its rows in
  its usual place, since GPUI breaks a tie between equally deep bindings by
  registration order. The `mdoc_editor` and `gpui_pdf` crates keep their own
  keymaps for their tests and other hosts; the app registers their table rows
  from the table and the rest as plumbing.
- **Plumbing stays out.** Tab inside a control group, moving inside menus,
  choosers and the palette's list, caret and deletion basics in the editor
  and text fields, the editor's own Esc and the OS character palette stay in
  their modules and are not listed.
- **Keys are bindings.** A user-facing key is a bound action, not raw key
  handling, so the table can list it: the source preview's Page Up / Down,
  Home, End and Go to Page, and the confirm dialog's Don't Save (⌘D / Alt+D).
  Enter and Esc inside a dialog stay its own.
- **Shown keys.** A row shows the current platform's keys only. Alternates
  that are bound but not shown (⌘+ for Zoom in, the editor's Ctrl keys on
  macOS) stay bound; a row with no key on this platform is not shown.
- **Palette.** Its shortcut labels, the sidebar's ⌘K and Home hints and
  Home's Open hint come from the table.
- **Panel.** A read-only modal sheet, like the confirm dialog
  ([ADR 0064](0064-dialog-and-role-semantics.md)): a filter field, then the
  rows grouped by context. The filter matches every word against the label,
  the group or a key. The list is a Tab stop that scrolls with the arrows,
  Page Up / Down, Home and End; Tab stays inside, F6 does nothing, Esc or a
  click outside closes it, and focus goes back where it was. A command that
  moves focus (⌘N, Ctrl+Tab, Find) closes it and runs; the palette, Settings
  and Open Case do nothing while it is open. Over Settings, its document
  command restrictions still apply. Switching apps keeps it open.
- **Opening it.** `?` when no text field has focus (Home, sidebar, toolbar,
  panels); ⌘/ (Ctrl+/) everywhere, which also closes it; the palette action
  "Keyboard shortcuts"; Help → Keyboard Shortcuts on macOS. ⌘/ and Help also
  work from a model comparison window. In the editor and text fields `?`
  types a question mark.

## Invariants

- A test fails if a bound action is neither in the table nor named plumbing,
  if a table row is not registered, or if a crate's own keymap and the table
  disagree on either platform (`src/shortcuts/tests.rs`).
- A test fails if the palette shows a key the table does not list, or if the
  sheet does not draw every row (`src/workspace/tests/shortcuts_sheet.rs`).

## Consequences

- A new user-facing key is a table row; the sheet, the palette and the
  invariant follow from it.
- The headless renders include the sheet in both themes.
- The acceptance checklist checks `?` and ⌘/ / Ctrl+/ (H23).
