# Merge hint

Status: Accepted, 2026-10-29. Amends
[ADR 0068](0068-short-and-inflected-name-variants.md) (proposed forms).

## Problem

Several mentions of one subject get separate aliases until the user links them
with Same as…; a missed link hands the AI two people where there is one.
ADR 0068 proposes short forms but leaves them as separate identities.

## Decision

- **Signals** that two identities very likely name the same subject:
  - an ADR 0068 short or inflected form with exactly one candidate full name;
  - wordings that differ only in case, whitespace or punctuation (any
    category, e.g. "Northwind Ltd" / "Northwind Ltd.");
  - OCR or typo near-duplicates: edit distance 1, both names 6+ characters;
  - reversed Russian name order ("Павлова Марина" / "Марина Павлова").
- **Not signals:** initials; a short form with two or more candidates
  (relatives sharing a surname); Latin/Cyrillic transliteration.
- **Hint.** One line under the group header of the shorter (or later)
  identity: "Looks like Maya Bennett · Merge ×". No score. Every hint shows,
  each on its own identity.
- **Merge** links into the fuller identity, which keeps its alias and primary
  wording; the other alias is retired and keeps restoring (ADR 0041). One undo
  step, with an Undo toast. Nothing links without the click.
- **Dismissal** (×) holds for that pair: stored in the case when the document
  has one, otherwise for the tab's life.

## Consequences

ADR 0068's "proposed, never linked" stands; the hint shortens Same as… to one
click where the signal is strong.

References: [interview](../design/roadmap-grill-2026-10-29.md),
[2026-10-17 spec](https://github.com/lawkitt/mdoc/blob/f428df4/ROADMAP.md).
