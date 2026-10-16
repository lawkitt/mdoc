# Use reviewable local pseudonymization

Status: Accepted, 2026-10-01. Superseded in part by
[ADR 0041](0041-cases-and-ai-response-restoration.md) (cases store mappings),
[ADR 0034](0034-pseudonymization-supported-feature.md) (no experimental label),
and ADRs 0020, 0032 and 0048 (review surfaces).

## Decision

mdoc assists replacement of identifying information locally, with consistent
placeholders and user review. Automatic detection never establishes that text is
anonymous. Reuse existing open-source libraries and models rather than growing a
detection engine; the shipped integration is Rust, with no bundled Python worker
or Docker.

## Review contract

- Pseudonymize is explicit; scanning runs in the background and highlights
  candidates in the editor. Normal editing and find stay available.
- Generic annotations and painted geometry belong in `mdoc-editor`; detection,
  review policy, grouping and mapping belong in the app.
- Review names, organizations, contact details, addresses and identity, tax and
  bank identifiers. Ordinary dates, amounts and legal references stay unless the
  user selects them. Manual additions and exclusions are allowed.
- Repeats of an exact value are reviewed together with an explicit
  single-occurrence option. Initials, inflections and other variants stay
  separate until the user links them; linking always needs a user action.
- Applying is one undoable edit. Keep changes no text. Edits invalidate or
  revalidate affected candidates; stale revisions or offsets never replace the
  wrong text.
- Placeholders are stable category tokens (`PERSON_1`, `ORG_1`, `ADDRESS_1`).
- Scan everything Copy Markdown transfers, including link destinations, image
  paths, code and HTML; show hidden-source candidates beside their containing
  element. Preserve Markdown syntax. Source files, filenames and PDF/DOCX
  originals are outside replacement scope.
- Rescan keeps valid mappings and explicit exclusions. Cancel stops pending
  detection and rejects late results without reverting applied edits. Remaining
  counts never block copy or save.
- Models are pinned, verified downloads taken only through explicit setup, then
  run offline. Opening and editing documents never depend on a model.
- Without a case, mappings live only in the tab and end when it closes.

## Detector

GLiNER2 via gliner2-rs (Hub feature off) with the pinned privacy PII export,
CPU only, sharing the app's ONNX Runtime. One scan at a time; each scan loads and
drops its engine. Scans cover the full Markdown source in overlapping windows
checked against actual token counts and reject, rather than truncate,
pathological windows. Limits: 2 MiB source, 512 tokens per window, a two-minute
cooperative deadline. Model load and a native inference call cannot be
interrupted mid-call.

Tokens start with a letter, end with a letter or digit, and use ASCII letters,
digits, `_` and `-`. HTML tag and attribute names, attribute quotes, list
prefixes and link delimiters are protected; spans crossing syntax or line
boundaries need manual narrowing.

## Consequences

No candidate passed the [initial qualification](../../tests/fixtures/pseudonymization/README.md);
GLiNER2 ships as review assistance, not as a qualified detector. Open
qualification work is tracked in the ROADMAP.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0002-reviewable-local-pseudonymization.md),
[inline verification](../../tests/fixtures/pseudonymization/inline-review.md).
