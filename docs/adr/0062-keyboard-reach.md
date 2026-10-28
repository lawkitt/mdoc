# Keyboard reach by regions

Status: Accepted, 2026-10-22. Supersedes in part [ADR 0057](0057-native-platform-acceptance.md):
plain keyboard reach is now required.
Amended by [ADR 0074](0074-one-focus-ring.md): the one outside focus ring.

## Problem

Plain Tab could not reach the toolbar or the sidebar: the editor takes Tab for
Indent, and nothing moved focus between parts of the window. 0.1.0 accepted
reach through a screen reader or Full Keyboard Access only (checklist A4,
H18).

## Decision

- **F6 / ⇧F6 cycle regions** on macOS and Windows, wrapping: sidebar →
  toolbar → editor → replacements panel → source preview → comments → sidebar.
  Hidden regions are skipped. On Home, which has no editor, the cycle is
  sidebar → Home content. F6 is the only chord (on MacBook keyboards fn+F6
  unless function keys are standard).
- **Landing:** a region takes focus on the control last focused in it
  (however focus left it), otherwise its first control; the editor gets its
  caret back. Regions follow what is drawn: in a narrow window only the
  shown pane of Markdown and Original takes part. A sidebar tab is
  remembered as the tab, not its row: F6 into the sidebar lands on that tab
  in the layout drawn now, the expanded list or the collapsed rail (which
  opens its reveal), even if the sidebar was collapsed or expanded since. A
  closed tab, or one folded into its case, falls back to the region's first
  control.
- **Tab / ⇧Tab** move to the next / previous control in window order, with a
  visible focus ring, across region edges. The document's notices strip
  (with the narrow-window pane switch), the find bar and the view switcher
  belong to the toolbar's region: Tab reaches them, Esc returns to the editor
  from the strip and the switcher, and F6 remembers them. The editor is no Tab stop, but Tab does not skip it: Tab
  past the toolbar region's last control enters the editor, and ⇧Tab from the
  region after the editor returns to it. Tab and ⇧Tab indent and outdent
  there; F6 leaves.
  The mention popup, menus (including the Replacements panel's row and filter
  menus) and dialogs keep Tab inside them.
- **Menus and lists take focus.** Opening the More menu, the source
  preview's More menu (the tools a narrow preview has no room for) or the
  Open Case list moves focus into it, and Tab / ⇧Tab stay inside. Esc closes it and
  gives focus back: the More button, or where Open Case was chosen.
- **Esc** closes a menu or the notices popover first; in the sidebar or
  toolbar with nothing open it returns focus to the editor.
- **Modal surfaces** (Settings, the Open Case list, the palette) keep focus:
  F6 does nothing while one is open. The Keychain explainer and the case
  category list keep focus too.
- **Sidebar tab list:** one Tab stop, on the row the arrows last reached
  (else the active tab). ↑ / ↓ move between the rows shown (not folded case
  members), Enter opens, ⌫ closes the tab, and Tab leaves the list.
- **Sidebar context menu:** ⇧F10 on a tab opens its menu in the expanded
  sidebar or collapsed rail. Tab reaches case actions, including Categories
  to detect. That list takes focus, keeps Tab inside, and Esc or Close returns
  to the sidebar row that opened the menu.
- **Home:** Tab from Home reaches its controls (Open files, Recents and so on).
  Each Cases and Recents row is a Tab stop with the button role and a name
  (file, folder, pseudonymized, when opened); Enter or Space opens it.
- **Focus handles on buttons** that a control tracks itself (Keychain
  Continue, the setup cards' buttons) are marked as Tab stops, or Tab skips
  them once focus leaves them.

## Invariants

- Every control is reachable and can be activated by keyboard alone on both
  platforms, without a screen reader or Full Keyboard Access.
- Tests cover the F6 order with and without the optional regions, return to
  the last focused control (a tab across a sidebar collapse and expand), Tab
  across region edges in both sidebar layouts, and the sidebar list keys.

## Consequences

Checklist rows A4 and H18 drop the 0.1.0 exception and check F6 regions.

Code: `Workspace::cycle_region` and `DocumentView::focus_regions`; tests in
`src/workspace/tests/focus_regions.rs` (`f6_*`, `tab_from_home_*`,
`tab_moves_between_regions_*`, `sidebar_tab_list_*`);
`Workspace::tab_row_in_layout` maps a remembered tab row to the drawn layout.
