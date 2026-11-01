# Flaw hunt loop

Status: Accepted, 2026-11-01. Applies
[ADR 0092](0092-recognition-correction-pipeline.md); runs under the fleet's
hub (fleet ADR 0019).

## Problem

Recognition flaws are found by hand, one survey at a time (#301, U27).
0.4.0 runs for hours with the maintainer away, so agents must find flaws,
record them, fix them and know when to stop, without grading their own
homework or fitting rules to the sentences they wrote.

## Decision

- **Roles.** A **hunter** (Codex Sol, on the Windows PC) writes realistic
  synthetic legal documents from slots, so their gold is known by
  construction, runs `just recognize --gold` and reports each miss, false
  hit and boundary error. A **curator** (an Opus hub sub-agent) audits each
  case's gold against [the guide](../../tests/fixtures/recognition/GUIDE.md),
  drops bad cases, makes Mac snapshots and opens one fixtures-only PR per
  hunt round. A **fixer** (Opus) takes a flaw class, fixes it in the rule
  pack or pipeline and opens one PR per class. Codex reviews Opus PRs.
- **Fixtures.** One Markdown file per fixture:
  `tests/fixtures/recognition/{en,ru}/<kind>/<id>.md`, front matter
  `status`, `flaw`, `issue`, `kind`, `source`, a `⟦CATEGORY|text⟧` gold
  body, and its model snapshot `<id>.model.json` beside it. The hybrid and
  render corpora are imported; their originals stay frozen for research
  reproduction. Fixtures are synthetic and CC0: no real people, no copied
  document text.
- **Status and the ratchet.** `pass`, `known-flaw` (expected to fail;
  strict, so the gate fails when it starts passing until it flips) and
  `model-limit` (no layer can fix it; the curator's reason; listed for the
  maintainer). `pass` never goes back without the maintainer.
- **Snapshots.** Each fixture's raw model output is committed. The
  pipeline runs on snapshots anywhere in milliseconds; the Mac is the
  reference platform. The **detection gate** (`local/macos`, on every PR
  touching `mdoc-detect`, `mdoc-pii`, the rule pack or the fixtures)
  reruns the model, fails on a stale snapshot, and asserts every fixture's
  spans. Its model run has a 5-minute budget; past it, fixtures split into
  a per-PR set and a nightly full set. `local/windows` runs the
  snapshot-based tests only; once per run the hub compares Windows and Mac
  output and files drift.
- **Flaw classes.** One issue per class (label `flaw`, the 0.4.0
  milestone), not per case. A fixer closes the class when its fixtures
  pass or are `model-limit`.
- **Scoreboard.** Recall and precision per language × category × kind,
  and the counts per status. Recall first; precision is ratcheted: a fix
  adds no false hit on a negative fixture.
- **Holdout.** About 60 documents (30 EN, 30 RU, every kind, 10 negatives)
  generated before any fix lands, kept only in the private
  `lawkitt/mdoc-holdout`, never in a fixer's brief. The hub runs it every
  5 fix merges and at the run's end and logs aggregates only. Falling
  holdout recall, or holdout gains under half the fixture gains over two
  checkpoints, pauses the fixers and parks a decision.
- **Coverage and stopping.** Kinds: contract, claim, judgment, power of
  attorney, letter, OCR scan, DOCX-converted, tables; each in EN and RU. A
  cell is saturated after two hunt rounds with no new kind of flaw; the
  hunter moves on. The loop ends when every cell is saturated and no
  `known-flaw` is left, or by the fleet's stops.
- **Autonomy.** Agents decide alone: rule-pack data, pipeline code in
  `mdoc-detect` and `mdoc-pii` within ADR 0092, fixtures, the guide's
  examples. They park as a `decision` issue and move on: a new category, UI
  beyond ADR 0092, the guide's rules, deleting or loosening a fixture, the
  threshold, the model or its export.

## Consequences

- The gate makes every fixed flaw stay fixed; the strict status records
  every fix.
- Fixer work runs on any machine; only snapshot refreshes need the Mac.
- A hunter and a fixer on different providers miss different things; the
  holdout still measures what both miss.

Terms: [glossary](../../CONTEXT.md). References:
[grill](../design/recognition-grill-2026-11-01.md).
