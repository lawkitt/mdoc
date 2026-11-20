# Bundled spellcheck

Status: Accepted, 2026-10-10.

## Why

Spellcheck is there to surface conversion and OCR errors before the AI
handoff; ordinary typo squiggles come with it. It must give the same
misspellings for the same text on every platform, so mdoc bundles
`spellbook` (pure Rust, Hunspell dictionaries) at an exact version, with
unmodified LibreOffice en_US, en_GB and ru_RU dictionaries compressed into
the binary and a curated legal word list per language, offline only. The
binary grows by about 1.6 MB. English and Russian mix in one document, each
word's language chosen by its script; a word is flagged in English only if
en_US and en_GB both reject it, and mixed-script words are always flagged.
Spellcheck never changes text unless the user picks a suggestion.

## Rejected

- **The OS spellcheckers**: their results depend on the machine, and Linux
  has none.
- **Repairing words split by hyphens across lines**: repairs belong in the
  conversion forks; spellcheck flags them.

## Revisit when

mdoc claims a third document language.
