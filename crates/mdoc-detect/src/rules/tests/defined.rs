//! Defined terms and generic words.
use super::*;
use crate::rules::defined::definitions;
use crate::rules::test_support::{pairs, shown};
use mdoc_pii::Recognizer;

/// Every whole-word occurrence of each text in `spans`, as a model
/// would tag them (earlier texts first, none inside another), after
/// the span fixes.
fn fixed_every(source: &str, spans: &[(&str, Category)]) -> Vec<String> {
    let mut detections: Vec<Detection> = Vec::new();
    for (text, category) in spans {
        for (start, _) in source.match_indices(text) {
            let before = source[..start].chars().next_back();
            let after = source[start + text.len()..].chars().next();
            let end = start + text.len();
            if before.is_some_and(char::is_alphanumeric)
                || after.is_some_and(char::is_alphanumeric)
                || detections
                    .iter()
                    .any(|d| d.range.start < end && start < d.range.end)
            {
                continue;
            }
            detections.push(Detection {
                range: start..start + text.len(),
                category: *category,
                score: 0.9,
                recognizer: Recognizer::Model,
            });
        }
    }
    detections.sort_by_key(|d| d.range.start);
    pack().fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
    detections
        .into_iter()
        .map(|d| source[d.range].to_owned())
        .collect()
}

/// The terms a document defines, in every form of definition (#390).
#[test]
fn definitions_are_found_in_brackets_after_hereinafter_and_in_bold() {
    for (source, terms) in [
        (
            "Larkfield LLC (the “Distributor”) sells",
            vec!["Distributor"],
        ),
        (
            "Larkfield LLC (the \"Distributor\") sells",
            vec!["Distributor"],
        ),
        ("the land (“Townhouse”) is let", vec!["Townhouse"]),
        (
            "Larkfield LLC (hereinafter referred to as the “Buyer”)",
            vec!["Buyer"],
        ),
        ("Larkfield LLC (hereinafter, “Buyer”)", vec!["Buyer"]),
        (
            "Larkfield LLC and Quillon LLC (collectively, the “Parties”)",
            vec!["Parties"],
        ),
        ("Larkfield LLC (each a “Party”)", vec!["Party"]),
        (
            "the Kew Project (the “Kew Project” or the “Project”)",
            vec!["Kew Project", "Project"],
        ),
        (
            "Larkfield LLC (hereinafter the Distributor)",
            vec!["Distributor"],
        ),
        (
            "**“Franchised Business”** means the shops",
            vec!["Franchised Business"],
        ),
        ("“Territory” has the meaning in clause 2", vec!["Territory"]),
        ("ООО «Север» (далее – «Общество»)", vec!["Общество"]),
        (
            "ООО «Север» (далее по тексту – Положение)",
            vec!["Положение"],
        ),
        (
            "ООО «Север», именуемое в дальнейшем «Поставщик», и",
            vec!["Поставщик"],
        ),
        // Not a definition: a reference, a plain bracket, a quotation.
        ("as stated (see “Exhibit A”)", vec![]),
        ("the network (Network) runs", vec![]),
        ("he said “Distributor” twice", vec![]),
    ] {
        assert_eq!(
            definitions(source)
                .iter()
                .map(|d| d.term)
                .collect::<Vec<_>>(),
            terms,
            "{source}"
        );
    }
}

#[test]
fn generic_vocabulary_keeps_a_companys_defined_short_name() {
    for (source, names, expected) in [
        (
            "Network International Ltd (“Network”). Network shall pay…",
            vec!["Network International Ltd", "Network"],
            vec!["Network International Ltd", "Network", "Network"],
        ),
        (
            "ООО «Контракт» (далее — «Контракт»). Контракт платит.",
            vec!["ООО «Контракт»", "Контракт"],
            vec!["ООО «Контракт»", "Контракт", "Контракт"],
        ),
        (
            "Larkfield LLC (the “Network”). Network International Ltd (“Network”). Network pays.",
            vec!["Network"],
            vec!["Network", "Network", "Network", "Network"],
        ),
        (
            "“Network” means Network International Ltd. Network pays.",
            vec!["Network International Ltd", "Network"],
            vec!["Network", "Network International Ltd", "Network"],
        ),
    ] {
        assert_eq!(
            fixed_every(
                source,
                &names
                    .into_iter()
                    .map(|name| (name, Category::Organization))
                    .collect::<Vec<_>>()
            ),
            expected,
            "{source}",
        );
    }
    // A role assigned to a different company is still generic.
    assert!(
        fixed_every(
            "Larkfield LLC (the “Network”). Network shall pay…",
            &[("Network", Category::Organization)],
        )
        .is_empty()
    );
}

