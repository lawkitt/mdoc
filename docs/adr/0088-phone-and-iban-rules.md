# Phone and IBAN rules

Status: Accepted, 2026-10-31. Amends
[ADR 0017](0017-selected-rust-presidio-reuse.md) (extra categories) and
[ADR 0014](0014-highlighted-pii-replacements.md) (phone and bank rules
deferred); corrects [ADR 0085](0085-mobile-apps.md)'s list of rules.

## Problem

Phones below the mobile device floor scan with the structured rules alone
(`mdoc_detect::scan_rules`, ADR 0085). The rules are email, INN, OGRN/OGRNIP
and SNILS; there are no phone or IBAN rules, labelled or not. ADR 0085 and
the ADR 0087 setup card say otherwise, and the public fixtures' `07700 900461`,
`01632 960 418` and `GB82 WEST 1234 5698 7654 32` go unfound. ADR 0014 left
phone and bank rules out because the model already found every gold value on
desktop; it never weighed a tier without the model. Evidence is in the
[grill record](../design/rules-phones-ibans-grill-2026-10-31.md).

## Decision

Add a phone rule and an IBAN rule to `mdoc-detect`'s structured rules. They
run on every tier: full scans add the same detections as rules-only scans.

**IBAN** (`Recognizer::Iban`, Bank, score 0.95) counts with no label when it
is uppercase, starts with a country code from the IBAN registry, has that
country's exact length, is written as one block or in groups of four, and
passes mod-97. Reuse Presidio's IBAN pattern, narrowed to these forms, and
its `validate_iban`; mdoc owns the country-length table.

**Phone** (`Recognizer::Phone`, Phone, score 0.9) counts with no label in
three shapes, written with the usual separators (space, hyphen, dot,
brackets):

- international: `+` and a country code, then 7–14 more digits;
- UK national: a leading `0` in UK groupings (`07xxx xxxxxx`,
  `0xxxx xxxxxx`, `0xxx xxx xxxx`, `0xx xxxx xxxx`);
- Russian national: `8 (xxx) xxx-xx-xx` and its spaced forms.

After a phone label on the same line, ending within 8 separator characters
(`tel`, `phone`, `telephone`, `mobile`, `call`, `телефон`, `тел.`), any value
of 10–15 digits counts as well, including US national shapes and bare runs.
Bare digit runs and US national shapes never count without that label. The
phone patterns are mdoc's own, in a Presidio `PatternRecognizer` without a
validator; Presidio's `PhoneRecognizer` and libphonenumber stay unused.

Both rules reject a match that touches a digit or letter on either side, and
scan hidden Markdown source (link destinations, HTML attributes) as the email
rule does. Rule hits keep their precedence over model hits; overlapping rule
hits fall back to score order.

Russian account numbers and БИК stay out of scope (ROADMAP "Later").

## Consequences

- libphonenumber marks the Ofcom drama ranges used by every public fixture
  as invalid, and its 40-region list turned INNs, БИК values and contract
  numbers into phones in probes. Owned shapes find the fixtures and keep
  identifiers out; numbers in other countries' national formats are found
  only with `+CC` or a label.
- On desktop a rule false positive now beats a correct model hit, so the
  implementation PR is gated on: the rules alone find the three fixture
  values; no false positives on either qualification corpus; exact spans on
  gold PHONE/BANK values they fire on; a fixed negative list stays undetected
  (space-separated amounts, case and article numbers, dates, INN/OGRN/SNILS/
  БИК, UK company numbers, bad-checksum or wrong-length IBANs, the
  `202 555 0147` price); the frozen-corpus test gains PHONE and IBAN rows
  from this rule set; desktop full scans lose no true positive.
- OCR-damaged values (`+7 (495) 555-О1-47`) are not normalised; below the
  floor they are left to manual marking.
- Once the rules ship, the setup card and Settings say mdoc "still finds
  email addresses, phone numbers and IBANs, and tax and registration numbers
  next to their labels". If they miss iOS 0.1's TestFlight build, the card
  drops the phone and IBAN claim instead.
