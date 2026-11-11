//! Defined terms and generic words.
use super::*;
use crate::rules::defined::definitions;
use crate::rules::test_support::{pairs, shown, shown_at};
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
    // A person's span goes as well, but a title keeps a surname (#390,
    // run 4).
    assert_eq!(
        fixed_every(
            "Larkfield LLC (the “Estate”) pays Mr Estate.",
            &[("Estate", Category::Person)]
        ),
        ["Estate"]
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

/// Run 4's shape of #390: the model proposes a defined party noun as a
/// person, at its definition and at a mention, and the review would copy
/// the wording to every other mention. A defined generic term goes in
/// any category, so nothing seeds its repeats.
#[test]
fn defined_party_nouns_proposed_as_people_go_with_their_repeats() {
    let source = "This agreement is made between Marrowind Tea LLC (“Company”) and \
                  XXXXXXXX (“Distributor”). Distributor shall pay Company monthly. \
                  Distributor’s Affiliates may resell. Distributor shall report to \
                  Company. The Distributor keeps the books.";
    assert_eq!(
        shown_at(
            source,
            &[
                ("Marrowind Tea LLC", 0, "organization"),
                ("Company", 0, "person"),
                ("Distributor", 0, "person"),
                ("Distributor", 1, "person"),
                ("Distributor’s Affiliates", 0, "person"),
                ("Company", 1, "organization"),
            ]
        ),
        pairs(&[("Marrowind Tea LLC", "ORG")])
    );
    // Without a definition the same person spreads to each mention: the
    // repeats the definition now stops.
    assert_eq!(
        shown_at(
            "Distributor shall pay monthly. Distributor shall report.",
            &[("Distributor", 0, "person")]
        ),
        pairs(&[("Distributor", "PERSON"), ("Distributor", "PERSON")])
    );
}

/// The nouns of run 4's documents, defined after "hereinafter" or "the",
/// with possessives, plurals and the term's other generic words (#390).
#[test]
fn defined_party_terms_go_in_every_form() {
    let source = "Tollmere Events Ltd (hereinafter referred to as the “Promoter”) \
                  engages Ivo Saltash (the “Artist”) at Brackwater Hall (the “Venue”). \
                  Pellock Brewing Co (the “Manufacturer”) supplies the bar. The \
                  Promoter’s staff meet the Artist; the Manufacturer pays the Promoter. \
                  Each party may be the Disclosing Party or the Receiving Party (each \
                  a “Disclosing Party” or “Receiving Party”). The Indemnifying Party \
                  (the “Indemnifying Party”) holds the Indemnified Parties (the \
                  “Indemnified Parties”) harmless. Hollin Staffing Ltd supplies Edda \
                  Varne (the “Agency Worker”) to the Hirer (the “Hirer”); the Agency \
                  Worker reports to the Hirer. The Licensees (the “Licensees”) pay the \
                  Licensor (the “Licensor”).";
    assert_eq!(
        shown_at(
            source,
            &[
                ("Tollmere Events Ltd", 0, "organization"),
                ("Promoter", 0, "person"),
                ("Ivo Saltash", 0, "person"),
                ("Artist", 0, "person"),
                ("Venue", 0, "organization"),
                ("Pellock Brewing Co", 0, "organization"),
                ("Manufacturer", 0, "person"),
                ("Promoter’s", 0, "person"),
                ("Artist", 1, "person"),
                ("Manufacturer", 1, "organization"),
                ("Disclosing Party", 0, "person"),
                ("Receiving Party", 1, "organization"),
                ("Indemnifying Party", 1, "person"),
                ("Indemnified Parties", 0, "person"),
                ("Hollin Staffing Ltd", 0, "organization"),
                ("Edda Varne", 0, "person"),
                ("Agency Worker", 0, "person"),
                ("Hirer", 0, "person"),
                ("Licensees", 0, "person"),
                ("Licensor", 1, "organization"),
            ]
        ),
        pairs(&[
            ("Tollmere Events Ltd", "ORG"),
            ("Ivo Saltash", "PERSON"),
            ("Pellock Brewing Co", "ORG"),
            ("Hollin Staffing Ltd", "ORG"),
            ("Edda Varne", "PERSON"),
        ])
    );
}

/// Russian party nouns defined after «именуемое в дальнейшем» or «далее»
/// go as people too, in their case forms; the parties' names stay (#390).
#[test]
fn russian_defined_party_nouns_proposed_as_people_go() {
    let source = "ООО «Сосновый бор», именуемое в дальнейшем «Поставщик», и Мирон \
                  Ильич Тарасов (далее – Покупатель) заключили договор. Поставщик \
                  передаёт товар Покупателю, Покупатель платит Поставщику. ИП Лада \
                  Кирсанова (далее – Арендатор) платит Арендодателю (далее – \
                  Арендодатель); Арендатора известили.";
    assert_eq!(
        shown_at(
            source,
            &[
                ("ООО «Сосновый бор»", 0, "organization"),
                ("Поставщик", 0, "person"),
                ("Мирон Ильич Тарасов", 0, "person"),
                ("Покупатель", 0, "person"),
                ("Покупателю", 0, "person"),
                ("Поставщику", 0, "organization"),
                ("Лада Кирсанова", 0, "person"),
                ("Арендатор", 0, "person"),
                ("Арендодателю", 0, "person"),
                ("Арендатора", 0, "person"),
            ]
        ),
        pairs(&[
            ("ООО «Сосновый бор»", "ORG"),
            ("Мирон Ильич Тарасов", "PERSON"),
            ("Лада Кирсанова", "PERSON"),
        ])
    );
}

/// What the definition must not take (#390): the same word where the
/// document defines nothing, a name beside the word, a title or an
/// initial before it, and a term with a coined or real word.
#[test]
fn defined_party_noun_guards_keep_names() {
    // Not defined: a company and a person that share the word stay.
    assert_eq!(
        shown(
            "Distributor Holdings Ltd pays Ann Seller; Seller signs.",
            &[
                ("Distributor Holdings Ltd", "organization"),
                ("Ann Seller", "person"),
            ]
        ),
        pairs(&[
            ("Distributor Holdings Ltd", "ORG"),
            ("Ann Seller", "PERSON"),
            ("Seller", "PERSON"),
        ])
    );
    // Defined: a given name, a title or an initial keeps the model's
    // person, the defined word alone goes. (The review then spreads a
    // named person's surname as it always does.)
    assert_eq!(
        fixed_every(
            "Quillon Freight LLC (the “Seller”) sells to Ann Seller. Mr Seller and J. Seller agree; the Seller delivers.",
            &[
                ("Ann Seller", Category::Person),
                ("J. Seller", Category::Person),
                ("Seller", Category::Person),
            ]
        ),
        ["Ann Seller", "Seller", "J. Seller"]
    );
    assert_eq!(
        fixed_every(
            "Банки (далее — «Поставщики»). Поставщиков И. П. и г-н Поставщиков согласны; Поставщики платят.",
            &[
                ("Поставщиков И. П.", Category::Person),
                ("Поставщиков", Category::Person),
                ("Поставщики", Category::Person),
            ]
        ),
        ["Поставщиков И. П.", "Поставщиков"]
    );
    // A surname on a party's stem stays a person: "-ов" and "-ев" make
    // one in a person's span (#450), though «Кредиторов» counts in an
    // organization's.
    for (source, surname) in [
        (
            "ООО «Бор» (далее – «Подрядчик»). Свидетель Подрядчиков пояснил.",
            "Подрядчиков",
        ),
        (
            "ООО «Бор» (далее – «Заказчик»). Свидетель Заказчиков пояснил.",
            "Заказчиков",
        ),
        (
            "ООО «Бор» (далее – «Покупатель»). Свидетель Покупателев пояснил.",
            "Покупателев",
        ),
        (
            "Банки (далее – «Кредиторы»). Свидетель Кредиторов пояснил.",
            "Кредиторов",
        ),
        (
            "ООО «Бор» (далее – «Агент»). Свидетель Агентов пояснил.",
            "Агентов",
        ),
    ] {
        assert_eq!(
            shown(source, &[(surname, "person")]),
            pairs(&[(surname, "PERSON")]),
            "{source}"
        );
    }
    // A spelled-out legal form right after a word of its own ends a
    // company's name, though the document defines both words ("The Venue
    // Company", "Purchaser Corporation"); a term it defines whole goes, and
    // so do a defined party's group (no legal form) and joined parties.
    let source = concat!(
        "Brackwater Hall (the “Venue”) and Acme Ltd (the “Company”) hire The ",
        "Venue Company. Pellow LLC (the “Supplier”), Orrin LLC (the ",
        "“Principal”) and Kell Ltd (the “Purchaser”) join the Supplier Group, ",
        "the Principal Group and the Purchaser Group; Purchaser Corporation ",
        "objects. Tarn LLC (the “Sponsor”) and Wyle LLC (the “Seller”) and the ",
        "Sponsor’s Group pay. Vane LLC (the “Agency Group”) pays; the Agency ",
        "Group, Seller and the Company agree.",
    );
    assert_eq!(
        shown_at(
            source,
            &[
                ("Acme Ltd", 0, "organization"),
                ("The Venue Company", 0, "organization"),
                ("Pellow LLC", 0, "organization"),
                ("Orrin LLC", 0, "organization"),
                ("Kell Ltd", 0, "organization"),
                ("Supplier Group", 0, "organization"),
                ("Principal Group", 0, "organization"),
                ("the Purchaser Group", 0, "organization"),
                ("Purchaser Corporation", 0, "organization"),
                ("Tarn LLC", 0, "organization"),
                ("Wyle LLC", 0, "organization"),
                ("Sponsor’s Group", 0, "organization"),
                ("Vane LLC", 0, "organization"),
                ("Agency Group", 1, "organization"),
                ("Seller and the Company", 0, "organization"),
            ]
        ),
        pairs(&[
            ("Acme Ltd", "ORG"),
            ("The Venue Company", "ORG"),
            ("Pellow LLC", "ORG"),
            ("Orrin LLC", "ORG"),
            ("Kell Ltd", "ORG"),
            ("Purchaser Corporation", "ORG"),
            ("Tarn LLC", "ORG"),
            ("Wyle LLC", "ORG"),
            ("Vane LLC", "ORG"),
        ])
    );
    // A coined or real word in the term defines nothing: the person and
    // the company keep their short names.
    let source = "Edda Brannock (“Brannock”) and Larkfield Supply LLC (“Larkfield”) \
                  fund the Kew Project (the “Kew Project”). Brannock and Larkfield sign.";
    assert_eq!(
        shown_at(
            source,
            &[
                ("Edda Brannock", 0, "person"),
                ("Brannock", 1, "person"),
                ("Larkfield Supply LLC", 0, "organization"),
                ("Larkfield", 1, "organization"),
            ]
        ),
        pairs(&[
            ("Edda Brannock", "PERSON"),
            ("Brannock", "PERSON"),
            ("Larkfield Supply LLC", "ORG"),
            ("Larkfield", "ORG"),
            ("Brannock", "PERSON"),
            ("Larkfield", "ORG"),
        ])
    );
}