#[test]
fn wrapped_definitions_suppress_generic_organization_proposals() {
    for source in [
        "Larkfield LLC (the\n“Distributor”). The Distributor pays.",
        "Larkfield LLC (the\r\n“Distributor”). The Distributor pays.",
        "Larkfield LLC (hereinafter\nreferred to as the “Distributor”). The Distributor pays.",
        "Larkfield LLC (the “Franchised\nBusiness”). Franchised Business pays.",
        "Банки (далее\n— «Кредиторы»). Кредиторы платят.",
        "Quillon LLC (the\n“Distributor”). Distributor pays.",
        "ООО «Север»\n(далее —\n«Кредитор»). Кредитор платит.",
    ] {
        assert!(
            fixed_every(
                source,
                &[
                    ("Distributor", Category::Organization),
                    ("Franchised\nBusiness", Category::Organization),
                    ("Franchised Business", Category::Organization),
                    ("Кредиторы", Category::Organization),
                    ("Кредитор", Category::Organization),
                ]
            )
            .is_empty(),
            "{source}"
        );
    }
    // Wrapping is bounded by the same 200-character bracket limit.
    let source = format!(
        "Larkfield LLC (the{}“Distributor”). The Distributor pays.",
        "\n".repeat(200)
    );
    assert_eq!(
        fixed_every(&source, &[("Distributor", Category::Organization)]),
        ["Distributor", "Distributor"]
    );
}

#[test]
fn single_quoted_definitions_suppress_generic_organization_proposals() {
    for source in [
        "Larkfield Ltd (the ‘Distributor’). The Distributor pays.",
        "Larkfield Ltd (the 'Distributor'). The Distributor pays.",
        "‘Distributor’ means Larkfield Ltd. The Distributor pays.",
        "'Distributor' means Larkfield Ltd. The Distributor pays.",
        "**‘Distributor’** shall mean Larkfield Ltd. The Distributor pays.",
        "ООО «Север», именуемое в дальнейшем ‘Кредитор’. Кредитор платит.",
    ] {
        assert!(
            fixed_every(
                source,
                &[
                    ("Distributor", Category::Organization),
                    ("Кредитор", Category::Organization),
                ]
            )
            .is_empty(),
            "{source}"
        );
    }
    // An unmatched quotation is insufficient evidence to drop a name.
    assert!(definitions("Larkfield Ltd (the ‘Distributor\").").is_empty());
    assert!(definitions("Larkfield Ltd (the 'Distributor’).").is_empty());
}

#[test]
fn russian_defined_nouns_include_genitive_plurals_but_keep_people() {
    let source = "Банки (далее — «Кредиторы»). Согласие Кредиторов обязательно.";
    assert!(
        fixed_every(
            source,
            &[
                ("Кредиторы", Category::Organization),
                ("Кредиторов", Category::Organization),
            ]
        )
        .is_empty()
    );
    // The noun's case forms do not change the PERSON surname guards.
    let source = "Банки (далее — «Кредиторы»). Кредиторов И. П. и г-н Кредиторов согласны.";
    assert_eq!(
        fixed_every(source, &[("Кредиторов", Category::Person)]),
        ["Кредиторов И. П.", "Кредиторов"]
    );
    assert_eq!(
        fixed_every(
            "Банки (далее — «Кредиторы»). Кредиторин согласен.",
            &[("Кредиторин", Category::Organization)],
        ),
        ["Кредиторин"]
    );
    assert!(
        fixed_every(
            "Компании (далее — «Должники»). Согласие Должников обязательно.",
            &[
                ("Должники", Category::Organization),
                ("Должников", Category::Organization)
            ],
        )
        .is_empty()
    );
}

