# mdoc-spell

Offline English/Russian spellcheck for mdoc
([ADR 0036](../../docs/adr/0036-bundled-spellcheck.md)). Wraps
[spellbook](https://github.com/helix-editor/spellbook) (pure-Rust Hunspell) with
built-in en_US, en_GB and ru_RU dictionaries, so results are the same on every
platform. Knows nothing about Markdown or PII: the host passes excluded byte
ranges (`mdoc_editor::spell_exclusions` plus alias and entity spans).

- Language per word by script; English accepts US or UK spellings; Russian
  treats ё as е for lookup.
- Words mixing Latin and Cyrillic letters are always flagged, even inside
  entity spans, and suggest their single-script spelling first.
- Skipped: single letters, identifiers with `_`, bare URLs and e-mail
  addresses; ALL-CAPS and digit words unless enabled.
- `Accepted` adds the host's user dictionary and ignored words (exact
  spellings; lowercase entries also accept capitalized forms).
- `LineCache` reuses results for unchanged lines.

Dictionaries and the curated legal word lists live in
[dictionaries/](dictionaries/README.md). Fixture evidence:
`cargo run --release -p mdoc-spell --example fixture_report [-- --words]`.

GPL-3.0-or-later like the app; the dictionaries keep their own licenses.
