# Annotation guide

What a recognition fixture marks, and where each span starts and ends
([ADR 0092](../../../docs/adr/0092-recognition-correction-pipeline.md),
[ADR 0093](../../../docs/adr/0093-flaw-hunt-loop.md)). Gold is
`⟦CATEGORY|text⟧`; anything unmarked must not be proposed. The rules are
the maintainer's: agents add examples, never change a rule (a change is a
`decision` issue).

## Rules

1. **Person** (`PERSON`): a name, or any part or form of one, of a real or
   fictional individual. Each mention is its own span: "Павловой",
   "Ms Bennett's" (span "Bennett"), "М. С.", "Мариша". A title, role or
   determiner is never inside the span ("Ms ⟦PERSON|Bennett⟧", "the
   Employer", "Заказчик") and never a span on its own.
2. **Organization** (`ORG`): a named company, firm or private body, with
   its legal form ("⟦ORG|Northbridge Legal Ltd⟧", "⟦ORG|ООО «Север»⟧").
   Courts, tribunals, ministries and other public bodies are not spans
   (a user adds one to Always). Defined terms ("the Company", "Общество")
   are not spans.
3. **Address** (`ADDRESS`): a street address, from its first element (flat,
   house, street or index) to its postcode or city, whichever ends it:
   "⟦ADDRESS|Flat 3, 41 Larch Road, Bristol BS6 9ZY⟧",
   "⟦ADDRESS|197101, Санкт-Петербург, Ольховый переулок, д. 41, кв. 3⟧".
   A city or country alone is not an address.
4. **Date** (`DATE`): only a date in a personal context: next to born, DOB,
   date of birth, дата рождения, д.р., род. The label is outside the span
   ("born ⟦DATE|14 March 1988⟧"). Every other date stays unmarked:
   signing, deadlines, hearings, statutes.
5. **Case number** (`CASE`): a claim or case number after a label (Claim No, Case
   No, Дело №, номер дела); the label is outside the span ("Claim No
   ⟦CASE|6001842/2026⟧"). Statute, article, section and contract-clause
   numbers stay unmarked.
6. **Identifiers** (`EMAIL`, `PHONE`, `IDENTITY`, `TAX`, `BANK`): the value
   only, without its label; an invalid or OCR-damaged value is still
   marked.
7. **Hidden source** (link targets, image alt text, HTML) is marked like
   visible text.
8. **Never marked:** amounts, percentages, legal references, defined terms,
   roles, titles, job titles, generic category words ("email", "телефон"),
   laws, courts and public bodies.

## Fixtures

Imported corpora are re-marked by these rules: the settlement demo's date
of birth and claim number become gold, so they start as `known-flaw`.

- Synthetic only: invented people, organisations and identifiers; email
  and URL domains under `.invalid`; no copied real document text. CC0-1.0.
- Front matter: `status` (`pass`, `known-flaw`, `model-limit`), `flaw`
  (class), `issue`, `kind` (contract, claim, judgment, power-of-attorney,
  letter, ocr, docx, tables), `source` (`hunt-rN`, `import`, `demo`), and
  for `model-limit` a `reason`.
- A negative fixture marks nothing; its job is precision.

## Examples

Agents add examples here, one line each, when a curator rejects or settles
a boundary case.

- An address with no postcode or city ends at its last element: "Адрес получателя: ⟦ADDRESS|ул. Тихих фонарей, д. 83, кв. 16⟧" (hunt-ru-r1-01).
- "оглы" belongs to the patronymic, inside the span: "⟦PERSON|Нияз Рауф оглы Иль-Булат⟧" (hunt-ru-r1-05).
- A name split by a line-end hyphen is one span across the break, hyphen and newline included: "⟦PERSON|Верес-\nтова Дарья Ильинична⟧" (hunt-ru-r1-04).
- A staff or employee ID assigned to one person is `IDENTITY`, letters only too: "Staff ID: ⟦IDENTITY|VELK-QUEN⟧" (hunt-en-r3-04).
- A surname-first name is one span, comma included: "⟦PERSON|VALECROFT, Jorin P.⟧" (hunt-en-r3-04).
- A generational suffix belongs to the name, inside the span: "⟦PERSON|Kaelen Renn III⟧" (hunt-en-r3-08).
- A surname's lower-case particle is inside the span: "⟦PERSON|Генриетта Яновна де Лаваль⟧", "на фамилию ⟦PERSON|де Лаваль⟧" (hunt-ru-r2-01).
- Name parts in separate table columns are each their own span: "| ⟦PERSON|Кий⟧ | ⟦PERSON|Ева⟧ | ⟦PERSON|Оскаровна⟧ |" (hunt-ru-r2-09).
- An identifier wrapped across an OCR line is one span, newline included: "Эл. почта: ⟦EMAIL|ksen.rav@\nsvod.invalid⟧" (hunt-ru-r2-03).
- Post-nominal letters (KC, QC, LLB) are outside the span, like a title: "⟦PERSON|Gavira Vennick⟧ KC" (hunt-en-r8-01).
