# Recognition correction pipeline

Status: Accepted, 2026-11-01. Amends
[ADR 0002](0002-reviewable-local-pseudonymization.md) (ordinary dates stay),
[ADR 0068](0068-short-and-inflected-name-variants.md) (proposed, never
linked; first names) and [ADR 0080](0080-merge-hint.md) (initials).
How flaws are found and fixed: [ADR 0093](0093-flaw-hunt-loop.md).

## Problem

The pinned GLiNER2 model misses what no threshold recovers: neither demo's
home address at any score, no date-of-birth label, a claim number only as a
garbled 0.14 address span, "Марина" alone at 0.29 (#301). The corrections
mdoc has are hardcoded lists in `mdoc-detect` (`TITLES_AND_ROLES`,
`DETERMINERS`, `RU_ROLE_STEMS`, `LEGAL_FORMS`), and a short form of a name
gets an alias of its own: the Russian demo gives one person six (U27).
Fine-tuning needs training data, a new pinned export and requalification.

## Decision

- **A correction pipeline after the model, in layers.** At scan time in
  `mdoc-detect`: model → `never` and `shape` filters → `context` rules →
  structured identifiers (email, INN, SNILS, OGRN) → `span` fixes → the
  user's lists. After a decision, in `mdoc-pii`: propagation. Each
  detection keeps its recognizer (`Model`, a rule, `Propagation`,
  `UserList`). The pipeline targets devices that run the model; the
  no-model path (`scan_rules`) is not a target.
- **The rule pack is data compiled in.** TOML in
  `crates/mdoc-detect/rules/{shared,en,ru}.toml`, read with `include_str!`
  and validated by a unit test, each entry commented. Sections:
  - `never`: titles, roles, determiners, category words, defined terms;
    never a name on their own.
  - `context`: a label proposes a category for the value beside it.
  - `shape`: what a span of each category must look like (today's
    `identifying`).
  - `span`: boundary fixes (trim a title, extend an address to its
    postcode).
  - `variants`: nicknames for given names (Марина → Мариша; Elizabeth →
    Liz), used only for names already in the document.

  `guess_category`'s legal forms and date and name shapes move in, so
  manual marks and the model share one source.
- **Dates only in a personal context.** A date next to born, DOB, date of
  birth, дата рождения, д.р., род. is proposed as Date; other dates stay.
  No date shifting.
- **Case number: a new category** (`Category::CaseNumber`, token `CASE`,
  aliases `CASE_1`; named so it never reads as a saved case). Claim and
  case numbers in a labelled context only (Claim No, Case No, Дело №,
  номер дела), so statute and article numbers stay. It restores like any alias.
- **Addresses: anchor and extend.** Anchors: a postcode shape (UK, US ZIP,
  RU six-digit index) or a street-type word with a house number (Road,
  Street, Flat; ул., проспект, переулок, д., кв., офис). The span extends
  to the clause or line boundary.
- **Settings and the popup.** Settings › Detection › Categories to detect
  gains "Dates of birth" and "Case numbers", both on by default; the
  popup's category menu has Date and Case number; Case number takes the
  next category colour.
- **Propagation proposes, linked when the candidate is unique.** When a
  full name is applied or marked, its forms elsewhere in the document
  (surname alone, first name alone, first name + patronymic, surname with
  initials, ADR 0068's inflections, the same for first names, `variants`
  nicknames) become proposals with recognizer `Propagation`. A form with
  exactly one candidate full name joins that identity: the user's Apply or
  mark of the full name is the linking action. A form with two or more
  candidates keeps ADR 0068's own identity and ADR 0080's hint. The popup
  can unlink one mention. A form that is also a common word ("Rose",
  "Мира") is proposed only capitalised mid-sentence.
- **Link signals (U27)**, unique candidate only: first name + patronymic;
  surname with every initial matching. An initial that matches a different
  given name breaks the link.
- **The user's lists.** Keep on an identity adds its wordings to the
  case's Never list (undoing the Keep removes them); a manual mark adds
  its wording to Always. Matching is whole-word, ignoring case and
  whitespace; a person's Always entry also yields propagation's forms. The
  case page shows both lists (remove only); Settings › Detection has
  user-wide Always and Never lists (add, remove). A document outside a
  case uses the user-wide lists only; in a case, the case's lists win over
  the user-wide ones. A wording is on one list at a time: the later
  addition moves it. A Keep lists an identity's wordings once none of its
  mentions is still proposed or replaced. Both live in the
  keychain-protected store, since they can hold client names.
- **Preprocessing is pipeline code**: windowing, table cells read as
  sentences, OCR hyphen joins, HTML entities decoded before detection and
  readiness (`Ann&#97; Petrova`). The threshold and the model stay as
  pinned.

## Consequences

- ADR 0002's "ordinary dates stay" holds for every date outside a personal
  context; the detector's never-Date rule ends.
- ADR 0068's short forms join their name when the candidate is unique, so
  the AI sees one person; relatives sharing a surname are as safe as
  before. ADR 0068's "a first name alone is never proposed" and ADR 0080's
  "not signals: initials" are replaced as above.
- Rules can add false hits; the negative fixtures of ADR 0093 hold
  precision.
- A miss no layer can fix stays a known model limit, listed for the
  maintainer.

References: [grill](../design/recognition-grill-2026-11-01.md),
[roadmap grill](../design/roadmap-grill-2026-11-01.md), #301.
