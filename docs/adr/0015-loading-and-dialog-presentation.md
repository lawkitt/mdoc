# Consistent loading feedback and action-oriented dialogs

Status: Accepted, 2026-10-07. Replacement popup content is superseded by
[ADR 0032](0032-replacement-popup-redesign.md) and
[ADR 0048](0048-workspace-shell-redesign.md).

## Decision

- **Activity**: one restrained style for app-owned loading states: an accent
  spinner with a short task label, and real progress bars only where progress
  is measurable. Labels show at once; animation starts after about 150 ms;
  usable content stays visible; cancellation is unchanged. Animation honours
  Reduce Motion and refreshes at most 30 fps.
- **Download progress** is per current file and disappears for verification
  and runtime checks, where byte counts would mislead.
- **Dialogs**: app-owned popups and dialogs share padding, borders, fields and
  button hierarchy (`src/ui.rs`); native OS dialogs stay native.
- **Content**: original → replacement is the dominant content of replacement
  surfaces; nonessential text goes, with no Details disclosures. Actionable
  errors and essential context stay.
- **Selection** in the shared single-line input is a translucent foreground
  tint with an inherited-colour caret, readable in both themes.

## Invariants

Presentation only: processing, explicit activation, cancellation and
stale-result guards, replacement/Keep/undo semantics, source state and
provenance are unchanged.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0015-loading-and-dialog-presentation.md),
[interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/loading-and-dialogs.md).
