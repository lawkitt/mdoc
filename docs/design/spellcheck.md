# Spellcheck design

Status: interview complete, 2026-10-10. Accepted as
[ADR 0036](../adr/0036-bundled-spellcheck.md); this file is the interview record.

## Assessment of `os-spellcheck` (0.2.0)

Wraps `NSSpellChecker` (macOS) and `ISpellChecker` (Windows); no-op elsewhere.
Used only by the `mdoc-editor` demo.

Kept ideas: cheap `check` → byte ranges, lazy per-word `suggestions`; no gpui
dependency; feeds the editor's `Diagnostic` / `set_diagnostics` / `on_suggest`.

Gaps for mdoc: no configuration (language, ignore, skip rules); one language per
session (no mixed EN/RU); Linux unsupported; results vary with OS version and
installed dictionaries (not testable or reproducible); not Markdown-aware;
synchronous whole-document check on the main thread per edit.

Rust alternatives considered: `spellbook` (pure Rust, Hunspell dictionaries,
MPL-2.0, Helix), `zspell` (less mature), `hunspell-rs` (C++ FFI), `symspell`
(no morphology; weak for Russian), `harper-core` (English only), `enchant`
(Linux C FFI).

## Decisions

1. **Purpose: editing typos and conversion/OCR error spotting.** OCR and
   conversion errors in extracted text are the main reason to build it;
   ordinary typo squiggles come with it.
2. **Languages: English and Russian, mixed within a document.** The language
   for each word is chosen by its script (Cyrillic → ru, Latin → en), not by
   detecting the language of the whole document.
3. **Same results on every platform.** macOS, Windows and Linux flag the same
   words for the same text and settings. This rules out OS spell checkers as
   the main engine.
4. **Configurability, first version:** language selection in Settings, a global
   and per-document on/off switch, and skip rules (code, URLs, ALL-CAPS, words
   with digits, pseudonymization aliases, …). **Not included:** a persistent
   user dictionary, per-document "Ignore", importing custom word lists, and a
   choice of engine. *Revised after implementation:* Ignore and a user
   dictionary were added (see 24).
5. **Dictionaries are built into the binary** (en_US and ru_RU). They work
   offline with no first-use setup or download.
6. **Engine: `spellbook`** (pure Rust, MPL-2.0, Hunspell dictionaries). Before
   committing to it, run ru_RU against a Russian legal-text sample and check
   accuracy and load time. Fall back to `hunspell-rs` if it shows real gaps.
   spellbook is alpha (its API may break) and has no phonetic suggestions.
   Pin an exact version.
7. **Built-in legal word list** for EN and RU, shipped alongside the base
   dictionaries. Users can't edit it. It's the only way false positives are
   reduced besides the skip rules; there is no user dictionary and no Ignore.
8. **On by default globally. The per-document switch lasts only for the
   session**: nothing is written to the user's file or to per-path app state.
9. **Skip rules.** Always skipped: fenced and inline code, URLs, autolinks and
   link or image targets, raw HTML, pseudonymization aliases, e-mail
   addresses. **Skipped by default, each with its own switch in Settings:**
   ALL-CAPS words and words containing digits. ALL-CAPS contract headings can
   hide OCR errors, so some users will want them checked.
10. **How misspellings are shown:** squiggles with right-click suggestions, a
    misspelling count, and next/previous keyboard navigation. No sidebar list.
11. **Invariant: checking never runs on the UI thread.** Checking runs in the
    background block by block, with results cached by a hash of each block's
    text, so an edit rechecks only the blocks that changed. Suggestions run in
    the background when the user right-clicks. Numerical budgets wait for
    measured baselines.
12. **Delete `os-spellcheck` and add `crates/mdoc-spell`**, keeping the
    check-then-suggest split. The `mdoc-editor` demo moves to the new crate.
    The OS backends can be recovered from git if ever needed.
13. **Responsibilities.** `mdoc-spell` knows nothing about Markdown or PII. It
    takes text, excluded byte ranges and skip settings. It owns word
    splitting, choosing the language from the alphabet, the word-level skip
    rules (ALL-CAPS, digits, e-mail, bare URLs), the dictionaries and the
    cache. The host builds the structural exclusions (code, link targets,
    HTML, aliases) from the editor's Markdown parse and from `mdoc-pii`.
14. **English accepts US and UK spellings.** A word is flagged only if both
    en_US and en_GB reject it. Settings shows a single "English" language.
15. **Russian е/ё are the same letter for lookup.** The text is never changed.
16. **Words that mix Latin and Cyrillic letters are always flagged**, and the
    suggestions include the word written in a single alphabet. This applies
    only to words made of letters; tokens that contain digits or underscores
    are unaffected.
17. **The legal word list is a hand-curated plain-text file per language in
    the repo.** It is grown from false positives on `tests/fixtures` and
    reviewed in PRs. It holds real words only, never names.
18. **UI.** A Settings → Spelling section with a master switch, English and
    Russian checkboxes, and switches for checking ALL-CAPS words and words
    with digits. A compact indicator in the editor header shows the count,
    holds the per-document switch, and offers next/previous. Keyboard
    shortcuts for next/previous. The exact visuals get decided in
    implementation and recorded in the ADR.
19. **Words split by hyphens across lines are out of scope.** Both halves get
    flagged. Repairs belong in the conversion forks (ADR 0001).
20. **Text covered by detected or proposed PII entities is excluded** from
    dictionary checks, since the pseudonymization review already makes the
    user look at those names. The mixed-alphabet check (16) still runs inside
    entity spans.
21. **Dictionaries are built in compressed and loaded lazily in the
    background** the first time a document opens with spellcheck on. A
    language that is turned off is never loaded. Until loading finishes,
    nothing is flagged and the indicator shows a loading state. The size
    impact is measured, not assumed.
22. **Qualification.** The spellbook Russian check (6) is a go/no-go gate for
    the engine. Otherwise this follows the usual implementation and platform
    acceptance. A false-positive count on the fixtures is recorded in
    `docs/evidence/spellcheck/` as the regression baseline. There is no numeric
    accuracy gate.
23. **Roadmap: Next step #4**, after responsiveness, AI-handoff qualification
    and platform acceptance. It depends on the editor-layout baseline from
    step 1.
24. **Ignore and Add to dictionary (follow-up).** "Ignore in this document"
    skips every instance for the session and is never saved. "Add to
    dictionary" saves exact spellings to one word list in mdoc's data folder,
    used by all documents. Settings → Spelling → Manage… opens a searchable,
    virtualized list with Remove and Undo, and adds typed words.
    Russian word forms are not expanded; each form is added separately.

## Dictionary sources and licences

- en_US and en_GB: SCOWL size 60 Hunspell builds (LibreOffice). Permissive
  licence; keep the copyright and permission notices. The affix file has a
  BSD-style licence from Kuenning.
- ru_RU: Lebedev's Hunspell dictionary (LibreOffice). BSD-style licence.
  Modified versions must be marked as modified, and the author's name must not
  be used for endorsement. Implementation found it already UTF-8, so it ships
  unmodified.
- en_GB turned out to be LGPL (compatible with the GPL app).
- Notices: beside the dictionaries and in NOTICE.md (THIRD-PARTY-LICENSES is
  generated from Rust crates only).
