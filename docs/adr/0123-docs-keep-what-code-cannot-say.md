# Docs keep what the code can't say

Status: Accepted, 2026-11-20. Replaces ADR 0118.

## Why

At 300–400 PRs a week, a doc that restates behaviour goes stale within days,
and agents follow the stale copy. ADR 0118 cut `docs/` from 154k to 30k
words, but the specs then filled to their budgets, the code kept about 1,800
citations of ADRs whose text was gone, and grills went on writing product
ADRs. The code is the record of what mdoc does; a doc earns its place only
by saying what the code can't: intent, a promise, or why code that looks
wrong is right. The rules live in `docs/development.md` (Writing docs).

## Rejected

- **No docs at all.** Rejected choices and promises such as "never send a
  document off the device" aren't visible in code, and agents would propose
  the rejected options again.
- **Keep 0118's specs and budgets.** The budgets became targets and the
  specs became caches of the code.
- **A ratchet lint against old patterns and citations.** It adds machinery
  that itself needs upkeep; the maintainer runs the `codebase-doctor` skill
  by hand instead.
- **Keep the ADR index and `Folds:` lines.** They existed to resolve
  citations, which are gone from the code; git history resolves an old
  number.

## Revisit when

Agents repeat a mistake that a deleted rule would have prevented, or a
doctor pass keeps finding the same drift.