/// A term the document defines in generic words is no organization,
/// wherever it stands, in any of its forms; the parties' names and a
/// term with a coined word stay (#390).
#[test]
fn defined_terms_of_generic_words_are_no_organizations() {
    let source = "This agreement is between Larkfield Supply LLC (the “Distributor”) \
                  and Quillon Freight LLC (the “Sponsor”). The Distributor sells in \
                  the Territory (the “Territory”); the Sponsor’s Franchised Business \
                  (the “Franchised Business”) funds the Kew Project (the “Project”). \
                  The Project ends when the Distributor leaves the Territory. \
                  Quillon (the “Quillon Group”) agrees.";
    assert_eq!(
        fixed_every(
            source,
            &[
                ("Larkfield Supply LLC", Category::Organization),
                ("Quillon Freight LLC", Category::Organization),
                ("Distributor", Category::Organization),
                ("Sponsor’s", Category::Organization),
                ("Sponsor", Category::Organization),
                ("Territory", Category::Organization),
                ("Franchised Business", Category::Organization),
                ("Kew Project", Category::Organization),
                ("Project", Category::Organization),
                ("Quillon", Category::Organization),
            ]
        ),
        [
            "Larkfield Supply LLC",
            "Quillon Freight LLC",
            "Quillon",
            "Quillon"
        ]
    );
    // Bold quotes, other terms and the title made of them.
    let source = "RESELLER AGREEMENT\n\nLarkfield Supply LLC (the “Reseller”) buys from \
                  Quillon Freight LLC. The Reseller pays. **“Concessionaire”** means a \
                  shop; each Concessionaire reports to the Reseller. The Estate (the \
                  “Estate”) is let by the Reseller’s Affiliates.";
    assert_eq!(
        fixed_every(
            source,
            &[
                ("RESELLER AGREEMENT", Category::Organization),
                ("Larkfield Supply LLC", Category::Organization),
                ("Quillon Freight LLC", Category::Organization),
                ("Reseller", Category::Organization),
                ("Concessionaire", Category::Organization),
                ("Estate", Category::Organization),
                ("Affiliates", Category::Organization),
            ]
        ),
        // "Affiliates" is a role word too (#385), so it goes as well.
        ["Larkfield Supply LLC", "Quillon Freight LLC"]
    );
    // The same words without a definition are proposed as before.
    assert_eq!(
        fixed_every(
            "The Distributor sells in the Territory.",
            &[("Distributor", Category::Organization)]
        ),
        ["Distributor"]
    );
    // A person's span is never dropped for it.
    assert_eq!(
        fixed_every(
            "Larkfield LLC (the “Estate”) pays Mr Estate.",
            &[("Estate", Category::Person)]
        ),
        ["Estate", "Estate"]
    );
}

/// A defined short form of a real name stays an organization: a coined
/// word, a ticker, a surname firm, a company's own name; and a generic
/// word with a name beside it or a legal form is a name (#390, #506).
#[test]
fn real_short_names_stay_when_defined() {
    let source = "Quillon Freight LLC (“Quillon”), Harbour Chase Bank Ltd (the “Chase”),                       Berkshire Hills Bancorp (“BHLB”), Twentieth Century Fox (“Fox”) and                       Vell & Ostrand LLP (the “Vell”) met Larkfield Supply LLC (the                       “Distributor”). Quillon, Chase, BHLB, Fox and Vell sued the                       Distributor. Distributor Quillon and Distributor LLC agreed.";
    let names = [
        "Distributor Quillon",
        "Distributor LLC",
        "Quillon Freight LLC",
        "Harbour Chase Bank Ltd",
        "Twentieth Century Fox",
        "Vell & Ostrand LLP",
        "Larkfield Supply LLC",
        "Quillon",
        "Chase",
        "BHLB",
        "Fox",
        "Vell",
        "Distributor",
    ];
    let found = fixed_every(source, &names.map(|name| (name, Category::Organization)));
    let count = |name: &str| found.iter().filter(|f| *f == name).count();
    // Each short form is proposed where it is defined and where it is
    // used; the defined generic word nowhere alone.
    for (name, times) in [
        ("Quillon Freight LLC", 1),
        ("Harbour Chase Bank Ltd", 1),
        ("Twentieth Century Fox", 1),
        ("Vell & Ostrand LLP", 1),
        ("Larkfield Supply LLC", 1),
        ("Distributor Quillon", 1),
        ("Distributor LLC", 1),
        ("Quillon", 2),
        ("Chase", 2),
        ("BHLB", 2),
        ("Fox", 2),
        ("Vell", 2),
        ("Distributor", 0),
    ] {
        assert_eq!(count(name), times, "{name}: {found:?}");
    }
}

