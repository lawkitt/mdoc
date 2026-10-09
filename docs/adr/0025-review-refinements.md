# Review refinements: categories, cancel, scope outlines, drag-and-drop

Status: Accepted and implemented, 2026-10-09. See the
[design interview](../design/pseudonymization-review-refinements.md).

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
  month-name and blank-fill dates; otherwise Other. Supersedes ADR 0024's
  Person fallback.
- **Cancel addition**: a ↶ icon in the popup header and an "Added · ↶" panel
  footer notice while the addition (and an immediate Apply) are the latest
  history steps; one click undoes both.
- **Panel**: group rows toggle on click with a ▸/▾ chevron; one group open.
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
originals, Keep/Undo semantics and colours (ADR 0023), and ADR 0024's entry
points, trimming and structural checks.
