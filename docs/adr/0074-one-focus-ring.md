# One focus ring

Status: Accepted, 2026-10-28. Amends
[ADR 0062](0062-keyboard-reach.md) ("visible focus ring") and
[ADR 0067](0067-theme-contrast-and-type-ramp.md) (focus accent).

## Problem

A fill alone cannot distinguish keyboard focus from selection, hover or a
toggle that is on. The separate focus styles on sidebar rows, Settings rows
and toolbar toggles allowed a focused control to look unfocused
([#194](https://github.com/lawkitt/mdoc/issues/194)).

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

- Every Tab stop uses the same ring in light and dark, including toggles
  that are on. Headless renders cover an on-state toggle, a selected sidebar
  row and a Settings row in both themes. Windows GUI verification checks
  the ring with the panels open, since #194 was found there.
