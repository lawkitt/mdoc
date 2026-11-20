# Use reviewable local pseudonymization

Status: Accepted, 2026-10-01.

## Why

Lawyers need to send documents to AI assistants without sending client
names, and no detector is reliable enough to promise anonymity. So mdoc
replaces identifying information on the device with consistent placeholders
(`PERSON_1`, `ORG_1`) that the user reviews before anything leaves.
Detection assists the review; it never establishes that text is anonymous.
Detection reuses open-source models rather than growing an engine: GLiNER2
through gliner2-rs with a pinned PII export, on the CPU through the app's
ONNX Runtime, installed only by explicit, verified setup. Opening and
editing a document never depend on a model.

## Rejected

- **Automatic anonymization without review**: no candidate passed the
  initial qualification, so a missed name would leave silently.
- **A bundled Python worker or Docker**: a second runtime to ship and update
  on every platform, beside a Rust app.
- **Detection in the cloud**: the document would leave the device before
  anyone reviewed it.

## Revisit when

A pinned detector passes qualification on the pseudonymization fixtures.
