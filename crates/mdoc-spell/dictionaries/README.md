# Bundled dictionaries

Hunspell dictionaries compiled into mdoc (compressed by `build.rs`). Unmodified
copies from [LibreOffice/dictionaries](https://github.com/LibreOffice/dictionaries)
at commit `32b006a2c22a4ac7e8ed3f03346f7b3d85a970a4` (2026-08-21).

| Files | Source path | License | Notice |
|---|---|---|---|
| `en_US.aff`, `en_US.dic` | `en/` | SCOWL (MIT-like) word list; affix file under Kuenning's BSD-style terms | [README_en_US.txt](README_en_US.txt) |
| `en_GB.aff`, `en_GB.dic` | `en/` | LGPL (Bartlett, Kelk, Brown, Pinto; from Atkinson's word list) | [README_en_GB.txt](README_en_GB.txt) |
| `ru_RU.aff`, `ru_RU.dic` | `ru_RU/` | BSD-style, © 1997–2008 Alexander I. Lebedev | [README_ru_RU.txt](README_ru_RU.txt) |

SHA-256:

```text
e746c882dd6f303c2c46e7452804b9201115a6942cfeb15f18f8edf774d2e24e  en_US.aff
f0b1a234bd178bdd01875b2a392a9647f888b8fe879f79c52aae62c2759b3647  en_US.dic
0fd6ed120ef28957847d98ba5149b117e27116cf81b5aa36208453f6755a36fd  en_GB.aff
04e90f34f5263bf26780e9c4a442e9ad16584e227af49ddd1b3b21b01df5b29c  en_GB.dic
38ce7d4af78e211e9bafe4bf7e3d6a2c420591136cb738ec6648f8fdf6524cd7  ru_RU.aff
f6047416a0204adbecf3a451b874ec8a97ee37e2cbc714466ef04d8dbcc0d6fc  ru_RU.dic
```

The files are kept byte-for-byte; ru_RU is already UTF-8 upstream. mdoc's
additions are separate supplements loaded on top, not edits to these files:

- [legal-en.dic](legal-en.dic): legal vocabulary in `.dic` line format with
  en_US affix flags (`/SM` = plural + possessive). Added to en_US only; a word
  passes if en_US or en_GB accepts it.
- [legal-ru.dic](legal-ru.dic): the same for ru_RU, with flags copied from a
  dictionary word that declines the same way (e.g. `возмездный/AS` like
  `безвозмездный/AS`).

Add only real words, never names, and only after the
[fixture report](../examples/fixture_report.rs) shows them flagged.
