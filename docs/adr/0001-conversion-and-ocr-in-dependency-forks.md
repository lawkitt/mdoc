# Conversion and OCR belong in dependency forks

Status: Accepted, 2026-10-01.

## Why

Extraction quality (reading order, tables, OCR) is the hard part of mdoc,
and AnyDoc and pdf-inspector already do most of it. Forks in their own
repositories, pinned by exact tested revision, let mdoc add what it needs
(offline OCR behind explicit consent, Cyrillic models, recognition output
before fusion) without a second extractor in the app. Algorithm and
output-quality changes go into the forks; the app owns scheduling, consent,
warnings, the document lifecycle and the Markdown handoff. Every PDF goes to
pdf-inspector, so AnyDoc is built without its `pdf` feature. Upstream is
checked at each desktop milestone's start: pdf-inspector takes it as whole
merges, keeping its history; AnyDoc, a squashed snapshot, takes
cherry-picks with only the mechanical changes they need. Either fork may
port an unmerged community PR, with its author's attribution; fork changes
are not offered upstream. A passing build never validates an extraction
change: an update needs the fork's checks with OCR on and a before-and-after
comparison on the EN/RU fixtures and the public corpus, every changed output
explained, and a structural change gets its own review.

## Rejected

- **A subtree, a merged crate or a crates.io release**: each upstream update
  would become a hand port into mdoc's tree.
- **Fixing extraction in the app**: two implementations that drift apart.
- **Zorite's journal, property, template and import/export features**: only
  its correctness and rendering fixes are ported, with attribution.

## Revisit when

Upstream takes mdoc's OCR extensions.
