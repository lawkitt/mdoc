# Mobile Markdown fixture

`review.md` exercises the Markdown spans `mdoc-ffi` gives the mobile apps
(`crates/mdoc-ffi/src/markdown.rs`): headings, emphasis, links, code, a
table, a quote, a list, a fenced block, a rule and a document colour, with
Cyrillic text and emoji so UTF-8 and UTF-16 offsets differ. The Rust unit
tests, the iOS styling tests and the review screenshots read it.

The people, addresses and numbers are invented: the email uses the reserved
example.com domain, the phone number is in Ofcom's drama range and the IBAN
is the standard documentation example.

`large-table.md` is a contract whose two-column table runs for screens,
with paragraphs after it: edit mode keeps the place in it, and a word
selected in a cell, without stopping mdoc (ADR 0114). Its text is invented;
its one email, the only name to review, uses the reserved example.com domain.
