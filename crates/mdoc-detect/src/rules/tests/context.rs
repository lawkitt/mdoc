//! Labelled values: case numbers, identifiers, dates of birth and register numbers.
use super::*;
use crate::rules::test_support::{cases, pairs, proposed, shown};

/// Claim and case numbers after their label, the value only (ADR 0092);
/// statute and article numbers and "case" as a common word stay out.
#[test]
fn labelled_case_numbers_are_proposed() {
    let found = |source: &str| -> Vec<(String, Category)> {
        pack()
            .context(source)
            .into_iter()
            .map(|d| (source[d.range].to_owned(), d.category))
            .collect()
    };
    for (source, value) in [
        ("**Claim number:** 6001842/2026 (Employment", "6001842/2026"),
        ("Claim No. 1234 was settled", "1234"),
        ("Case No. 1:26-cv-01234-JAB, filed", "1:26-cv-01234-JAB"),
        ("Claim No CL-2026-000123.", "CL-2026-000123"),
        ("Case No. 2026/314A; see", "2026/314A"),
        ("Дело № 1234", "1234"),
        ("**Дело №** 2-1842/2026", "2-1842/2026"),
        ("proceedings are numbered 2026/314.", "2026/314"),
        ("case number 7707083894.", "7707083894"),
        ("Formula x + 2; case 202-555-0169.", "202-555-0169"),
        ("**Номер дела:** 2-1842/2026", "2-1842/2026"),
        ("дело № А40-123456/2026.", "А40-123456/2026"),
        ("Article 309; дело А40-987654/2026; цена", "А40-987654/2026"),
    ] {
        let found = found(source);
        assert!(!found.is_empty(), "{source}");
        for (text, category) in found {
            assert_eq!((text.as_str(), category), (value, Category::CaseNumber));
        }
    }
    for source in [
        "Under section 4.2 and article 309, the claim above is settled.",
        "in case 30 days pass",
        "in this case. 12345 people",
        "case abc12345",
        "по делу 15 свидетелей",
        "showcase 2026/314",
        "Claim No. 12. Under article 12 and section 12, payment is due.",
        "In case 2026-03-12 is a non-business day, payment falls due.",
        "Case No. 12.03.2026",
        "Дело № 12",
        "необходимо архивное дело 58239 из фонда",
        "личное дело № 4521 работника",
        "Case No. 1234_5678. See section 1234.",
    ] {
        assert!(found(source).is_empty(), "{source}");
    }
}

/// Russian identifiers take their label's category whatever their
/// checksum, beside the label or under it as a table column's header.
#[test]
fn labelled_russian_identifiers_take_their_category() {
    let found = |source: &str| -> Vec<(String, Category)> {
        let mut found: Vec<_> = pack()
            .context(source)
            .into_iter()
            .map(|d| (source[d.range].to_owned(), d.category))
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    };
    let tax = |v: &str| (v.to_owned(), Category::Tax);
    let id = |v: &str| (v.to_owned(), Category::Identity);
    assert_eq!(
        found("ИНН: 7701234567; СНИЛС: 123-456-789 00; ИНН: 77O1234567."),
        [id("123-456-789 00"), tax("7701234567"), tax("77O1234567")]
    );
    assert_eq!(found("**СНИЛС:** 000-000-000 00"), [id("000-000-000 00")]);
    assert_eq!(
        found("Код карточки: CARD-918264<br>Счёт"),
        [id("CARD-918264")]
    );
    assert_eq!(
        found(
            "| ИНН | **СНИЛС** | Цена |\n| --- | :-: | --- |\n| 5401000039 | 345-678-912 26 | 1234567890 |\n| нет | — | 12 |\n"
        ),
        [id("345-678-912 26"), tax("5401000039")]
    );
    for source in [
        "Договор № 5401000039, цена 660100000231 рублей.",
        "ИНН: 123; СНИЛС: 12-34",
        "| Номер | Цена |\n| --- | --- |\n| 5401000039 | 345-678-912 26 |\n",
        "ИНН\n\n| 5401000039 |",
        // An adjacent table's columns are its own.
        "| ИНН |\n| --- |\n| 7701234567 |\n| Цена |\n| --- |\n| 1234567890 |\n",
    ] {
        let found = found(source);
        assert!(
            found
                .iter()
                .all(|(v, _)| v != "1234567890" && v != "5401000039"),
            "{source}: {found:?}"
        );
    }
    // No trailing pipe: the last cell still counts.
    assert_eq!(
        found("| Заказчик | ИНН\n| --- | ---\n| ООО | 7701234567\n"),
        [tax("7701234567")]
    );
    for source in [
        "Договор № 5401000039, цена 660100000231 рублей.",
        "ИНН: 123; СНИЛС: 12-34",
    ] {
        assert!(found(source).is_empty(), "{source}");
    }
}

/// Issue #381: a Russian identifier after its label is proposed with
/// the label's category whatever its checksum, a digit short or long
/// or with an OCR letter for a digit (guide rule 6: an invalid or
/// damaged value is still marked). Valid values stay as they were.
#[test]
fn labelled_invalid_russian_identifiers_are_proposed() {
    for (source, value, category) in [
        // A failing checksum.
        ("СНИЛС: 123-456-789 00.", "123-456-789 00", "IDENTITY"),
        ("СНИЛС 12345678900", "12345678900", "IDENTITY"),
        ("ИНН: 7701234567.", "7701234567", "TAX"),
        ("ОГРН: 1027700132190", "1027700132190", "TAX"),
        // Valid ones, unchanged.
        ("СНИЛС: 112-233-445 95.", "112-233-445 95", "IDENTITY"),
        ("ИНН: 7707083893.", "7707083893", "TAX"),
        ("ИНН: 500100732259", "500100732259", "TAX"),
        ("ОГРН: 1027700132195", "1027700132195", "TAX"),
        // A digit short or long, an OCR letter for a digit.
        ("СНИЛС: 123-456-789 0.", "123-456-789 0", "IDENTITY"),
        ("СНИЛС: 123-456-7890 00.", "123-456-7890 00", "IDENTITY"),
        ("СНИЛС: 1234567890", "1234567890", "IDENTITY"),
        ("СНИЛС: 12З-456-789 0O.", "12З-456-789 0O", "IDENTITY"),
        ("ИНН 770123456", "770123456", "TAX"),
        ("ИНН 77012345678", "77012345678", "TAX"),
        ("ИНН 77012З4567", "77012З4567", "TAX"),
        ("ИНН 77B1234567", "77B1234567", "TAX"),
        ("ОКПО: 9988776B", "9988776B", "TAX"),
        // A passport's series and number.
        ("Паспорт: 4510 123456.", "4510 123456", "IDENTITY"),
        ("паспорт 46 82 731604, выдан", "46 82 731604", "IDENTITY"),
        ("владельцу паспорта № 4512345678", "4512345678", "IDENTITY"),
        (
            "паспорт гражданина РФ серии и номер 4510 123456",
            "4510 123456",
            "IDENTITY",
        ),
        ("Паспорт: 4510 12345.", "4510 12345", "IDENTITY"),
        ("Паспорт: 46I7 82О634", "46I7 82О634", "IDENTITY"),
        // Bank details and a card's number.
        (
            "Расчётный счёт: 40702810900000001234.",
            "40702810900000001234",
            "BANK",
        ),
        (
            "р/с 4070281090000000123 в банке",
            "4070281090000000123",
            "BANK",
        ),
        (
            "корр. счёт: 30101810400000000225",
            "30101810400000000225",
            "BANK",
        ),
        ("БИК: 044525225.", "044525225", "BANK"),
        ("БИК: 04452522.", "04452522", "BANK"),
        (
            "Номер карты: 4111 1111 1111 1112.",
            "4111 1111 1111 1112",
            "BANK",
        ),
        ("карта № 4276 3800 1234 5678", "4276 3800 1234 5678", "BANK"),
        (
            "банковская карта № 4276380012345678",
            "4276380012345678",
            "BANK",
        ),
        // A personnel number.
        ("Табельный номер: VK-731628", "VK-731628", "IDENTITY"),
        ("табельный № 731628", "731628", "IDENTITY"),
        ("таб. № 004512", "004512", "IDENTITY"),
        // The value in the next cell of a table's row.
        ("| ИНН | 7701234567 |", "7701234567", "TAX"),
        ("| Паспорт | 4510 123456 |", "4510 123456", "IDENTITY"),
    ] {
        assert_eq!(
            proposed(source),
            [(value.to_owned(), category.to_owned())],
            "{source}"
        );
    }
    for source in [
        // No label: a failing checksum stays the model's.
        "Отправить 12345678900 и 7701234567 и 123-456-789 00 без меток.",
        "Номер 4111 1111 1111 1112 и 40702810900000001234.",
        // A label in prose, or a word after it.
        "Номер СНИЛС указывается в анкете.",
        "СНИЛС: нет.",
        "Паспорт: не предъявлен.",
        "Табельный номер: не присвоен",
        "Табельный номер работника 12345 указан в приказе.",
        "банковская карта утеряна",
        // Another field's value after the label.
        "СНИЛС: 01.02.2026",
        "СНИЛС: 1 000 000 рублей",
        "СНИЛС: +7 900 000-00-00",
        "ИНН: 01.02.2026",
        "ИНН: 1 000 000 рублей",
        "Паспорт 12.03.2026 выдан",
        "паспорт выдан 12 марта 2026 года",
        // Too short for its label, or a series split from its number.
        "ИНН: 123; СНИЛС: 12-34",
        "карта № 12",
        "Табельный № 2026",
        // A bare "карта" takes a card's four groups only, never a run or
        // a process's or a fuel card's number; "и" after "паспорт" is
        // no filler; a damaged ИНН's length waits for "ИНН" alone.
        "карта № 4276380012345678",
        "Технологическая карта № 4276 3800 1234 5678",
        "Топливная карта 7824861090001234567",
        "паспорт и 89161234567",
        "КПП/ИНН 770101001/7701234567",
        "БИК/ИНН 044525225/7701234567",
        "таб. 12345",
        "БИК/к/с 044525225/30101810400000000225",
        "| ИНН | КПП |\n| --- | --- |\n| нет | — |\n",
    ] {
        assert!(
            proposed(source).is_empty(),
            "{source}: {:?}",
            proposed(source)
        );
    }
}

