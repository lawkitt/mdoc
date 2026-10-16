# Pseudonymization UI polish

Status: Accepted, 2026-10-09. Superseded in part by
[ADR 0032](0032-replacement-popup-redesign.md) (colours, popup) and
[ADR 0048](0048-workspace-shell-redesign.md) (layout, panel rows, Undo button).

## Problem

Restore conflated "undo this replacement" with "never replace this", and the
popup had two scope systems.

## Decision

- **One scope** (this mention / same wording / entire entity) governs every
  popup action. Apply and Keep act only on mentions in scope that are in the
  matching state; each action is one undo step, and the popup stays on the
  active mention.
- **Keep original** on an applied mention reverts it and records Keep. Taking a
  replacement back to proposed (same alias) is undo (⌘Z).
- **Applied and proposed are distinct colours** in the editor and the panel.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0023-pseudonymization-ui-polish.md),
[interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-ui-polish.md).
