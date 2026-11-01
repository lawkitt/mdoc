---
status: known-flaw
flaw: digit-false-hit
issue: 375
kind: contract
source: import
expect: false hit: BANK "12345678900"
---
IBAN category; ⟦BANK|GB28NWBK60161331926819⟧ appears as an unlabeled invalid example. Article 12345678900; `invoice_id=40702810500000009876`; case number ⟦CASE|7707083894⟧.