/// Issue #381 in English: a passport's and a card's number after their
/// labels, whatever their check, a digit short or long or with an OCR
/// letter for a digit.
#[test]
fn labelled_invalid_english_identifiers_are_proposed() {
    for (source, value, category) in [
        ("Passport: 123456789", "123456789", "IDENTITY"),
        ("Passport: 12345678O", "12345678O", "IDENTITY"),
        ("**Passport No.** 12345678", "12345678", "IDENTITY"),
        ("Passport No. C01234567", "C01234567", "IDENTITY"),
        (
            "Card number: 4111 1111 1111 1112",
            "4111 1111 1111 1112",
            "BANK",
        ),
        ("Credit card: 4111111111111111", "4111111111111111", "BANK"),
        ("card 4111 1111 1111 1111", "4111 1111 1111 1111", "BANK"),
        (
            "Card number: 4111 1111 1111 111",
            "4111 1111 1111 111",
            "BANK",
        ),
    ] {
        assert_eq!(
            proposed(source),
            [(value.to_owned(), category.to_owned())],
            "{source}"
        );
    }
    for source in [
        "Unlabelled 4111 1111 1111 1112 and 123-45-6789 here.",
        "Passport: pending",
        "passport 12/03/2026",
        "the passport was renewed in 2026",
        "Credit card fees of $1,250.00",
        // "Card" alone takes four groups only, never an ID, SIM or
        // loyalty card's number; a passport starts with a capital or a
        // digit.
        "SIM card 8901260123456789012",
        "ID card 1234 5678 9012 3456",
        "Loyalty card 6006491123456789",
        "card 6006491123456789",
        "Passport No1234567",
        "Passport: lo123456",
        "card ending 1234",
    ] {
        assert!(
            proposed(source).is_empty(),
            "{source}: {:?}",
            proposed(source)
        );
    }
}

/// #381, the maintainer's decision: a US Social Security number is
/// IDENTITY, like СНИЛС, after each of its labels and when damaged
/// (guide rule 6); the model's TAX guess on a labelled one gives way,
/// beside money too. An EIN, a TIN and an ITIN stay TAX, and an
/// unlabelled SSN-shaped number keeps the model's category, as before.
#[test]
fn a_labelled_social_security_number_is_identity() {
    for (source, value) in [
        // Each label form.
        ("SSN 123-45-6789", "123-45-6789"),
        ("SSN: 123-45-6789", "123-45-6789"),
        ("SSN # 123-45-6789", "123-45-6789"),
        ("SSN No. 123-45-6789", "123-45-6789"),
        ("**SSN:** 123-45-6789", "123-45-6789"),
        ("S.S.N. 123-45-6789", "123-45-6789"),
        ("Social Security No. 123-45-6789", "123-45-6789"),
        ("Social Security Number: 123-45-6789", "123-45-6789"),
        ("social security no 123-45-6789", "123-45-6789"),
        ("Social Security # 123-45-6789", "123-45-6789"),
        ("| SSN | 123-45-6789 |", "123-45-6789"),
        ("Social Security Number (SSN): 123-45-6789", "123-45-6789"),
        ("SSN (Social Security Number): 123-45-6789", "123-45-6789"),
        // Damaged: no dashes, one missing, spaces for dashes, a digit
        // short or long, grouped in threes, OCR letters for digits.
        ("SSN: 123456789", "123456789"),
        ("SSN: 12345-6789", "12345-6789"),
        ("SSN: 123 45 6789", "123 45 6789"),
        ("SSN: 123-45-678", "123-45-678"),
        ("SSN: 123-45-67890", "123-45-67890"),
        ("SSN: 123 456 789", "123 456 789"),
        ("SSN: l23-45-67B9", "l23-45-67B9"),
        ("Social Security No. 123-45-678.", "123-45-678"),
    ] {
        assert_eq!(
            proposed(source),
            [(value.to_owned(), "IDENTITY".to_owned())],
            "{source}"
        );
    }
    // The model's TAX guess gives way, on the value, over its label, or
    // beside money; its IDENTITY guess stays.
    for (source, span, value, label) in [
        ("SSN: 123-45-6789", "123-45-6789", "123-45-6789", "tax_id"),
        (
            "SSN: 123-45-6789",
            "SSN: 123-45-6789",
            "123-45-6789",
            "tax_id",
        ),
        (
            "Social Security No. 123 45 6789",
            "123 45 6789",
            "123 45 6789",
            "tax_id",
        ),
        (
            "SSN 123 45 6789 $500",
            "123 45 6789",
            "123 45 6789",
            "tax_id",
        ),
        (
            "SSN 123-45-6789, paid $15,000",
            "123-45-6789",
            "123-45-6789",
            "tax_id",
        ),
        (
            "SSN: 123-45-6789",
            "123-45-6789",
            "123-45-6789",
            "national_id_number",
        ),
    ] {
        assert_eq!(
            shown(source, &[(span, label)]),
            pairs(&[(value, "IDENTITY")]),
            "{source} {span:?} {label}"
        );
    }
    // EIN, TIN and ITIN stay TAX.
    for (source, value) in [
        ("EIN 12-3456789", "12-3456789"),
        ("EIN: 98-7654321", "98-7654321"),
        ("TIN: 123-45-6789", "123-45-6789"),
        ("ITIN 912-70-1234", "912-70-1234"),
        ("Tax ID 12-3456789", "12-3456789"),
    ] {
        assert_eq!(
            shown(source, &[(value, "tax_id")]),
            pairs(&[(value, "TAX")]),
            "{source}"
        );
        assert!(proposed(source).is_empty(), "{source}");
    }
    // An unlabelled SSN-shaped number is proposed by no rule and keeps
    // the model's category.
    let source = "Reference 123-45-6789 here.";
    assert!(proposed(source).is_empty());
    for (label, category) in [("tax_id", "TAX"), ("national_id_number", "IDENTITY")] {
        assert_eq!(
            shown(source, &[("123-45-6789", label)]),
            pairs(&[("123-45-6789", category)]),
            "{label}"
        );
    }
    // No value after the label, or another field's.
    for source in [
        "Social Security Administration",
        "the Social Security Administration in 2026",
        "SSN: pending",
        "SSN: 01/02/2026",
        "SSN 12-3456789",
        "Social Security benefits of $1,250.00",
        "SSN: 123456789012",
        // A bare ten-digit run is a phone, not an SSN a digit long.
        "Social Security: 8007721213",
        "Call Social Security 8007721213 today",
        "SSN: 2125550142",
    ] {
        assert!(
            proposed(source).is_empty(),
            "{source}: {:?}",
            proposed(source)
        );
    }
}

