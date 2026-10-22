# macOS caret moves

Status: Accepted, 2026-10-22.

## Problem

On macOS the editor had no ⌘←/→/↑/↓, with or without ⇧, and no ⌥⌫, ⌥⌦ or
⌘⌫; Windows has its Ctrl moves. Found in the 0.1.0 RC runs
([ADR 0057](0057-native-platform-acceptance.md)). The macOS keys were bound
under `cfg!(target_os = "macos")`, so tests on other platforms never saw them.

## Decision

- **⌘← / ⌘→** go to the start / end of the visual (wrapped) line, as in
  TextEdit. On a line with a hidden list, task or quote marker, ⌘← lands after
  the marker and a second ⌘← goes to the true start, like Home. On a wrapped
  continuation line ⌘← goes to that line's start.
- **⌘↑ / ⌘↓** go to the document start / end (`DocumentStart`,
  `DocumentEnd`, which reveal nothing).
- **⇧** with each of the four extends the selection.
- **Deletion:** ⌥⌫ / ⌥⌦ delete the word left / right; ⌘⌫ deletes to the start
  of the visual line.
- **Unchanged:** Home / End on macOS (the paragraph's start and end), ⌥←/→
  word moves, and ⌃A / ⌃E (Select All and Code; no Emacs keys).
- **The keymap is a function of the platform**, so tests on any OS check both
  the macOS and the Windows / Linux sets.

## Invariants

- Tests cover each move and deletion, with and without ⇧, on the macOS
  keymap, including a wrapped line and a list item.
- In the mention popup ⌘-arrows keep stepping to the first / last mention
  ([ADR 0055](0055-stable-compact-mention-popup.md)); the editor's moves apply
  only with the editor focused.

References: [ROADMAP 0.2.0 must-fix "macOS caret moves"](../../ROADMAP.md#must-fix).
