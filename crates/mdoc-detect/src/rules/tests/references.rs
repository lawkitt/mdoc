//! Legal references, cited cases and defined party codes.
use super::*;
use crate::rules::test_support::{pairs, shown, shown_at};
use mdoc_pii::Recognizer;

/// #546: defined party labels disappear before the review propagates
/// names, whether the model calls them people or organizations.
#[test]
fn defined_party_codes_are_not_names() {
    let source = "The claimants are Ivo Penrow (“C1”) and Edda Brannock (“C2”); \
        the defendants are Pellam Rooke Ltd (“D1”) and Harrow Ostrand (“D2”).\n\
        a. C1 sued D1; C1's trustees and D2 replied; D3 – D5 took no part.";
    for label in ["person", "organization"] {
        assert_eq!(
            shown(
                source,
                &[
                    ("Ivo Penrow", "person"),
                    ("Edda Brannock", "person"),
                    ("Pellam Rooke Ltd", "organization"),
                    ("Harrow Ostrand", "person"),
                    ("C1", label),
                    ("C2", label),
                    ("D1", label),
                    ("D2", label),
                    ("a. C1", label),
                    ("C1's trustees", label),
                    ("D3 – D5", label),
                ]
            ),
            pairs(&[
                ("Ivo Penrow", "PERSON"),
                ("Edda Brannock", "PERSON"),
                ("Pellam Rooke Ltd", "ORG"),
                ("Harrow Ostrand", "PERSON")
            ])
        );
    }
}

#[test]
fn party_code_definitions_keep_the_real_name_and_cover_each_series() {
    for (role, code, next) in [
        ("Claimant", "C1", "C12"),
        ("Defendant", "D1", "D12"),
        ("Plaintiff", "P1", "P12"),
        ("Respondent", "R1", "R12"),
    ] {
        for definition in [
            format!("Mrs Penrow ({code})"),
            format!("Pellam Ltd (\"{code}\", as appropriate)"),
            format!("{role}: {code}"),
            format!("{code} – {role}"),
        ] {
            let source = format!("{definition}. {role}: {next}; {next}’s trustees.\nc. {next}");
            assert_eq!(
                shown(
                    &source,
                    &[
                        (code, "person"),
                        (next, "organization"),
                        (&format!("{next}’s trustees"), "organization"),
                        (&format!("c. {next}"), "person")
                    ]
                ),
                [],
                "{source}"
            );
        }
    }
    assert_eq!(
        shown(
            "Mrs Penrow (C1) called C1.",
            &[("Mrs Penrow", "person"), ("C1", "person")]
        ),
        pairs(&[("Penrow", "PERSON")])
    );
    assert_eq!(
        shown(
            "Mrs Penrow (C1) called C1.",
            &[("Mrs Penrow (C1)", "person")]
        ),
        pairs(&[("Penrow", "PERSON")])
    );
    assert_eq!(
        shown(
            "Pellam Ltd (“D1”) sued D1.",
            &[("Pellam Ltd (“D1”)", "organization")]
        ),
        pairs(&[("Pellam Ltd", "ORG")])
    );
}

/// A role before a company name must not suppress its trading name.
#[test]
fn party_code_company_continuations_do_not_establish_a_series() {
    for (source, company) in [
        (
            "Claimant C1 Holdings Ltd, trading as C1, paid the invoice.",
            "C1 Holdings Ltd",
        ),
        ("Claimant C1 Holdings paid C1.", "C1 Holdings"),
        ("Claimant C1 Holdings2 paid C1.", "C1 Holdings2"),
        ("Claimant C1 trading as C1 paid the invoice.", "C1"),
        (
            "Claimant C1 international holdings ltd paid C1.",
            "C1 international holdings ltd",
        ),
        ("Claimant C1 of North Quay paid C1.", "C1 of North Quay"),
        ("Claimant C1 t/a C1 paid the invoice.", "C1"),
        (
            "Claimant C1 (international holdings) Ltd, trading as C1, paid the invoice.",
            "C1 (international holdings) Ltd",
        ),
    ] {
        // Use the later occurrence for the trading/short name: the
        // first C1 is inside the company's full name.
        let raw: Vec<_> = [
            (source.find(company).unwrap(), company),
            (source.rfind("C1").unwrap(), "C1"),
        ]
        .into_iter()
        .map(|(start, text)| crate::RawEntity {
            start,
            end: start + text.len(),
            label: "organization".into(),
            score: 0.8,
            text: text.into(),
        })
        .collect();
        let actual: Vec<_> = crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .map(|span| (source[span.range].to_owned(), span.category))
            .collect();
        assert_eq!(
            actual,
            pairs(&[(company, "ORG"), ("C1", "ORG")]),
            "{source}"
        );
    }
    // A company wrapped onto the next line, or with a space before its
    // legal-form comma, still opens with its code: the code marks stay
    // with only the code or short name marked by the model.
    for source in [
        "Claimant C1\nHoldings Ltd, trading as C1, paid.",
        "Claimant C1\r\nHoldings Ltd, trading as C1, paid.",
        "Claimant C1\nLogistics Ltd, trading as C1, paid.",
        "Claimant C1\nInternational Holdings Ltd, trading as C1, paid.",
        "Claimant C1\nholdings ltd paid C1.",
        "Claimant C1\nLtd, trading as C1, paid.",
        "Claimant C1 , Inc, trading as C1, paid.",
        "Claimant C1 , LLC, trading as C1, paid.",
        "Claimant C1, Inc, trading as C1, paid.",
    ] {
        for at in [source.find("C1").unwrap(), source.rfind("C1").unwrap()] {
            let raw = [crate::RawEntity {
                start: at,
                end: at + 2,
                label: "organization".into(),
                score: 0.8,
                text: "C1".into(),
            }];
            let actual: Vec<_> = crate::recognition::proposals(source, &raw)
                .unwrap()
                .into_iter()
                .map(|span| source[span.range].to_owned())
                .collect();
            assert!(actual.iter().any(|text| text == "C1"), "{source} at {at}");
        }
    }
    // A heading layout still defines its codes.
    assert_eq!(
        shown(
            "First Claimant\nC1\nSecond Claimant\nC2\nC1 lost.",
            &[("C1", "organization")]
        ),
        []
    );
    // A role and code before a lowercase clause still define a code.
    assert_eq!(
        shown(
            "Claimant C1 and Defendant D1 called C1.",
            &[("C1", "organization")]
        ),
        []
    );
}

#[test]
fn party_code_definition_removal_consumes_quotes_and_inner_spaces() {
    for bracket in [
        "('C1')",
        "( C1 )",
        "( 'C1' )",
        "(‘C1’)",
        "( ‘C1’ )",
        "(\"C1\")",
        "( \"C1\" )",
        "(“C1”)",
        "( “C1” )",
        "('C1', as appropriate)",
        "( ‘C1’, as appropriate )",
        "(C1, as appropriate)",
        "( C1, as appropriate )",
        "(\t'C1'\t)",
        "(\u{a0}“C1”\u{a0})",
    ] {
        let definition = format!("Mrs Penrow {bracket}");
        let source = format!("{definition} called C1.");
        let expected = pairs(&[("Penrow", "PERSON")]);
        assert_eq!(
            shown(&source, &[(&definition, "person")]),
            expected,
            "{source}"
        );
        for label in ["person", "organization"] {
            assert_eq!(
                shown(&source, &[("Mrs Penrow", "person"), (bracket, label)]),
                expected,
                "{source}: {label}"
            );
        }
        let company = format!("Pellam Ltd {bracket}");
        let source = format!("{company} called C1.");
        assert_eq!(
            shown(&source, &[(&company, "organization")]),
            pairs(&[("Pellam Ltd", "ORG")]),
            "{source}"
        );
    }
}

/// Uncertain labels, short real names, other code series and real
/// identifiers stay: the cost of losing a real value is higher.
#[test]
fn unexplained_codes_and_names_beside_party_codes_stay() {
    for source in [
        "C1 met D12 and R2.",
        "The claimant is C1 Ltd.",
        "Penrow (C100) called C1.",
        "The product C1 arrived.",
        "Claimant C1 Ltd called C1.",
        "Defendant C1, Inc called C1.",
    ] {
        assert!(
            shown(source, &[("C1", "organization")])
                .iter()
                .any(|(text, _)| text == "C1" || text == "C1 Ltd"),
            "{source}"
        );
    }
    assert_eq!(
        shown("Claimant C1 called C1.", &[("C1", "organization")]),
        []
    );
    let source = "Mrs Penrow (C1). D1 met R2; C100 and C1A attended with C1 Penrow. \
        Staff ID: C123. C1's trustee Ivo Penrow replied.\nМ1. О1.";
    assert_eq!(
        shown(
            source,
            &[
                ("D1", "person"),
                ("R2", "organization"),
                ("C100", "organization"),
                ("C1A", "organization"),
                ("C1 Penrow", "person"),
                ("C1's trustee Ivo Penrow", "person"),
                ("М1", "person"),
                ("О1", "person")
            ]
        ),
        pairs(&[
            ("D1", "PERSON"),
            ("R2", "ORG"),
            ("C100", "ORG"),
            ("C1A", "ORG"),
            ("C1 Penrow", "PERSON"),
            ("C123", "IDENTITY"),
            ("C1's trustee Ivo Penrow", "PERSON"),
            ("М1", "PERSON"),
            ("О1", "PERSON")
        ])
    );
    // A range with a series that was never established is uncertain.
    assert_eq!(
        shown("Claimant C1: C3 – R5.", &[("C3 – R5", "organization")]),
        pairs(&[("C3 – R5", "ORG")])
    );
}

/// Review #599: a defined series must not hide a real name or an
/// unexplained standalone code that may itself name an organization.
#[test]
fn established_series_keep_names_and_unexplained_members() {
    assert_eq!(
        shown(
            "Pellam Rooke Ltd (D1). The goods went to D3 Logistics Ltd.",
            &[("Pellam Rooke Ltd", "organization"), ("D3", "organization")]
        ),
        pairs(&[("Pellam Rooke Ltd", "ORG"), ("D3", "ORG")])
    );
    let source = "Mrs Penrow (C2). Claimant C1 Holdings Ltd, trading as C1.";
    let raw: Vec<_> = [
        (source.find("Penrow").unwrap(), "Penrow", "person"),
        (
            source.find("C1 Holdings Ltd").unwrap(),
            "C1 Holdings Ltd",
            "organization",
        ),
        (source.rfind("C1").unwrap(), "C1", "organization"),
    ]
    .into_iter()
    .map(|(start, text, label)| crate::RawEntity {
        start,
        end: start + text.len(),
        label: label.into(),
        score: 0.8,
        text: text.into(),
    })
    .collect();
    let actual: Vec<_> = crate::recognition::proposals(source, &raw)
        .unwrap()
        .into_iter()
        .map(|span| (source[span.range].to_owned(), span.category))
        .collect();
    assert_eq!(
        actual,
        pairs(&[
            ("Penrow", "PERSON"),
            ("C1 Holdings Ltd", "ORG"),
            ("C1", "ORG")
        ])
    );
    assert_eq!(
        shown(
            "Mrs Penrow (C1) watched C4.",
            &[("Mrs Penrow", "person"), ("C4", "organization")]
        ),
        pairs(&[("Penrow", "PERSON"), ("C4", "ORG")])
    );
    // Even the very same code can also open a name. Keep both its
    // full form and later short mentions when their meanings collide.
    for (source, name, label) in [
        (
            "Mrs Penrow (C1). C1 Holdings Ltd, trading as C1.",
            "C1 Holdings Ltd",
            "organization",
        ),
        (
            "Mrs Brannock (C1). C1 Penrow replied for C1.",
            "C1 Penrow",
            "person",
        ),
    ] {
        let raw: Vec<_> = [
            (source.find(name).unwrap(), name),
            (source.rfind("C1").unwrap(), "C1"),
        ]
        .into_iter()
        .map(|(start, text)| crate::RawEntity {
            start,
            end: start + text.len(),
            label: label.into(),
            score: 0.8,
            text: text.into(),
        })
        .collect();
        let actual = crate::recognition::proposals(source, &raw).unwrap();
        for entity in raw {
            assert!(
                actual
                    .iter()
                    .any(|span| span.range == (entity.start..entity.end)),
                "{source}: {}",
                entity.text
            );
        }
    }
    // The model may mark only the code at the start of a company,
    // even when that exact code has been defined for another party.
    for name in ["D3 Logistics Ltd", "D3 – Logistics Ltd"] {
        let source = format!("Pellam Rooke Ltd (D3). {name} received the goods.");
        let start = source.rfind("D3").unwrap();
        let raw = [crate::RawEntity {
            start,
            end: start + 2,
            label: "organization".into(),
            score: 0.8,
            text: "D3".into(),
        }];
        assert!(
            crate::recognition::proposals(&source, &raw)
                .unwrap()
                .iter()
                .any(|span| span.range == (start..start + 2)),
            "{source}"
        );
    }
}