/// The Mac hub's review of #580 (#390): a company's long form keeps its
/// abbreviation, irregular plurals are their singular's term, listed
/// companions go beside a defined term, uppercase possessives go.
#[test]
fn defined_terms_review_cases() {
    // A long form with a legal form, spelled out or abbreviated, is a
    // company's, never a filing's.
    for (source, short) in [
        ("Contract Corporation (“CC”). CC supplies goods.", "CC"),
        ("Contract Company (“CC”). CC supplies goods.", "CC"),
        ("Contract Incorporated (“CI”). CI supplies goods.", "CI"),
        (
            "Contract Holdings Limited (“CHL”). CHL supplies goods.",
            "CHL",
        ),
        ("Contract Corp (“CC”). CC supplies goods.", "CC"),
        (
            "Agreement Statement LLC (“ASL”). ASL supplies goods.",
            "ASL",
        ),
    ] {
        assert_eq!(
            fixed_every(source, &[(short, Category::Organization)]),
            [short, short],
            "{source}"
        );
    }
    // A company's short name defined from a spelled-out form stays.
    assert_eq!(
        fixed_every(
            "Network Corporation (“Network”). Network pays.",
            &[("Network", Category::Organization)]
        ),
        ["Network", "Network", "Network"]
    );
    // A filing still names no one.
    assert!(
        fixed_every(
            "Corporation Statement of Material Fact (“CSMF”). CSMF ¶ 2.",
            &[("CSMF", Category::Organization)]
        )
        .is_empty()
    );
    // Irregular plurals, both directions.
    for (singular, plural) in [
        ("Subsidiary", "Subsidiaries"),
        ("Territory", "Territories"),
        ("Business", "Businesses"),
    ] {
        for (defined, used) in [(singular, plural), (plural, singular)] {
            let source = format!("Larkfield LLC (the “{defined}”). The {used} pay.");
            assert!(
                fixed_every(
                    &source,
                    &[
                        (defined, Category::Organization),
                        (used, Category::Organization)
                    ]
                )
                .is_empty(),
                "{source}"
            );
        }
    }
    // Listed generic words go beside a defined one; a real word keeps
    // the span.
    assert_eq!(
        fixed_every(
            "Quillon LLC (the “Sponsor”). The Sponsor Business and Sponsor Quillon pay.",
            &[
                ("Sponsor Business", Category::Organization),
                ("Sponsor Quillon", Category::Organization),
            ]
        ),
        ["Sponsor Quillon"]
    );
    // Uppercase possessives, straight and curly.
    assert!(
        fixed_every(
            "Quillon LLC (the “Distributor”). DISTRIBUTOR’S and DISTRIBUTOR'S duties.",
            &[
                ("DISTRIBUTOR’S", Category::Organization),
                ("DISTRIBUTOR'S", Category::Organization),
            ]
        )
        .is_empty()
    );
}

/// A filing's abbreviation, defined after a long form of generic words,
/// names no one; a company's does (#390).
#[test]
fn abbreviations_of_generic_filings_are_no_organizations() {
    let source = "Def.’s Statement of Uncontested Material Fact (“DSUMF”) ¶ 2 and \
                  Pl.’s Motion for Summary Judgment (“PMSJ”) ¶ 3. DSUMF ¶ 4; PMSJ. \
                  Quillon Freight LLC (“QF”) and Berkshire Hills Bancorp (“BHB”) \
                  met; QF and BHB agreed.";
    assert_eq!(
        fixed_every(
            source,
            &[
                ("DSUMF", Category::Organization),
                ("PMSJ", Category::Organization),
                ("QF", Category::Organization),
                ("BHB", Category::Organization),
            ]
        ),
        ["QF", "BHB", "QF", "BHB"]
    );
}

