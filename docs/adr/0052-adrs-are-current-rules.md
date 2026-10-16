# ADRs are current rules; history lives in git

Status: Accepted, 2026-10-16.

## Problem

`docs/adr` grew to 51 ADRs (195 KB). Status lines narrate the interview ("the
user confirmed … authorized with 'implement'"), several ADRs carry long
"As implemented" and "Amendment" sections, and partly superseded rules still
read as current. `docs/design` keeps 10 interview records (83 KB) and
`docs/evidence` 35 files (1.3 MB) of dated runs, most of them only historical.
Code cites ADRs by number about 460 times.

## Decision

- **Numbers are stable IDs.** No merging or renumbering of ADRs.
- **Trim in place.** Each ADR keeps Problem, Decision, Invariants (if any),
  Consequences and an optional References line. "As implemented" and amendment
  sections are folded into the decision.
- **Current rules only.** A partly superseded ADR shows only what still holds,
  plus one line: `Superseded in part by ADR NNNN: <what>`.
- **Stubs for fully superseded ADRs.** Title, `Superseded by ADR NNNN.`, and a
  commit permalink to the last full text. The audit found one: 0013. Process
  ADRs whose invariants still steer work (0021, 0029) stay as short ADRs.
- **Dropped records stay reachable.** A trimmed ADR whose verification,
  implementation or amendment record was removed links its full earlier text by
  permalink in `References`.
- **Status line format:** `Status: Accepted, YYYY-MM-DD. Amended by ADR X;
  superseded in part by ADR Y.` No conversation narration. Interview links move
  to a `References` line.
- **Design interviews leave the tree.** `docs/design/*` is deleted; links become
  commit permalinks (the existing precedent in 25 ADRs). Still-live content is
  moved into an ADR or ROADMAP first.
- **Open work goes to the roadmap.** The open qualification gates in
  `pseudonymization-production-readiness.md` become ROADMAP "Done when" items;
  ADR 0016 keeps the scope rule. The Anoni review link in ROADMAP is pinned.
- **Evidence stays only while used.** Keep evidence that the current roadmap
  baselines or a rerun script need: `stabilization/2026-10-16` and
  `build-cache/2026-10-16`. All other dated runs (pseudonymization 10-01 to
  10-08, spellcheck) are deleted and pinned; tool READMEs that regenerate
  tables from them name the commit to check out.
- **Unreferenced media is deleted** (`docs/screenshots`). `docs/images` and
  `docs/product-media` stay.
- `docs/development.md` gets a short ADR-writing note with these rules.

## Consequences

- An ADR reads as the rule in force; why it changed is in git and permalinks.
- New ADRs follow the status format and avoid conversation narration.
- Evidence must be cited by a live consumer to stay in the tree.
- ADR text shrank from about 195 KB to about 130 KB; `docs/design` (83 KB) and
  1.2 MB of evidence left the tree.