#[test]
fn party_code_evidence_stops_at_clauses_and_ignores_titles() {
    for source in [
        "Claimant C1, Mrs Penrow, sued. C1 lost.",
        "First Claimant\nC1\nSecond Claimant\nC2",
        "Claimant C1 Mrs Penrow sued. C1 lost.",
    ] {
        assert!(
            shown(source, &[("C1", "organization")]).is_empty(),
            "{source}"
        );
    }
    assert_eq!(
        shown(
            "Claimant C1, Mrs Penrow, sued. C1 lost.",
            &[("Mrs Penrow", "person"), ("C1", "organization")]
        ),
        pairs(&[("Penrow", "PERSON")])
    );
    assert_eq!(
        shown(
            "Pellam Rooke plc (D1). D1 lost.",
            &[("Pellam Rooke plc", "organization"), ("D1", "organization")]
        ),
        pairs(&[("Pellam Rooke plc", "ORG")])
    );
}

#[test]
fn established_code_pairs_and_lists_go_only_as_whole_codes() {
    for list in ["C1 and C2", "D1, D2 and D3", "D1, D2, and D3", "C1 and D2"] {
        let source = format!("Mrs Penrow (C1). Pellam Ltd (D1). {list} replied.");
        for label in ["person", "organization"] {
            assert_eq!(
                shown(
                    &source,
                    &[
                        ("Mrs Penrow", "person"),
                        ("Pellam Ltd", "organization"),
                        (list, label)
                    ]
                ),
                pairs(&[("Penrow", "PERSON"), ("Pellam Ltd", "ORG")]),
                "{source}: {label}"
            );
        }
    }
    for list in [
        "C1 and R2",
        "C1 and C2 Penrow",
        "D1, D2 and D3 Logistics Ltd",
        "C1 and Ivo Penrow",
    ] {
        let source = format!("Mrs Penrow (C1). Pellam Ltd (D1). {list} replied.");
        assert!(
            shown(&source, &[(list, "organization")])
                .iter()
                .any(|(text, _)| text == list),
            "{source}"
        );
    }
}

#[test]
fn party_codes_use_the_decoded_document_and_keep_other_categories() {
    let source = "Mrs Penrow (&ldquo;C1&rdquo;).\n(a) C1’s trustees; C3—C5; staff C1.";
    assert_eq!(
        shown(
            source,
            &[("(a) C1’s trustees", "organization"), ("C3—C5", "person")]
        ),
        []
    );
    let raw = [crate::RawEntity {
        start: source.rfind("C1").unwrap(),
        end: source.rfind("C1").unwrap() + 2,
        label: "national_id_number".into(),
        score: 0.8,
        text: "C1".into(),
    }];
    assert!(
        crate::correct(source, &raw)
            .iter()
            .any(|d| d.category == Category::Identity)
    );
}

/// US and UK citations are no identifiers, addresses or names: reporter,
/// Westlaw and law-report citations, codes and their abbreviations,
/// sections, the record and a form's outline letter (guide rule 8,
/// issues #499 and #480).
#[test]
fn english_legal_references_are_never_proposed() {
    let source = "As held at 912 F.3d 455, 460 and 77 N.E.2d 310, and 424 U.S. at 38 \
        (527 U.S. 706; 33 A.D.3d 352; 904 F.2d 275, 277), under 26 U.S.C. § 6212, \
        31 U.S.C. 3903, I.R.C. § 61, 48 CFR 52.212-1, FAR 52.212-1, 34 CFR §300.8, \
        E.O. 13526, Release No. 34-98765, 2031 WL 1234567, [2031] STC 412 and ICR 801 \
        (Tr. 112-14; Tr. 435-36; ECF Doc. #12-3; 123sa; see n. 14; Id. at 41; \
        ROA.977), for tax year 2029/30, the U.S. District Court ruled.\n\nJ. Additional Questions\n";
    let raw = [
        ("912 F.3d 455, 460 and 77 N.E.2d 310", "address"),
        ("424 U.S. at 38", "address"),
        ("U.S.", "organization"),
        ("A.D.", "person"),
        ("904 F.2d 275, 277", "phone_number"),
        ("6212", "tax_id"),
        ("31 U.S.C. 3903", "bank_account"),
        ("S.C.", "person"),
        ("R.C.", "person"),
        ("48 CFR 52.212-1", "tax_id"),
        ("FAR 52.212-1", "tax_id"),
        ("300.8", "tax_id"),
        ("13526", "tax_id"),
        ("34-98765", "tax_id"),
        ("2031 WL 1234567", "tax_id"),
        ("STC 412", "tax_id"),
        ("ICR 801", "bank_account"),
        ("Tr. 112-14", "tax_id"),
        ("435-36", "tax_id"),
        ("#12-3", "tax_id"),
        ("123sa", "tax_id"),
        ("n. 14", "passport_number"),
        ("Id. at 41", "passport_number"),
        ("ROA.977", "national_id_number"),
        ("2029/30", "tax_id"),
        ("J.", "person"),
    ];
    assert_eq!(shown(source, &raw), []);
    assert_eq!(
        shown("Before the U.S. District Court.", &[("U.S.", "person")]),
        []
    );
    // The same abbreviation as a span alone, whatever its neighbours.
    for text in ["U.S.", "U.K.", " U.S.A. "] {
        assert!(!pack().identifying(text, Category::Organization), "{text}");
        assert!(!pack().identifying(text, Category::Person), "{text}");
    }
}

/// A real identifier beside a citation stays: a tax number, a case
/// number after its label, a phone, a company, a person, a university.
#[test]
fn identifiers_beside_a_citation_stay() {
    let source = "Under 26 U.S.C. § 6212 (EIN 98-7654321), Case No. 1:26-cv-01234 \
        (citing 424 U.S. at 38), call +1 (202) 555-0164 (904 F.2d 275, 277). \
        U.S. Bank paid; USC agreed; GB 123 4567 89 is the VAT number.\n\n\
        J. Ostrand\n";
    let raw = [
        ("EIN 98-7654321", "tax_id"),
        ("+1 (202) 555-0164", "phone_number"),
        ("U.S. Bank", "organization"),
        ("USC", "organization"),
        ("GB 123 4567 89", "tax_id"),
        ("J. Ostrand", "person"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("98-7654321", "TAX"),
            ("1:26-cv-01234", "CASE"),
            ("+1 (202) 555-0164", "PHONE"),
            ("U.S. Bank", "ORG"),
            ("USC", "ORG"),
            ("GB 123 4567 89", "TAX"),
            ("J. Ostrand", "PERSON"),
        ])
    );
    // A tax year is no tax number; a number in another shape stays.
    assert!(!pack().identifying("2014/15", Category::Tax));
    assert!(!pack().identifying("2014-2015", Category::Tax));
    assert!(pack().identifying("98-7654321", Category::Tax));
    assert!(pack().identifying("123-45-6789", Category::Tax));
}

/// Russian law, article and court-act numbers are no tax numbers; the
/// INN beside them stays (guide rule 8, issue #480).
#[test]
fn russian_legal_references_are_never_proposed() {
    let source = "на основании Федерального закона от 26.10.2002 № 127-ФЗ, Федерального \
        закона № 999-ФЗ и статьи 431 ГК РФ, ст. 428 ГК РФ, п. 2 ст. 450 ГК РФ, \
        Определения Верховного Суда РФ от 12.03.2031 № 305-ЭС31-1234 и УПД № 412, \
        ИНН 7701234567, тел. +7 (495) 123-45-67";
    let raw = [
        ("127-ФЗ", "tax_id"),
        ("999-ФЗ", "tax_id"),
        ("431 ГК РФ", "tax_id"),
        ("428 ГК РФ", "tax_id"),
        ("450", "tax_id"),
        ("305-ЭС31-1234", "tax_id"),
        ("412", "tax_id"),
        ("7701234567", "tax_id"),
        ("+7 (495) 123-45-67", "phone_number"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[("7701234567", "TAX"), ("+7 (495) 123-45-67", "PHONE")])
    );
    // A court's own words may sit between the act and its number.
    let source = "Определение судебной коллегии по экономическим спорам Верховного Суда РФ \
        от 12.03.2031 № 305-ЭС31-1234, Приказ Минфина России от 01.02.2031 № 12н";
    assert_eq!(
        shown(source, &[("305-ЭС31-1234", "tax_id"), ("12н", "tax_id")]),
        []
    );
    // A value between an act and a "№" stays, and so does a number
    // after "№" that is no act's.
    let source = "Постановление в отношении ООО «Вега», ИНН 7701234567, № 17";
    assert_eq!(
        shown(source, &[("7701234567", "tax_id")]),
        pairs(&[("7701234567", "TAX")])
    );
    let source = "Постановление направлено владельцу паспорта № 4512345678.";
    assert_eq!(
        shown(source, &[("4512345678", "passport_number")]),
        pairs(&[("4512345678", "IDENTITY")])
    );
}

