# Spellcheck evidence (2026-10-10)

Baseline for [ADR 0036](../../adr/0036-bundled-spellcheck.md). A regression
measure, not a qualification gate. Apple Silicon macOS, release build,
spellbook 0.4.2, dictionaries as pinned in
[crates/mdoc-spell/dictionaries](../../../crates/mdoc-spell/dictionaries/README.md).

## Engine gate: spellbook with ru_RU

Passed. On the Russian OCR fixtures' ground-truth transcripts spellbook flagged
about 1.5% of words before the legal word list was added, mostly single-letter
abbreviation parts (`г.`, `т.е.`, `п.`, `ч.`, now skipped) and real legal words
missing from ru_RU (`возмездного`, `неполученные`, now in the legal list). On the
OCR output it caught the recognition errors (`иили`, `обстотельства`,
`Федерашии`, merged words such as `настощегодоговора`) and suggested the
intended words (`иили` → `или`). Loading all three dictionaries takes about
65–70 ms; a Russian suggestion takes about 50 ms, which is why suggestions run
off the UI thread.

## Fixture flags

`cargo run --release -p mdoc-spell --example fixture_report`, no exclusions
(names are not excluded here; in the app, detected entities are).

| Fixture | Words | Ground-truth flags | OCR-output flags |
|---|---:|---:|---:|
| en-doclaynet-financial-12 | 435 | 14 | 13 |
| en-doclaynet-financial-9 | 208 | 8 | 6 |
| en-doclaynet-law-34 | 575 | 43 | 43 |
| en-doclaynet-law-4 | 491 | 9 | 3 |
| en-supreme-digest-1 | 1358 | 9 | 57 |
| en-supreme-digest-10 | 1294 | 8 | 55 |
| ru-apartment-handover | 401 | 4 | 23 |
| ru-confidentiality | 331 | 0 | 3 |
| ru-education-contract | 617 | 0 | 33 |
| ru-employment-duties | 413 | 0 | 0 |
| ru-franchise-termination | 319 | 0 | 3 |
| ru-goods-quality | 634 | 1 | 27 |
| **Total** | 7076 | 96 | 266 |

The remaining ground-truth flags are almost all proper names (Setswana place
names in `law-34`, company and person names) and words hyphenated across lines
in the source transcripts (`tremen dous`). Mixed-script words such as `іi`
(Cyrillic і) are flagged in OCR output only.

## Size

Compressed dictionaries add 1.24 MB of data; the release binary grew from
30.32 MB to 31.92 MB.

## Native check

A temporary macOS app bundle with a sample EN/RU document showed squiggles,
the badge count, the icon menu and its per-document switch, right-click
suggestions applied from the background lookup, and the Settings → Spelling
section in the dark theme. "Turn off" was checked headlessly only, since it
saves the real settings file. Not checked: light theme, Windows, Linux, IME,
screen readers.
