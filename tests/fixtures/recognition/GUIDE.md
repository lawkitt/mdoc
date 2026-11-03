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
   An individual entrepreneur's name is PERSON with «ИП» outside: «ИП
   ⟦PERSON|Мохов Л. Р.⟧» (#484 item 5). Lettered codes that stand for
   initials are PERSON, trimmed to the code: «⟦PERSON|М1⟧.» (#484 item 1).
   A person's nickname or handle is PERSON (#484 item 8).
2. **Organization** (`ORG`): a named company, firm or private body, with
   its legal form ("⟦ORG|Northbridge Legal Ltd⟧", "⟦ORG|ООО «Север»⟧").
   Courts, tribunals, ministries and other public bodies are not spans
   (a user adds one to Always). Defined terms ("the Company", "Общество")
   are not spans.
   The dot of Inc., Ltd., Corp., Co., N.A., Grp., Org., Ins., Ass'n and
   Ctr. stays outside the span, unlike a PERSON initial's dot: "⟦ORG|Sorrent,
   Inc⟧." (#506 item 1).
   A short form of a company's name is ORG, and so are a group name, a
   collective name for several companies, a ticker and a named trust or
   settlement: "⟦ORG|Chase⟧", "the ⟦ORG|Thorne⟧ group", "⟦ORG|BHLB⟧"
   (#506 item 11). A firm's short name that is a partner's surname is ORG
   (#506 item 10). Rule 2's defined terms are generic words only.
   A shop sign or other trading name of a business is ORG («магазин
   «⟦ORG|Кедрач⟧»»), and so is a TV call sign (#484 item 8, #506 item 13).
   A company named by its domain is ORG when the domain names the company
   (a legal form follows, or it is a verb's subject): "⟦ORG|LOANSQUARE.COM,
   INC⟧." (#553 item 6).
   Universities and colleges are ORG whoever owns them, a «ФГБОУ ВО …
   университет» included (#484 item 4).
   A department or committee of a private body is unmarked; the parent's
   name is the ORG: "⟦ORG|Harlow College⟧ Chemistry Department" (#506
   item 9).
3. **Address** (`ADDRESS`): a street address, from its first element (flat,
   house, street or index) to its postcode or city, whichever ends it:
   "⟦ADDRESS|Flat 3, 41 Larch Road, Bristol BS6 9ZY⟧",
   "⟦ADDRESS|197101, Санкт-Петербург, Ольховый переулок, д. 41, кв. 3⟧".
   A city or country alone is not an address.
   City, state and ZIP (or an index, region and city) with no street is
   an address when a ZIP or postcode is present: "⟦ADDRESS|Ashby Falls, OH
   45999⟧" (#506 item 6). A box address runs from "P.O. Box" or «а/я» to
   the postcode or city: "⟦ADDRESS|P.O. Box 4410, Ashby Falls, OH 45999⟧"
   (#506 item 7).
   A named private house is an address even alone ("⟦ADDRESS|Chalet Brume
   Haute⟧"); a named commercial property (a shopping centre) is unmarked
   alone and opens the span inside a full address (#553 item 2). Bare
   street names, alone or two at a corner, are unmarked (#553 item 3).
   A public body's or a court's own street address is an address like
   anyone's; the body's name stays outside (#553 item 1).
4. **Date** (`DATE`): only a date in a personal context: next to born, DOB,
   date of birth, дата рождения, д.р., род. The label is outside the span
   ("born ⟦DATE|14 March 1988⟧"). Every other date stays unmarked:
   signing, deadlines, hearings, statutes.
5. **Case number** (`CASE`): a claim or case number after a label (Claim No, Case
   No, Дело №, номер дела); the label is outside the span ("Claim No
   ⟦CASE|6001842/2026⟧"). Statute, article, section and contract-clause
   numbers stay unmarked.
   The document's own labelled case number is CASE wherever it repeats,
   label or not (a page header); a page or entry number beside it stays
   outside (#553 item 7). A related proceeding of the same parties is CASE
   ("Adv. Pro. No. ⟦CASE|31-01992⟧"); a docket number inside a precedent's
   citation is unmarked (#506 item 3). УИД, an administrative-offence
   protocol number and an appellate «05АП-…» number are CASE; a document
   GUID is unmarked (#484 item 7). An agency's charge number after "#" is
   CASE (#553 item 5). "Proof of Claim No. 5" is unmarked (#553 item 4).
6. **Identifiers** (`EMAIL`, `PHONE`, `IDENTITY`, `TAX`, `BANK`): the value
   only, without its label; an invalid or OCR-damaged value is still
   marked.
   ОГРН, ОГРНИП and ОКПО are TAX, like ИНН (#484 item 6). A public body's
   or a court's own phone, fax, e-mail and registration numbers are marked
   like anyone's, and its account number and БИК are BANK; КБК and ОКТМО
   are budget codes and stay unmarked (#553 item 1, #484 item 6).
   Reference numbers not assigned to a person stay unmarked: a vacancy, an
   NTSB ID, an EPA registration, a contract, a seed accession number (#506
   item 12), and document form numbers such as a vessel's ownership
   certificate or a notarial power of attorney's «64АА…» (#484 items 2
   and 3).
7. **Hidden source** (link targets, image alt text, HTML) is marked like
   visible text. A URL is marked by rule 9; a name or email inside a
   marked URL is not marked again.
8. **Never marked:** amounts, percentages, legal references, defined terms,
   roles, titles, job titles, generic category words ("email", "телефон"),
   laws, courts and public bodies, and web addresses of courts, public
   bodies, laws and legal databases (consultant.ru, garant.ru,
   pravo.gov.ru).
   Placeholders that stand for a party in a depersonalised text are never
   marked, whatever their form: «ФИО1», «ФИО 2», «ИП ФИО1», «Потерпевший
   №1», "John Doe", "Jane Does 1-25", "DOE SUPERVISORS I-X", "ROE
   ENTITIES" and a bare "I-X" after them; a real name or initials beside
   one are still marked, alone: «ИП ФИО3 ⟦PERSON|К-Л.М.⟧» (#484 item 1).
   A public body is one set up by statute and owned by the state to act
   for it, and a statutory regulator: a public-law company («ППК
   «Роскадастр»»), PBGC, Росатом, NHS trusts and their hospitals, ФГАУ,
   ФГКУ, ГУП, МУП, federal laboratories, a foreign state port authority, a
   bar chamber, a law society (#484 item 4). Only its name and its web
   address stay unmarked; its contact details and numbers are marked
   (rules 3 and 6, #553 item 1).
   A cited case's name is a legal reference, both sides and "In re"
   captions included, and so are its short form ("in Dias") and its facts
   discussed in prose; a party of the case at hand is marked wherever it
   appears, citations included (#506 item 2). The authors or editors of a
   cited work are unmarked inside a bibliographic citation; people named in
   prose stay PERSON (#506 item 4). A doctrine or equation named after a
   person ("the Miranda warnings") is unmarked (#506 item 5).
   Also never marked: deities (#506 item 8); events, venues, broadcasts,
   buildings named after a person ("Trump Tower") and clinic site codes
   (#506 item 14); app, product and platform names («Сбербанк Онлайн»,
   «МЭТС») (#484 item 8).
9. **Web address** (`URL`): a URL that points to a person's or a private
   organization's page or file (a profile, a personal or company site, a
   share link with a token, any URL that holds a name, email or ID), from
   its scheme (or `www.`, or its domain when written bare, "сайт
   ⟦URL|ivanov-law.ru⟧") to its end, without trailing punctuation. A
   Markdown link's target is its own span (`[the claim](⟦URL|https://…⟧)`).
   `mailto:` and `tel:` links stay EMAIL and PHONE (ADR 0094).

## Fixtures

Imported corpora are re-marked by these rules: the settlement demo's date
of birth and claim number become gold, so they start as `known-flaw`.

- Synthetic only: invented people, organisations and identifiers; email
  and URL domains under `.invalid`; no copied real document text. CC0-1.0.
- Front matter: `status` (`pass`, `known-flaw`, `model-limit`), `flaw`
  (class), `issue`, `kind` (contract, claim, judgment, power-of-attorney,
  letter, ocr, docx, tables), `source` (`hunt-rN`, `probe-rN` for a probe
  round's synthetic reproduction, `import`, `demo`), and for `model-limit`
  a `reason`.
- A negative fixture marks nothing; its job is precision.

## Examples

Agents add examples here, one line each, when a curator rejects or settles
a boundary case.

- An address with no postcode or city ends at its last element: "Адрес получателя: ⟦ADDRESS|ул. Тихих фонарей, д. 83, кв. 16⟧" (hunt-ru-r1-01).
- A Russian address that opens with its region keeps the region inside: "⟦ADDRESS|Тверская область, г. Ржев, ул. Тихих арок, д. 47, корп. 2, стр. 1, лит. Б, пом. 9⟧" (hunt-ru-r5-02).
- "оглы" belongs to the patronymic, inside the span: "⟦PERSON|Нияз Рауф оглы Иль-Булат⟧" (hunt-ru-r1-05).
- A name split by a line-end hyphen is one span across the break, hyphen and newline included: "⟦PERSON|Верес-\nтова Дарья Ильинична⟧" (hunt-ru-r1-04).
- A staff or employee ID assigned to one person is `IDENTITY`, letters only too: "Staff ID: ⟦IDENTITY|VELK-QUEN⟧" (hunt-en-r3-04).
- A surname-first name is one span, comma included: "⟦PERSON|VALECROFT, Jorin P.⟧" (hunt-en-r3-04), "⟦PERSON|Маркович, Савелий Игоревич⟧" (hunt-ru-r3-04).
- A generational suffix belongs to the name, inside the span: "⟦PERSON|Kaelen Renn III⟧" (hunt-en-r3-08).
- Post-nominal letters (KC, QC, LLB) are outside the span, like a title: "⟦PERSON|Gavira Vennick⟧ KC" (hunt-en-r8-01).
- A surname's lower-case particle is inside the span: "⟦PERSON|Генриетта Яновна де Лаваль⟧", "на фамилию ⟦PERSON|де Лаваль⟧" (hunt-ru-r2-01).
- A lower-case legal form opens the ORG span like «ООО»: "⟦ORG|адвокатское бюро «Валерия Конц»⟧" (hunt-ru-r6-05).
- Name parts in separate table columns are each their own span: "| ⟦PERSON|Кий⟧ | ⟦PERSON|Ева⟧ | ⟦PERSON|Оскаровна⟧ |" (hunt-ru-r2-09).
- Name parts in a passport's machine-readable zone are each a PERSON span, without the filler «<»: "P<RUS⟦PERSON|KRENC⟧<<⟦PERSON|LIDIIA⟧<⟦PERSON|ARSENEVNA⟧<<<" (hunt-ru-r7-05).
- An identifier wrapped across an OCR line is one span, newline included: "Эл. почта: ⟦EMAIL|ksen.rav@\nsvod.invalid⟧" (hunt-ru-r2-03).
- A phone extension belongs to the number, inside the span: "⟦PHONE|+1 (202) 555-0164 x207⟧" (hunt-en-r5-07).
- The title of a work is not a span, even when it reads like a name: «Кедровый свет» (hunt-ru-r3-07).
- An invisible character inside a name or value (soft hyphen, zero-width space, combining mark) stays inside the span (hunt-ru-r3-05, hunt-ru-r3-08, hunt-ru-r3-11).
- A writ's series and number is a court document reference and stays unmarked: «Серия и номер исполнительного листа: ФС 086731924» (hunt-ru-r4-12).
- A notary is a person, not a public body: «Нотариус: ⟦PERSON|Лидия Игоревна Брюн⟧» (hunt-ru-r4-01).
- A case name's "& Anor" or "& Ors" stays outside the party, and a declaration names its declarant: "the claim by ⟦ORG|Pellow Holdings Ltd⟧ & Anor", "(⟦PERSON|Ostrand⟧ Decl. ¶ 3)" (probe-en-r1-legal-citations).
- A form's, a standard's, a classifier's, a waybill's and a patent's numbers are no identifiers, and neither is a toll-free prefix alone: «Форма № Р11001», «СП 1.23456.2030», «ОКВЭД 47.11», «ТН № 47», "Patent US 99/123,456", "NAICS 999999", 'the toll-free "800" number' stay unmarked; a whole toll-free number is a phone: "⟦PHONE|8-800-555-35-35⟧" (probe-ru-r1-form-numbers, probe-en-r1-patent-schedule).
- A citation's numbers are a legal reference, never an identifier: "26 U.S.C. § 6212", "2031 WL 1234567", "Tr. 112-14", "tax year 2029/30", "№ 999-ФЗ", "статьи 431 ГК РФ" stay unmarked (probe-en-r1-citation-tax, probe-ru-r1-law-numbers).
- A cadastral number or a right's registration number pins down one property, so it is `IDENTITY`, the value only: "кадастровый номер ⟦IDENTITY|99:07:0012345:678⟧", "номер регистрации права ⟦IDENTITY|99:07:0012345:678-99/011/2031-4⟧" (the maintainer's decision on #484, item 2).
- A certificate or register number assigned to one person is `IDENTITY` (rule 6), the value only: an attorney's, an arbitration manager's or a notary's register or certificate number, an attorney's bar number: "регистрационный номер в сводном реестре ⟦IDENTITY|99887⟧", "удостоверение адвоката № ⟦IDENTITY|9917⟧", "Ohio State Bar No. ⟦IDENTITY|0099911⟧", "SBN ⟦IDENTITY|299911⟧" (the maintainer's decision on #484, item 3; #483).
