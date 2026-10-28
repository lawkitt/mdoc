# One focus ring

Status: Proposed, 2026-10-28 (0.3.0 grill). Amends
[ADR 0062](0062-keyboard-reach.md) ("visible focus ring") and
[ADR 0067](0067-theme-contrast-and-type-ramp.md) (focus accent).

## Problem

There are 22 separate `focus_visible` styles. Sidebar rows and Settings rows
show focus with the same fill as selection or hover, and a toolbar toggle that
is on paints the same `placeholder_bg` as its focus style, so a focused toggle
looks unfocused ([#194](https://github.com/lawkitt/mdoc/issues/194)).

## Decision

- **One helper.** `ui::focus_ring` replaces every `focus_visible` style: a 2 px
  accent ring drawn outside the control with an offset, never a fill. Hover,
  selection and on-state keep their fills, and the ring sits on top of them.
- **Sidebar and Settings rows** stop using fill as focus.
- **Both platforms, both themes.**

## Invariants

- `style::contrast_tests` checks that the ring reaches 3:1 against every
  background it can sit on, the toggle-on fill and the selection fill included.

## Consequences

- Done when every Tab stop shows the same ring in light and dark, including
  toggles that are on, and #194 closes (ROADMAP 0.3.0 item 5). Checked on
  Windows by a GUI run, since #194 was found there.
