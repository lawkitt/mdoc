# Recognition correction pipeline

Status: Accepted, 2026-11-01.

## Why

The pinned GLiNER2 model misses what no threshold recovers: home addresses,
dates of birth, claim numbers, a first name alone. Fine-tuning needs
training data, a new pinned export and requalification, so mdoc corrects
after the model, in layers: filters, context rules and structured
identifiers at scan time in `mdoc-detect`, propagation of a name's forms
after a decision in `mdoc-pii`, and the user's Always and Never lists. Every
detection keeps its recognizer, and the rule pack is data compiled into the
binary and reviewed like code. Rules can add false hits, so the negative
fixtures hold precision: a rule adds no false hit on them. A miss no layer
can fix is recorded as a model limit, never hidden. Ordinary dates, amounts
and legal references stay, and dates are never shifted.

## Rejected

- **Fine-tuning or swapping the model**: training data, a new export and
  requalification for every fix.
- **Lowering the threshold**: it trades misses for false hits in every
  category.

## Revisit when

A new detector qualifies, or rules can no longer keep the negative fixtures
clean.
