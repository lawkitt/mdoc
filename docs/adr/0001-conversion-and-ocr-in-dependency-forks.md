# Conversion and OCR belong in dependency forks

Status: accepted, 2026-10-01.

mdoc relies on AnyDoc and pdf-inspector for document extraction and OCR. Algorithm
and output-quality improvements belong in those forks rather than a separate
app implementation, avoiding divergent conversion logic. The app owns scheduling,
explicit OCR consent, warnings, document lifecycle, and Markdown handoff; converter
and editor crates remain host-agnostic.