/// English labels decide a bank account (a slash between sort code and
/// number is fine) and a staff ID, even one of letters only.
#[test]
fn english_labels_decide_bank_accounts_and_staff_ids() {
    let found = |source: &str| -> Vec<(String, Category)> {
        pack()
            .context(source)
            .into_iter()
            .map(|d| (source[d.range].to_owned(), d.category))
            .collect()
    };
    assert_eq!(
        found("**Bank account:** 47-29-16/00318572"),
        [("47-29-16/00318572".to_owned(), Category::Bank)]
    );
    assert_eq!(
        found("<td>Bank account: 62-18-43/00492716<br>GBP 12</td>"),
        [("62-18-43/00492716".to_owned(), Category::Bank)]
    );
    assert_eq!(
        found("IBAN: GB28NWBK60161331926819."),
        [("GB28NWBK60161331926819".to_owned(), Category::Bank)]
    );
    assert_eq!(
        found("**Staff ID:** VELK-QUEN"),
        [("VELK-QUEN".to_owned(), Category::Identity)]
    );
    assert_eq!(
        found("Employee ID: TERN-WISP, signed"),
        [("TERN-WISP".to_owned(), Category::Identity)]
    );
    for source in [
        "the bank account was closed in 2026",
        "Staff ID: pending",
        "Employee ID: Maya Bennett",
        "account number 12345",
    ] {
        assert!(found(source).is_empty(), "{source}");
    }
}

/// What the review shows: a case number's other mentions join it, and a
/// labelled number too short to propose never spreads to statute and
/// clause numbers (the review finds a wording everywhere).
#[test]
fn a_case_number_reaches_its_repeats_and_never_a_statute_number() {
    let shown = |source: &str| -> Vec<String> {
        crate::recognition::proposals(source, &[])
            .unwrap()
            .into_iter()
            .filter(|span| span.category == "CASE")
            .map(|span| source[span.range].to_owned())
            .collect()
    };
    assert!(shown("Claim No. 12. Under article 12 and section 12, payment is due.").is_empty());
    assert_eq!(
        shown("Claim No 6001842/2026. The claim 6001842/2026 is settled under article 309."),
        ["6001842/2026", "6001842/2026"]
    );
}

/// Issue #364: US caption labels give the document's own case number,
/// the label and a judge's initials outside.
#[test]
fn us_caption_labels_propose_their_case_number() {
    for (source, number) in [
        ("§ CIVIL ACTION NO. 5:31-cv-999 §", "5:31-cv-999"),
        ("CIVIL ACTION NO.: 4:31-cv-2999", "4:31-cv-2999"),
        ("Civil Action No. § 4:31-cv-2999", "4:31-cv-2999"),
        ("Civil Action No. 31-2999 (ZZ)", "31-2999"),
        ("C.A. No. 2031-0999-ZZ", "2031-0999-ZZ"),
        ("under Civil Docket No. 1:31-cv-00999.", "1:31-cv-00999"),
        ("**Docket No.:** 1:31-cv-04731-QL", "1:31-cv-04731-QL"),
        ("INDEX NO. 31 CV99999", "31 CV99999"),
        ("Index No. 652134/2031", "652134/2031"),
        ("Trial Court No. Q2031999", "Q2031999"),
        ("Trial Court No. 352-364031-31", "352-364031-31"),
        ("Cause No. 2031-12345", "2031-12345"),
        ("in cause number 09-31-00999-CV.", "09-31-00999-CV"),
        ("No. 09-31-00999-CV\n", "09-31-00999-CV"),
        ("___\n**NO. 09-31-00999-CV**\n___", "09-31-00999-CV"),
        ("| No. 09-31-00999-CV |", "09-31-00999-CV"),
        ("Adv. Pro. No. 31-01992", "31-01992"),
        ("Adversary Proceeding No. 31-01992 (ZZ)", "31-01992"),
        (
            "a Charge of Discrimination (#460-2031-01234) with the EEOC",
            "460-2031-01234",
        ),
        ("EEOC Charge No. 460-2031-01234", "460-2031-01234"),
    ] {
        assert_eq!(cases(source), [number], "{source}");
    }
}

/// The new labels never take a statute's, an article's, a docket
/// entry's, a page's or a date's number, nor a precedent's docket in its
/// citation (#506 item 3) or a Texas number away from a caption line.
#[test]
fn us_case_labels_leave_statutes_entries_pages_dates_and_precedents() {
    for source in [
        "Civil Action No. 12 under 42 U.S.C. § 1983 and Article 12.",
        "a civil action no later than 2031-03-01",
        "See Docket No. 45 and Docket No. 1234 at page 2031.",
        "Docket No. 2031-03-01",
        "Index No. 12, page 2031",
        "a service charge of $1234 and a charge no. 2031",
        "for good cause no 1234 days",
        "In No. 02-23-00310-CV the court held, as in No. 02-23-00311-CV.",
        "Proof of Claim No. 5",
    ] {
        assert!(cases(source).is_empty(), "{source}");
    }
    // A precedent's docket is part of its citation; the same label for
    // the case at hand, or an earlier case of its parties, is CASE.
    let caption = "Maren Quill v. Branwick Freight LLC\n\nIndex No. 652134/2031\n\n";
    for citation in [
        "Grant v. Finecy, Docket No. 2031-0099 (N.J. Super. 2031)",
        "Grant v. Finecy, Index No. 31 CV99999 (Sup. Ct. N.Y. 2031)",
        "Grant v. Finecy, Index No. 650001/2030 (Sup. Ct. N.Y. 2031)",
        "Grant v. Finecy, Cause No. 2031-12345 (Tex. Dist. Ct. 2031)",
        "Grant v. Finecy, Civil Docket No. 1:30-cv-00977, 2031 WL 1234567",
        "Grant v. Finecy, Civil Action No. 1:30-cv-00977 (D. Del. 2031)",
        "Grant v. Finecy, No. 02-23-00310-CV (Tex. App. 2031)",
    ] {
        let source = format!("{caption}The court relied on {citation}.");
        assert_eq!(cases(&source), ["652134/2031"], "{source}");
    }
    let source = format!(
        "{caption}It denied a motion in Quill v. Branwick Freight LLC, Index No. 650001/2030 (Sup. Ct. N.Y. 2030)."
    );
    assert_eq!(cases(&source), ["652134/2031", "650001/2030"]);
}

/// The Russian labels of #484 item 7 and an act's header (issue #364):
/// a case's УИД, an appellate proceeding's number and an administrative
/// protocol's number are CASE, the label and a date outside.
#[test]
fn russian_case_labels_take_uid_appeal_and_protocol_numbers() {
    for (source, number) in [
        (
            "г. Энск Дело № А99-9999-99/2031 01 марта 2031 г.",
            "А99-9999-99/2031",
        ),
        (
            "УИД 77RS0001-02-2031-009999-99",
            "77RS0001-02-2031-009999-99",
        ),
        (
            "**УИД:** 25RS0010-01-2031-001234-56",
            "25RS0010-01-2031-001234-56",
        ),
        (
            "апелляционное производство № 05АП-1234/2031",
            "05АП-1234/2031",
        ),
        (
            "по апелляционному производству №05АП-1234/2031",
            "05АП-1234/2031",
        ),
        (
            "протокол об административном правонарушении № 99 АБ 123456 составлен",
            "99 АБ 123456",
        ),
        (
            "протоколом об административном правонарушении от 01.03.2031 № 18810099310001234567",
            "18810099310001234567",
        ),
        (
            "протоколом № 77 МР 0123456 об административном правонарушении",
            "77 МР 0123456",
        ),
    ] {
        assert_eq!(cases(source), [number], "{source}");
    }
    for source in [
        "протокол № 3 общего собрания участников от 01.03.2031",
        "протоколом № 12/2031 заседания совета",
        "по статье 12.8 КоАП РФ, часть 1 статьи 1234",
        "протокол об административном правонарушении № 12",
        "УИД документа не указан, страница 2031",
    ] {
        assert!(cases(source).is_empty(), "{source}");
    }
}

