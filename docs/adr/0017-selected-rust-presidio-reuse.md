# Reuse selected Rust Presidio components through mdoc's detector adapter

Status: Accepted, 2026-10-08.

## Why

Structured identifiers need patterns and checksums, and the MIT-licensed
`presidio-analyzer` has tested ones. mdoc pins it with default features off
and reuses only its pattern and validator configuration and its SNILS
checksum, for email and contextual checksum-valid INN and SNILS, beside
mdoc's own labelled ОГРН/ОГРНИП. Upstream defaults would regress mdoc: its
email pattern is ASCII only, its SNILS defaults are permissive, and it has no
INN recognizer. So mdoc keeps its Unicode patterns, strict same-line
context, digit boundaries and INN checksum. Matches are exact UTF-8 byte
spans of the Markdown; review, syntax protection and provenance stay
app-owned.

## Rejected

- **The full analyzer, the anonymizer or `PatternRecognizer::analyze`**: the
  last removes overlaps quadratically, and mdoc already resolves them.
- **Gazetteers, the optional ONNX or POS features, extra categories or a
  rules-only fallback**: GLiNER2 on the shared ONNX Runtime stays the
  detector.

## Revisit when

Upstream gains Unicode email, strict SNILS rules or an INN recognizer.
