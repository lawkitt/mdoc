# Settings categories

Status: Accepted, 2026-10-28. Amends
[ADR 0048](0048-workspace-shell-redesign.md) (Settings overlay) and the Settings
layout of [ADR 0006](0006-local-model-settings.md).

## Problem

Settings is one long 600 px scroll: Appearance, Text recognition,
Pseudonymization, an Advanced fold, Spelling, Saved cases, Recent files and
Updates. Number fields wait for a Save button, while every other choice saves
at once, and Reset and Done sit in a footer. Anoni shows a calmer layout: a
category list, then one row per setting.

## Decision

- **Overlay with categories.** The overlay stays (focus handling and "click
  outside closes" are unchanged) and widens to about 800×600, within the
  window. On the left is a category list (180 px, narrowing to 140 px below
  about 640 px; names may wrap). The selected category's rows fill the right.
- **Categories.**
  - **General:** Appearance; Automatic updates (the "Check when mdoc starts"
    switch, a status line such as "Up to date · just now" or "Version X ready",
    and Check now / Restart now on the right, with ADR 0038/0040 behaviour
    unchanged); Recent files (Clear); and a Keyboard shortcuts row that opens
    the [ADR 0073](0073-keyboard-shortcuts-panel.md) sheet.
  - **Detection:** model, threshold, Compare pseudonymization.
  - **Recognition:** OCR model, resolution, minimum confidence, Compare text
    recognition.
  - **Spelling:** the switches and Your dictionary. The dictionary opens as a
    drill-in inside the content pane with "← Spelling", and the list stays
    visible.
  - **Cases:** Ask before closing an unsaved mapping.
  - **About:** version and build identity (Release, Protected or Dev,
    [ADR 0045](0045-prompt-free-case-key.md)), Help, Website, Report an issue,
    the privacy note and the app licence. The **About mdoc** menu item opens it
    on every platform.
- **One row per setting.** Title and description on the left, the control on
  the right, hairline dividers. Switches replace checkboxes; segmented
  controls stay. Each model is a radio-list row: name and size/status on the
  left, the radio and its action (Download, Repair, Cancel, Remove) on the
  right, and the model's source and licence stay on its row. The Advanced
  fold goes; its rows move into their categories.
- **Saving.** Everything saves on change. Numbers save on Enter or blur; an
  invalid value shows an inline error and keeps the last good one. There is no
  Save button. Each category ends with a quiet **Reset** row that resets only
  that category. Done becomes a corner ×; Esc and an outside click also close.
- **Navigation.** The category list is a tab list: ↑/↓ move between categories
  and show each at once, and Tab moves into the content. There are no ⌘-digit
  jumps. The palette lists every category ("Settings › Detection"). Existing
  deep links (model setup, updates) open their category. Otherwise Settings
  opens on General.
- **Layout only.** No tone, accent colour, UI language, profiles, global
  shortcut or AI connection.

## Invariants

- Every current setting can be reached by keyboard and saves on change, in
  both themes.
- A failed save restores the previous value and says so, as today.

## Consequences

The ADR 0073 shortcut table gains no keys. Headless renders cover one screen
per category instead of one long scroll.