/// #553 item 7 and issue #364: the document's own labelled case number
/// is CASE wherever it repeats, in any letter case, label or not; a page
/// or entry number beside it stays outside.
#[test]
fn a_labelled_case_number_reaches_its_repeats_in_any_letter_case() {
    assert_eq!(
        cases("АПЕЛЛЯЦИОННОЕ ОПРЕДЕЛЕНИЕ № 33А-2090/2031\n\nпо делу № 33а-2090/2031"),
        ["33А-2090/2031", "33а-2090/2031"]
    );
    assert_eq!(
        cases(
            "CIVIL ACTION NO.: 4:31-cv-2999\n\n4:31-cv-2999 12\n\nCase 4:31-CV-2999 Document 5 Page 3 of 9"
        ),
        ["4:31-cv-2999", "4:31-cv-2999", "4:31-CV-2999"]
    );
    // Not inside a longer number, nor where a precedent cites it.
    assert_eq!(
        cases("Civil Action No. 31-2999\n\nexhibit 31-2999-7 and 131-2999"),
        ["31-2999"]
    );
}

/// A case number's repeat in another letter case never takes a part of
/// an email, an account or an address, which keep their values, the
/// model's or the rules' (the #364 review, P1-1).
#[test]
fn a_case_repeat_leaves_emails_accounts_and_addresses_whole() {
    let email =
        "Claim No. CL-2026-000123\n\nWrite to claims+cl-2026-000123@insurer.example for updates.";
    let expected = pairs(&[
        ("CL-2026-000123", "CASE"),
        ("claims+cl-2026-000123@insurer.example", "EMAIL"),
    ]);
    assert_eq!(shown(email, &[]), expected);
    assert_eq!(
        shown(email, &[("claims+cl-2026-000123@insurer.example", "email")]),
        expected
    );
    for (source, raw, expected) in [
        (
            "Case No. KL-31-0042\n\nContact kl-31-0042@harrow-law.example.",
            ("kl-31-0042@harrow-law.example", "email"),
            [
                ("KL-31-0042", "CASE"),
                ("kl-31-0042@harrow-law.example", "EMAIL"),
            ],
        ),
        (
            "Claim No. GB-2031-4471\n\nPay to account gb-2031-4471-0098 at Brindle Bank.",
            ("gb-2031-4471-0098", "bank_account"),
            [("GB-2031-4471", "CASE"), ("gb-2031-4471-0098", "BANK")],
        ),
        (
            "Case No. 2A-1234\n\nThe office is at Suite 2a-1234, 12 Harbour Road, Brindle, BR1 4QQ.",
            (
                "Suite 2a-1234, 12 Harbour Road, Brindle, BR1 4QQ",
                "address",
            ),
            [
                ("2A-1234", "CASE"),
                (
                    "Suite 2a-1234, 12 Harbour Road, Brindle, BR1 4QQ",
                    "ADDRESS",
                ),
            ],
        ),
    ] {
        assert_eq!(shown(source, &[raw]), pairs(&expected), "{source}");
    }
    // Nor a web address's path or an email's local part.
    for source in [
        "Claim No. CL-2026-000123\n\nSee https://portal.insurer.example/claims/cl-2026-000123/status for updates.",
        "по делу № 33а-2090/2031\n\nE-mail: 33А-2090/2031@sud.example",
    ] {
        assert_eq!(cases(source).len(), 1, "{source}");
    }
    // The model reading the repeat itself as TAX makes it no other
    // value: the repeat on exactly that span is CASE.
    let source = "по делу № 33а-9999/2031\n\nДело № 33А-9999/2031 рассмотрено.";
    assert_eq!(
        shown(source, &[("33А-9999/2031", "tax_id")]),
        pairs(&[("33а-9999/2031", "CASE"), ("33А-9999/2031", "CASE")])
    );
}

/// A precedent's docket in the standard federal, Delaware and New York
/// forms stays in its citation: "C.A. No.", "Civ. A. No.", a judge's
/// initials, a docket entry's pin, a LEXIS or Slip Opinion reporter,
/// and a table of authorities' wrapped "No." line (the #364 review,
/// P1-2).
#[test]
fn a_precedents_docket_in_its_usual_forms_stays_unmarked() {
    let caption =
        "IVO HARROW, Plaintiff,\n\nv.\n\nQUILLON LTD, Defendant.\n\nCase No. 1:31-cv-00999\n\n";
    for citation in [
        "See Grant v. Finecy, Civil Action No. 21-2873 (ABJ), 2031 WL 1234567, at *3 (D.D.C. Mar. 1, 2031).",
        "See Grant v. Finecy, Index No. 650001/2030, 2031 N.Y. Slip Op. 31234(U) (Sup. Ct. N.Y. Cnty. 2031).",
        "The court relied on Grant v. Finecy, C.A. No. 2019-0123-JRS (Del. Ch. 2031).",
        "See Grant v. Finecy, Civ. A. No. 30-0977 (E.D. Pa. 2031).",
        "See Grant v. Finecy, Civil Action No. 1:30-cv-00977, ECF No. 45 at 3 (D. Del. Mar. 1, 2031).",
        "See Grant v. Finecy, Cause No. 2031-12345, 2031 Tex. App. LEXIS 1234 (Tex. App. 2031).",
        "Grant v. Finecy,\nNo. 02-23-00310-CV, 2031 Tex. App. LEXIS 1234 (Tex. App.—Fort Worth 2031)",
        "See Grant v. Finecy, Civil Action No. 21-2873 (ABJ) (D.D.C. 2031).",
    ] {
        let source = format!("{caption}{citation}");
        assert_eq!(cases(&source), ["1:31-cv-00999"], "{source}");
    }
    // A wrapped sentence's "No." is no caption either, cited or not.
    assert!(cases("as held in the appeal,\nNo. 02-23-00310-CV, the court").is_empty());
}

/// A docket's entries and their attachments are no case numbers, nor
/// is a date, in prose or in a "Docket No" column (the #364 review,
/// P2-1, P3 item 4); a case number's shapes after "Docket No." are.
#[test]
fn docket_entries_and_dates_are_no_docket_numbers() {
    for source in [
        "See the declaration (Docket No. 112-15) and the exhibit (Docket No. 1234-5).",
        "See Declaration of Ivo Harrow, Docket No. 1045-3, at 4.",
        "The motion (Docket Number 45-12) was denied.",
        "(Docket No. 10452)",
        "| Docket No | Filed |\n|---|---|\n| 112-15 | 01/03/2031 |\n| 1234 | 02/03/2031 |",
        "Docket No. 03-01-2031",
    ] {
        assert!(cases(source).is_empty(), "{source}");
    }
    for (source, number) in [
        ("Docket No. A-1234-30", "A-1234-30"),
        ("Docket No. 2031-0099", "2031-0099"),
        ("Docket No. MER-L-001234-31", "MER-L-001234-31"),
        ("Docket No. 09-31-00999-CV", "09-31-00999-CV"),
        ("Docket No. 31 CV 1234", "31 CV 1234"),
    ] {
        assert_eq!(cases(source), [number], "{source}");
    }
}

/// New York's "-against-" caption is the case at hand's, so a precedent
/// cited in a sentence with its "Index No." is none (the #364 review,
/// P2-2); a one-line caption with its index number still is.
#[test]
fn an_against_caption_leaves_a_cited_index_number_unmarked() {
    let source = "MAREN QUILL,\n\nPlaintiff,\n\n-against-\n\nBRANWICK FREIGHT LLC,\n\nDefendant.\n\nIndex No. 652134/2031\n\nPlaintiff relies on Grant v. Finecy, Index No. 650001/2030 (Sup. Ct. N.Y. 2030).";
    let raw = [
        ("MAREN QUILL", "person"),
        ("BRANWICK FREIGHT LLC", "organization"),
        ("Grant", "person"),
        ("Finecy", "person"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("MAREN QUILL", "PERSON"),
            ("BRANWICK FREIGHT LLC", "ORG"),
            ("652134/2031", "CASE"),
        ])
    );
    // Without "-against-", a docket citation in a sentence is still no
    // caption.
    let source = source.replace("-against-", "");
    assert_eq!(cases(&source), ["652134/2031"]);
    let source = "Maren Quill v. Branwick Freight LLC, Index No. 652134/2031 (Sup. Ct. N.Y. Cnty. 2031)\n\nThe defendant moves to dismiss.";
    assert_eq!(
        shown(
            source,
            &[
                ("Maren Quill", "person"),
                ("Branwick Freight LLC", "organization")
            ]
        ),
        pairs(&[
            ("Maren Quill", "PERSON"),
            ("Branwick Freight LLC", "ORG"),
            ("652134/2031", "CASE"),
        ])
    );
}

