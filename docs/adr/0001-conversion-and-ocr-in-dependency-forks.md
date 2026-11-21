# Conversion and OCR belong in dependency forks

Status: Accepted, 2026-10-01.

## Why

Extraction (reading order, tables, OCR) is mdoc's hard part, and AnyDoc and
pdf-inspector do most of it. Forks pinned by exact tested revision let mdoc
add what it needs (consented offline OCR, Cyrillic models) without
a second extractor. Algorithm and quality changes go into the forks; the app
owns scheduling, consent, warnings, the document lifecycle and the Markdown
handoff. Every PDF goes to pdf-inspector; AnyDoc builds without `pdf`. Each
desktop milestone, pdf-inspector merges upstream whole, and AnyDoc, a
squashed snapshot, takes mechanical cherry-picks. Either fork may port an
unmerged community PR, with attribution; nothing is offered upstream. A
passing build never validates an extraction change: a pin moves after the
fork's checks with OCR on, mdoc's checks, macOS and Windows OCR smoke runs
(a missing one blocks) and a before-and-after comparison on the EN/RU
fixtures and public corpus, every change explained; a structural change gets
its own review. Craft leaf crates without hayro or krilla pin by revision;
code on their patched hayro 0.7, which doesn't travel, or behind a crate's
API (DOC decryption) is ported with attribution.

## Rejected

- **A subtree, a merged crate or a crates.io release**: each upstream update
  would become a hand port.
- **Fixing extraction in the app**: two drifting implementations.
- **Zorite's journal, property, template and import/export features**: only
  its correctness and rendering fixes are ported, with attribution.

## Revisit when

Upstream takes mdoc's OCR extensions, or craft-libs reaches crates.io or
our hayro.
