# macOS caret moves

Status: Accepted, 2026-10-22. Soft wraps revised 2026-10-24 (caret
affinity).

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
- **Tables and right-to-left rows.** In a table cell ⌘← / ⌘→ go to the
  cell's start / end and ⌘⌫ deletes only to the cell's start. On a bidi row
  ⌘← / ⌘→ go to the caret stops painted furthest left / right (an RTL row's
  logical end / start); ⌘⌫ still deletes to the start in reading order.
- **Soft wraps.** ⌘→ on a wrapped row stops before the space the row broke
  at. On a row that broke inside a word (a long URL, unspaced CJK) it goes
  after the last character, onto the wrap offset with *upstream* affinity.
- **Caret affinity.** A caret on a wrap offset draws at the start of the
  row below (downstream, the default) or at the end of the row above
  (upstream). Upstream comes only from ⌘→, Up/Down whose goal is past a
  row's end, and a click past a row's end; any other move or an edit drops
  it. Bidi rows ignore affinity (they keep their own rows).
- **Table cells.** Up/Down step through a cell's own visual rows (wrapped
  text, `<br>` lines) at the caret's x before moving to the same column of
  the next table row, which they enter at that x.
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
  keymap, including a wrapped line, a row broken inside a word, a list item,
  a table cell and an RTL line, and affinity: drawing, Up/Down, clicks and
  a wrapped or `<br>` table cell (`caret_tests.rs`).
- In the mention popup ⌘-arrows keep stepping to the first / last mention
  ([ADR 0055](0055-stable-compact-mention-popup.md)); the editor's moves apply
  only with the editor focused.

References: ROADMAP 0.2.0 must-fix "macOS caret moves" (done; see CHANGELOG).
