# Bundled spellcheck

Status: Accepted and implemented, 2026-10-10. Interview record:
[spellcheck design](../design/spellcheck.md). Evidence:
[spellcheck baseline](../evidence/spellcheck/README.md).

## Problem

Converted and OCR'd legal Markdown contains extraction errors that are easy to
miss before AI handoff. The retained `os-spellcheck` crate wraps the macOS and
Windows checkers. It has no configuration, uses one language per session, does
nothing on Linux, gives results that depend on the machine, ignores Markdown
structure, and checks the whole document on the main thread on every edit.
[ADR 0029](0029-codebase-cleanup-charter.md) kept it only as groundwork.

## Decision

- **Purpose**: surface conversion and OCR errors; ordinary typo squiggles come
  with it.
- **Engine**: `spellbook` (pure Rust, MPL-2.0, Hunspell dictionaries), pinned
  to an exact version. Gate: ru_RU must hold up on Russian legal text, judged
  on accuracy and load time; otherwise fall back to `hunspell-rs`.
- **Languages**: English and Russian, mixed in one document. The language for
  each word is chosen by its script. English accepts both US and UK
  spellings: a word is flagged only if en_US and en_GB both reject it. Russian
  treats е and ё as the same letter for lookup. **Mixed-script words** are
  always flagged, with single-script suggestions.
- **Dictionaries**: unmodified en_US (SCOWL), en_GB (LGPL) and ru_RU (Lebedev,
  already UTF-8) from LibreOffice, compressed into the binary at build time,
  plus a curated **legal word list** per language that users can't edit,
  loaded as a supplement in `.dic` line format. Loaded lazily in the
  background; a language that is turned off is never loaded. Offline only.
- **Configuration**: a Settings → Spelling section, after the model sections
  and their Advanced options, with a master switch (on by default), English
  and Russian checkboxes, and switches for checking ALL-CAPS words and words
  with digits (both skipped by default). The per-document
  switch lasts only for the session. There is no word-list import and no
  engine choice.
- **User words** (added after the first version): a flagged word's right-click
  menu offers **Ignore in this document** (every instance, session-only, never
  saved) and **Add to dictionary** (the **user dictionary**: exact spellings
  in `spelling-words.txt` beside `settings.json`, shared by all documents, at
  most 10,000 words of up to 64 characters). A lowercase entry also accepts
  capitalized and upper-case forms; other word forms must be added
  separately. Settings → Spelling shows a one-line summary with **Manage…**,
  which opens a dictionary view in the same dialog: a search field that
  filters as you type and offers to add the typed word (button or Enter), a
  virtualized alphabetical list with Remove on each row, and "Removed … ·
  Undo" for six seconds. Escape returns to Settings. Reset defaults leaves the
  dictionary alone. The word file is not watched, so edits made to it while
  mdoc runs are overwritten by the next change. User words override every rule, including
  the mixed-script check.
- **Exclusions**: the host supplies **spell exclusions** (code, link targets,
  raw HTML, aliases and detected or proposed PII entity spans) from the
  editor's Markdown parse and `mdoc-pii`. The mixed-script check still runs
  inside entity spans.
- **Crates**: `os-spellcheck` is deleted. A new `crates/mdoc-spell`, which
  knows nothing about Markdown or PII, owns word splitting, choosing the
  language by script, skip rules, dictionaries and the per-line cache. It
  keeps the check-then-suggest split. `mdoc-editor` provides
  `spell_exclusions` from its own Markdown scanner, and `on_suggest` returns a
  task so suggestions resolve after the menu opens.
- **UI**: squiggles with right-click suggestions (the menu shows "Finding
  suggestions…" until they arrive), plus a toolbar spelling icon (left of Pseudonymize) with a badge
  counting misspellings (99+ at most; no badge at zero, while loading or when
  the document isn't checked). Its menu has two items: "Check in this
  document" (the session-only switch) and "Turn off" / "Turn on" (the saved
  Settings master switch). When either is off the icon stays visible, faded
  and without a badge. Next/previous misspelling are keyboard-only: ⌘; / ⇧⌘;
  (Ctrl+; / Ctrl+Shift+;). The icon is hidden for source-only tabs and empty
  documents. No sidebar list.

## Invariants

- Checking and suggestions never run on the UI thread. The UI thread copies the
  text and PII ranges; parsing, exclusions and checking run in the background,
  with results cached per line (by the line's text and exclusions). Edits
  recheck after a 300 ms pause; stale results are discarded by editor revision.
- The same text and settings give the same misspellings on every platform.
- Spellcheck never changes document text unless the user picks a suggestion.

## Consequences

- The release binary grows by about 1.6 MB (1.24 MB of compressed dictionaries).
- Names inside PII entity spans aren't dictionary-checked, so OCR errors in
  names are caught only when they are mixed-script.
- Words split by hyphens across lines are flagged rather than repaired; repairs
  belong in the conversion forks ([ADR 0001](0001-conversion-and-ocr-in-dependency-forks.md)).
- Qualification follows the usual platform acceptance. A false-positive count
  on the fixtures is recorded in `docs/evidence/spellcheck/` as the regression
  baseline, not as a gate.
- Dictionary notices live beside the dictionaries and in NOTICE.md;
  THIRD-PARTY-LICENSES covers Rust crates only.
- Supersedes the `os-spellcheck` exception in ADR 0029.
