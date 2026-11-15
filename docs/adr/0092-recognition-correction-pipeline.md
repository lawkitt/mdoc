# Recognition correction pipeline

Status: Accepted, 2026-11-01.

## Problem

The pinned GLiNER2 model misses what no threshold recovers: home addresses,
dates of birth, claim numbers, a first name alone (#301). Fine-tuning needs
training data, a new pinned export and requalification.

## Decision

- **Correct after the model, in layers**, instead of fine-tuning or swapping
  the model. At scan time in `mdoc-detect`: model → `never` and `shape`
  filters → `context` rules → structured identifiers → `span` fixes → the
  user's lists. After a decision, in `mdoc-pii`: propagation of a name's
  forms. Every detection keeps its recognizer (`Model`, a rule,
  `Propagation`, `UserList`).
- **The rule pack is data compiled in:** TOML in
  `crates/mdoc-detect/rules/{shared,en,ru}.toml`, read with `include_str!` and
  validated by a unit test, each entry commented. Manual marks and the model
  share it.
- **New categories:** dates of birth (Date in a personal context only) and
  labelled case numbers (`Category::CaseNumber`, token `CASE`, named so it
  never reads as a saved case). Both restore like any alias.
- **Propagation links when the candidate is unique.** A form of a full name
  with exactly one candidate joins that identity as a proposal; with two or
  more it stays separate. Nothing is replaced until the user applies it.
- **User lists.** Keep feeds a case's Never list and a manual mark its Always
  list; user-wide lists live in Settings. Both live in the keychain-protected
  store, because they can hold client names.
- The threshold and the model stay as pinned; the pipeline targets devices
  that run the model.

## Invariants

- Ordinary dates, amounts and legal references stay; dates are never shifted.
- Statute and article numbers are never case numbers.
- A rule adds no false hit on a negative recognition fixture.
- A miss no layer can fix is recorded as a model limit, never hidden.

## Consequences

Rules can add false hits, so the negative fixtures hold precision. The rules
themselves are in the recognition spec.

References: [full text before the docs doctor](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0092-recognition-correction-pipeline.md),
[grill](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/design/recognition-grill-2026-11-01.md),
[roadmap grill](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/design/roadmap-grill-2026-11-01.md).
