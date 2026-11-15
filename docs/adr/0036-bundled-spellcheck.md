# Bundled spellcheck

Status: Accepted, 2026-10-10.

## Decision

- **Purpose:** surface conversion and OCR errors before AI handoff; ordinary
  typo squiggles come with it.
- **Engine:** `spellbook` (pure Rust, MPL-2.0, Hunspell dictionaries), pinned
  to an exact version, instead of the OS checkers, whose results depend on
  the machine and which do nothing on Linux.
- **Dictionaries:** unmodified en_US (SCOWL), en_GB (LGPL) and ru_RU
  (Lebedev) from LibreOffice, compressed into the binary at build time, plus
  a curated legal word list per language that users can't edit. Loaded
  lazily in the background; a language that is off is never loaded. Offline
  only; dictionary notices live beside the dictionaries and in NOTICE.md.
- **Languages:** English and Russian mixed in one document, the language of
  each word chosen by its script. A word is flagged in English only if en_US
  and en_GB both reject it. Russian treats е and ё alike. Mixed-script words
  are always flagged.
- **User dictionary:** exact spellings in `spelling-words.txt` beside
  `settings.json`, shared by all documents, at most 10,000 words of up to
  64 characters. A lowercase entry also accepts capitalized and upper-case
  forms. User words override every rule. The file is not watched.
  "Ignore in this document" and the per-document switch are session-only
  and never saved. Reset defaults leaves the user dictionary alone.
- **Defaults:** on, both languages, ALL-CAPS words and words with digits
  skipped.
- **Crates:** `crates/mdoc-spell` knows nothing about Markdown or PII and owns
  word splitting, choosing the language by script, skip rules, dictionaries
  and the per-line cache. The host supplies spell exclusions (code, link
  targets, raw HTML, aliases and PII spans) from `mdoc-editor` and
  `mdoc-pii`. The mixed-script check still runs inside PII spans.

## Invariants

- Checking and suggestions never run on the UI thread. Results are cached
  per line by the line's text and exclusions; edits recheck after a 300 ms
  pause, and stale results are discarded by editor revision.
- The same text and settings give the same misspellings on every platform.
- Spellcheck never changes document text unless the user picks a suggestion.

## Consequences

The release binary grows by about 1.6 MB. Names inside PII spans aren't
dictionary-checked. Words split by hyphens across lines are flagged, not
repaired; repairs belong in the conversion forks.

References: [full text before the docs doctor](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0036-bundled-spellcheck.md),
[interview](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/spellcheck.md).