/// A related proceeding of the parties cited by its docket keeps every
/// name it holds, its other party's too, with its number (the #364
/// review, P2-3).
#[test]
fn an_earlier_case_keeps_its_other_party() {
    let source = "In re Ivo Harrow\n\nChapter 7\n\nThe trustee sued in Brendt Holloway v. Harrow, Adv. Proc. No. 31-01992 (Bankr. S.D.N.Y. 2031).";
    let shown_here = shown(
        source,
        &[("Ivo Harrow", "person"), ("Brendt Holloway", "person")],
    );
    for expected in pairs(&[("Brendt Holloway", "PERSON"), ("31-01992", "CASE")]) {
        assert!(
            shown_here.contains(&expected),
            "{expected:?} in {shown_here:?}"
        );
    }
    let source = "IVO HARROW, Plaintiff,\n\nv.\n\nQUILLON LTD, Defendant.\n\nIndex No. 652134/2031\n\nHarrow had earlier sued in Harrow v. Pellam Corp., Index No. 650001/2030 (Sup. Ct. N.Y. 2030).";
    let shown_here = shown(
        source,
        &[
            ("IVO HARROW", "person"),
            ("QUILLON LTD", "organization"),
            ("Pellam Corp.", "organization"),
        ],
    );
    for expected in pairs(&[("Pellam Corp", "ORG"), ("650001/2030", "CASE")]) {
        assert!(
            shown_here.contains(&expected),
            "{expected:?} in {shown_here:?}"
        );
    }
}

/// The review's other wrong case numbers (#364, P3 items 1-4): a card's
/// or a phone line's "charge", a "Charge" column of amounts, an OCR
/// period glued to the value, "C.A." inside "U.S.C.A." and a price
/// index; and the misses it found that are now proposed.
#[test]
fn charges_amounts_statutes_and_price_indexes_are_no_case_numbers() {
    assert_eq!(
        shown(
            "Your card was billed for charge #4532-0151-1283-0366 on 1 March.",
            &[("4532-0151-1283-0366", "bank_account")]
        ),
        pairs(&[("4532-0151-1283-0366", "BANK")])
    );
    assert_eq!(
        shown(
            "For questions about this charge No. 555-201-4477 is open daily.",
            &[("555-201-4477", "phone_number")]
        ),
        pairs(&[("555-201-4477", "PHONE")])
    );
    for source in [
        "| Date | Charge | Payment |\n|---|---|---|\n| 01/03/2031 | 1234.50 | 0 |\n\nTotal due 1234.50.",
        "under 18 U.S.C.A. No. 1234-5 of the code",
        "Consumer Price Index No. 2031-03 for March",
        // The period ends the value, and the review then reads the
        // number as part of "31-2999.The": no wrong "31-2999.The".
        "Civil Action No. 31-2999.The plaintiff filed.",
    ] {
        assert!(cases(source).is_empty(), "{source}");
    }
    for (source, number) in [
        ("Charge No. 460-2031-01234 was dismissed.", "460-2031-01234"),
        ("CIVIL ACTION NO.:\n§ 4:31-cv-2999", "4:31-cv-2999"),
        (
            "протоколом об административном правонарушении от 01 марта 2031 г. № 99 АБ 123456",
            "99 АБ 123456",
        ),
    ] {
        assert_eq!(cases(source), [number], "{source}");
    }
    assert!(cases("CIVIL ACTION NO.\n\n4:31-cv-2999").is_empty());
}

/// A date in a personal context is proposed as Date, the label left out
/// (#363); every other date stays (ADR 0092).
#[test]
fn dates_of_birth_are_proposed_and_other_dates_stay() {
    let dates = |source: &str| -> Vec<String> {
        pack()
            .context(source)
            .into_iter()
            .filter(|d| d.category == Category::Date)
            .map(|d| source[d.range].to_owned())
            .collect()
    };
    for (source, date) in [
        (
            "Ms Maya Bennett, born 14 March 1988, National",
            "14 March 1988",
        ),
        ("born on 2 May 1990.", "2 May 1990"),
        ("**DOB:** 23 September 1991. That", "23 September 1991"),
        ("Date of birth: March 4, 1979;", "March 4, 1979"),
        ("дата рождения 14 марта 1988 г., СНИЛС", "14 марта 1988 г."),
        (
            "Дата рождения: 19 февраля 1987 года. В",
            "19 февраля 1987 года",
        ),
        ("Иванов И. И., род. 14.03.1988, паспорт", "14.03.1988"),
        ("д.р. 01.02.1990", "01.02.1990"),
        ("| DOB | 14/03/1988 |", "14/03/1988"),
        ("| Дата рождения | 14.03.1988 г. |", "14.03.1988 г."),
        ("DOB:\n14/03/1988", "14/03/1988"),
        (
            "Дата рождения: «14» марта 1988 г., паспорт",
            "«14» марта 1988 г.",
        ),
        ("| DOB | 14/03/1988 |", "14/03/1988"),
        ("| Дата рождения | 14.03.1988 г. |", "14.03.1988 г."),
        (
            "DOB:
14/03/1988",
            "14/03/1988",
        ),
        (
            "Дата рождения: «14» марта 1988 г., паспорт",
            "«14» марта 1988 г.",
        ),
    ] {
        assert_eq!(dates(source), [date], "{source}");
    }
    for source in [
        "Signed on 14 March 2026 in Bristol.",
        "The hearing is listed for 12.03.2026.",
        "She was born in Leeds in 1988.",
        "born 14 Marchmont 1988",
        "Род деятельности: 2026 год",
        "Adobe 14 March 1988",
    ] {
        assert!(dates(source).is_empty(), "{source}");
    }
}

/// A birth date's repeat elsewhere is another date and stays, like any
/// other date (#466 option 1, GUIDE rule 4); a labelled repeat counts.
#[test]
fn a_birth_dates_repeat_elsewhere_stays_unmarked() {
    let source = "DOB: 14/03/1988. Signed 14/03/1988, due 01/04/2026. DOB 14/03/1988.";
    let dates: Vec<_> = crate::recognition::proposals(source, &[])
        .unwrap()
        .into_iter()
        .filter(|span| span.category == "DATE")
        .map(|span| source[span.range].to_owned())
        .collect();
    assert_eq!(dates, ["14/03/1988", "14/03/1988"]);
}

