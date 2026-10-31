# Standard macOS menus

Status: Accepted, 2026-10-18. Amended 2026-11-01 by the UI polish grill
(decision 10): Open Recent, Restore Clipboard, the View items, mdoc Help,
Check for Updates… and the copy keys.

## Problem

GPUI makes the first menu the application menu. mdoc set one menu named
"File", so macOS showed it as "mdoc": no File, Edit or Window menu, no About,
Hide (⌘H) or Services, and no menu-bar Undo, Copy or Paste. Native Open and
Save panels had no ⌘C/⌘V, since those come from the Edit menu. Found by the
2026-10-18 platform-acceptance dry run ([ADR 0057](0057-native-platform-acceptance.md)).

## Decision

- **Menus** (`src/menus.rs`): *mdoc* (About mdoc, Check for Updates…,
  Settings…, Services, Hide mdoc ⌘H, Hide Others ⌥⌘H, Show All, Quit mdoc),
  *File* (New, Open…, Open Recent, Open Case…, Convert to Markdown, Save,
  Save As…, Copy Markdown ⇧⌘M, Copy for AI ⇧⌘C, Paste AI Reply, Close Tab),
  *Edit* (Undo, Redo, Cut, Copy, Paste, Select All, Find…, Find and
  Replace…, Find Next, Find Previous, Pseudonymize, Pseudonymize Clipboard,
  Restore Clipboard), *View* (Home ⇧⌘H, Search ⌘K, Toggle Sidebar ⌃⌘S;
  Edit, Original, Pseudonymized, Side by Side ⌘\\; Show Replacements; Zoom
  In, Zoom Out, Actual Size), *Window* (Minimize ⌘M, Zoom, Next Tab,
  Previous Tab) and *Help* (mdoc Help, Keyboard Shortcuts,
  [ADR 0073](0073-keyboard-shortcuts-panel.md)). Hide, Services and their
  shortcuts, and ⌃⌘S, are macOS only; on Windows Check for Updates… is in
  Help.
- **Open Recent** lists up to 10 files from Recents (the folder after a name
  two files share), then Clear Menu, which empties Recents as Settings'
  Clear does; it is rebuilt whenever Recents change.
- **Restore Clipboard** restores the active tab's aliases (its case's, else
  its live mapping's) in the clipboard text, as the shelf's Restore does
  ([ADR 0084](0084-menu-bar-shelf.md)). **Show Replacements** opens the panel
  when the toolbar would offer it. **Check for Updates…** opens Settings at
  About and checks now. **mdoc Help** opens what the sidebar's ? opens.
- **Copy keys.** Copy for AI ⇧⌘C (Ctrl+Shift+C) and Copy Markdown ⇧⌘M
  (Ctrl+Shift+M): ⌥⇧⌘C, the first pick, is Pseudonymize Clipboard's.
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
