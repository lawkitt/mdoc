# Standard macOS menus

Status: Accepted, 2026-10-18.

## Problem

GPUI makes the first menu the application menu. mdoc set one menu named
"File", so macOS showed it as "mdoc": no File, Edit or Window menu, no About,
Hide (⌘H) or Services, and no menu-bar Undo, Copy or Paste. Native Open and
Save panels had no ⌘C/⌘V, since those come from the Edit menu. Found by the
2026-10-18 platform-acceptance dry run ([ADR 0057](0057-native-platform-acceptance.md)).

## Decision

- **Menus** (`src/menus.rs`): *mdoc* (About mdoc, Settings…, Services, Hide
  mdoc ⌘H, Hide Others ⌥⌘H, Show All, Quit mdoc), *File* (New, Open…, Open
  Case…, Convert to Markdown, Save, Save As…, Copy Markdown, Copy for AI, Paste
  AI Response, Close Tab), *Edit* (Undo, Redo, Cut, Copy, Paste, Select All,
  Find…, Find Next, Find Previous, Pseudonymize, Pseudonymize Clipboard),
  *View* (Toggle Sidebar ⌃⌘S, Before / After, Zoom In, Zoom Out, Actual Size) and
  *Window* (Minimize ⌘M, Zoom, Next Tab, Previous Tab) and *Help* (Keyboard
  Shortcuts, [ADR 0073](0073-keyboard-shortcuts-panel.md)). Hide, Services and
  their shortcuts, and ⌃⌘S, are macOS only.
- **Shortcuts come from the keymap.** Menu items show the binding GPUI finds
  for their action; no shortcut is defined twice.
- **Edit uses the editor's actions with OS actions** (`cut:`, `copy:`,
  `paste:`, `selectAll:`), so they reach native panels. Text fields
  (`SearchInput`) handle the editor's Cut, Copy, Paste and Select All too, so
  the Edit menu acts on whichever has focus.
- **About mdoc** opens Settings at Updates, which shows the version.
- The Keychain explainer swallows About like Settings (ADR 0043).

## Invariants

- Every menu item reaches a handler in a document tab
  (`every_menu_item_reaches_a_handler_in_a_document`).
- Edit-menu actions in a text field change only that field
  (`edit_menu_reaches_the_find_field`).