/// A two-digit year after DOB, and a date before "birth date", "date of
/// birth", «года рождения» or «г.р.», wrapped at a line break or not, are
/// dates of birth; the label stays out (#363). A bare year, a hearing
/// date and a date beside another "birth" stay.
#[test]
fn dates_of_birth_with_short_years_and_labels_after() {
    let dates = |source: &str| -> Vec<String> {
        crate::recognition::proposals(source, &[])
            .unwrap()
            .into_iter()
            .filter(|span| span.category == "DATE")
            .map(|span| source[span.range].to_owned())
            .collect()
    };
    for (source, date) in [
        ("DOB 3 Jun 61. Nationality: Ruritanian.", "3 Jun 61"),
        ("DOB: 03/06/61; POB: Zenda.", "03/06/61"),
        ("| DOB | 3 Sept. 1961 |", "3 Sept. 1961"),
        ("He gave a March 9, 1984 birth date.", "March 9, 1984"),
        ("He gave a March 9,\n1984 birth date.", "March 9,\n1984"),
        ("a 9 March\r\n1984 birth date", "9 March\r\n1984"),
        ("with a 03/06/1961 date of birth", "03/06/1961"),
        ("born March 9,\n1984, in Zenda", "March 9,\n1984"),
        (
            "Ответчик Иванов И. И., 01.01.1980 года рождения, проживает",
            "01.01.1980",
        ),
        ("Петров П. П., 14 марта 1988 г.р., паспорт", "14 марта 1988"),
        ("Сидоров С. С. (01.02.1990 г. р.)", "01.02.1990"),
        ("Орлова О. О., 01.02.1990 г.р.", "01.02.1990"),
        ("Иванов И. И., 01.01.1980 г.р., проживает", "01.01.1980"),
        ("Иванов И. И., 1 января 1980 г. рождения", "1 января 1980"),
        ("He gave a March 9, 1984 date of birth.", "March 9, 1984"),
        ("He gave a 9 March 1984 date-of-birth.", "9 March 1984"),
        ("He gave a March 9, 1984 birthdate.", "March 9, 1984"),
        ("the claimant's 12/04/1979 date of birth", "12/04/1979"),
        (
            "the visa form records a 12/04/1979 date of birth for his brother",
            "12/04/1979",
        ),
        ("DOB 03-Jun-61; POB Zenda.", "03-Jun-61"),
        ("DOB 03-Jun-1961; POB Zenda.", "03-Jun-1961"),
        ("Birthdate: 03/04/1980", "03/04/1980"),
        // A capitalised label opens the next field, and only its own
        // value is a date of birth (#363 review P1-2).
        (
            "Date of issue 01/02/2020 Date of birth 03/04/1980",
            "03/04/1980",
        ),
        (
            "Date of death: 12 March 2020 Date of birth: 1 January 1950",
            "1 January 1950",
        ),
        (
            "Passport issued 14 March 2015  date of birth  3 June 1961",
            "3 June 1961",
        ),
        ("Hearing date 3 Jun 26 Date of birth 3 Jun 61", "3 Jun 61"),
        (
            "| Issued | 01/02/2020 Date of birth 03/04/1980 |",
            "03/04/1980",
        ),
    ] {
        assert_eq!(dates(source), [date], "{source:?}");
    }
    for source in [
        "The hearing is listed for 3 Jun 26.",
        "Hearing: 12 March 2026\nDate of birth: unknown",
        "Signed 12 March 2026. Date of birth: unknown.",
        "She was born in 1988 in Leeds.",
        "a birth certificate No. 12/03/1988 issued 14 March 1988",
        "On March 9, 1984 birth certificates were reissued.",
        "Rule 123 March 1984 birth date",
        // Over a blank line a date is two paragraphs' text: one span
        // would join them.
        "He gave a March 9,\n\n1984 birth date.",
        "Иванов И. И., 1980 г.р., проживает",
        "Иванов И. И., 1980 года рождения",
        "свидетельство о рождении от 01.01.1980 выдано",
        "решение от 01.01.2026 г. вступило в силу",
        // A label after a date: lowercase, an article or a possessive
        // before the date, and the phrase ends with it (review P1-2).
        "Signed 12 March 2026 Date of birth: unknown",
        "On 12 March 2026 date of birth evidence was filed.",
        "By 1 March 2026 birth date records must be corrected.",
        "the court's 12 March 2026 birth date correction order",
        "He gave a 12 March 2026 date of birth: unknown",
        "3 June 1961 date of birth",
        // «г.р.» lowercase, its dots written, on the date's line: no
        // initial after «г.» and no account (review P2-1).
        "Решением суда от 12.03.2026 г. Р.А. Иванову отказано.",
        "Дата: 12.03.2026 г.\nР. А. Иванов",
        "12.03.2026 г.                Р.А. Иванов",
        "договор от 01.02.2020 Г.Р. Петровой подписан",
        "оплачено 01.02.2020 г. р/с 40702810000000000001",
        // A wrapped year ends its line or punctuation follows (review
        // P2-3).
        "Date of birth: 14 March\n2026 hearing listed.",
        "born on March 9,\n2019 the claimant moved.",
        // A number's tail is no date of its own (review P3).
        "Passport AB-12/04/1979 date of birth unknown",
        "Passport No. 12/04/1979 date of birth",
    ] {
        assert!(dates(source).is_empty(), "{source:?}: {:?}", dates(source));
    }
}

/// #363 review P1-1: a date before its label marks its repeats only
/// before the same label; the date of a hearing, a decision or a
/// contract that repeats it stays, and so does one at a paragraph's
/// start. Each source's DATE spans, by where they start.
#[test]
fn a_date_before_its_label_repeats_only_before_that_label() {
    let dates = |source: &str| -> Vec<usize> {
        crate::recognition::proposals(source, &[])
            .unwrap()
            .into_iter()
            .filter(|span| span.category == "DATE")
            .map(|span| span.range.start)
            .collect()
    };
    let nth = |source: &str, value: &str, n: usize| -> usize {
        source.match_indices(value).nth(n).unwrap().0
    };
    for source in [
        "Решение от 12.03.2026 вынесено. Заседание 12.03.2026 окончено.\n\n12.03.2026 г.     Р.А. Иванов",
        "The hearing of 3 June 1961 was adjourned to 3 June 1961 at noon.\n\n3 June 1961 date of birth",
        "по договору от 12.03.2026 г. Р.А. Иванов передал. Иск подан по договору от 12.03.2026 и принят.",
    ] {
        assert!(dates(source).is_empty(), "{source:?}: {:?}", dates(source));
    }
    let source = "He gave a 12 March 1984 birth date. He gave a 12 March 1984 address too.";
    assert_eq!(dates(source), [nth(source, "12 March 1984", 0)]);
    let source = "Иванов И. И., 01.01.1980 года рождения. Договор займа от 01.01.1980 подписан.\n\n01.01.1980 г. Иванов";
    assert_eq!(dates(source), [nth(source, "01.01.1980", 0)]);
    // The same label after a repeat: the same date of birth, even where
    // the rule itself would not take it (no article before it).
    let source = "He gave a 12 March 1984 birth date. 12 March 1984 birth date, he said.";
    let both = [
        nth(source, "12 March 1984", 0),
        nth(source, "12 March 1984", 1),
    ];
    assert_eq!(dates(source), both);
    let source = "Иванов И. И., 01.01.1980 г.р. Иск Иванова (01.01.1980 г. р.) удовлетворён.";
    let both = [nth(source, "01.01.1980", 0), nth(source, "01.01.1980", 1)];
    assert_eq!(dates(source), both);
}

/// #363 review P2-2, P2-3 and P3: a model's name or address beside a
/// labelled date keeps its own words, its part away from the label; a
/// wrapped year never takes the next line's house number; a passport
/// number stays whole.
#[test]
fn names_and_addresses_beside_a_date_of_birth_stay() {
    for (source, raw, expected) in [
        (
            "Иванов Иван Иванович, 01.01.1980 года рождения",
            &[("Иванов Иван Иванович, 01.01.1980", "person")][..],
            &[("Иванов Иван Иванович", "PERSON"), ("01.01.1980", "DATE")][..],
        ),
        (
            "Ответчик Петров П. П., 14 марта 1988 г.р.",
            &[("Петров П. П., 14", "person")],
            &[("Петров П. П.", "PERSON"), ("14 марта 1988", "DATE")],
        ),
        (
            "DOB 3 Jun 61 IVANOV, Ivan",
            &[("61 IVANOV, Ivan", "person")],
            &[("3 Jun 61", "DATE"), ("IVANOV, Ivan", "PERSON")],
        ),
        (
            "DOB 3 Jun 1961 Ivo Harrow",
            &[("1961 Ivo Harrow", "person")],
            &[("3 Jun 1961", "DATE"), ("Ivo Harrow", "PERSON")],
        ),
        // No article before the date: no date of birth, the name as
        // the model gave it.
        (
            "Ivanov Ivan 3 Jun 61 birth date",
            &[("Ivanov Ivan 3", "person")],
            &[("Ivanov Ivan 3", "PERSON")],
        ),
        (
            "He gave a March 9,\n1984 birth date. Ivo Harrow signed.",
            &[("Ivo Harrow", "person")],
            &[("March 9,\n1984", "DATE"), ("Ivo Harrow", "PERSON")],
        ),
        (
            "Date of birth: 4 July\n1600 Pennsylvania Avenue NW, Washington",
            &[("1600 Pennsylvania Avenue NW, Washington", "address")],
            &[("1600 Pennsylvania Avenue NW, Washington", "ADDRESS")],
        ),
        (
            "Date of birth: March 9,\n1984 Harrow Road, Zenda",
            &[("1984 Harrow Road, Zenda", "address")],
            &[("1984 Harrow Road, Zenda", "ADDRESS")],
        ),
        (
            "Ivo Harrow, born March 9,\n1984 Harrow Road, Zenda",
            &[
                ("Ivo Harrow", "person"),
                ("1984 Harrow Road, Zenda", "address"),
            ],
            &[
                ("Ivo Harrow", "PERSON"),
                ("1984 Harrow Road, Zenda", "ADDRESS"),
            ],
        ),
        (
            "Passport AB-12/04/1979 date of birth unknown",
            &[("AB-12/04/1979", "passport_number")],
            &[("AB-12/04/1979", "IDENTITY")],
        ),
        (
            "Passport No. 12/04/1979 date of birth",
            &[("12/04/1979", "passport_number")],
            &[("12/04/1979", "IDENTITY")],
        ),
    ] {
        assert_eq!(shown(source, raw), pairs(expected), "{source:?}");
    }
}

