# Keyboard shortcuts panel

Status: Proposed, 2026-10-28 (0.3.0 grill).

## Problem

mdoc binds about 150 keys in seven `bind_keys` functions, and the command
palette shows shortcuts typed in by hand (`shortcut("⌘N", "Ctrl+N")`). The two
lists can drift apart, and no screen lists what the keys are. Anoni 0.86.1
shows every shortcut by context.

## Decision

- **One table.** A new module holds every user-facing binding: action, keys per
  platform, context group (Everywhere, Editor, Popup, Panel) and label. The
  `bind_keys` functions register from it, and the palette's shortcut labels
  come from it. Bindings that exist only as internal plumbing (Tab cycling
  inside a control group, for example) stay in their modules and are not
  listed.
- **Panel.** A read-only modal sheet, like the confirm dialog
  ([ADR 0064](0064-dialog-and-role-semantics.md)): a filter field, then the
  bindings grouped by context, showing only the current platform's keys.
  Esc closes it.
- **Opening it.** `?` when no text field has focus (Home, sidebar, toolbar,
  panels); ⌘/ (Ctrl+/) everywhere; the palette action "Keyboard Shortcuts";
  Help → Keyboard Shortcuts on macOS. In the editor, `?` types a question mark.

## Invariants

- A test fails if a bound user-facing action is missing from the table, or if
  the palette shows a shortcut that isn't in it.

## Consequences

- Done when every bound action appears in the panel, from the one table
  (ROADMAP 0.3.0 item 3).
- The acceptance checklist gains a row for `?` and ⌘/ / Ctrl+/.
