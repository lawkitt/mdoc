# Pseudonymization UI polish

Status: Accepted and implemented, 2026-10-09. See the
[historical design interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-ui-polish.md).

## Problem

The Replacements panel sits at the far right, beyond the Original preview, away
from the text it describes. Apply exists only as a panel-wide batch. The popup
has two scope systems: chips for alias/category changes, and separate fixed-scope
Keep/Restore buttons. Restore conflates "undo this replacement" with "never
replace this". Applied and proposed highlights differ only in opacity. Expanded
mention rows in the panel have no hover or selected styling.

## Decision

- **Layout**: Markdown | Replacements | Original. The panel keeps its adaptive
  width and full height; Original gets its own × close. This supersedes
  [ADR 0020](0020-direct-replacement-workspace.md)'s right-edge placement.
- **One popup scope**: the This mention / Same wording / Entire entity chips
  govern every popup action, with counts on each action. This supersedes ADR 0020
  Q10's independent Keep/Restore scope.
- **Actions**: proposed mentions get `Apply · N` and `Keep original · N`;
  applied mentions get an ↶ Undo icon (back to proposed, same alias) and
  `Keep original · N` (revert and record Keep). Apply/Undo act only on
  matching-state mentions in scope. Each action is one undo step, and the popup
  stays on the active mention.
- **Applied state is a distinct colour**: teal (`search_accent`) for applied,
  amber (`alert_warning`) for proposed, in the editor, popup badge and panel
  markers. Group rows show `K of N applied` when mixed.
- **Panel rows** have hover and selected states. The active mention row follows
  popup navigation. Applied mention rows offer ↶ for that mention only.
- **Footer** undo controls use the ↶ icon. After Keep, `Kept N ↶` remains
  until the next review action or text edit.

## Unchanged

The term Keep, stable aliases, occurrence provenance, live-tab originals, the
batch Apply replacements action, undo/redo semantics, exact Copy Markdown and
experimental status remain as in ADRs 0020–0022.