/// Close variants of the English citations #511 covers are no tax,
/// bank or identity values either: codes without spaces or dots, state
/// statutes, the Civil Procedure Rules by rule or part, agencies'
/// letters and rulings, a running header's entry and page, and years
/// (guide rules 4 and 8, issue #480).
#[test]
fn english_citation_variants_are_never_proposed() {
    let source = "Under 19CFR141.37, 42 USC Section 1981, 42 U.S.C. Sec. 1983, NRS 41.1395, \
        N.R.S. § 41.085, 54 FR 35305, 49FR17754, CPR r71.8, CPR r.38.4, CPR r 71.5, CPR \
        Part 35 and the CPR, Part 19.12 (Release Number 34-52926; FDIC FIL-111-2006; OTS \
        No. 2006-49; CEQ # 20060476; T.D. 84-93; Rev. Rul. 2031-12; PLR 203112001; Fed. R. \
        Civ. P. 12(b)(6)). In Reply Refer To: AESO/SE 22410-2005-F-0506.\n\
        15-49008-wsd Doc 19-1 Filed 06/12/15\nTXSD Page 7 of 8\n\
        For the years 2013 through 2016 and in March 2023 nothing changed.";
    let raw = [
        ("19CFR141.37", "tax_id"),
        ("42 USC Section 1981", "tax_id"),
        ("1983", "bank_account"),
        ("NRS 41.1395", "tax_id"),
        ("41.085", "tax_id"),
        ("54 FR 35305", "tax_id"),
        ("49FR17754", "tax_id"),
        ("CPR r71.8", "national_id_number"),
        ("r.38.4", "tax_id"),
        ("CPR r 71.5", "national_id_number"),
        ("CPR Part 35", "passport_number"),
        ("Part 19.12", "tax_id"),
        ("Release Number 34-52926", "bank_account"),
        ("FIL-111-2006", "tax_id"),
        ("OTS No. 2006-49", "tax_id"),
        ("CEQ # 20060476", "tax_id"),
        ("T.D. 84-93", "tax_id"),
        ("Rev. Rul. 2031-12", "tax_id"),
        ("203112001", "tax_id"),
        ("12(b)(6)", "tax_id"),
        ("AESO/SE 22410-2005-F-0506", "organization"),
        ("15-49008-wsd", "tax_id"),
        ("Doc 19-1", "tax_id"),
        ("TXSD Page 7 of 8", "tax_id"),
        ("2013 through 2016", "tax_id"),
        ("2023", "tax_id"),
    ];
    let failures: Vec<String> = raw
        .iter()
        .filter_map(|span| {
            let actual = shown(source, &[*span]);
            (!actual.is_empty()).then(|| format!("{span:?}: {actual:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // A year alone is no tax number; a real one stays.
    for year in ["2023", " 2013 through 2016 ", "2029 and 2030"] {
        assert!(!pack().identifying(year, Category::Tax), "{year:?}");
    }
    for value in ["98-7654321", "123456789", "123-45-6789", "20231"] {
        assert!(pack().identifying(value, Category::Tax), "{value:?}");
    }
}

/// A docket number is the case at hand's after "No." (CASE, with its
/// repeats), a cited case's inside its citation, and never a tax, bank
/// or identity value (guide rule 5, #506 item 3, issue #480).
#[test]
fn docket_numbers_are_case_numbers_or_nothing() {
    let source = "Ivo Harrow v. Quillon Freight LLC\nNo. 09-31-00999-CV\n\n\
        See Pellam v. Ostrand, No. 09-30-00111-CV (Tex. App. 2030), and 1:31-cv-00999-ZZ \
        (see 02 CV 3651). Index No. 06 CV 15490 is the related action. 31-99999-zzz Doc 7 \
        Filed 06/12/31.\n09-31-00999-CV Page 2 of 3\n";
    let raw = [
        ("Ivo Harrow", "person"),
        ("Quillon Freight LLC", "organization"),
        ("09-31-00999-CV", "tax_id"),
        ("09-30-00111-CV", "tax_id"),
        ("1:31-cv-00999-ZZ", "tax_id"),
        ("02 CV 3651", "bank_account"),
        ("06 CV 15490", "tax_id"),
        ("31-99999-zzz", "national_id_number"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("Ivo Harrow", "PERSON"),
            ("Quillon Freight LLC", "ORG"),
            ("09-31-00999-CV", "CASE"),
            ("06 CV 15490", "CASE"),
            ("09-31-00999-CV", "CASE"),
        ])
    );
    // A bankruptcy header's case number is CASE where a label names it,
    // without the entry beside it.
    let source = "Case: 15-49008-wsd\n15-49008-wsd Doc 19-1 Filed 06/12/15\n";
    assert_eq!(
        shown(source, &[("15-49008-wsd Doc 19-1", "tax_id")]),
        pairs(&[("15-49008-wsd", "CASE"), ("15-49008-wsd", "CASE")])
    );
    for (value, category) in [
        ("01-24-00779-CV", Category::Tax),
        ("1:21-cv-00616-RP", Category::Tax),
        ("02 CV 3651", Category::Bank),
        ("15-49008-wsd Doc 19-1", Category::Tax),
        ("01-24-00779-CV", Category::Identity),
    ] {
        assert!(!pack().identifying(value, category), "{value:?}");
    }
}

/// Beside each English citation a real value and a party stay: a tax
/// number, an account, a phone, a passport, a company and a person
/// (issue #480).
#[test]
fn identifiers_beside_citation_variants_stay() {
    let source = "Under 19CFR141.37 Quillon Freight LLC (EIN 98-7654321) paid from account \
        number 12345678; NRS 41.1395 binds Ivo Harrow, tel. +1 (202) 555-0164, passport \
        123456789 (CPR r71.8); Release Number 34-52926, SSN 123-45-6789; for the years \
        2013 through 2016 (TXSD Page 7 of 8) Rowan Pell paid.";
    let raw = [
        ("Quillon Freight LLC", "organization"),
        ("98-7654321", "tax_id"),
        ("12345678", "bank_account"),
        ("Ivo Harrow", "person"),
        ("+1 (202) 555-0164", "phone_number"),
        ("123456789", "passport_number"),
        ("123-45-6789", "tax_id"),
        ("Rowan Pell", "person"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("Quillon Freight LLC", "ORG"),
            ("98-7654321", "TAX"),
            ("12345678", "BANK"),
            ("Ivo Harrow", "PERSON"),
            ("+1 (202) 555-0164", "PHONE"),
            ("123456789", "IDENTITY"),
            ("123-45-6789", "TAX"),
            ("Rowan Pell", "PERSON"),
        ])
    );
}

/// Close variants of the Russian references #511 covers: a law's number
/// spaced, reversed or in lower case, an article of a code by its full
/// name, a municipal act after a quoted name, a letter's number whole or
/// in pieces, a transfer document with its word, a trading protocol's
/// number and a date's month (guide rules 4 and 8, issue #480).
#[test]
fn russian_reference_variants_are_never_proposed() {
    let source = "Постановление Администрации Энского района «Кедровый» от 25.03.2031 № \
        833-ПА, по УПД № 137 от 06.06.2031, протоколу об определении участников торгов \
        144355-МЭТС/1, Письмо Минфина России от 30.04.2031 N 52н, письмо № \
        02-09-11/05-07-31116 и № 06-18/27628, Закон РФ «О защите прав потребителей» от \
        07.02.1992 № 2300-1, Положение Банка России от 29.06.2031 N 762-П, ст. 428 \
        Гражданского кодекса РФ, ст. 12.8 Кодекса Российской Федерации об \
        административных правонарушениях, Федерального закона № 999 - ФЗ, ФЗ-127 и 44-фз.";
    let raw = [
        ("03", "bank_account"),
        ("833-ПА", "national_id_number"),
        ("УПД № 137", "tax_id"),
        ("144355-МЭТС/1", "tax_id"),
        ("52н", "tax_id"),
        ("05-07-31116", "bank_account"),
        ("06-18/27628", "tax_id"),
        ("2300-1", "tax_id"),
        ("762-П", "national_id_number"),
        ("428 Гражданского кодекса РФ", "tax_id"),
        ("12.8 Кодекса Российской Федерации", "tax_id"),
        ("999 - ФЗ", "tax_id"),
        ("ФЗ-127", "tax_id"),
        ("44-фз", "tax_id"),
    ];
    let failures: Vec<String> = raw
        .iter()
        .filter_map(|span| {
            let actual = shown(source, &[*span]);
            (!actual.is_empty()).then(|| format!("{span:?}: {actual:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Beside each Russian reference real values, a party and the case's
/// own number stay; a page number comes off the case number's repeat,
/// and a court case's number after "№" is no letter's (issue #480).
#[test]
fn identifiers_beside_russian_reference_variants_stay() {
    let source = "Дело № А28-3207/2031. Постановление Администрации Энского района \
        «Кедровый» от 25.03.2031 № 833-ПА получила Лидия Брюн, ИНН 7701234567, КПП \
        770101001, ОГРН 1027700132195, р/с 40702810900000012345 от 30.03.2031, паспорт \
        4512 345678, тел. +7 (495) 123-45-67; по УПД № 137 ООО «Кедровый бор».\n\
        3 А28-3207/2031\n";
    let raw = [
        ("Лидия Брюн", "person"),
        ("7701234567", "tax_id"),
        ("770101001", "tax_id"),
        ("1027700132195", "tax_id"),
        ("40702810900000012345", "bank_account"),
        ("4512 345678", "passport_number"),
        ("+7 (495) 123-45-67", "phone_number"),
        ("ООО «Кедровый бор»", "organization"),
        ("3 А28-3207/2031", "tax_id"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("А28-3207/2031", "CASE"),
            ("Лидия Брюн", "PERSON"),
            ("7701234567", "TAX"),
            ("770101001", "TAX"),
            // ОГРН is TAX like ИНН (#484 item 6, #563).
            ("1027700132195", "TAX"),
            ("40702810900000012345", "BANK"),
            ("4512 345678", "IDENTITY"),
            ("+7 (495) 123-45-67", "PHONE"),
            ("ООО «Кедровый бор»", "ORG"),
            ("А28-3207/2031", "CASE"),
        ])
    );
    // A court case's number after "№" is no letter's: the model's span
    // stays, as do a dotted account and an appellate number.
    let source = "Решение суда № 2-1842/2031, счёт 40817.810.12.03.2031, № 09АП-1234/2031";
    assert_eq!(
        shown(
            source,
            &[
                ("2-1842/2031", "national_id_number"),
                ("40817.810.12.03.2031", "bank_account"),
                ("09АП-1234/2031", "tax_id"),
            ]
        ),
        pairs(&[
            ("2-1842/2031", "IDENTITY"),
            ("40817.810.12.03.2031", "BANK"),
            ("09АП-1234/2031", "TAX"),
        ])
    );
}

/// A source, the model's spans in it (each wording's `n`th mention)
/// and what the review should show.
type Shown<'a> = (
    &'a str,
    &'a [(&'a str, usize, &'a str)],
    &'a [(&'a str, &'a str)],
);

/// Each case's proposals against what it should show; the failures,
/// one line each.
fn failures(cases: &[Shown<'_>]) -> Vec<String> {
    cases
        .iter()
        .filter_map(|(source, raw, expected)| {
            let actual = shown_at(source, raw);
            (actual != pairs(expected)).then(|| format!("{source:?}: {actual:?}, not {expected:?}"))
        })
        .collect()
}

/// A court's docket number the model takes for a tax, bank or identity
/// value is a case number under any label, in a list or alone, and
/// never goes unmarked (guide rule 5, #480 review).
#[test]
fn docket_numbers_under_any_label_stay_case_numbers() {
    let cases: &[Shown<'_>] = &[
        (
            "UNITED STATES DISTRICT COURT\nIvo Harrow v. Quillon Freight LLC\nDocket: \
            1:21-cv-00616-RP\n",
            &[("1:21-cv-00616-RP", 0, "tax_id")],
            &[("1:21-cv-00616-RP", "CASE")],
        ),
        (
            "Docket Number: 1:21-cv-00616-RP\n",
            &[("1:21-cv-00616-RP", 0, "bank_account")],
            &[("1:21-cv-00616-RP", "CASE")],
        ),
        (
            "NUMBER 13-21-00234-CV\n\nCOURT OF APPEALS\nIvo Harrow, Appellant, v. Quillon \
            Freight LLC, Appellee.\n",
            &[("13-21-00234-CV", 0, "tax_id")],
            &[("13-21-00234-CV", "CASE")],
        ),
        (
            "NUMBER 13-21-00234-CV\n\nIvo Harrow, Appellant, v. Quillon Freight LLC, \
            Appellee.\n\nWe affirm in 13-21-00234-CV.\n",
            &[
                ("13-21-00234-CV", 0, "tax_id"),
                ("13-21-00234-CV", 1, "tax_id"),
            ],
            &[("13-21-00234-CV", "CASE"), ("13-21-00234-CV", "CASE")],
        ),
        (
            "NUMBERS 13-21-00234-CV & 13-21-00235-CV\n\nCOURT OF APPEALS\n",
            &[
                ("13-21-00234-CV", 0, "tax_id"),
                ("13-21-00235-CV", 0, "tax_id"),
            ],
            &[("13-21-00234-CV", "CASE"), ("13-21-00235-CV", "CASE")],
        ),
        (
            "NOS. 01-24-00779-CV, 01-24-00780-CV\n",
            &[
                ("01-24-00779-CV", 0, "tax_id"),
                ("01-24-00780-CV", 0, "tax_id"),
            ],
            &[("01-24-00779-CV", "CASE"), ("01-24-00780-CV", "CASE")],
        ),
        (
            "Nos. 01-24-00779-CV & 01-24-00780-CV\n",
            &[
                ("01-24-00779-CV", 0, "tax_id"),
                ("01-24-00780-CV", 0, "tax_id"),
            ],
            &[("01-24-00779-CV", "CASE"), ("01-24-00780-CV", "CASE")],
        ),
        (
            "Case Nos. 1:21-cv-00616-RP, 1:21-cv-00617-RP\n",
            &[
                ("1:21-cv-00616-RP", 0, "tax_id"),
                ("1:21-cv-00617-RP", 0, "tax_id"),
            ],
            &[("1:21-cv-00616-RP", "CASE"), ("1:21-cv-00617-RP", "CASE")],
        ),
        (
            "Cause Number 01-24-00779-CV\nIvo Harrow v. Quillon Freight LLC\n",
            &[("01-24-00779-CV", 0, "national_id_number")],
            &[("01-24-00779-CV", "CASE")],
        ),
        (
            "Civil Action 1:21-cv-00616-RP\nIvo Harrow v. Quillon Freight LLC\n",
            &[("1:21-cv-00616-RP", 0, "tax_id")],
            &[("1:21-cv-00616-RP", "CASE")],
        ),
        (
            "Index # 06 CV 15490\nIvo Harrow v. Quillon Freight LLC\n",
            &[("06 CV 15490", 0, "tax_id")],
            &[("06 CV 15490", "CASE")],
        ),
        (
            "In re Ivo Harrow, Debtor.\nBankruptcy No. 15-49008-wsd\nChapter 7\n",
            &[("15-49008-wsd", 0, "tax_id")],
            &[("15-49008-wsd", "CASE")],
        ),
        (
            "Ivo Harrow, Plaintiff, v. Quillon Freight LLC, Defendant.\nAdv. No. \
            15-04123-wsd\n",
            &[("15-04123-wsd", 0, "tax_id")],
            &[("15-04123-wsd", "CASE")],
        ),
        (
            "Adv. Pro. No. 31-01992-wsd\n",
            &[("31-01992-wsd", 0, "tax_id")],
            &[("31-01992-wsd", "CASE")],
        ),
        (
            "In re Ivo Harrow, Debtor. Chapter 7, Bankruptcy 15-49008-wsd\n",
            &[("15-49008-wsd", 0, "tax_id")],
            &[("15-49008-wsd", "CASE")],
        ),
        (
            "No. 01-24-00779-CR\nThe State of Texas v. Ivo Harrow\n",
            &[("01-24-00779-CR", 0, "tax_id")],
            &[("01-24-00779-CR", "CASE")],
        ),
        // "No." inside the model's span stays outside the case number.
        (
            "NO. 14-31-00999-CV\n",
            &[("NO. 14-31-00999-CV", 0, "tax_id")],
            &[("14-31-00999-CV", "CASE")],
        ),
    ];
    let failed = failures(cases);
    assert!(failed.is_empty(), "{}", failed.join("\n"));
    // Russian: a case number under another label or none is CASE too,
    // its page number left out.
    let cases: &[Shown<'_>] = &[
        (
            "Апелляционное производство № 33-1234/2031\nСудья Брюн Л.\n",
            &[("33-1234/2031", 0, "tax_id")],
            &[("33-1234/2031", "CASE")],
        ),
        (
            "Материал № 13-123/2031\n",
            &[("13-123/2031", 0, "tax_id")],
            &[("13-123/2031", "CASE")],
        ),
        (
            "№ 2-1842/2031\nРЕШЕНИЕ\nИменем Российской Федерации\n",
            &[("2-1842/2031", 0, "tax_id")],
            &[("2-1842/2031", "CASE")],
        ),
        (
            "Апелляционное производство № 33-1234/2031\nДело № 2-1842/2031\n",
            &[("33-1234/2031", 0, "tax_id"), ("2-1842/2031", 0, "tax_id")],
            &[("33-1234/2031", "CASE"), ("2-1842/2031", "CASE")],
        ),
        (
            "Определение Арбитражного суда Кировской области от 01.03.2031 № \
            А28-3207/2031\n",
            &[("А28-3207/2031", 0, "tax_id")],
            &[("А28-3207/2031", "CASE")],
        ),
        (
            "3 А28-3207/2031\n",
            &[("3 А28-3207/2031", 0, "tax_id")],
            &[("А28-3207/2031", "CASE")],
        ),
    ];
    let failed = failures(cases);
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// A Russian act's, letter's or law's lead before "№" takes no account,
/// БИК, card, passport or tax number after an author's word, and no
/// surname that starts like "Закон"; an arbitration court's case number
/// after an act stays the model's (#480 review).
#[test]
fn values_after_an_act_lead_stay() {
    let cases: &[Shown<'_>] = &[
        (
            "Письмо ООО «Ромашка» Счёт N 40702810900000012345\n",
            &[("40702810900000012345", 0, "bank_account")],
            &[("40702810900000012345", "BANK")],
        ),
        (
            "Письмо ПАО Сбербанк БИК № 044525225\n",
            &[("044525225", 0, "bank_account")],
            &[("044525225", "BANK")],
        ),
        (
            "Письмо Брюн Л. Карта № 4276123456789012\n",
            &[("4276123456789012", 0, "bank_account")],
            &[("4276123456789012", "BANK")],
        ),
        (
            "Положение Сотрудника Брюн Л. № 4276123456789012\n",
            &[("4276123456789012", 0, "bank_account")],
            &[("4276123456789012", "BANK")],
        ),
        (
            "Законов Иван Петрович N 4512345678, проживает в г. Кирове.\n",
            &[
                ("Законов Иван Петрович", 0, "person"),
                ("4512345678", 0, "passport_number"),
            ],
            &[
                ("Законов Иван Петрович", "PERSON"),
                ("4512345678", "IDENTITY"),
            ],
        ),
        (
            "Письмо Минфина России от 30.03.2031 № 7701234567\n",
            &[("7701234567", 0, "tax_id")],
            &[("7701234567", "TAX")],
        ),
        (
            "передаточному документу N 40702810900000012345\n",
            &[("40702810900000012345", 0, "bank_account")],
            &[("40702810900000012345", "BANK")],
        ),
        (
            "Определение Арбитражного суда Кировской области от 01.03.2031 № \
            А28-3207/2031\n",
            &[("А28-3207/2031", 0, "national_id_number")],
            &[("А28-3207/2031", "IDENTITY")],
        ),
    ];
    let failed = failures(cases);
    assert!(failed.is_empty(), "{}", failed.join("\n"));
    // The references themselves still go.
    let source = "Закон РФ от 07.02.1992 № 2300-1, Законом № 999-ФЗ, Письмо Минфина России \
        от 30.03.2031 № 02-09-11/05-03-99999, Определение Верховного Суда РФ от \
        12.03.2031 № 305-ЭС31-1234.";
    for (text, label) in [
        ("2300-1", "tax_id"),
        ("999-ФЗ", "tax_id"),
        ("02-09-11/05-03-99999", "bank_account"),
        ("305-ЭС31-1234", "tax_id"),
    ] {
        assert_eq!(shown(source, &[(text, label)]), Vec::new(), "{text:?}");
    }
    // A date's month after «от» is no account, its year short or not.
    for source in ["письмо от 30.03.15 N 52н.", "письмо от 30.03.2015 N 52н."] {
        assert_eq!(
            shown(source, &[("03", "bank_account")]),
            Vec::new(),
            "{source:?}"
        );
    }
}

/// A running header's page count, a Treasury Decision, a dotted date,
/// the Federal Register and a header's entry number each take only
/// their own shape: a party's name, a phone, a sort code, an address
/// and an identity number beside a look-alike stay (#480 review).
#[test]
fn neighbours_of_citation_look_alikes_stay() {
    let cases: &[Shown<'_>] = &[
        (
            "DECLARATION OF HARROW Page 2 of 8\n",
            &[("HARROW", 0, "person")],
            &[("HARROW", "PERSON")],
        ),
        (
            "Prepared for ACME Page 2 of 8\n",
            &[("ACME", 0, "organization")],
            &[("ACME", "ORG")],
        ),
        (
            "Contacts: Smith, T.D. 555-0164; Jones, A.B. 555-0199.\n",
            &[
                ("555-0164", 0, "phone_number"),
                ("555-0199", 0, "phone_number"),
            ],
            &[("555-0164", "PHONE"), ("555-0199", "PHONE")],
        ),
        (
            "Sort code: 20.03.12\nAccount number: 12345678\n",
            &[
                ("20.03.12", 0, "bank_account"),
                ("12345678", 0, "bank_account"),
            ],
            &[("20.03.12", "BANK"), ("12345678", "BANK")],
        ),
        (
            "Pay to Ivo Harrow, sort code 20.03.12, account 12345678.\n",
            &[
                ("20.03.12", 0, "bank_account"),
                ("12345678", 0, "bank_account"),
            ],
            &[("20.03.12", "BANK"), ("12345678", "BANK")],
        ),
        (
            "Счёт 12.03.31 банк\n",
            &[("12.03.31", 0, "bank_account")],
            &[("12.03.31", "BANK")],
        ),
        (
            "Quillon Freight LLC, Unit 12 FR 75008 Paris\n",
            &[("75008", 0, "address")],
            &[("75008", "ADDRESS")],
        ),
        (
            "Identity Document 12345 Page 2 of the file.\n",
            &[("12345", 0, "national_id_number")],
            &[("12345", "IDENTITY")],
        ),
    ];
    let failed = failures(cases);
    assert!(failed.is_empty(), "{}", failed.join("\n"));
    // The look-alikes themselves still go.
    let source = "TXSD Page 7 of 8; T.D. 84-93; T.D. 9876; 54 FR 35305, 35310; 49FR17754 \
        at 2; Document 12 Filed 06/12/21; от 30.03.2015 N 52н.";
    for (text, label) in [
        ("TXSD Page 7 of 8", "tax_id"),
        ("84-93", "tax_id"),
        ("9876", "tax_id"),
        ("54 FR 35305", "tax_id"),
        ("49FR17754", "tax_id"),
        ("Document 12", "national_id_number"),
        ("03", "bank_account"),
    ] {
        assert_eq!(shown(source, &[(text, label)]), Vec::new(), "{text:?}");
    }
}

/// A citation is found as detection reads it: through character
/// references and across a soft line break. A company named like a
/// code keeps its name; a blank line, a table row or a list item ends
/// the citation (#480, #499).
#[test]
fn citations_wrapped_or_encoded_are_found_and_neighbours_stay() {
    for source in [
        "under 26 U.S.C. &sect; 6212 the notice",
        "under 26 U.S.C.\n6212 the notice",
        "under 26 U.S.C.\r\n6212 the notice",
    ] {
        assert_eq!(shown(source, &[("6212", "tax_id")]), [], "{source:?}");
    }
    assert_eq!(
        shown(
            "see 904 F.2d\n275, 277, and",
            &[("904 F.2d\n275, 277", "phone_number")]
        ),
        []
    );
    assert_eq!(
        shown("ООО «C.F.R.» paid.", &[("C.F.R.", "organization")]),
        pairs(&[("ООО «C.F.R.»", "ORG")])
    );
    for source in [
        "Under 26 U.S.C.\n\n123-45-6789 is the number.",
        "Under 26 U.S.C.\r\n\r\n123-45-6789 is the number.",
        "## Under 26 U.S.C.\n123-45-6789 is the number.",
        "Under 26 U.S.C.\n1. 123-45-6789 is the number.",
    ] {
        assert_eq!(
            shown(source, &[("123-45-6789", "tax_id")]),
            pairs(&[("123-45-6789", "TAX")]),
            "{source:?}"
        );
    }
}

/// "& Anor" and "& Ors" come off a party and name no one; a declaration
/// is its declarant's, a person's (issue #499).
#[test]
fn citation_words_and_declarations_are_no_companies() {
    let source = "The claim by Pellow Holdings Ltd & Anor and Vey Ltd & Ors fails. Mr Ivo \
        Ostrand confirms it (Ostrand Decl. ¶ 3; Ruel Decl. ¶ 4).";
    let raw = [
        ("Pellow Holdings Ltd & Anor", "organization"),
        ("Vey Ltd", "organization"),
        ("Ors", "organization"),
        ("Ivo Ostrand", "person"),
        ("Ostrand Decl", "organization"),
        ("Ruel Decl", "organization"),
    ];
    // "Ruel" is no known person: the span stays as the model gave it.
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("Pellow Holdings Ltd", "ORG"),
            ("Vey Ltd", "ORG"),
            ("Ivo Ostrand", "PERSON"),
            ("Ostrand", "PERSON"),
            ("Ruel Decl", "ORG"),
        ])
    );
    assert!(!pack().identifying("Anor", Category::Person));
    assert!(!pack().identifying("Ors", Category::Organization));
}

/// Issue #499's follow-up probes: citation parts name nobody, while
/// the declarant and people following a reporter keep their full names.
#[test]
fn citation_parts_follow_up_probes() {
    let mut failures = Vec::new();
    for (source, text, label) in [
        ("See 424 U.S. at 38.", "424 U.S. at 38", "address"),
        (
            "See 904 F.2d 275, 277.",
            "904 F.2d 275, 277",
            "phone_number",
        ),
        ("See 33 A.D.3d 352.", "A.D.", "person"),
        ("Under 42 U.S.C. § 1983.", "S.C.", "person"),
        ("Under I.R.C. § 61.", "I.R.C.", "person"),
        (
            "See n. 14; Tr. 112-14; 123sa.",
            "n. 14",
            "national_id_number",
        ),
        (
            "Under Chapter 7 of the Bankruptcy Code.",
            "Chapter 7",
            "organization",
        ),
        ("Chapter 11 Case", "Chapter 11", "organization"),
        ("R (on the application of Penrow)", "R", "person"),
        ("See at p. 478A.", "478A", "address"),
        ("See 1931 S.C. (H.L.) 13.", "S.C.", "person"),
        ("See 1931 S.C. (H.L.) 13.", "H.L", "person"),
        ("See [2031] All ER (Comm) 9.", "Comm", "organization"),
        (
            "Under National Instrument 45-999.",
            "National Instrument 45-999",
            "national_id_number",
        ),
        ("Under PD 57ZZ.", "PD 57ZZ", "organization"),
        (
            "See 352 DM - Aviation Safety.",
            "352 DM - Aviation Safety",
            "organization",
        ),
        (
            "See AHCP/CCAA p. S-19.",
            "AHCP/CCAA p. S-19",
            "organization",
        ),
        ("See Decl. of Ivo Penrow ¶ 4.", "Decl", "person"),
        ("See Decl. ¶ 4.", "Decl", "person"),
        ("See Dep. of Ivo Penrow at 4.", "Dep", "person"),
        ("See Aff. of Ivo Penrow ¶ 4.", "Aff", "person"),
        ("See Ex. 1 to Penrow Decl.", "Ex. 1", "person"),
        ("Under Tex. R. Civ. P. 91a.", "Tex. R. Civ", "organization"),
        ("Under Tex. R. App. P. 9.", "Tex. R. App", "organization"),
        ("Under Fed. R. Civ. P. 12.", "Fed. R. Civ", "organization"),
        (
            "See (Tex. App.—Slocombe 2031, pet. filed).",
            "Slocombe 2031",
            "address",
        ),
        ("See Section XII.", "Section XII", "organization"),
        (
            "As held in Pellam v. Ostrand, 999 S.W.3d 1 (Tex. App.—Slocombe 2031, pet. filed), No. 09-31-00888-CV.",
            "09-31-00888-CV",
            "national_id_number",
        ),
    ] {
        let actual = shown(source, &[(text, label)]);
        if !actual.is_empty() {
            failures.push(format!("{source:?}: {actual:?}"));
        }
    }
    for (source, text, expected) in [
        ("(Ostrand Decl. ¶ 3)", "Ostrand Decl", "Ostrand"),
        ("(Penrow Dep. at 4)", "Penrow Dep", "Penrow"),
        ("(Penrow Aff. ¶ 4)", "Penrow Aff", "Penrow"),
        (
            "[2031] 1 W.L.R. 12, Ivo Ostrand LJ",
            "Ivo Ostrand",
            "Ivo Ostrand",
        ),
        (
            "[2031] 1 W.L.R. Ivo Ostrand LJ",
            "Ivo Ostrand",
            "Ivo Ostrand",
        ),
    ] {
        let actual = shown(source, &[(text, "person")]);
        let wanted = pairs(&[(expected, "PERSON")]);
        if actual != wanted {
            failures.push(format!("{source:?}: {actual:?}, expected {wanted:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A capital S followed by a Roman-looking initial names a person,
/// including when the model leaves the last period outside its span.
#[test]
fn citation_parts_keep_witness_initials() {
    let source = "The witness S. M. testified.";
    for name in ["S. M", "S. M."] {
        assert_eq!(
            shown(source, &[(name, "person")]),
            pairs(&[(name, "PERSON")])
        );
    }
    for reference in ["Section XII", "Sec. XII", "§ XII", "s. 12", "s. 12(a)"] {
        let source = format!("See {reference}.");
        assert!(
            shown(&source, &[(reference, "organization")]).is_empty(),
            "{source}"
        );
    }
}

/// A company's full name protects its short name throughout the
/// document, including when it resembles a chapter reference.
#[test]
fn citation_parts_keep_established_company_short_names() {
    let source = "Chapter 11 Ltd supplied the books.\nChapter 11 refused repayment.";
    assert_eq!(
        shown_at(
            source,
            &[
                ("Chapter 11 Ltd", 0, "organization"),
                ("Chapter 11", 1, "organization")
            ]
        ),
        pairs(&[("Chapter 11 Ltd", "ORG"), ("Chapter 11", "ORG")])
    );
    for source in [
        source,
        "Chapter 11 refused repayment.\nChapter 11 Ltd supplied the books.",
        "Chapter 11 refused repayment.\nChapter 11 Books Ltd supplied the books.",
        "Under Chapter 11, repayment is due.\nCHAPTER 11 LTD supplied the books.",
        "# Parties\nUnder Chapter 11, repayment is due.\nChapter 11\nLtd supplied the books.",
    ] {
        // The full name is evidence even without a model span over it.
        let at = source.find("Chapter 11").unwrap();
        assert!(
            pack().identifying_at(
                source,
                at..at + "Chapter 11".len(),
                "Chapter 11",
                Category::Organization
            ),
            "{source}"
        );
    }
    for source in [
        "Under Chapter 11, repayment is due.",
        "SubChapter 11 Ltd supplied the books.\nUnder Chapter 11, repayment is due.",
        "Chapter 11 applies to the debtor Ltd.\nUnder Chapter 11, repayment is due.",
    ] {
        let at = source.rfind("Chapter 11").unwrap();
        assert!(
            !pack().identifying_at(
                source,
                at..at + "Chapter 11".len(),
                "Chapter 11",
                Category::Organization
            ),
            "{source}"
        );
    }
}

/// A comma before a paragraph or a page:line pinpoint supplies the
/// citation context needed to remove an abbreviated document label.
#[test]
fn citation_parts_trim_document_labels_before_pinpoints() {
    for (source, name) in [
        ("(Penrow Decl., ¶ 3)", "Penrow Decl"),
        ("(Penrow Decl., ¶ 3)", "Penrow Decl."),
        ("(Penrow Dep. 12:4–13:2)", "Penrow Dep"),
        ("(Penrow Dep. 12:4–13:2)", "Penrow Dep."),
        ("(Penrow Dep., 12:4-13:2)", "Penrow Dep"),
        ("(Penrow Dep. 12:4-9)", "Penrow Dep"),
    ] {
        assert_eq!(
            shown(source, &[(name, "person")]),
            pairs(&[("Penrow", "PERSON")]),
            "{source}"
        );
    }
    for source in [
        "The witness (Penrow Decl., who testified).",
        "The witness (Penrow Dep. 12:4smith).",
        "The witness (Penrow Dep. 12).",
        "The witness (Penrow Dep. 12:4–).",
    ] {
        let name = if source.contains("Decl") {
            "Penrow Decl"
        } else {
            "Penrow Dep"
        };
        assert_eq!(
            shown(source, &[(name, "person")]),
            pairs(&[(name, "PERSON")]),
            "{source}"
        );
    }
}

/// Complete company spans are protected before reference rejection,
/// and invalid Roman words never become section numbers (g1 finding 1).
#[test]
fn citation_parts_keep_complete_companies_and_reject_invalid_romans() {
    for (source, name) in [
        ("The supplier is Section LLC.", "Section LLC"),
        ("The books were supplied by Chapters LLC.", "Chapters LLC"),
        ("See Section LLC.", "Section LLC"),
        ("See Chapters LLC.", "Chapters LLC"),
        ("The supplier is Chapter LLC.", "Chapter LLC"),
    ] {
        assert_eq!(
            shown(source, &[(name, "organization")]),
            pairs(&[(name, "ORG")]),
            "{source}"
        );
    }
    // No ORG/legal-form protection participates in this check: the
    // Roman grammar itself must refuse LLC and other invalid words.
    for roman in ["LLC", "IIV", "VX", "IIII", "MMMM"] {
        let source = format!("See § {roman}.");
        assert!(
            !pack().in_reference(&source, 7..7 + roman.len()),
            "{source}"
        );
    }
    for roman in [
        "I",
        "IV",
        "IX",
        "XII",
        "XL",
        "XC",
        "CD",
        "CM",
        "MDCLXVI",
        "MMMCMXCIX",
    ] {
        let reference = format!("Section {roman}");
        let source = format!("See {reference}.");
        assert!(
            shown(&source, &[(&reference, "organization")]).is_empty(),
            "{source}"
        );
    }
}

/// Personal-ID labels win over reference shapes, and ambiguous bare
/// PD/DM codes need affirmative legal context (g1 finding 2).
#[test]
fn citation_parts_keep_labelled_personal_identifiers() {
    for (source, value, label) in [
        ("Employee badge: PD 57ZZ.", "PD 57ZZ", "national_id_number"),
        ("Employee ID: 352 DM.", "352 DM", "national_id_number"),
        ("Employee ID: PD 57ZZ.", "PD 57ZZ", "national_id_number"),
        (
            "The witness presented passport PD 57ZZ.",
            "PD 57ZZ",
            "passport_number",
        ),
        ("Staff number: 352 DM.", "352 DM", "national_id_number"),
        (
            "Badge number: National Instrument 45-999.",
            "National Instrument 45-999",
            "national_id_number",
        ),
        (
            "Employee ID: PD 57ZZ-9842.",
            "PD 57ZZ-9842",
            "national_id_number",
        ),
        ("The value is PD 57ZZ.", "PD 57ZZ", "national_id_number"),
        ("The value is 352 DM.", "352 DM", "national_id_number"),
    ] {
        assert_eq!(
            shown(source, &[(value, label)]),
            pairs(&[(value, "IDENTITY")]),
            "{source}"
        );
    }
    for (source, value) in [
        ("Under PD 57ZZ.", "PD 57ZZ"),
        ("See 352 DM.", "352 DM"),
        ("See 352 DM - Aviation Safety.", "352 DM - Aviation Safety"),
        ("Practice Direction PD 57ZZ.", "PD 57ZZ"),
    ] {
        assert!(
            shown(source, &[(value, "national_id_number")]).is_empty(),
            "{source}"
        );
    }
}

/// Case, wrapping and an independently recognized trading name all
/// protect a company's short name; uncertainty keeps it (g1 finding 3).
#[test]
fn citation_parts_keep_company_casing_wraps_and_trading_names() {
    for (source, full, short_n, expected_full) in [
        (
            "CHAPTER 11 LTD supplied the books. Chapter 11 refused a refund.",
            "CHAPTER 11 LTD",
            0,
            "CHAPTER 11 LTD",
        ),
        (
            "Chapter 11 Books supplied the books. Chapter 11 refused a refund.",
            "Chapter 11 Books",
            1,
            "Chapter 11 Books",
        ),
        (
            "Chapter 11\nLtd supplied the books. Chapter 11 refused a refund.",
            "Chapter 11\nLtd",
            1,
            "Chapter 11",
        ),
        (
            "Chapter 11 LLC supplied the books. Chapter 11 refused repayment.",
            "Chapter 11 LLC",
            1,
            "Chapter 11 LLC",
        ),
        // Affirmative reference cues still defer to independent
        // company evidence, including a later model-only trading name.
        (
            "CHAPTER 11 LTD supplied the books. See Chapter 11.",
            "CHAPTER 11 LTD",
            0,
            "CHAPTER 11 LTD",
        ),
        (
            "Chapter 11\nLtd supplied the books. See Chapter 11.",
            "Chapter 11\nLtd",
            1,
            "Chapter 11",
        ),
        (
            "Chapter 11 Books supplied the books. See Chapter 11.",
            "Chapter 11 Books",
            1,
            "Chapter 11 Books",
        ),
    ] {
        assert_eq!(
            shown_at(
                source,
                &[
                    (full, 0, "organization"),
                    ("Chapter 11", short_n, "organization")
                ]
            ),
            pairs(&[(expected_full, "ORG"), ("Chapter 11", "ORG")]),
            "{source}"
        );
    }
    assert_eq!(
        shown(
            "The books were supplied by Chapter 11. Chapter 11 refused a refund.",
            &[("Chapter 11", "organization")]
        ),
        pairs(&[("Chapter 11", "ORG"), ("Chapter 11", "ORG")])
    );
    for source in [
        "The applicant trades as Section IV.",
        "Chapter 11.",
        "Chapter 11 governs repayment.",
    ] {
        let name = if source.contains("Section") {
            "Section IV"
        } else {
            "Chapter 11"
        };
        assert_eq!(
            shown(source, &[(name, "organization")]),
            pairs(&[(name, "ORG")]),
            "{source}"
        );
    }
    let source = "See Chapter 11. Chapter 11 Books supplied the books.";
    assert_eq!(
        shown_at(
            source,
            &[
                ("Chapter 11", 0, "organization"),
                ("Chapter 11 Books", 0, "organization")
            ]
        ),
        pairs(&[("Chapter 11", "ORG"), ("Chapter 11 Books", "ORG")])
    );
    let source = "C.F.R. LLC supplied the books. See C.F.R.";
    // Source evidence matching must accept punctuation belonging to
    // the short spelling, independently of category shape filters.
    assert!(pack().company_short_name(source, "C.F.R."));
}

/// A shared given name and punctuation prove no document label, and
/// an attendance time is no pinpoint (g1 finding 4).
#[test]
fn citation_parts_keep_abbreviated_surnames_and_attendance_times() {
    for (source, full, abbreviated) in [
        (
            "Ivo Affleck testified (recorded as Ivo Aff.).",
            "Ivo Affleck",
            "Ivo Aff",
        ),
        (
            "Ivo Depp testified (recorded as Ivo Dep.).",
            "Ivo Depp",
            "Ivo Dep",
        ),
        (
            "Ivo Affleck testified (recorded as Ivo Aff.; present).",
            "Ivo Affleck",
            "Ivo Aff",
        ),
        (
            "Ivo Affleck testified; Ivo Aff. ¶ 4.",
            "Ivo Affleck",
            "Ivo Aff",
        ),
    ] {
        assert_eq!(
            shown_at(source, &[(full, 0, "person"), (abbreviated, 1, "person")]),
            pairs(&[(full, "PERSON"), (abbreviated, "PERSON")]),
            "{source}"
        );
    }
    assert_eq!(
        shown(
            "Witness: Ivo Dep. at 09:30; clerk: Orin Vale.",
            &[("Ivo Dep", "person"), ("Orin Vale", "person")]
        ),
        pairs(&[("Ivo Dep", "PERSON"), ("Orin Vale", "PERSON")])
    );
    for (source, name) in [
        ("Ivo Dep. at 09:30 signed the form.", "Ivo Dep"),
        ("Ivo Aff. at 09:30 signed the form.", "Ivo Aff"),
        ("Ivo Decl. at 09:30 signed the form.", "Ivo Decl"),
    ] {
        assert_eq!(
            shown(source, &[(name, "person")]),
            pairs(&[(name, "PERSON")]),
            "{source}"
        );
    }
    assert_eq!(
        shown(
            "Ivo Affleck testified (recorded as Ivo Dep.).",
            &[("Ivo Affleck", "person"), ("Ivo Dep", "person")]
        ),
        pairs(&[("Ivo Affleck", "PERSON"), ("Ivo Dep", "PERSON")])
    );
    assert_eq!(
        shown(
            "See Decl. of Ivo Penrow ¶ 4.",
            &[("Decl. of Ivo Penrow", "person")]
        ),
        pairs(&[("Ivo Penrow", "PERSON")])
    );
}

/// A citation-part fix must never hide a real name, company or number
/// that merely resembles its marker (issue #499).
#[test]
fn citation_parts_keep_real_names_and_identifiers() {
    assert_eq!(
        shown(
            "The witness (Ivo Decl.) testified.",
            &[("Ivo Decl", "person")]
        ),
        pairs(&[("Ivo Decl", "PERSON")])
    );
    for name in [
        "Declan Penrow",
        "Ivo Affleck",
        "Ms Decl",
        "Mr Aff",
        "Ivo Decl",
        "Decl",
        "Aff",
        "Ex",
    ] {
        let expected = name
            .strip_prefix("Ms ")
            .or_else(|| name.strip_prefix("Mr "))
            .unwrap_or(name);
        let source = format!("The witness is {name}.");
        assert_eq!(
            shown(&source, &[(name, "person")]),
            pairs(&[(expected, "PERSON")]),
            "{source}"
        );
    }
    for name in [
        "Decl Research Ltd",
        "Aff Analytics LLC",
        "Chapter 11 Ltd",
        "National Instrument 45-999 Ltd",
        "PD 57ZZ Ltd",
        "Section XII LLC",
        "Comm Trading Ltd",
    ] {
        let source = format!("The applicant is {name}.");
        assert_eq!(
            shown(&source, &[(name, "organization")]),
            pairs(&[(name, "ORG")]),
            "{source}"
        );
    }
    for (source, text) in [
        ("See Decl. of Ivo Penrow ¶ 4.", "Decl. of Ivo Penrow"),
        (
            "See Ex. 1 to Ivo Penrow Decl. ¶ 4.",
            "Ex. 1 to Ivo Penrow Decl",
        ),
    ] {
        assert_eq!(
            shown(source, &[(text, "person")]),
            pairs(&[("Ivo Penrow", "PERSON")])
        );
    }
    // The same docket in this action or its caption must stay; only
    // a docket attached to a precedent is suppressed.
    for source in [
        "Case No. 09-31-00888-CV.",
        "Pellam v. Ostrand, 999 S.W.3d 1 (Tex. App.—Slocombe 2031, pet. filed), No. 09-31-00888-CV.",
        "The judgment under appeal is Pellam v. Ostrand, 999 S.W.3d 1 (Tex. App.—Slocombe 2031, pet. filed), No. 09-31-00888-CV.",
    ] {
        let detections = shown(source, &[("09-31-00888-CV", "national_id_number")]);
        assert!(
            detections.iter().any(|(text, _)| text == "09-31-00888-CV"),
            "{source}"
        );
    }
    assert_eq!(
        shown(
            "S.C. Penrow and H.L. Venn testified.",
            &[("S.C. Penrow", "person"), ("H.L. Venn", "person")]
        ),
        pairs(&[("S.C. Penrow", "PERSON"), ("H.L. Venn", "PERSON")])
    );
    assert_eq!(
        shown(
            "R. Penrow lives at 478A Cedar Road, Slocombe 45999.",
            &[
                ("R. Penrow", "person"),
                ("478A Cedar Road, Slocombe 45999", "address")
            ]
        ),
        pairs(&[
            ("R. Penrow", "PERSON"),
            ("478A Cedar Road, Slocombe 45999", "ADDRESS")
        ])
    );
    assert_eq!(
        shown(
            "Account 45-999; ID S-19; badge 57ZZ.",
            &[
                ("45-999", "bank_account"),
                ("S-19", "national_id_number"),
                ("57ZZ", "national_id_number")
            ]
        ),
        pairs(&[
            ("45-999", "BANK"),
            ("S-19", "IDENTITY"),
            ("57ZZ", "IDENTITY")
        ])
    );
}

/// The real model run of #499's fixture: a Lord Justice's post-nominal
/// comes off the judge's name, and a badge after its bare label is
/// one person's identifier even when the model misses it.
#[test]
fn citation_parts_trim_lord_justices_and_mark_badges() {
    for (source, raw, expected) in [
        (
            "See [2031] All ER (Comm) 9, per Ivo Ostrand LJ; Ostrand Aff. ¶ 3.",
            "Ivo Ostrand LJ",
            vec![("Ivo Ostrand", "PERSON"), ("Ostrand", "PERSON")],
        ),
        (
            "Per Edda Venn LJJ, the appeal fails.",
            "Edda Venn LJJ",
            vec![("Edda Venn", "PERSON")],
        ),
        // "J" may be an initial and stays.
        (
            "The witness J Ostrand testified.",
            "J Ostrand",
            vec![("J Ostrand", "PERSON")],
        ),
    ] {
        assert_eq!(
            shown(source, &[(raw, "person")]),
            pairs(&expected),
            "{source}"
        );
    }
    for (source, badge) in [
        ("The witness provided badge 88ZZ.", "88ZZ"),
        ("Badge: 4471, issued at the gate.", "4471"),
        ("Badge no. KX42 was returned.", "KX42"),
    ] {
        assert_eq!(
            shown(source, &[]),
            pairs(&[(badge, "IDENTITY")]),
            "{source}"
        );
    }
    for source in [
        "Badge holders may enter.",
        "The badge 12 times failed.",
        "Wear your badge at all times.",
    ] {
        assert!(shown(source, &[]).is_empty(), "{source}");
    }
    // A labelled badge the model gives whole keeps its whole value.
    assert_eq!(
        shown(
            "Employee badge: PD 57ZZ.",
            &[("PD 57ZZ", "national_id_number")]
        ),
        pairs(&[("PD 57ZZ", "IDENTITY")])
    );
}

/// Exercise the CC0 fixture through correction and review with the
/// faulty model boundaries from the probes, without inventing a model
/// snapshot. The model gate supplies its own real output later (#499).
#[test]
fn citation_parts_fixture_keeps_its_declarants() {
    let fixture = crate::recognition::Fixture::parse(
        std::path::Path::new("en/judgment/probe-en-r3-citation-parts.md"),
        include_str!(
            "../../../../../tests/fixtures/recognition/en/judgment/probe-en-r3-citation-parts.md"
        ),
    )
    .unwrap();
    fixture.validate().unwrap();
    fixture.validate_layout().unwrap();
    let source = fixture.source.as_str();
    let mut raw: Vec<_> = fixture
        .gold
        .iter()
        .map(|span| {
            let mut range = span.range.clone();
            let label = match span.category.as_str() {
                "PERSON" => "person",
                "ORG" => "organization",
                "IDENTITY" => "national_id_number",
                other => panic!("unexpected category {other}"),
            };
            if label == "person" {
                for word in [" Decl", " Aff", " Dep"] {
                    if source[range.end..].starts_with(word) {
                        range.end += word.len();
                        break;
                    }
                }
            }
            crate::RawEntity {
                start: range.start,
                end: range.end,
                label: label.into(),
                score: 0.8,
                text: source[range].into(),
            }
        })
        .collect();
    for (text, label) in [
        ("Chapter 11", "organization"),
        ("S.C.", "person"),
        ("H.L", "person"),
        ("Comm", "organization"),
        ("National Instrument 45-999", "national_id_number"),
        ("PD 57ZZ", "organization"),
        ("478A", "address"),
        ("Section XII", "organization"),
        ("352 DM - Aviation Safety", "organization"),
        ("AHCP/CCAA p. S-19", "organization"),
        ("Decl", "person"),
        ("Ex. 1", "person"),
        ("Tex. R. Civ", "organization"),
        ("Pellam", "person"),
        ("Brindle", "person"),
        ("Slocombe 2031", "address"),
        ("09-31-00888-CV", "national_id_number"),
        ("R", "person"),
    ] {
        // R is the judicial-review marker, not a letter in "Research".
        let start = if text == "R" {
            source.find("R (on").unwrap()
        } else {
            source.find(text).unwrap()
        };
        raw.push(crate::RawEntity {
            start,
            end: start + text.len(),
            label: label.into(),
            score: 0.8,
            text: text.into(),
        });
    }
    // The fixture also names Chapter 11 Ltd and Comm Trading Ltd.
    // Their short names stay everywhere under the conservative name
    // policy, even in references the fixture's existing gold omits.
    // Keep this expectation here without changing the fixture.
    let mut expected = fixture.gold.clone();
    for text in ["Chapter 11", "Comm"] {
        let start = source.find(text).unwrap();
        expected.push(crate::recognition::Span {
            range: start..start + text.len(),
            category: "ORG".into(),
        });
    }
    expected.sort();
    assert_eq!(
        crate::recognition::proposals(source, &raw).unwrap(),
        expected
    );
}

/// A cited case's parties are a legal reference, both sides, "In re"
/// too, and so is its reporter short cite; a party of the case at hand
/// stays marked in a citation (guide rule 8, #506 item 2, issue #561).
#[test]
fn cited_cases_parties_are_references_and_the_case_at_hands_stay() {
    // The issue's reproduction.
    let source = "Pellam v. Ostrand, 912 F.3d 455 (9th Cir. 2031), and In re Draycott, \
        77 B.R. 310, apply. Plaintiff Ivo Harrow cites Harrow v. Quillon Ltd, 2031 WL \
        1234567.";
    let raw = [
        ("Pellam", 0, "organization"),
        ("Ostrand", 0, "person"),
        ("Draycott", 0, "person"),
        ("Ivo Harrow", 0, "person"),
        ("Harrow", 1, "person"),
        ("Quillon Ltd", 0, "organization"),
    ];
    assert_eq!(
        shown_at(source, &raw),
        pairs(&[("Ivo Harrow", "PERSON"), ("Harrow", "PERSON")])
    );
    // Both sides in one span, "Ex parte", a legal form after a comma,
    // English reports and a precedent's docket number with its court.
    let source = "Ivo Harrow v. Quillon Ltd\n\nSee Easter v. Powell, 467 F.3d 459 (5th Cir. 2031); Ex parte Murrin \
        Bros. 1885, Ltd, 12 So. 3d 4; News Group Newspapers Ltd v Brindle [2031] EWHC 12 \
        (Ch); Grant v. Finecy, Case No. 4:20-cv-01234 (N.D. Tex. 2031).";
    let raw = [
        ("Easter v. Powell", 0, "person"),
        ("Murrin Bros. 1885, Ltd", 0, "organization"),
        ("News Group Newspapers Ltd", 0, "organization"),
        ("Brindle", 0, "person"),
        ("Grant", 0, "person"),
        ("Finecy", 0, "organization"),
    ];
    assert_eq!(shown_at(source, &raw), []);
    // The caption names the case at hand's parties: its earlier
    // decision's citation keeps them and its case number, a precedent
    // loses its parties, and the caption's own case number stays.
    let source = "IVO HARROW v. QUILLON LTD, Case No. 1:31-cv-00412\n\nThe claimant \
        relies on Harrow v. Quillon Ltd, Case No. 1:30-cv-00977 (N.D. Cal. 2030), and on \
        Pellam v. Ostrand Holdings Ltd, 2031 WL 1234567.";
    let raw = [
        ("IVO HARROW", 0, "person"),
        ("QUILLON LTD", 0, "organization"),
        ("Harrow", 0, "person"),
        ("Quillon Ltd", 0, "organization"),
        ("Pellam", 0, "person"),
        ("Ostrand Holdings Ltd", 0, "organization"),
    ];
    assert_eq!(
        shown_at(source, &raw),
        pairs(&[
            ("IVO HARROW", "PERSON"),
            ("QUILLON LTD", "ORG"),
            ("1:31-cv-00412", "CASE"),
            ("Harrow", "PERSON"),
            ("Quillon Ltd", "ORG"),
            ("1:30-cv-00977", "CASE"),
        ])
    );
    // A party named only after its role is the case at hand's too; a
    // short citation's name goes like the full one's.
    let source = "The Defendant, Mr Quill, relies on Quill v. Ostrand, 912 F.3d 455, and \
        Pellam, 912 F.3d at 460; Pellam, 2031 WL 1234567, at *3, agrees.";
    let raw = [
        ("Quill", 0, "person"),
        ("Quill", 1, "person"),
        ("Ostrand", 0, "person"),
        ("Pellam", 0, "person"),
        ("Pellam", 1, "person"),
    ];
    assert_eq!(
        shown_at(source, &raw),
        pairs(&[("Quill", "PERSON"), ("Quill", "PERSON")])
    );
    // A name with no citation after it is no cited case's.
    let source = "Ivo Harrow v. Quillon Ltd was heard on 3 May.";
    assert_eq!(
        shown(
            source,
            &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")]
        ),
        pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")])
    );
}

/// "Id." and "Ibid." cite authority without naming anyone (#561).
#[test]
fn id_and_ibid_are_references() {
    assert_eq!(
        shown("Id. Ibid.", &[("Id", "person"), ("Ibid", "person")]),
        []
    );
}

/// The case at hand's people stay beside a cited case: a sentence's end
/// is no party's, a docket number's citation ends with its court, a
/// title or a role names the case at hand's party, emphasis included,
/// and the caption below a citation in a heading is still the caption
/// (#561).
#[test]
fn cited_cases_leave_the_case_at_hands_people() {
    /// A source, the model's spans and what is shown.
    type Case<'a> = (
        &'a str,
        &'a [(&'a str, usize, &'a str)],
        &'a [(&'a str, &'a str)],
    );
    let cases: [Case; 6] = [
        (
            "The witness was Ivo Harrow. Pellam v. Ostrand, 912 F.3d 455, controls.",
            &[("Ivo Harrow", 0, "person"), ("Pellam", 0, "person")],
            &[("Ivo Harrow", "PERSON")],
        ),
        (
            "In Pellam v. Ostrand, No. 22-1234, counsel was Ivo Harrow (admitted 2020).",
            &[("Ivo Harrow", 0, "person"), ("Ostrand", 0, "person")],
            &[("Ostrand", "PERSON"), ("Ivo Harrow", "PERSON")],
        ),
        (
            "Mr Pellam signed the agreement. Pellam v. Ostrand, 912 F.3d 455, controls.",
            &[
                ("Pellam", 0, "person"),
                ("Pellam", 1, "person"),
                ("Ostrand", 0, "person"),
            ],
            &[("Pellam", "PERSON"), ("Pellam", "PERSON")],
        ),
        (
            "Plaintiff Ivo Harrow v. Defendant Quillon Ltd, Case No. 1:31-cv-00412 (N.D. \
             Cal. 2031).",
            &[
                ("Ivo Harrow", 0, "person"),
                ("Quillon Ltd", 0, "organization"),
            ],
            &[
                ("Ivo Harrow", "PERSON"),
                ("Quillon Ltd", "ORG"),
                ("1:31-cv-00412", "CASE"),
            ],
        ),
        (
            "Plaintiff _Pellam_ signed. Pellam v. Ostrand, 912 F.3d 455, controls.",
            &[
                ("Pellam", 0, "person"),
                ("Pellam", 1, "person"),
                ("Ostrand", 0, "person"),
            ],
            &[("Pellam", "PERSON"), ("Pellam", "PERSON")],
        ),
        (
            "# Motion under Pellam v. Ostrand, 912 F.3d 455\n\n## Pellam v. Quillon Ltd\n",
            &[
                ("Pellam", 0, "person"),
                ("Ostrand", 0, "person"),
                ("Pellam", 1, "person"),
                ("Quillon Ltd", 0, "organization"),
            ],
            &[
                ("Pellam", "PERSON"),
                ("Pellam", "PERSON"),
                ("Quillon Ltd", "ORG"),
            ],
        ),
    ];
    for (source, raw, expected) in cases {
        assert_eq!(shown_at(source, raw), pairs(expected), "{source:?}");
    }
    // A cited case in emphasis is one too.
    assert_eq!(
        shown(
            "*Pellam v. Ostrand*, 912 F.3d 455, controls; *Pellam*, 912 F.3d at 460.",
            &[("Pellam", "person"), ("Ostrand", "person")]
        ),
        []
    );
}

/// Where nothing settles that a name is only a precedent's, it stays: a
/// legal form's period ends a party, a caption with its own case number
/// and court is the caption wherever it stands, a full name in prose is
/// the case at hand's, the caption's words count undetected, and a title
/// counts through emphasis (#561, second review).
#[test]
fn uncertain_cited_names_stay() {
    let preamble = "Counsel line\n".repeat(30);
    let late_caption =
        format!("{preamble}Pellam v. Quillon Ltd\n\nSee Pellam v. Ostrand, 912 F.3d 455.\n");
    type Case<'a> = (
        &'a str,
        &'a [(&'a str, usize, &'a str)],
        &'a [(&'a str, &'a str)],
    );
    let cases: [Case; 6] = [
        (
            "The supplier was Quillon Ltd. Pellam v. Ostrand, 912 F.3d 455, controls.",
            &[("Quillon Ltd", 0, "organization"), ("Pellam", 0, "person")],
            &[("Quillon Ltd", "ORG")],
        ),
        (
            "Ivo Harrow v. Quillon Ltd, Case No. 1:31-cv-00412 (N.D. Cal. 2031).\n\nThe \
             court considers the motion.",
            &[
                ("Ivo Harrow", 0, "person"),
                ("Quillon Ltd", 0, "organization"),
            ],
            &[
                ("Ivo Harrow", "PERSON"),
                ("Quillon Ltd", "ORG"),
                ("1:31-cv-00412", "CASE"),
            ],
        ),
        (
            "Our client Ivo Pellam signed the agreement. Ivo Pellam v. Ostrand, 912 F.3d \
             455, is his earlier case.",
            &[
                ("Ivo Pellam", 0, "person"),
                ("Ivo Pellam", 1, "person"),
                ("Ostrand", 0, "person"),
            ],
            &[("Ivo Pellam", "PERSON"), ("Ivo Pellam", "PERSON")],
        ),
        (
            "Ivo Harrow v. Quillon Ltd\n\nEarlier: Harrow v. Quillon Ltd, 2031 WL 1234567.",
            &[
                ("Ivo Harrow", 0, "person"),
                ("Harrow", 1, "person"),
                ("Quillon Ltd", 1, "organization"),
            ],
            &[
                ("Ivo Harrow", "PERSON"),
                ("Quillon Ltd", "ORG"),
                ("Harrow", "PERSON"),
                ("Quillon Ltd", "ORG"),
            ],
        ),
        (
            &late_caption,
            &[("Pellam", 0, "person"), ("Pellam", 1, "person")],
            &[("Pellam", "PERSON"), ("Pellam", "PERSON")],
        ),
        (
            "Mr *Pellam* signed the agreement. Pellam v. Ostrand, 912 F.3d 455, controls.",
            &[
                ("Pellam", 0, "person"),
                ("Pellam", 1, "person"),
                ("Ostrand", 0, "person"),
            ],
            &[("Pellam", "PERSON"), ("Pellam", "PERSON")],
        ),
    ];
    for (source, raw, expected) in cases {
        assert_eq!(shown_at(source, raw), pairs(expected), "{source:?}");
    }
}

/// A suffix or abbreviation can end the previous sentence (#561, r4).
#[test]
fn cited_parties_start_after_the_last_sentence_boundary() {
    for suffix in ["Jr.", "Sr.", "II.", "III.", "Gen.", "J."] {
        let name = format!("Ivo Harrow {suffix}");
        for citation in [
            "Pellam v. Ostrand, 912 F.3d 455, controls.",
            "Pellam, 912 F.3d at 460, controls.",
        ] {
            let source = format!("The witness was {name} {citation}");
            let cases = pack().cited(&source);
            assert_eq!(cases.len(), 1, "{source}");
            assert_eq!(cases[0].range.start, source.find("Pellam").unwrap());
            assert_eq!(
                shown_at(
                    &source,
                    &[("Ivo Harrow", 0, "person"), ("Pellam", 0, "person")]
                ),
                pairs(&[("Ivo Harrow", "PERSON")]),
                "{source}"
            );
        }
    }
    // The exact reported example keeps the model's suffix too.
    let source = "The witness was Ivo Harrow Jr. Pellam v. Ostrand, 912 F.3d 455, controls.";
    assert_eq!(
        shown(
            source,
            &[("Ivo Harrow Jr.", "person"), ("Pellam", "person")]
        ),
        pairs(&[("Ivo Harrow Jr.", "PERSON")])
    );
    // A boundary inside the other party or after "In re" cannot
    // establish a full citation, so its preceding name stays too.
    for source in [
        "Pellam v. Ivo Harrow Jr. Ostrand, 912 F.3d 455, controls.",
        "In re Ivo Harrow Jr. Pellam, 912 F.3d 455, controls.",
    ] {
        assert_eq!(
            shown(source, &[("Ivo Harrow", "person")]),
            pairs(&[("Ivo Harrow", "PERSON")]),
            "{source}"
        );
    }
}

/// Unicode spaces and line breaks cannot hide a sentence boundary (r5).
#[test]
fn cited_sentence_boundaries_accept_unicode_whitespace() {
    for space in [
        " ", "\u{00a0}", "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}",
        "\u{2005}", "\u{2006}", "\u{2007}", "\u{2008}", "\u{2009}", "\u{200a}", "\u{202f}",
        "\u{205f}", "\u{3000}", "\t", "\n", "\r\n",
    ] {
        for citation in [
            "Pellam v. Ostrand, 912 F.3d 455, controls.",
            "Pellam, 912 F.3d at 460, controls.",
        ] {
            let source = format!("The witness was Ivo Harrow Jr.{space}{citation}");
            assert_eq!(
                shown(&source, &[("Ivo Harrow", "person"), ("Pellam", "person")]),
                pairs(&[("Ivo Harrow", "PERSON")]),
                "{source:?}"
            );
        }
    }
    let source = "The witness was Ivo Harrow Jr.\u{00a0}Pellam v. Ostrand, 912 F.3d 455.";
    assert_eq!(
        shown(source, &[("Ivo Harrow", "person")]),
        pairs(&[("Ivo Harrow", "PERSON")])
    );
}

/// Attribution words separate the preparer from a cited party (r5).
#[test]
fn cited_parties_do_not_absorb_attributions() {
    for connector in ["for", "by", "to", "with", "from"] {
        for citation in [
            "Pellam v. Ostrand, 912 F.3d 455.",
            "Pellam, 912 F.3d at 460.",
        ] {
            let source = format!("Brief prepared by Ivo Harrow {connector} {citation}");
            let cases = pack().cited(&source);
            assert_eq!(cases.len(), 1, "{source}");
            assert_eq!(cases[0].range.start, source.find("Pellam").unwrap());
            assert_eq!(
                shown(&source, &[("Ivo Harrow", "person"), ("Pellam", "person")]),
                pairs(&[("Ivo Harrow", "PERSON")]),
                "{source}"
            );
        }
    }
}

/// Even an overbroad rule pack cannot discard a model's name prefix.
#[test]
fn uncertain_party_prefixes_stay_even_with_a_permissive_pattern() {
    let pack = Pack::load(&[(
        "test.toml",
        r"[[reference]]
pattern = '(?P<party>Ivo Harrow (?:for|by|to|with|from|of) Pellam) v\. Ostrand, 912 F\.3d 455'",
    )])
    .unwrap();
    for connector in ["for", "by", "to", "with", "from", "of"] {
        let source =
            format!("Brief prepared by Ivo Harrow {connector} Pellam v. Ostrand, 912 F.3d 455.");
        let start = source.find("Ivo Harrow").unwrap();
        let mut detections = vec![Detection {
            range: start..start + "Ivo Harrow".len(),
            category: Category::Person,
            score: 0.8,
            recognizer: Recognizer::Model,
        }];
        pack.cited_cases(&mdoc_pii::view::View::new(&source), &mut detections);
        assert_eq!(detections.len(), usize::from(connector != "of"), "{source}");
    }
}

/// A caption protects both parties when a party or reporter wraps (r7).
#[test]
fn wrapped_citation_bearing_captions_keep_both_parties() {
    for caption in [
        "Ivo Harrow v.\nQuillon Ltd, 912 F.3d 455 (9th Cir. 2031)",
        "Ivo Harrow\nv. Quillon Ltd, 912 F.3d 455 (9th Cir. 2031)",
        "Ivo Harrow\nv.\nQuillon Ltd, 912 F.3d 455 (9th Cir. 2031)",
        "Ivo Harrow v. Quillon Ltd\n[2024] EWHC 12 (Ch)",
    ] {
        let source = format!("{caption}\n\nOpinion");
        assert_eq!(
            shown(
                &source,
                &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")]
            ),
            pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")]),
            "{source}"
        );
    }
}

/// Caption discovery accepts all of the citation rule's introducers (r7).
#[test]
fn citation_bearing_captions_accept_every_introducer() {
    for introducer in ["Ex parte", "Matter of", "In the Matter of", "In re"] {
        let source = format!("{introducer} Ivo Harrow, 12 So. 3d 4 (Ala. 2024)\n\nOpinion");
        assert_eq!(
            shown(&source, &[("Ivo Harrow", "person")]),
            pairs(&[("Ivo Harrow", "PERSON")]),
            "{source}"
        );
    }
    for introducer in ["v.", "vs."] {
        let source =
            format!("Ivo Harrow {introducer} Quillon Ltd, 12 So. 3d 4 (Ala. 2024)\n\nOpinion");
        assert_eq!(
            shown(
                &source,
                &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")]
            ),
            pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")]),
            "{source}"
        );
    }
}

/// Prose identifying the decision under review protects all its parties (r7).
#[test]
fn decisions_under_review_keep_the_case_at_hands_parties() {
    let parties = &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")];
    let expected = pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")]);
    let citation = "Ivo Harrow v. Quillon Ltd\n[2024] EWHC 12 (Ch).";
    let source = format!("The judgment under appeal is {citation}");
    assert_eq!(shown(&source, parties), expected, "{source}");
    for phrase in [
        "under appeal",
        "appealed from",
        "appeal from",
        "the decision below",
        "the judgment below",
        "the order below",
        "on appeal from",
        "the court below",
        "this appeal",
        "the present case",
        "the instant case",
        "this case",
        "this action",
    ] {
        for phrase in [phrase.to_owned(), phrase.to_uppercase()] {
            let source = format!("We identify {phrase} as {citation}");
            assert_eq!(shown(&source, parties), expected, "{source}");
        }
    }
    // The window counts characters, including multibyte ones, and
    // includes a phrase starting exactly 60 characters before the case.
    let source = format!("under appeal{} {citation}", "é".repeat(47));
    assert_eq!(shown(&source, parties), expected, "{source}");
    // A phrase outside that window does not establish the case at hand.
    let source = format!("under appeal{} {citation}", "é".repeat(48));
    assert_eq!(shown(&source, parties), [], "{source}");
}

/// A document's citation-bearing caption protects its parties everywhere.
#[test]
fn citation_bearing_captions_keep_the_case_at_hands_parties() {
    for caption in [
        "# Ivo Harrow v. Quillon Ltd [2031] EWHC 12 (Ch)",
        "Ivo Harrow v. Quillon Ltd, 912 F.3d 455 (9th Cir. 2031)",
        "**Ivo Harrow v. Quillon Ltd [2031] EWHC 12 (Ch)**",
        "# *Ivo Harrow v. Quillon Ltd*, 912 F.3d 455",
    ] {
        let source =
            format!("{caption}\n\nJudgment\n\nEarlier: Harrow v. Quillon Ltd, 912 F.3d 455.");
        assert_eq!(
            shown_at(
                &source,
                &[
                    ("Ivo Harrow", 0, "person"),
                    ("Quillon Ltd", 0, "organization"),
                    ("Harrow", 1, "person"),
                    ("Quillon Ltd", 1, "organization"),
                ]
            ),
            pairs(&[
                ("Ivo Harrow", "PERSON"),
                ("Quillon Ltd", "ORG"),
                ("Harrow", "PERSON"),
                ("Quillon Ltd", "ORG"),
            ]),
            "{source}"
        );
    }
    // A standalone caption needs no judgment label or detected parties.
    let source = "**In re Quillon Ltd**, 77 B.R. 310\n\nEarlier: In re Quillon Ltd, 77 B.R. 310.";
    assert_eq!(
        shown_at(source, &[("Quillon Ltd", 1, "organization")]),
        pairs(&[("Quillon Ltd", "ORG"), ("Quillon Ltd", "ORG")])
    );
    for label in [
        "Judgment",
        "JUDGMENT",
        "Approved Judgment",
        "Opinion",
        "OPINION",
        "Order",
        "ORDER",
        "Before: Judge Quill",
    ] {
        // The label is the third nonempty line after a prose-wrapped
        // caption, which cannot qualify by standing alone.
        let source = format!(
            "Case of Ivo Harrow v. Quillon Ltd [2031] EWHC 12 (Ch), heard today\n\nCourt\n\nCivil Division\n\n{label}"
        );
        assert_eq!(
            shown(
                &source,
                &[("Ivo Harrow", "person"), ("Quillon Ltd", "organization")]
            ),
            pairs(&[("Ivo Harrow", "PERSON"), ("Quillon Ltd", "ORG")]),
            "{source}"
        );
    }
    // A label beyond the third nonempty line does not establish a caption.
    let source = "Motion under Ivo Harrow v. Quillon Ltd [2031] EWHC 12 (Ch), cited\nOne\nTwo\nThree\nJudgment";
    assert_eq!(shown(source, &[("Ivo Harrow", "person")]), []);
}

/// A docket with court/year does not disqualify the caption, even
/// after a reporter precedent, and "In re" is a caption too (#561, r4).
#[test]
fn docket_only_captions_follow_reporter_precedents() {
    for precedent in [
        "Motion under Pellam v. Ostrand, 912 F.3d 455",
        "Motion under In re Draycott, 77 B.R. 310",
        "Motion under Pellam v. Ostrand, No. 22-5678, 2031 WL 1234567",
    ] {
        for caption in [
            "Ivo Harrow v. Quillon Ltd, Case No. 1:31-cv-00412 (N.D. Cal. 2031).",
            "Ivo Harrow vs. Quillon Ltd, Case No. 1:31-cv-00412 (N.D. Cal. 2031).",
            "In re Quillon Ltd, Case No. 22-1234 (Bankr. D. Mass. 2031).",
            "**In re Quillon Ltd**, Case No. 22-1234 (Bankr. D. Mass. 2031).",
            "In re Quillon Ltd",
        ] {
            let source = format!("{precedent}\n\n{caption}");
            let mut raw = vec![("Quillon Ltd", 0, "organization")];
            if caption.starts_with("Ivo") {
                raw.push(("Ivo Harrow", 0, "person"));
            }
            if precedent.contains("Pellam") {
                raw.push(("Pellam", 0, "person"));
                raw.push(("Ostrand", 0, "person"));
            } else {
                raw.push(("Draycott", 0, "person"));
            }
            let mut expected = Vec::new();
            if caption.starts_with("Ivo") {
                expected.push(("Ivo Harrow", "PERSON"));
            }
            expected.push(("Quillon Ltd", "ORG"));
            if caption.contains("1:31") {
                expected.push(("1:31-cv-00412", "CASE"));
            } else if caption.contains("22-1234") {
                expected.push(("22-1234", "CASE"));
            }
            assert_eq!(shown_at(&source, &raw), pairs(&expected), "{source}");
        }
    }
    // A wrapped caption still protects undetected party words.
    let source = "Ivo Harrow\nv.\nQuillon Ltd\n\nEarlier: Harrow v. Quillon Ltd, 912 F.3d 455.";
    assert_eq!(
        shown_at(
            source,
            &[("Harrow", 1, "person"), ("Quillon Ltd", 1, "organization")]
        ),
        pairs(&[
            ("Harrow", "PERSON"),
            ("Quillon Ltd", "ORG"),
            ("Harrow", "PERSON"),
            ("Quillon Ltd", "ORG"),
        ])
    );
    // Undetected caption words protect a name in an earlier decision.
    let source = "In re Quillon Ltd\n\nEarlier: In re Quillon Ltd, 77 B.R. 310.";
    assert_eq!(
        shown_at(source, &[("Quillon Ltd", 1, "organization")]),
        pairs(&[("Quillon Ltd", "ORG"), ("Quillon Ltd", "ORG")])
    );
}
