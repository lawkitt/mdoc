# Rules-only phones and IBANs grill, 2026-10-31

Interview record for ROADMAP item "[Detect] Rules-only phones and IBANs"
(iOS 0.1, from the ADR 0087 UI survey). Decisions land in an ADR once
confirmed; this file keeps the research and the design tree.

## Research facts

### There are no phone or IBAN rules today

The roadmap item says the rules "need a label word nearby". They do not
exist at all. `crates/mdoc-detect/src/structured.rs` has four rules:

| Rule | Category | Validator | Needs a label |
| --- | --- | --- | --- |
| Email (Unicode pattern) | EMAIL | none | no |
| INN | TAX | local checksum | `инн`, `inn`, `tax id` |
| OGRN/OGRNIP | IDENTITY | mod 11/13 | `огрн`, `огрнип`, `ogrn`, `ogrnip` |
| SNILS | IDENTITY | Presidio checksum, not all zeros | `снилс`, `snils` |

A label must end within 8 separator characters before the value, on the same
line (`context`), and the value must not touch other digits (`boundary`).
`mdoc_pii::Recognizer` has only `Model`, `Email`, `Inn`, `Snils`, `Ogrn`.
Phones and IBANs come only from the GLiNER2 model (`phone_number`, `iban`,
`bank_account`, `routing_number` labels), so below the detector floor they
are never found, labelled or not. `call 01632 960 418` and
`Pay to GB82 WEST …` in `review.md` both have label-like words and still
were not found.

So two shipped claims are wrong today:

- ADR 0085 and the `scan_rules` doc comment list "emails, phones, IBAN,
  INN/SNILS and the rest" as the structured rules.
