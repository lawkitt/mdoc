# Keyboard reach by regions

Status: Accepted, 2026-10-22. Supersedes in part [ADR 0057](0057-native-platform-acceptance.md):
plain keyboard reach is now required.

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
- **Landing:** a region takes focus on the control last focused in it,
  otherwise its first control; the editor gets its caret back.
- **Tab / ⇧Tab** move to the next / previous control with a visible focus
  ring, going on into the next region at a region's end; in the editor Tab
  indents. Esc closes menus; in the sidebar or toolbar it returns focus to
  the editor.
- **Sidebar tab list:** ↑ / ↓ move between tabs, Enter opens, ⌫ closes the
  tab, and Tab leaves the list for the next control, as in the replacements
  panel.
- **Home:** Tab from Home reaches its controls (Open files, Recents and so on).

## Invariants

- Every control is reachable and can be activated by keyboard alone on both
  platforms, without a screen reader or Full Keyboard Access.
- Tests cover the F6 order with and without the optional regions, return to
  the last focused control, and the sidebar list keys.

## Consequences

Checklist rows A4 and H18 drop the 0.1.0 exception and check F6 regions.

Code: `Workspace::cycle_region` and `DocumentView::focus_regions`; tests in
`workspace_tests.rs` (`f6_*`, `tab_from_home_*`, `sidebar_tab_list_*`).