/// The Russian shape: «(далее – <Слово>)», «именуемое в дальнейшем
/// «Поставщик»», any case form; a short name stays (#390).
#[test]
fn russian_defined_terms_are_no_organizations() {
    let source = "Утверждено Положение о закупках (далее – Положение); ООО «Кедровое» \
                  (далее – Учреждение). Учреждение возражает против Положения, \
                  Учреждению отказано. ООО «Север» (далее – «Север»), именуемое в \
                  дальнейшем «Займодавец», передало заём. Займодавца известили, \
                  Займодавцу выплатили; Север согласен.";
    assert_eq!(
        fixed_every(
            source,
            &[
                ("Положение", Category::Organization),
                ("Положения", Category::Organization),
                ("Учреждение", Category::Organization),
                ("Учреждению", Category::Organization),
                ("Займодавец", Category::Organization),
                ("Займодавца", Category::Organization),
                ("Займодавцу", Category::Organization),
                ("ООО «Кедровое»", Category::Organization),
                ("ООО «Север»", Category::Organization),
                ("Север", Category::Organization),
            ]
        ),
        ["ООО «Кедровое»", "ООО «Север»", "Север", "Север"]
    );
    // Not defined: a company's name with the same word stays.
    assert_eq!(
        fixed_every(
            "Положение утверждено ООО «Север».",
            &[("Положение", Category::Organization)]
        ),
        ["Положение"]
    );
}

/// #501, #481 and #488's reproductions as the user is shown them: only
/// the company is proposed; abbreviations defined beside a programme,
/// a standard or a condition go wherever they stand, a company's and a
/// person's stay, and a field code heading a table's column is no
/// company.
#[test]
fn generic_names_and_their_abbreviations_are_not_shown() {
    let source = "Northvale Ledger Services LLC, a Wyoming limited liability company, \
        runs the Bright Paths Program under the National Reading Access Standard \
        (NRAS) for patients with Acute Kidney Strain (AKS); see 4 Brindle on \
        Insolvency ¶ 12.03. The Crew reports to Assignment. NRAS applies; AKS is \
        rare. VAT on EFT payments, F.O.B. destination; the LEA's IEP and the RNG \
        used by Quillon Freight LLC (“QF”). Apply through Timber Trade Online and \
        FileFlow Manager; the app is on the AppStore. Ossian Draycott (“OD”) \
        and QF agree; OD signs.";
    let raw = [
        ("Northvale Ledger Services LLC", "organization"),
        ("Wyoming limited liability company", "organization"),
        ("Bright Paths Program", "organization"),
        ("National Reading Access Standard", "organization"),
        ("NRAS", "organization"),
        ("Acute Kidney Strain", "organization"),
        ("AKS", "person"),
        ("Brindle on Insolvency", "organization"),
        ("Crew", "organization"),
        ("Assignment", "organization"),
        ("VAT", "organization"),
        ("EFT", "organization"),
        ("F.O.B. destination", "organization"),
        ("LEA's", "organization"),
        ("IEP", "organization"),
        ("RNG", "organization"),
        ("Quillon Freight LLC", "organization"),
        ("QF", "organization"),
        ("Timber Trade Online", "organization"),
        ("FileFlow Manager", "organization"),
        ("AppStore", "organization"),
        ("Ossian Draycott", "person"),
        ("OD", "person"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("Northvale Ledger Services LLC", "ORG"),
            ("Quillon Freight LLC", "ORG"),
            ("QF", "ORG"),
            ("Ossian Draycott", "PERSON"),
            ("OD", "PERSON"),
            ("QF", "ORG"),
            ("OD", "PERSON"),
        ])
    );
    // Field codes heading the columns are no companies, and a code in a
    // body row stays; a header with any other cell keeps its spans
    // (review of #543).
    let source = "| FLDA | FLDB |\n|---|---|\n| FLDC | 12 |\n\n\
        | HSBC | Quillon Freight LLC |\n| :-- | --: |\n| 1 | 2 |\n";
    assert_eq!(
        shown(
            source,
            &[
                ("FLDA", "organization"),
                ("FLDB", "organization"),
                ("FLDC", "organization"),
                ("HSBC", "organization"),
                ("Quillon Freight LLC", "organization"),
            ]
        ),
        pairs(&[
            ("FLDC", "ORG"),
            ("HSBC", "ORG"),
            ("Quillon Freight LLC", "ORG"),
        ])
    );
    // A company's initials stay where the model missed its long form,
    // which has a word of its own (review of #543).
    let source = "International Business Machines (IBM) supplied it; IBM invoiced.";
    assert_eq!(
        shown(source, &[("IBM", "organization")]),
        pairs(&[("IBM", "ORG"), ("IBM", "ORG")])
    );
}
