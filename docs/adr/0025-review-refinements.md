# Review refinements: categories, cancel, scope outlines, drag-and-drop

Status: Accepted, 2026-10-09. Panel groups and the popup header are
superseded by [ADR 0033](0033-replacements-panel-redesign.md),
[ADR 0044](0044-panel-selection-and-tree-polish.md) and
[ADR 0048](0048-workspace-shell-redesign.md).

## Problem

Manual additions fall back to Person ("Планируемая дата" → PERSON_8). A
mistaken Replace can only be reversed with ⌘Z or a Keep decision. The panel
cannot collapse a group, the editor does not show which mentions a group or
scope chip covers, and moving a mention between groups needs the popup
chooser. The editable alias does not look editable.

## Decision

- **Categories**: add **Date** (`DATE_n`) and **Other** (`REDACTED_n`, last in
  the picker), for manual additions only. Guess Person only for name-shaped text
  (1–4 capitalized words, hyphens and initials allowed); recognise numeric,
  month-name and blank-fill dates; otherwise Other.
- **Cancel addition**: while the addition (and an immediate Apply) are the
  latest history steps, one Cancel undoes both (the popup's Cancel action since
  ADR 0032).
- **Scope outlines**: a thin state-coloured outline around in-scope mentions
  for the selected panel group, the selected popup scope chip, and a hovered
  chip (preview). No scrollbar marks yet.
- **Drag-and-drop**: mention → group assigns one mention; group → group
  merges; mention → empty space splits a new entity. Accent outline and
  "Link to ALIAS" / "New entity" hint; one undo step; no confirmation;
  cross-category adopts the target category.
- **Alias cue**: a single ~700 ms accent glow on the alias field (and on the
  category chip after a fallback guess) for manual-addition popups and the
  first detected-mention popup per tab; static under reduced motion. Hover
  shows a pencil; a click selects the token and opens suggestions.

## Unchanged

Detector categories, stable aliases, occurrence provenance, live-tab
originals, Keep/Undo semantics (ADR 0023), and ADR 0024's entry points,
trimming and structural checks.

References: [interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-review-refinements.md).