/// Cadastral and registration numbers and numbers assigned to one person
/// are IDENTITY, the value only; a cadastral quarter, a notarial act's
/// entry and other numbers stay (#484 items 2 and 3, #483).
#[test]
fn cadastral_and_personal_register_numbers_are_identity() {
    let found = |source: &str| -> Vec<(String, Category)> {
        let mut found: Vec<(String, Category)> = pack()
            .context(source)
            .into_iter()
            .chain(pack().spans(source))
            .filter(|d| d.category == Category::Identity)
            .map(|d| (source[d.range].to_owned(), d.category))
            .collect();
        found.dedup();
        found
    };
    let id = |text: &str| vec![(text.to_owned(), Category::Identity)];
    for (source, value) in [
        ("кадастровый номер 99:07:0012345:678", "99:07:0012345:678"),
        (
            "с кадастровым номером 99:07:0012345:678.",
            "99:07:0012345:678",
        ),
        (
            "Кадастровый номер земельного участка: 99:07:0012345:678",
            "99:07:0012345:678",
        ),
        ("КН 99:07:0012345:6789", "99:07:0012345:6789"),
        ("участок 99:07:001234:5 продан", "99:07:001234:5"),
        (
            "номер регистрации права 99:07:0012345:678-99/011/2031-4",
            "99:07:0012345:678-99/011/2031-4",
        ),
        (
            "номер государственной регистрации от 12.03.2031 № 99-99/011-99/011/001/2031-123/2",
            "99-99/011-99/011/001/2031-123/2",
        ),
        (
            "регистрационный номер в сводном реестре 99887, e-mail",
            "99887",
        ),
        (
            "регистрационный номер в сводном государственном реестре арбитражных управляющих 12345",
            "12345",
        ),
        (
            "регистрационный номер в реестре адвокатов Ольховской области 99/1234",
            "99/1234",
        ),
        ("рег. номер в реестре нотариусов: 99/123-н", "99/123-н"),
        ("удостоверение адвоката № 9917", "9917"),
        ("удостоверением № 99012", "99012"),
        ("Ohio State Bar No. 0099911", "0099911"),
        ("(Pa. Bar # 99911)", "99911"),
        ("Bar No.: 12345", "12345"),
        ("Texas Bar No. 24099911", "24099911"),
        ("SBN 299911", "299911"),
        ("ROWAN PELL, ESQ. (RP9911)", "RP9911"),
        ("Officer Ivo Ostrand, Shield No. 9912.", "9912"),
        ("реестровый номер адвоката 99/1234", "99/1234"),
        (
            "| Bar No |
|---|
| 0099911 |
",
            "0099911",
        ),
        (
            "| Регистрационный номер в реестре адвокатов |
|---|
| 99/1234 |
",
            "99/1234",
        ),
    ] {
        assert_eq!(found(source), id(value), "{source}");
    }
    for source in [
        "кадастровый квартал 99:07:0012345",
        "реестровый номер 99/123-н/99-2031-1-456",
        "| Номер |
|---|
| 12345 |
",
        "кадастровая стоимость 1234567 рублей",
        "зарегистрировано в реестре № 99/123-н/99-2031-1-456",
        "в реестре нотариусов не значится",
        "время 12:30:45",
        "Bar association met in 2031",
        "the bar is 2 miles away",
    ] {
        assert!(found(source).is_empty(), "{source}");
    }
    // The model's tax, phone or bank guesses on a cadastral number go,
    // and the number is IDENTITY.
    for label in ["tax_id", "phone_number", "bank_account"] {
        let source = "Участок с кадастровым номером 99:07:0012345:678 передан.";
        assert_eq!(
            shown(source, &[("99:07:0012345:678", label)]),
            pairs(&[("99:07:0012345:678", "IDENTITY")]),
            "{label}"
        );
        let source = "Участок 99:07:0012345:678 передан.";
        assert_eq!(
            shown(source, &[("99:07:0012345:678", label)]),
            pairs(&[("99:07:0012345:678", "IDENTITY")]),
            "{label}"
        );
    }
    // A partial tax guess on a cadastral number gives way to the whole.
    let source = "Участок 99:07:0012345:678 продан.";
    assert_eq!(
        shown(source, &[("99:07:0012345", "tax_id")]),
        pairs(&[("99:07:0012345:678", "IDENTITY")])
    );
    // A bar number swallowed into an organization is the number alone.
    let source = "Counsel: Rowan Pell, Texas State Bar No. 24099911, for Quillon Freight LLC.";
    assert_eq!(
        shown(
            source,
            &[
                ("Rowan Pell", "person"),
                ("Texas State Bar No. 24099911", "organization"),
                ("Quillon Freight LLC", "organization"),
            ]
        ),
        pairs(&[
            ("Rowan Pell", "PERSON"),
            ("24099911", "IDENTITY"),
            ("Quillon Freight LLC", "ORG"),
        ])
    );
    // So is a bar's name before its number.
    let source = "Ohio State Bar No. 0099911";
    assert_eq!(
        shown(source, &[("Ohio State Bar", "organization")]),
        pairs(&[("0099911", "IDENTITY")])
    );
}

/// An account's label comes off its span, a colon with it, and a label
/// the trim takes no part of still gives its value ("acct"); a value's
/// own final letters stay, and a case number wrapped over lines is no
/// account (guide rule 6, issue #486).
#[test]
fn account_labels_stay_outside_bank_spans() {
    let source = "Pay to acct 1234567890 or IBAN: GB47 KELD 6012 4483 7196 02;             ref LU12 3456 7890 1234 ACCT. Claim No.
6001842
/
2026 is pending.
";
    let raw = [
        ("acct 1234567890", "bank_account"),
        ("IBAN: GB47 KELD 6012 4483 7196 02", "iban"),
        ("LU12 3456 7890 1234 ACCT", "bank_account"),
        (
            "6001842
/
2026",
            "bank_account",
        ),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("1234567890", "BANK"),
            ("GB47 KELD 6012 4483 7196 02", "BANK"),
            ("LU12 3456 7890 1234 ACCT", "BANK"),
        ])
    );
    let source = "Плательщик перечисляет на р/с: 40702810900000004711.
";
    let raw = [("р/с: 40702810900000004711", "bank_account")];
    assert_eq!(
        shown(source, &raw),
        pairs(&[("40702810900000004711", "BANK")])
    );
}

/// A dash may stand between a classifier's code and its value, as a
/// form's table writes it (issue #486).
#[test]
fn classifier_codes_after_a_dash_are_no_identifiers() {
    let source = "Коды: ОКФС - 61, ОКВЭД – 84.30; ИНН 7707083893.
";
    let raw = [
        ("61", "tax_id"),
        ("84.30", "tax_id"),
        ("7707083893", "tax_id"),
    ];
    assert_eq!(shown(source, &raw), pairs(&[("7707083893", "TAX")]));
}