- The ADR 0087 setup card and Settings text (PRs #285–#287) say phone
  numbers and IBANs are found "next to their labels".

### Why phones and IBANs were left out

The hybrid research of 2026-10-07 (`tests/fixtures/pseudonymization/hybrid/`)
prototyped five rule families outside the app: email, phone (a `+` prefix or
a nearby label, 10–15 digits), INN/SNILS (label and checksum), Russian
account/BIC (format and context), IBAN (seven-country length list and
mod-97). Q15 of that grill selected email and INN/SNILS only. ADR 0014: phone
and bank rules "added no exact hits" on the synthetic corpus, because the
model already found every gold phone and IBAN. The reason was desktop
redundancy, not precision. Nobody weighed a tier without the model.

On that corpus (PHONE and BANK only, true positives / false positives /
misses):

| Pipeline | PHONE | BANK |
| --- | --- | --- |
| Model | 13 / 10 / 0 | 14 / 10 / 0 |
| Prototype rules alone | 10 / 0 / 3 | 11 / 0 / 3 |
| Rules first, then model | 13 / 6 / 0 | 14 / 9 / 0 |

The prototype rules' misses are OCR-damaged values (`+1 202 555 O147`) and
the Russian account/BIC. The model's false positives are numbers it calls
phones or accounts: SNILS `123-456-789 64`, INN `7701234567`, BIC
`044525593`, case numbers `2026/314`, `А40-123456/2026`, amounts `12345.67`.

Structured rule hits win conflicts with model hits (`Review::ingest` sorts
rules first). A phone rule that matched an INN would therefore override the
model on desktop too, and two rules on one span fall back to score order.

### Probes with the libraries already in the tree

`presidio-analyzer` 0.1.11 ships `predefined::iban()` (generic pattern,
case-insensitive, mod-97 validator, context words) and a `PhoneRecognizer`
on `phonenumber-rs` 0.1.1 (libphonenumber metadata, 40 default regions,
accepts any valid `+CC` number or bare digit run, and separated national
numbers whose digit grouping matches the region's national format). Both are
in `Cargo.lock` already; mdoc uses neither. Probe code in the session
scratchpad, run over both qualification corpora and the public fixtures:

- **IBAN, unlabelled, mod-97**: found every gold IBAN (GB, DE, NL) and
  `GB82 WEST 1234 5698 7654 32` in `review.md`; no false positives, and it
  declined the invalid-checksum negative `GB28NWBK60161331926819`.
- **Phone, Presidio as shipped (40 regions)**: found 10 of 12 test phones
  but flagged INN `7701234567`, BIC `044525225`, contract number
  `5401000039`, company number `01234567`, `12345678900`, account
  `31926819`, and the corpus negatives `цена 202 555 0147 рублей` and
  `case 202-555-0169`. Bare digit runs cause most of it.
- **Requiring a separator or `+`**: left only the two deliberate negatives
  (a US-shaped price and case number).
- **libphonenumber rejects the fixtures' numbers by design.** `07700 900461`
  and `01632 960 418` are Ofcom drama ranges, which libphonenumber marks
  invalid for GB (`+44 7700 900461` fails too). Presidio matched
  `07700 900461` only because it happens to be valid in DE, IN, CN and ID.
  `555-01xx` US numbers, also fictional, do validate.
- **"Possible number" (length only) is too loose**: `1 150 000`,
  `1 725 000` and `123 4567` are possible GB numbers.

## Design tree

### Round 1 (confirmed: all recommendations accepted)

- **Q1 IBAN validator → unlabelled, strict.** An IBAN counts with no label
  when it is uppercase, starts with a country code from the IBAN registry,
  has that country's exact length, is written as one block or in groups of
  four, and passes mod-97. Mod-97 alone passes about 1 in 97 random
  letter-digit codes; the registry length removes nearly all of those.
- **Q2 Phone shapes → mdoc-owned shapes, no libphonenumber.** Unlabelled
  phones count in three shapes: international `+CC` followed by 7–14 more
  digits with the usual separators; UK national with a leading `0` in UK
  groupings (`07xxx xxxxxx`, `0xxxx xxxxxx`, `0xxx xxx xxxx`,
  `0xx xxxx xxxx`); Russian national `8 (xxx) xxx-xx-xx` and spaced forms.
  Never bare digit runs, never US national shapes alone. libphonenumber
  rejects the Ofcom drama ranges every public fixture uses, and its region
  list is what turned identifiers into phones.
- **Q3 Tiers → all tiers.** One rule set; `scan_rules` stays the subset of a
  full scan (ADR 0085). Rules fix model boundaries and categories; the
  false-positive gate (round 2) guards desktop, where a rule hit now beats a
  model hit.
- **Q4 Scope → phones and IBANs only.** Russian account numbers and БИК
  get a ROADMAP "Later" line (labelled rules; the account's check digit is
  keyed on the БИК).

### Round 2 (confirmed: all recommendations accepted)

- **Q5 Labelled phones → wider.** After a phone label (`tel`, `phone`,
  `telephone`, `mobile`, `call`, `телефон`, `тел.`; same line, within 8
  separator characters, as the other rules) any 10–15-digit value counts,
  including US national shapes and bare runs. `phone: 42` still declines.
- **Q6 Edges and hidden source.** Matches must not touch a digit or letter;
  hidden Markdown source (link destinations, HTML attributes) is scanned as
  for email.
- **Q7 Evidence.** `Recognizer::Phone` (score 0.9) and `Recognizer::Iban`
  (0.95). Bare runs being excluded, phones cannot share a span with INN,
  OGRN or SNILS; any rule overlap falls back to score order.
- **Q8 Wording.** ADR 0088 corrects ADR 0085 and amends ADRs 0014 and 0017.
  The `scan_rules` doc comment changes with the code. Once shipped, the card
  reads "mdoc still finds email addresses, phone numbers and IBANs, and tax
  and registration numbers next to their labels"; if the rules miss iOS
  0.1's TestFlight build, the card drops the phone and IBAN claim.
- **Q9 Gate for the implementation PR.** The fixture values found by rules
  alone; no false positives on either corpus; exact spans on gold
  PHONE/BANK; the fixed negative list stays undetected; frozen-corpus test
  gains PHONE and IBAN rows from this rule set; desktop full scans lose no
  true positive.
- **Q10 Reuse.** Presidio's IBAN pattern (narrowed) and `validate_iban`
  plus an mdoc country-length table; own phone patterns in a
  `PatternRecognizer`; `PhoneRecognizer` and `phonenumber-rs` stay unused.

### Round 3 (confirmed)

- **Q11 OCR-damaged values → no normalisation.** The rules do not map OCR
  look-alikes (`О`/`O` → `0`, `I`/`l` → `1`) before matching. Below the
  floor damaged values are left to manual marking; on desktop the model
  still finds most of them.

The maintainer confirmed shared understanding on 2026-10-31; the frontier is
empty. Implementation is a separate step, gated as in Q9.