/// #563: ОГРН, ОГРНИП and ОКПО are TAX like ИНН, a public body's own
/// too; the model's IDENTITY or BANK guess on one gives way, and with no
/// guess the label proposes it. КБК and ОКТМО are budget codes and stay
/// unmarked; the account and БИК stay BANK (guide rule 6, #484 item 6,
/// #553 item 1).
#[test]
fn state_register_numbers_are_tax_numbers() {
    // The issue's reproduction; its ИНН differs by a digit, as
    // `shown` would find "9900000017" inside "1029900000017".
    let source = "ООО «Север» (ОГРН 1029900000017, ОКПО 99887766), ИП Ветлин Н. О. \
        (ОГРНИП 329990000000012); получатель: УФК по Ольховской области, ИНН \
        9901000017, БИК 049999999, р/с 03100643000000019900, КБК \
        18210102010011000110, ОКТМО 99701000.";
    let expected = pairs(&[
        ("ООО «Север»", "ORG"),
        ("1029900000017", "TAX"),
        ("99887766", "TAX"),
        ("Ветлин Н. О.", "PERSON"),
        ("329990000000012", "TAX"),
        ("9901000017", "TAX"),
        ("049999999", "BANK"),
        ("03100643000000019900", "BANK"),
    ]);
    let common = [
        ("ООО «Север»", "organization"),
        ("Ветлин Н. О.", "person"),
        ("9901000017", "tax_id"),
        ("049999999", "routing_number"),
        ("03100643000000019900", "bank_account"),
        ("18210102010011000110", "bank_account"),
        ("99701000", "tax_id"),
    ];
    for guesses in [
        [
            ("1029900000017", "national_id_number"),
            ("99887766", "bank_account"),
            ("329990000000012", "national_id_number"),
        ],
        [
            ("ОГРН 1029900000017", "bank_account"),
            ("ОКПО 99887766", "national_id_number"),
            ("329990000000012", "passport_number"),
        ],
        [
            ("1029900000017", "bank_account"),
            ("99887766", "passport_number"),
            ("ОГРНИП 329990000000012", "national_id_number"),
        ],
    ] {
        let raw: Vec<_> = common.iter().chain(&guesses).copied().collect();
        assert_eq!(shown(source, &raw), expected, "{guesses:?}");
    }
    // With no model span the labels still propose the numbers.
    assert_eq!(
        shown(source, &[]),
        pairs(&[
            ("1029900000017", "TAX"),
            ("99887766", "TAX"),
            ("Ветлин Н. О.", "PERSON"),
            ("329990000000012", "TAX"),
            ("9901000017", "TAX"),
            // The БИК and the account after their labels (#381).
            ("049999999", "BANK"),
            ("03100643000000019900", "BANK"),
        ])
    );
    // A valid ОГРН's structured rule says TAX too.
    let source = "ОГРН 1027700132195; OGRN 1027700132195";
    assert_eq!(
        shown(source, &[("1027700132195", "national_id_number")]),
        pairs(&[("1027700132195", "TAX"), ("1027700132195", "TAX")])
    );
}

/// #563: the labels' forms, and an invalid or damaged value of one
/// digit fewer or more, which the guide marks too (rule 6).
#[test]
fn state_register_labels_take_their_forms() {
    for (source, values) in [
        ("ОГРН: 1027700132195.", &["1027700132195"][..]),
        ("ОГРНИП № 322213000038573", &["322213000038573"]),
        ("ОГРН ИП 322213000038573", &["322213000038573"]),
        ("ОГРН ИП: 322213000038573", &["322213000038573"]),
        ("(ОГРН:1027700132195)", &["1027700132195"]),
        ("**ОГРН:** 1027700132195", &["1027700132195"]),
        ("ОГРН (1029900000017)", &["1029900000017"]),
        (
            "основной государственный регистрационный номер (ОГРН) 1029900000017",
            &["1029900000017"],
        ),
        (
            "основной государственный регистрационный номер индивидуального \
                предпринимателя 329990000000012",
            &["329990000000012"],
        ),
        (
            "за основным государственным регистрационным номером 1029900000017",
            &["1029900000017"],
        ),
        // English labels, any case.
        ("OGRN: 1029900000017", &["1029900000017"]),
        ("ogrnip 329990000000012", &["329990000000012"]),
        ("PSRN 1029900000017", &["1029900000017"]),
        ("ОКПО: 99887766", &["99887766"]),
        ("код ОКПО 9988776655", &["9988776655"]),
        ("код по ОКПО 99887766", &["99887766"]),
        // One digit fewer or more, and OCR's letter for a digit.
        ("ОГРН 102990000001", &["102990000001"]),
        ("ОГРН 10299000000170", &["10299000000170"]),
        ("ОГРНИП 3299900000000123", &["3299900000000123"]),
        ("ОГРН 1О29900000017", &["1О29900000017"]),
        ("ОКПО 9988776", &["9988776"]),
        ("ОКПО 998877665", &["998877665"]),
    ] {
        let tax: Vec<(&str, &str)> = values.iter().map(|value| (*value, "TAX")).collect();
        assert_eq!(shown(source, &[]), pairs(&tax), "{source}");
    }
}

/// #563: labels joined by a slash leave their run of values to the
/// model, as before: a span over the whole run keeps its category and
/// its range, spans over each value stay TAX, and with no span nothing
/// is proposed. A budget code before "/ОКПО" is never ОКПО's value.
#[test]
fn state_register_slash_lists_stay_the_models() {
    for (source, run) in [
        (
            "ИНН/КПП/ОГРН 9901000017/990101001/1029900000017",
            "9901000017/990101001/1029900000017",
        ),
        (
            "ИНН / КПП / ОГРН 9901000017 / 990101001 / 1029900000017",
            "9901000017 / 990101001 / 1029900000017",
        ),
        (
            "ИНН/ОГРН 9901000017/1029900000017",
            "9901000017/1029900000017",
        ),
        (
            "ИНН/ОГРН: 9901000017/1029900000017",
            "9901000017/1029900000017",
        ),
        (
            "ОГРН/ИНН 1029900000017/9901000017",
            "1029900000017/9901000017",
        ),
        (
            "ОГРН/ИНН/КПП 1029900000017/9901000017/990101001",
            "1029900000017/9901000017/990101001",
        ),
        (
            "ОГРН/КПП 1029900000017/990101001",
            "1029900000017/990101001",
        ),
        ("ОГРН/ОКПО 1029900000017/99887766", "1029900000017/99887766"),
        ("ИНН/ОКПО 9901000017/99887766", "9901000017/99887766"),
        (
            "ИНН/КПП/ОКПО 9901000017/990101001/99887766",
            "9901000017/990101001/99887766",
        ),
        (
            "ИНН/ОГРНИП 990100001700/329990000000012",
            "990100001700/329990000000012",
        ),
    ] {
        assert_eq!(
            shown(source, &[(run, "tax_id")]),
            pairs(&[(run, "TAX")]),
            "{source}"
        );
        assert_eq!(shown(source, &[]), pairs(&[]), "{source}");
        let values: Vec<&str> = run.split('/').map(str::trim).collect();
        let raw: Vec<(&str, &str)> = values.iter().map(|value| (*value, "tax_id")).collect();
        let tax: Vec<(&str, &str)> = values.iter().map(|value| (*value, "TAX")).collect();
        assert_eq!(shown(source, &raw), pairs(&tax), "{source}");
    }
    let source = "ОКТМО/ОКПО 45382000/99887766";
    assert_eq!(shown(source, &[]), pairs(&[]));
    assert_eq!(
        shown(source, &[("99887766", "tax_id")]),
        pairs(&[("99887766", "TAX")])
    );
}

/// #563 guards: an ИНН stays TAX, a БИК and an account BANK; an amount
/// or a date beside an ОГРН is no part of it (#600); a run too short or
/// too long is no ОГРН or ОКПО; КБК, ОКТМО and other classifiers' codes
/// stay unmarked, whatever the model guessed.
#[test]
fn state_register_labels_take_nothing_else() {
    let source = "ИНН 9901000017, ОГРН 1029900000017 от 12.03.2031, БИК 049999999, \
        р/с 03100643000000019900; ОГРН 1199900000011 500 руб.";
    let raw = [
        ("9901000017", "tax_id"),
        ("1029900000017 от 12.03.2031", "national_id_number"),
        ("049999999", "bank_account"),
        ("03100643000000019900", "bank_account"),
        ("1199900000011 500", "tax_id"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("9901000017", "TAX"),
            ("1029900000017", "TAX"),
            ("049999999", "BANK"),
            ("03100643000000019900", "BANK"),
            ("1199900000011", "TAX"),
        ])
    );
    for source in [
        "ОГРН 10299000017",
        "ОГРН 10299000000170123",
        "ОКПО 998877",
        "ОКПО 998877665544",
        "ОГРН (при наличии) не указан",
        "номер 1029900000017",
    ] {
        assert_eq!(shown(source, &[]), pairs(&[]), "{source}");
    }
    let source = "КБК: 18210102010011000110, ОКТМО: 99701000, ОКАТО 99401000000, \
        ОКВЭД 47.11; (КБК 18210102010011000110, ОКТМО 99701000)";
    let raw = [
        ("18210102010011000110", "bank_account"),
        ("99701000", "tax_id"),
        ("99401000000", "tax_id"),
        ("47.11", "tax_id"),
        ("КБК 18210102010011000110", "bank_account"),
        ("ОКТМО 99701000", "national_id_number"),
    ];
    assert_eq!(shown(source, &raw), pairs(&[]));
}
