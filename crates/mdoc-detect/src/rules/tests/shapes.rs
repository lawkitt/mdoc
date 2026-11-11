//! `never` words and `shape`s.
use super::*;
use crate::rules::test_support::{pairs, shown, shown_at};
use mdoc_pii::Recognizer;

/// Digits that are not the model's category: long runs and bank codes
/// are no phones, bare ten- or eleven-digit numbers no bank details, a
/// rate or a writ series no tax identifier, a host without a dot no
/// email. Real ones stay.
#[test]
fn shapes_drop_numbers_that_are_not_their_category() {
    let keeps = |text: &str, category| pack().identifying(text, category);
    for (text, category) in [
        ("123456789012345678901234567890", Category::Phone),
        ("+1234567890123456789012345", Category::Phone),
        ("044525593", Category::Phone),
        ("12345678900", Category::Bank),
        ("5401000039", Category::Bank),
        ("20 percent tax", Category::Tax),
        ("НДС 20%", Category::Tax),
        ("ФС 086731924", Category::Tax),
        ("not-a-mail@localhost", Category::Email),
    ] {
        assert!(!keeps(text, category), "{text}");
    }
    for (text, category) in [
        ("+44 7700 900461", Category::Phone),
        ("8 (495) 123-45-67", Category::Phone),
        ("40702810900000001234", Category::Bank),
        ("GB28NWBK60161331926819", Category::Bank),
        ("044525225", Category::Bank),
        ("12345", Category::Bank),
        ("7707083893", Category::Tax),
        ("GB 123 4567 89", Category::Tax),
        ("maya.bennett@example.com", Category::Email),
    ] {
        assert!(keeps(text, category), "{text}");
    }
}

/// An English organization in sentence case is a heading, a document
/// kind or a team; title case, legal forms and Russian names stay.
#[test]
fn english_sentence_case_is_no_organization() {
    let org = |text: &str| pack().identifying(text, Category::Organization);
    for text in [
        "Distribution register",
        "Lasting power of attorney",
        "Financial authority",
        "Accounts team",
    ] {
        assert!(!org(text), "{text}");
    }
    for text in [
        "Harrow & Finch Ltd",
        "Copper Kite Metrology LLP",
        // One title-case word; Ofcom is a public body, never marked (#378).
        "Vantora",
        "Elm Row Legal LLP",
        "Переплётная орбита",
        "Sable Chime Music Ltd",
    ] {
        assert!(org(text), "{text}");
    }
}

/// A name wrapped exactly in «» is a title, not a person; one beside
/// quotes, or in straight quotes, stays.
#[test]
fn a_name_in_guillemets_is_a_title() {
    let person = |source: &str, name: &str| {
        let start = source.find(name).unwrap();
        pack().identifying_at(source, start..start + name.len(), name, Category::Person)
    };
    assert!(!person(
        "рукопись повести «Кедровый свет».",
        "Кедровый свет"
    ));
    for source in [
        "рукопись повести &laquo;Кедровый свет&raquo;.",
        "рукопись повести &#171;Кедровый свет&#187;.",
        "рукопись повести &#xAB;Кедровый свет&#xBB;.",
    ] {
        assert!(!person(source, "Кедровый свет"), "{source}");
    }
    assert!(person("Истец «Иван Петров» и", "Петров"));
    assert!(person("Истец &laquo;Иван Петров", "Петров"));
    // A real name in «» elsewhere stays a person, quotes written out or
    // as references.
    for source in [
        "В заявлении потерпевший назван «Иван Петров».",
        "В заявлении потерпевший назван &laquo;Иван Петров&raquo;.",
        "по прозвищу «Лёша Гром» в",
        "Игрок «Лёша Гром» дал показания",
        "Свидетель записал имя коллеги «Иван Петров».",
        "товарищ «Иван Петров» пришёл",
        "программист «Иван Петров» в",
        "Свидетель Роман «Сокол» дал показания",
        "Об этом рассказал «Иван Петров».",
        "в команде играл «Иван Петров» и",
        "его друг Марк «Сокол» в",
        // A company named after a person is the ORG span's; the person
        // span stays rather than leave the name unproposed.
        "ООО «Вера Снегирь» в",
        "коллегии «Вера Снегирь» в",
    ] {
        let name = ["Лёша Гром", "Сокол", "Вера Снегирь"]
            .into_iter()
            .find(|name| source.contains(name))
            .unwrap_or("Иван Петров");
        assert!(person(source, name), "{source}");
    }
    // A work, a product or a company named in «» is no person.
    for (source, name) in [
        ("роман «Анна Каренина» в", "Анна Каренина"),
        ("журнал «Вера Снегирь» за", "Вера Снегирь"),
        ("товарный знак «Иван Царевич» в", "Иван Царевич"),
        ("повести&nbsp;«Кедровый свет» в", "Кедровый свет"),
        (
            "повести&#160;&laquo;Кедровый свет&raquo; в",
            "Кедровый свет",
        ),
        ("в игре «Иван Царевич» за", "Иван Царевич"),
        ("в его рассказе «Иван Петров» есть", "Иван Петров"),
        ("над романом «Анна Каренина» в", "Анна Каренина"),
        ("сценарий «Иван Царевич» для", "Иван Царевич"),
        ("повести «Кедровый свет\u{200b}» в", "Кедровый свет"),
        ("повести \u{200b}«Кедровый свет» в", "Кедровый свет"),
    ] {
        assert!(!person(source, name), "{source}");
    }
    assert!(person("Истец Иван Петров.", "Иван Петров"));
    assert!(person("name=\"Alice Morgan\"", "Alice Morgan"));
}

/// Roles, role phrases, category words, defined terms and public bodies
/// are never names (#378, #379, #385, #390); names beside them still are.
#[test]
fn roles_category_words_and_public_bodies_are_never_names() {
    let pack = pack();
    for (text, category) in [
        ("Owner", Category::Person),
        ("Assignee", Category::Person),
        ("Technical Reviewer", Category::Person),
        ("Эксперту", Category::Person),
        ("Секретарь", Category::Person),
        ("Costs Officer", Category::Organization),
        ("Administrative Team", Category::Organization),
        ("Provider", Category::Organization),
        ("Общество", Category::Organization),
        ("Email", Category::Organization),
        ("Bank", Category::Organization),
        ("СНИЛС", Category::Organization),
        ("ГК РФ", Category::Organization),
        ("Address", Category::Address),
        ("County Court", Category::Organization),
        ("Ministry of Justice", Category::Organization),
        ("Employment Tribunal", Category::Organization),
        ("Ofcom", Category::Organization),
        ("Верховный Суд Российской Федерации", Category::Organization),
        ("Министерства юстиции", Category::Organization),
        ("Addresses", Category::Address),
        ("Technical Secretaries", Category::Person),
        ("Ministries of Justice", Category::Organization),
        ("Managing Director", Category::Person),
        ("Независимый эксперт", Category::Person),
    ] {
        assert!(!pack.identifying(text, category), "{text}");
    }
    for (text, category) in [
        ("Peter Lord", Category::Person),
        ("Иван Суднев", Category::Person),
        ("Barclays Bank", Category::Organization),
        ("Acme Company", Category::Organization),
        (
            "Общество с ограниченной ответственностью «Ромашка»",
            Category::Organization,
        ),
        ("14 Fennel Court, Lincoln", Category::Address),
        ("Garden Court Ltd", Category::Organization),
        ("Northbridge Department Store Ltd", Category::Organization),
        ("Cedar Arts Council", Category::Organization),
        ("ООО «Судак»", Category::Organization),
        ("John Clerk", Category::Person),
        ("Суднев", Category::Person),
        ("Банков", Category::Person),
        ("Компаниец", Category::Person),
        ("Подрядчиков", Category::Person),
        ("Супругов", Category::Person),
        ("Директоров", Category::Person),
        ("Истцова", Category::Person),
        ("Banks", Category::Person),
        ("Адресат", Category::Organization),
    ] {
        assert!(pack.identifying(text, category), "{text}");
    }
}

/// Pronouns, common words, religions and nationalities name no one
/// alone (#500, #492); with any other word they stay a name.
#[test]
fn common_words_alone_are_no_names() {
    let pack = pack();
    for (text, category) in [
        ("He", Category::Person),
        ("She", Category::Person),
        ("It", Category::Organization),
        ("We", Category::Organization),
        ("In", Category::Person),
        ("Others", Category::Person),
        ("Program", Category::Person),
        ("Programme", Category::Person),
        ("Judgment", Category::Person),
        ("Federal", Category::Person),
        ("Law School", Category::Organization),
        ("Christian", Category::Person),
        ("Christians", Category::Person),
        ("Islam", Category::Organization),
        ("Muslims", Category::Person),
        ("Americans", Category::Person),
        ("Maltese lawyer", Category::Person),
        ("American licensed drivers", Category::Person),
        ("United States Citizens", Category::Person),
        ("Citizenry", Category::Person),
        ("the French", Category::Person),
        ("New York", Category::Person),
        ("San Francisco", Category::Person),
        ("St. Louis", Category::Person),
        ("North Carolina", Category::Person),
        // An outline letter before a heading's common word, and whole
        // headings (review of #528).
        ("B. General Terms", Category::Person),
        ("Approved Judgment", Category::Person),
        ("American End Products", Category::Person),
    ] {
        assert!(!pack.identifying(text, category), "{text}");
    }
    for (text, category) in [
        ("General", Category::Person),
        ("B. General", Category::Person),
        ("He Jiankui", Category::Person),
        ("Jane Federal", Category::Person),
        ("Paul Program", Category::Person),
        ("John York", Category::Person),
        ("Sarah In", Category::Person),
        ("Christian Bale", Category::Person),
        ("Nazrul Islam", Category::Person),
        ("Dawn French", Category::Person),
        ("Georgia Quill", Category::Person),
        ("Virginia", Category::Person),
        ("Carolina", Category::Person),
        ("J. R. Clerk", Category::Person),
        ("J. R. French", Category::Person),
        ("J.R. French", Category::Person),
        ("B.", Category::Person),
        ("General Motors", Category::Organization),
        ("Harvard Law School", Category::Organization),
        ("British Airways", Category::Organization),
        ("Islamic Relief", Category::Organization),
        ("State Street", Category::Organization),
        ("New York Life", Category::Organization),
    ] {
        assert!(pack.identifying(text, category), "{text}");
    }
}

/// In context: a common word after a title, or one a person span in
/// the document has beside a name, is that person's name (#500); the
/// second word of a two-word place grows to the place and goes; a
/// caption's "In re" comes off the party.
#[test]
fn common_words_in_context_and_place_tails_and_captions() {
    let source = "Mr He met Dr. General. He appeals. Dawn French and Sarah In \
                  wrote; French and In agree, Christian too. Mr Lane lives in New \
                  York, John York in San Jose. In re Ossian Draycott; In the \
                  Matter of Quill; Ex parte Vell; In re Quillon Freight LLC. The \
                  Federal Election Commission met; Federal agreed. Ms St. Louis \
                  signed in St. Louis.";
    let at = |anchor: &str, text: &str| {
        let start = source.find(anchor).unwrap() + anchor.find(text).unwrap();
        start..start + text.len()
    };
    let mut detections: Vec<Detection> = [
        (at("Mr He", "He"), Category::Person),
        (at("Dr. General", "Dr. General"), Category::Person),
        (at("He appeals", "He"), Category::Person),
        (at("Dawn French", "Dawn French"), Category::Person),
        (at("Sarah In", "Sarah In"), Category::Person),
        (at("French and In", "French"), Category::Person),
        (at("French and In", "In"), Category::Person),
        (at("Christian too", "Christian"), Category::Person),
        (at("New York", "York"), Category::Person),
        (at("John York", "John York"), Category::Person),
        (at("San Jose", "Jose"), Category::Person),
        (
            at("In re Ossian Draycott", "In re Ossian Draycott"),
            Category::Person,
        ),
        (
            at("In the Matter of Quill", "In the Matter of Quill"),
            Category::Person,
        ),
        (at("Ex parte Vell", "Ex parte Vell"), Category::Person),
        (
            at("In re Quillon Freight LLC", "In re Quillon Freight LLC"),
            Category::Organization,
        ),
        // A public body names no one, so its words stay common.
        (
            at("Federal Election Commission", "Federal Election Commission"),
            Category::Person,
        ),
        (at("Federal agreed", "Federal"), Category::Person),
        // A title makes a place a surname; the place itself goes.
        (at("Ms St. Louis", "St. Louis"), Category::Person),
        (at("in St. Louis", "St. Louis"), Category::Person),
    ]
    .into_iter()
    .map(|(range, category)| Detection {
        range,
        category,
        score: 0.9,
        recognizer: Recognizer::Model,
    })
    .collect();
    let pack = pack();
    pack.fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
    let found: Vec<&str> = detections
        .iter()
        .map(|d| &source[d.range.clone()])
        .collect();
    assert_eq!(
        found,
        [
            "He",
            "General",
            "Dawn French",
            "Sarah In",
            "French",
            "In",
            "John York",
            "Ossian Draycott",
            "Quill",
            "Vell",
            "Quillon Freight LLC",
            "St. Louis",
        ]
    );
}

/// Template and heading words, group nouns for people, deities, a pay
/// grade and a programme's title name no one alone (#500, round 2);
/// plural surnames, given names in the plural, initials and any other
/// name word stay.
#[test]
fn template_words_group_nouns_and_deities_are_no_names() {
    let pack = pack();
    for (text, category) in [
        ("Fire", Category::Person),
        ("Fires", Category::Person),
        ("Class A fires", Category::Person),
        ("Location", Category::Person),
        ("Index", Category::Person),
        ("Indices", Category::Person),
        ("Manual", Category::Person),
        ("Briefing", Category::Person),
        ("Accident", Category::Person),
        ("Investigation", Category::Person),
        ("OPERATING", Category::Person),
        ("OPERATING MANUAL", Category::Person),
        ("Serious Accident Investigation", Category::Person),
        ("Serious Accident Investigation Team", Category::Person),
        ("Fire Safety", Category::Organization),
        ("Librarians", Category::Person),
        ("Hospitalists", Category::Person),
        ("Physicians", Category::Person),
        ("Biologists", Category::Person),
        ("Investors", Category::Person),
        ("Issuers", Category::Person),
        ("Importers", Category::Person),
        ("Intermediaries", Category::Person),
        ("Adults", Category::Person),
        ("Dashers", Category::Person),
        ("Founding Fathers", Category::Person),
        ("Female employees", Category::Person),
        ("God", Category::Person),
        ("Allah", Category::Organization),
        ("the Almighty", Category::Person),
        ("the Holy Spirit", Category::Person),
        ("GS-14", Category::Person),
        ("GS-13/14", Category::Person),
        ("GS 9", Category::Organization),
        ("Consumer Price Index", Category::Person),
    ] {
        assert!(!pack.identifying(text, category), "{text}");
    }
    for (text, category) in [
        ("the Smiths", Category::Person),
        ("Jones", Category::Person),
        ("Williams", Category::Person),
        ("Adams", Category::Person),
        ("Roberts", Category::Person),
        ("Rogers", Category::Person),
        ("Myers", Category::Person),
        ("Summers", Category::Person),
        ("Adrians", Category::Person),
        ("Julians", Category::Person),
        ("Brians", Category::Person),
        ("Edda Fire", Category::Person),
        ("Rhea Gillians", Category::Person),
        ("Jesus Navas", Category::Person),
        ("Jesus", Category::Person),
        ("Lindquists", Category::Person),
        ("Kardashians", Category::Person),
        ("Christian Head", Category::Person),
        ("Mary Ann Standard", Category::Person),
        ("Paul Program", Category::Person),
        ("J. R. Plan", Category::Person),
        ("A. Smith", Category::Person),
        ("K Ashdale", Category::Person),
        ("Ivo Plot", Category::Person),
        ("Ossian Draycott", Category::Person),
        ("General Accident", Category::Organization),
        ("Acme Fire Safety Ltd", Category::Organization),
        ("Index Ventures", Category::Organization),
        ("Physicians Mutual", Category::Organization),
        ("GS-14 Holdings Ltd", Category::Organization),
    ] {
        assert!(pack.identifying(text, category), "{text}");
    }
}

/// The issue's round 2 reproduction through the review: a contract's
/// headings, group nouns, plot labels, the article they would spread
/// to and the short "Ex p" of a caption name no one; the parties stay
/// (#500). A programme's title spreads no "Program", and a heading the
/// model marks ("FRANCHISE AGREEMENT") seeds no "Agreement" (#416).
#[test]
fn the_round_two_reproduction_proposes_only_the_parties() {
    let source = "FRANCHISE AGREEMENT\n\n11. OPERATING MANUAL\n\nSection Two: Program \
        Description\n\nThis Agreement binds the parties. Librarians and Hospitalists \
        briefed the Investigation Team. Plot B lies east of Plot A.\n\nA short note \
        follows on this application, Ex p Ossian Draycott; Ms Edda Brannock signed. \
        The CAFTA-DR Environmental Cooperation Program runs; the Program pays.\n\n\
        J. Recommendations\n\nM.\n\nGS-14 staff, Female employees and the Founding \
        Fathers prayed to God.";
    let raw = [
        ("FRANCHISE AGREEMENT", 0, "person"),
        ("OPERATING", 0, "person"),
        ("Program", 0, "person"),
        ("Agreement", 0, "person"),
        ("Librarians", 0, "person"),
        ("Hospitalists", 0, "person"),
        ("Investigation Team", 0, "person"),
        ("Ex p Ossian Draycott", 0, "person"),
        ("Edda Brannock", 0, "person"),
        ("CAFTA-DR Environmental Cooperation Program", 0, "person"),
        ("Program", 2, "person"),
        ("J.", 0, "person"),
        ("M.", 0, "person"),
        ("GS-14", 0, "person"),
        ("Female employees", 0, "person"),
        ("Founding Fathers", 0, "person"),
        ("God", 0, "person"),
    ];
    // The plots' letters, not a "B" or an "A" inside another word.
    let letter = |label: &str| source.find(label).unwrap() + "Plot ".len();
    let mut raw: Vec<crate::RawEntity> = raw
        .iter()
        .map(|(text, n, label)| {
            let (start, _) = source
                .match_indices(text)
                .nth(*n)
                .unwrap_or_else(|| panic!("{text:?} #{n} not in the source"));
            crate::RawEntity {
                start,
                end: start + text.len(),
                label: (*label).into(),
                score: 0.8,
                text: (*text).into(),
            }
        })
        .collect();
    for label in ["Plot B", "Plot A"] {
        let start = letter(label);
        raw.push(crate::RawEntity {
            start,
            end: start + 1,
            label: "person".into(),
            score: 0.8,
            text: source[start..start + 1].into(),
        });
    }
    let shown: Vec<_> = crate::recognition::proposals(source, &raw)
        .unwrap()
        .into_iter()
        .map(|span| (source[span.range].to_owned(), span.category))
        .collect();
    assert_eq!(
        shown,
        pairs(&[("Ossian Draycott", "PERSON"), ("Edda Brannock", "PERSON")])
    );
    // The hosted model starts the span inside "Ex p": "p Ossian Draycott".
    // A lower-case "p" before a name comes off; a capital initial
    // ("P Ossian", #387) stays.
    let source = "On this application, Ex p Ossian Draycott; and P Ossian signed.";
    for (text, expected) in [
        ("p Ossian Draycott", Some("Ossian Draycott")),
        ("P Ossian", Some("P Ossian")),
    ] {
        let start = source.find(text).unwrap();
        let raw = [crate::RawEntity {
            start,
            end: start + text.len(),
            label: "person".into(),
            score: 0.8,
            text: text.into(),
        }];
        let shown: Vec<_> = crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .filter(|span| span.category == "PERSON")
            .map(|span| source[span.range].to_owned())
            .collect();
        assert_eq!(shown.first().map(String::as_str), expected, "{text}");
    }
}

/// In context the same words stay a person's name: after a title, or
/// where a person span has them beside a name (#500). A plot's letter
/// leaves the name after it, and an initial before a surname stays.
#[test]
fn template_words_and_group_nouns_stay_names_in_context() {
    let source = "Mr Manual and Ms Gillians met Edda Fire, Rhea Gillians and Jesus \
        Navas. Fire, Gillians and Jesus signed. Ivo Plot, Judge K Ashdale and A. \
        Smith agreed.";
    let raw = [
        ("Manual", 0, "person"),
        ("Gillians", 0, "person"),
        ("Edda Fire", 0, "person"),
        ("Rhea Gillians", 0, "person"),
        ("Jesus Navas", 0, "person"),
        ("Fire", 1, "person"),
        ("Gillians", 2, "person"),
        ("Jesus", 1, "person"),
        ("Ivo Plot", 0, "person"),
        ("Ashdale", 0, "person"),
        ("A. Smith", 0, "person"),
    ];
    assert_eq!(
        shown_at(source, &raw),
        pairs(&[
            ("Manual", "PERSON"),
            ("Gillians", "PERSON"),
            ("Edda Fire", "PERSON"),
            ("Rhea Gillians", "PERSON"),
            ("Jesus Navas", "PERSON"),
            ("Fire", "PERSON"),
            ("Gillians", "PERSON"),
            ("Jesus", "PERSON"),
            ("Ivo Plot", "PERSON"),
            ("K Ashdale", "PERSON"),
            ("A. Smith", "PERSON"),
        ])
    );
}

/// A person span with a common word needs a word of its own, a name or
/// an initial, not a role or heading modifier: the model's "General
/// Program" names no one, and so neither makes nor spreads a lone
/// "Program"; "Paul Program" does both (#500).
#[test]
fn only_a_name_word_makes_a_common_word_a_name() {
    for (source, raw, expected) in [
        (
            "The General Program met. Section Two: Program Description.",
            [("General Program", 0, "person"), ("Program", 1, "person")],
            vec![],
        ),
        (
            "Paul Program met. Section Two: Program Description.",
            [("Paul Program", 0, "person"), ("Program", 1, "person")],
            vec![("Paul Program", "PERSON"), ("Program", "PERSON")],
        ),
    ] {
        assert_eq!(shown_at(source, &raw), pairs(&expected), "{source}");
    }
}

/// The pre-gate review's inputs for the #500 rules (P1 and P2): each is
/// proposed as before them, the model's spans and the review's spread
/// alike. Family plurals of surnames, three-word names ending in a head
/// word, a role word after a common word, an initial the model split
/// from its surname or a label put before, a party lettered "A" and a
/// boy called Jesus all stay names.
#[test]
fn the_500_rules_keep_the_reviews_names() {
    const P: &str = "person";
    let shown = |source: &str, raw: &[(&str, usize, &str)]| {
        let raw: Vec<crate::RawEntity> = raw
            .iter()
            .map(|(text, n, label)| {
                let (start, _) = source.match_indices(text).nth(*n).unwrap();
                crate::RawEntity {
                    start,
                    end: start + text.len(),
                    label: (*label).into(),
                    score: 0.9,
                    text: (*text).into(),
                }
            })
            .collect();
        crate::recognition::proposals(source, &raw)
            .unwrap()
            .iter()
            .map(|s| {
                format!(
                    "{}:{:?}@{}",
                    s.category,
                    &source[s.range.clone()],
                    s.range.start
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    for (source, raw, expected) in [
        // P1-1: a role word after a common word is a surname.
        (
            "Christian Head testified.",
            &[("Christian Head", 0, P)][..],
            r#"PERSON:"Christian Head"@0"#,
        ),
        (
            "Christian Senior testified.",
            &[("Christian Senior", 0, P)],
            r#"PERSON:"Christian Senior"@0"#,
        ),
        (
            "Christian Principal testified.",
            &[("Christian Principal", 0, P)],
            r#"PERSON:"Christian Principal"@0"#,
        ),
        (
            "German Chief testified.",
            &[("German Chief", 0, P)],
            r#"PERSON:"German Chief"@0"#,
        ),
        (
            "Law Head signed.",
            &[("Law Head", 0, P)],
            r#"PERSON:"Law Head"@0"#,
        ),
        (
            "Jesus Head testified.",
            &[("Jesus Head", 0, P)],
            r#"PERSON:"Jesus Head"@0"#,
        ),
        (
            "Christian Head testified. Mr Head left. Head said.",
            &[("Christian Head", 0, P), ("Head", 1, P), ("Head", 2, P)],
            r#"PERSON:"Christian Head"@0 PERSON:"Head"@29 PERSON:"Head"@40"#,
        ),
        // P1-2: a family's plural surname.
        (
            "The Lindquists appeal the order.",
            &[("Lindquists", 0, P)],
            r#"PERSON:"Lindquists"@4"#,
        ),
        (
            "John Lindquist and Mary Lindquist sued. The Lindquists appeal.",
            &[
                ("John Lindquist", 0, P),
                ("Mary Lindquist", 0, P),
                ("Lindquists", 0, P),
            ],
            r#"PERSON:"John Lindquist"@0 PERSON:"Mary Lindquist"@19 PERSON:"Lindquists"@44"#,
        ),
        (
            "John Lindquist sued. The Lindquists' house burned.",
            &[("John Lindquist", 0, P), ("Lindquists'", 0, P)],
            r#"PERSON:"John Lindquist"@0 PERSON:"Lindquists'"@25"#,
        ),
        (
            "JOHN LINDQUIST AND MARY LINDQUIST, THE LINDQUISTS",
            &[
                ("JOHN LINDQUIST", 0, P),
                ("MARY LINDQUIST", 0, P),
                ("LINDQUISTS", 0, P),
            ],
            r#"PERSON:"JOHN LINDQUIST"@0 PERSON:"MARY LINDQUIST"@19 PERSON:"LINDQUISTS"@39"#,
        ),
        (
            "Erik Lundqvist sued. The Lundqvists appeal.",
            &[("Erik Lundqvist", 0, P), ("Lundqvists", 0, P)],
            r#"PERSON:"Erik Lundqvist"@0 PERSON:"Lundqvists"@25"#,
        ),
        (
            "Kim Kardashian testified. The Kardashians own the house.",
            &[("Kim Kardashian", 0, P), ("Kardashians", 0, P)],
            r#"PERSON:"Kim Kardashian"@0 PERSON:"Kardashians"@30"#,
        ),
        (
            "Plaintiffs Aram Hagopian and Lucine Hagopian (the Hagopians) sued.",
            &[
                ("Aram Hagopian", 0, P),
                ("Lucine Hagopian", 0, P),
                ("Hagopians", 0, P),
            ],
            r#"PERSON:"Aram Hagopian"@11 PERSON:"Lucine Hagopian"@29 PERSON:"Hagopians"@50"#,
        ),
        (
            "Plaintiffs, the Bergquists, appeal.",
            &[("Bergquists", 0, P)],
            r#"PERSON:"Bergquists"@16"#,
        ),
        // P1-3: three-word names ending in a head word.
        (
            "Mary Ann Standard testified.",
            &[("Mary Ann Standard", 0, P)],
            r#"PERSON:"Mary Ann Standard"@0"#,
        ),
        (
            "Maria Jose Portal testified.",
            &[("Maria Jose Portal", 0, P)],
            r#"PERSON:"Maria Jose Portal"@0"#,
        ),
        (
            "Joan Maria Plans testified.",
            &[("Joan Maria Plans", 0, P)],
            r#"PERSON:"Joan Maria Plans"@0"#,
        ),
        (
            "Mary Ann Standard testified. Ms Standard left.",
            &[("Mary Ann Standard", 0, P), ("Standard", 1, P)],
            r#"PERSON:"Mary Ann Standard"@0 PERSON:"Standard"@32"#,
        ),
        (
            "Ana Maria Plans sued. Ms Plans won.",
            &[("Ana Maria Plans", 0, P), ("Plans", 1, P)],
            r#"PERSON:"Ana Maria Plans"@0 PERSON:"Plans"@25"#,
        ),
        // A head word the pattern keeps: titled or named elsewhere, the
        // name stays; with neither, the programme goes.
        (
            "Mary Ann Project testified. Ms Project left.",
            &[("Mary Ann Project", 0, P), ("Project", 1, P)],
            r#"PERSON:"Mary Ann Project"@0 PERSON:"Project"@31"#,
        ),
        (
            "Mary Ann Project testified. Ivo Project left.",
            &[("Mary Ann Project", 0, P), ("Ivo Project", 0, P)],
            r#"PERSON:"Mary Ann Project"@0 PERSON:"Ivo Project"@28"#,
        ),
        (
            "Mary Ann Project testified.",
            &[("Mary Ann Project", 0, P)],
            "",
        ),
        // P2-1: an initial with a name after it or a label before it.
        (
            "Signed:\nJ. Smith\nDirector",
            &[("J.", 0, P), ("Smith", 0, P)],
            r#"PERSON:"J."@8 PERSON:"Smith"@11"#,
        ),
        (
            "Signed:\nM.\nSmith\n",
            &[("M.", 0, P), ("Smith", 0, P)],
            r#"PERSON:"M."@8 PERSON:"Smith"@11"#,
        ),
        (
            "Witnesses:\nK. Ashdale",
            &[("K.", 0, P)],
            r#"PERSON:"K."@11"#,
        ),
        // A list letter alone still goes.
        ("Notes\n\nJ. Recommendations\n", &[("J.", 0, P)], ""),
        ("Notes\n\nM.\n\nThe end.", &[("M.", 0, P)], ""),
        // P2-2: a party lettered "A".
        (
            "Q. Did you go? A. Yes.\nA said no.",
            &[("A", 1, P)],
            r#"PERSON:"A"@15 PERSON:"A"@23"#,
        ),
        (
            "The claimant is A. A claimed asylum in 2019.",
            &[("A", 1, P)],
            r#"PERSON:"A"@16 PERSON:"A"@19"#,
        ),
        (
            "Then she left. A de Souza testified.",
            &[("A", 0, P)],
            r#"PERSON:"A"@15"#,
        ),
        (
            "A. A claimed asylum.\nB. B is her son.",
            &[("A", 1, P)],
            r#"PERSON:"A"@0 PERSON:"A"@3"#,
        ),
        // P2-3: "Jesus" as a given name; the deity by its phrases.
        (
            "Her son, Jesus, testified. Jesus was nine.",
            &[("Jesus", 0, P), ("Jesus", 1, P)],
            r#"PERSON:"Jesus"@9 PERSON:"Jesus"@27"#,
        ),
        (
            "Then Jesus testified.",
            &[("Jesus", 0, P)],
            r#"PERSON:"Jesus"@5"#,
        ),
        ("praise Jesus Christ", &[("Jesus Christ", 0, P)], ""),
        ("praise the Lord Jesus", &[("Lord Jesus", 0, P)], ""),
        (
            "Jesus of Nazareth taught.",
            &[("Jesus of Nazareth", 0, P)],
            "",
        ),
        (
            "Jesus Martinez sued. Jesus testified.",
            &[("Jesus Martinez", 0, P), ("Jesus", 1, P)],
            r#"PERSON:"Jesus Martinez"@0 PERSON:"Jesus"@21"#,
        ),
        (
            "Carol Christ wrote. Christ said no.",
            &[("Carol Christ", 0, P), ("Christ", 1, P)],
            r#"PERSON:"Carol Christ"@0 PERSON:"Christ"@20"#,
        ),
    ] {
        assert_eq!(shown(source, raw), expected, "{source:?}");
    }
}

/// Date masks (#487), common abbreviations (#481), the Internet,
/// software and site domains (#488) are no names; companies that share
/// their words keep them.
#[test]
fn masks_abbreviations_the_internet_and_products_are_no_names() {
    let pack = pack();
    for (text, category) in [
        ("ДД.ММ.ГГГГ", Category::Organization),
        ("ДД.ММ.ГГГГ", Category::Person),
        ("ДД.ММ.ГГГГ", Category::Date),
        ("ДД.ММ.ГГ", Category::Organization),
        ("ХХ.ХХ.ХХ", Category::Person),
        ("XX.XX.XXXX", Category::Organization),
        ("дд/мм/гггг", Category::Organization),
        ("«ДД.ММ.ГГГГ»", Category::Organization),
        ("ДД.ММ.ГГГГ г.", Category::Organization),
        ("ХХ.ХХ.ХХ №№", Category::Organization),
        ("ДД.ММ.ГГГГ №-осн", Category::Organization),
        ("ДД.ММ.ГГГГ уроженца", Category::Person),
        ("ТКО", Category::Organization),
        ("ТН", Category::Organization),
        ("ТТН", Category::Organization),
        ("АЗС", Category::Organization),
        ("СОУЭ", Category::Organization),
        ("АПС", Category::Organization),
        ("МЧД", Category::Organization),
        ("КЭП", Category::Organization),
        ("ЭП", Category::Organization),
        ("ЕИО", Category::Organization),
        ("ЭДО", Category::Organization),
        ("МЧД", Category::Person),
        ("Интернет", Category::Organization),
        ("«Интернет»", Category::Organization),
        ("Интернета", Category::Organization),
        ("Интернете", Category::Organization),
        ("сети «Интернет»", Category::Organization),
        (
            "информационно-телекоммуникационной сети «Интернет»",
            Category::Organization,
        ),
        (
            "информационно-телекоммуникационная сеть Интернет",
            Category::Organization,
        ),
        ("КонсультантПлюс", Category::Organization),
        ("{КонсультантПлюс}", Category::Organization),
        ("Ватсап", Category::Organization),
        ("Ватсапе", Category::Organization),
        ("WhatsApp", Category::Organization),
        ("Телеграм", Category::Organization),
        ("ПО «Контур»", Category::Organization),
        ("АСУ «Экспресс»", Category::Organization),
        ("программы «Сетка»", Category::Organization),
        ("NALOG.GOV.RU", Category::Organization),
        ("M4D.NALOG.GOV.RU", Category::Organization),
        ("example.invalid", Category::Organization),
        ("https://www.sait-primer.ru/", Category::Organization),
        ("сайт.рф", Category::Organization),
        ("«EXAMPLE.GOV.RU»", Category::Organization),
    ] {
        assert!(!pack.identifying(text, category), "{text}");
    }
    for (text, category) in [
        ("ООО «Контур»", Category::Organization),
        ("АО «ТТН-Логистик»", Category::Organization),
        ("ООО «АЗС»", Category::Organization),
        ("АЗС ООО «Бета-Ойл»", Category::Organization),
        ("ООО «КонсультантПлюс»", Category::Organization),
        ("ООО «Интернет Решения»", Category::Organization),
        ("Интернет Решения", Category::Organization),
        ("Сети «Ромашка»", Category::Organization),
        ("Сбербанк Онлайн", Category::Organization),
        ("Яндекс.Такси", Category::Organization),
        ("E.ON", Category::Organization),
        ("Alfa.Bank", Category::Organization),
        ("J.P.Morgan", Category::Organization),
        ("М.Г.", Category::Person),
        ("Х. М. Гусейнов", Category::Person),
        ("Дмитрий Дымов", Category::Person),
        ("12.03.2026", Category::Date),
        ("Тнеева", Category::Person),
    ] {
        assert!(pack.identifying(text, category), "{text}");
    }
}

/// In context: a product's name in «» after the word for it is no
/// company, inside the quotes or with them (#488); a company named
/// like an abbreviation or product keeps its name (#481).
#[test]
fn a_product_in_guillemets_is_no_company_and_a_company_stays() {
    let corrected = |source: &str, spans: &[&str]| -> Vec<String> {
        let mut detections: Vec<Detection> = spans
            .iter()
            .filter_map(|text| {
                let start = source.find(text).unwrap();
                let range = start..start + text.len();
                pack()
                    .identifying_at(source, range.clone(), text, Category::Organization)
                    .then_some(Detection {
                        range,
                        category: Category::Organization,
                        score: 0.9,
                        recognizer: Recognizer::Model,
                    })
            })
            .collect();
        pack().fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
        detections
            .into_iter()
            .map(|d| source[d.range].to_owned())
            .collect()
    };
    let none: [String; 0] = [];
    for (source, span) in [
        ("расчёт выполнен посредством ПО «Сетка» и", "Сетка"),
        ("расчёт выполнен посредством ПО «Сетка» и", "«Сетка»"),
        ("билеты оформлены в АСУ «Экспресс» и", "Экспресс"),
        ("в программе «Сетка» за", "Сетка"),
        ("в группах мессенджера «Ватсап» и", "Ватсап"),
        ("данные системы «Гарант» и", "«Гарант»"),
        ("размещено в сети «Интернет» на", "Интернет"),
        ("расчёт выполнен в ПО \"Сетка\" и", "Сетка"),
        ("расчёт выполнен в ПО \"Сетка\" и", "ПО \"Сетка\""),
        ("Система «Сетка» считает", "Сетка"),
    ] {
        assert_eq!(corrected(source, &[span]), none, "{source}: {span}");
    }
    for (source, span, whole) in [
        ("договор с ООО «Контур» от", "Контур", "ООО «Контур»"),
        (
            "на АЗС ООО «Бета-Ойл» в",
            "АЗС ООО «Бета-Ойл»",
            "ООО «Бета-Ойл»",
        ),
        ("товар передан ООО «АЗС» в", "АЗС", "ООО «АЗС»"),
        ("оператор АНО «ЭДО» в", "ЭДО", "АНО «ЭДО»"),
        // A trim never leaves a legal form alone.
        ("перевозчик АО ТТН в", "АО ТТН", "АО ТТН"),
        ("carrier ТТН LLC in", "ТТН LLC", "ТТН LLC"),
        (
            "перевозчик АО «ТТН-Логистик» в",
            "ТТН-Логистик",
            "АО «ТТН-Логистик»",
        ),
        (
            "лицензия ООО «КонсультантПлюс» на",
            "КонсультантПлюс",
            "ООО «КонсультантПлюс»",
        ),
        // "по" is a preposition, not ПО.
        ("по «Сетке» и", "Сетке", "Сетке"),
        // An app's name waits for the guide (#484).
        (
            "в приложении «Сбербанк Онлайн» и",
            "Сбербанк Онлайн",
            "Сбербанк Онлайн",
        ),
    ] {
        assert_eq!(corrected(source, &[span]), [whole], "{source}: {span}");
    }
    // A person's name in its own quotes after a work's word is a title.
    assert!(!pack().identifying_at(
        "рукопись повести «Кедровый свет».",
        32..61,
        "«Кедровый свет»",
        Category::Person
    ));
}

/// #482: a settlement's type before a place is no person's initial,
/// after a word that leads a place or with a place's ending.
#[test]
fn settlement_types_before_a_place_are_no_initials() {
    for (source, person) in [
        (
            "Ответчик, уроженец а. Новый Хутор, явился.",
            "а. Новый Хутор",
        ),
        ("Ответчик, уроженец а. Новый Хутор, явился.", "Хутор"),
        ("Ответчица, уроженка с. Кедровка, явилась.", "с. Кедровка"),
        ("Письмо из х. Тихий получено.", "х. Тихий"),
        (
            "Решение районного суда г. Ольховск; проведено в г. Ольховск.",
            "г. Ольховск",
        ),
        (
            "Решение Районного суда г. Пёнсан вступило в силу.",
            "г. Пёнсан",
        ),
        (
            "Осмотр проведен в пос. Кедровый ключ.",
            "пос. Кедровый ключ",
        ),
        ("Обыск проведен в ст. Кедровская.", "ст. Кедровская"),
        ("Список: с. Кедровка.", "с. Кедровка"),
        ("Список: г. Ольховск.", "г. Ольховск"),
    ] {
        assert!(
            shown(source, &[(person, "person")]).is_empty(),
            "{source}: {person}"
        );
    }
}

/// #482 review: a lowercase «г.» is a title or a year's end, and «с.»,
/// «ст.» a stray dot, before a real surname, which stays PERSON: a
/// genitive («-ука», «-ака», «-ка»), an adjective and a verb, a year on
/// the line before or with its ending («1990-х г.»), an instrumental
/// after «с.», a capitalised word after an adjective after «в».
#[test]
fn surnames_after_lowercase_types_stay_people() {
    for (source, person) in [
        (
            "Постановлением от 12 марта 2020
г. Ковальская признана потерпевшей.",
            "Ковальская",
        ),
        (
            "Постановлением от 12 марта 2020
г. Ковальчука признали потерпевшим.",
            "Ковальчука",
        ),
        (
            "Постановлением от 12 марта 2020
г. Кедровка признана потерпевшей.",
            "Кедровка",
        ),
        ("в 1990-х г. Ковальская переехала.", "Ковальская"),
        ("в 2020-м г. Ковальская работала.", "Ковальская"),
        ("в 2020-м г. Кедровка работала.", "Кедровка"),
        (
            "В судебном заседании г. Ковальская пояснила, что иск не признает.",
            "Ковальская",
        ),
        (
            "В судебном заседании г. Ковальская пояснила, что иск не признает.",
            "г. Ковальская",
        ),
        ("Возражения г. Ковальский поддержал.", "Ковальский"),
        ("Документы получены от г. Ковальчука лично.", "Ковальчука"),
        (
            "Документы получены от г. Ковальчука лично.",
            "г. Ковальчука",
        ),
        ("Подпись г. Новака заверена нотариусом.", "Новака"),
        ("Согласно показаниям г. Бойка установлено.", "Бойка"),
        ("Иск подан в интересах г. Марка.", "Марка"),
        ("Подписал ст. Ковальчука не видел.", "Ковальчука"),
        ("Показания с. Ковальчука оглашены.", "Ковальчука"),
        ("Показания с. Ковальчука оглашены.", "с. Ковальчука"),
        ("Суд с. Ковальчуком согласился.", "Ковальчуком"),
        ("Суд с. Ковальчуком согласился.", "с. Ковальчуком"),
        ("проведено в г. Нижний Ковальчук.", "Ковальчук"),
        // A place alone, with no word that leads one and no address
        // part after it, keeps the model's person: the price of
        // keeping a title's name ("г. Ковальская пояснила").
        ("Список: п. Кедровый ключ.", "п. Кедровый ключ"),
    ] {
        assert_eq!(
            shown(source, &[(person, "person")]),
            pairs(&[(person, "PERSON")]),
            "{source}: {person}"
        );
    }
}

/// #482 guards: initials, a title's name, a surname after a stray dot
/// and a name after the place stay PERSON.
#[test]
fn initials_beside_settlement_guards_stay_people() {
    for (source, person, expected) in [
        (
            "Ответчик Иванов А. Б. явился.",
            "Иванов А. Б.",
            "Иванов А. Б.",
        ),
        ("Договор подписал А. Петров.", "А. Петров", "А. Петров"),
        ("Заявление подал г-н Петров.", "г-н Петров", "Петров"),
        (
            "Договорился с. Петровым о встрече.",
            "с. Петровым",
            "с. Петровым",
        ),
        (
            "Договорился с П. Петровым о встрече.",
            "П. Петровым",
            "П. Петровым",
        ),
        (
            "Договор подписан в г. Ольховск Ивановым.",
            "Ивановым",
            "Ивановым",
        ),
        (
            "Протокол от 12.03.2026 г. Иванка Петрова подписала.",
            "Иванка",
            "Иванка",
        ),
        (
            "Свидетель А.С. Петров явился.",
            "А.С. Петров",
            "А.С. Петров",
        ),
    ] {
        assert_eq!(
            shown(source, &[(person, "person")]),
            pairs(&[(expected, "PERSON")]),
            "{source}"
        );
    }
}

/// Placeholders for a party are never marked, whatever the model
/// guessed; a real name beside one, and real names and companies called
/// alike, stay (guide rule 8, #484 item 1).
#[test]
fn placeholders_are_no_names() {
    let pack = pack();
    for (text, category) in [
        ("ФИО1", Category::Organization),
        ("ФИО1", Category::Person),
        ("ФИО1", Category::Identity),
        ("ФИО 2", Category::Person),
        ("«ФИО12»", Category::Organization),
        ("ИП ФИО1", Category::Organization),
        ("ИП ФИО1", Category::Person),
        ("ФИО1 и ФИО2", Category::Person),
        ("ФИО1, ФИО2", Category::Organization),
        ("John Doe", Category::Person),
        ("Jane Does 1-25", Category::Person),
        ("John Does l-25", Category::Person),
        ("Richard Roe", Category::Person),
        ("DOE SUPERVISORS I-X", Category::Organization),
        ("DOE SUPERVISORS", Category::Organization),
        ("DOE MEDICAL STAFF I-X", Category::Organization),
        ("ROE ENTITIES", Category::Organization),
        ("ROE CORPORATIONS I through X", Category::Organization),
        ("DOES 1 through 10", Category::Person),
        ("Does 1-10", Category::Person),
        ("Doe 3", Category::Person),
        ("DOE", Category::Person),
        ("DOE", Category::Organization),
        ("ROE", Category::Organization),
        ("Does", Category::Person),
        ("I-X", Category::Person),
        ("I-X", Category::Organization),
        ("1-25", Category::Organization),
        ("ФИО1", Category::Tax),
        ("ФИО1", Category::Phone),
        ("ФИО1", Category::Bank),
        // A role before it, a dash between two, a role-number
        // placeholder (#484 item 1, extended).
        ("конкурсного управляющего ФИО1", Category::Person),
        ("ФИО3 – ФИО6", Category::Organization),
        ("ФИО3-ФИО6", Category::Person),
        ("Потерпевший №1", Category::Person),
        ("потерпевшего № 2", Category::Organization),
        ("представителя потерпевшего №1", Category::Person),
        ("Свидетель №3", Category::Person),
        ("Потерпевший №1", Category::Identity),
        // Real initials after one come back alone by the span pattern.
        ("ИП ФИО3 А-Х.О.", Category::Organization),
        ("ФИО3 А-Х.О.", Category::Person),
        ("ФИО2 В. С.", Category::Person),
    ] {
        assert!(!pack.identifying(text, category), "{text}");
    }
    for (text, category) in [
        ("ФИО3 Иванов", Category::Person),
        ("Потерпевший Ветлин", Category::Person),
        ("ИП Мохов Л. Р. магазин «Кедрач»", Category::Organization),
        ("ИП «Кедрач»", Category::Organization),
        ("Мирон Павлович Грендель", Category::Person),
        ("Samuel Doe", Category::Person),
        ("Jane Doerr", Category::Person),
        ("John Roebuck", Category::Person),
        ("Doe Run Company", Category::Organization),
        ("Roe Holdings Ltd", Category::Organization),
        ("Ivo X", Category::Person),
        ("J. Doe", Category::Person),
        ("A. Roe", Category::Person),
        ("Doe", Category::Person),
    ] {
        assert!(pack.identifying(text, category), "{text}");
    }
    // In context: "ФИО:" is a label, the name after it a person; real
    // initials after a placeholder are PERSON alone, whether the model
    // took them with it, as an organization, or not at all. "Doe"
    // alone is a person's after a title or beside a real first name.
    let source = "ФИО: Мирон Грендель. Истец ФИО1 и ИП ФИО2 подали иск к ИП ФИО3 А-Х.О. \
        и к ФИО4 В. С., Потерпевший №1 возражал.";
    let raw = [
        ("Мирон Грендель", "person"),
        ("ФИО1", "organization"),
        ("ИП ФИО2", "organization"),
        ("ИП ФИО3 А-Х.О.", "organization"),
        ("Потерпевший №1", "person"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("Мирон Грендель", "PERSON"),
            ("А-Х.О.", "PERSON"),
            ("В. С.", "PERSON")
        ])
    );
    // After a role-number placeholder too, and before a speaker's
    // colon.
    let source = "Потерпевший №2 Л. И. пояснил. ФИО5 Р. Т.: «Возражаю».";
    let raw = [("Потерпевший №2 Л. И.", "person"), ("ФИО5 Р. Т.", "person")];
    assert_eq!(
        shown(source, &raw),
        pairs(&[("Л. И.", "PERSON"), ("Р. Т.", "PERSON")])
    );
    let source = "Plaintiffs sue John Does 1-25, DOE SUPERVISORS I-X and ROE ENTITIES; \
        DOE agreed.";
    let raw = [
        ("John Does 1-25", "person"),
        ("DOE SUPERVISORS", "organization"),
        ("I-X", "person"),
        ("ROE ENTITIES", "organization"),
        ("DOE", "person"),
    ];
    assert_eq!(shown(source, &raw), pairs(&[]));
    let source = "Witness J. Doe testified.";
    assert_eq!(
        shown(source, &[("J. Doe", "person")]),
        pairs(&[("J. Doe", "PERSON")])
    );
    let source = "Mr Doe testified.";
    assert_eq!(
        shown(source, &[("Doe", "person")]),
        pairs(&[("Doe", "PERSON")])
    );
    let source = "Doe left early. Samuel Doe agreed.";
    assert_eq!(
        shown(source, &[("Doe", "person"), ("Samuel Doe", "person")]),
        pairs(&[("Doe", "PERSON"), ("Samuel Doe", "PERSON")])
    );
}

/// Programmes, standards, conditions, treatises, a legal form with no
/// name, generic words, common abbreviations, and software, sites and
/// app platforms are no companies (#501, #481, #488); companies with
/// their legal form or a name of their own keep it.
#[test]
fn programmes_abbreviations_and_products_are_no_companies() {
    let pack = pack();
    for (text, category) in [
        // Programmes, services, plans, standards, projects (#501).
        ("Bright Paths Program", Category::Organization),
        ("the Bright Paths Programme", Category::Organization),
        ("VR services", Category::Organization),
        ("National Reading Access Standard", Category::Organization),
        (
            "Brindle Backwater Pumping Plant project",
            Category::Organization,
        ),
        ("Quality Assurance", Category::Organization),
        ("Phase II Trial", Category::Organization),
        ("Federal Employment", Category::Organization),
        ("Acute Kidney Strain", Category::Organization),
        ("Acute Lung Injury", Category::Organization),
        ("Respiratory Distress Syndrome", Category::Organization),
        // A legal form with no name, and a treatise.
        (
            "a Wyoming limited liability company",
            Category::Organization,
        ),
        (
            "New Jersey limited liability company",
            Category::Organization,
        ),
        ("a Delaware corporation", Category::Organization),
        ("an English private limited company", Category::Organization),
        ("4 Brindle on Insolvency", Category::Organization),
        ("Vell and Ostrand on Contracts", Category::Organization),
        // Generic words alone.
        ("Crew", Category::Organization),
        ("The Crew", Category::Organization),
        ("Crew", Category::Person),
        ("Operations", Category::Organization),
        ("Assignment", Category::Organization),
        ("Facilities", Category::Organization),
        ("Human Resources", Category::Organization),
        ("Services", Category::Organization),
        ("F.O.B. DESTINATION", Category::Organization),
        // Common abbreviations (#481).
        ("VAT", Category::Organization),
        ("EFT", Category::Organization),
        ("F.O.B.", Category::Organization),
        ("OEM", Category::Organization),
        ("SICAV", Category::Organization),
        ("NOBO", Category::Organization),
        ("LEA", Category::Organization),
        ("the LEA's", Category::Organization),
        ("IEP", Category::Organization),
        ("RNG", Category::Organization),
        ("MMS", Category::Person),
        ("K-12", Category::Organization),
        ("FOBTs", Category::Organization),
        // Sites, software, portals and app platforms (#488).
        ("Timber Trade Online", Category::Organization),
        ("FileFlow Manager", Category::Organization),
        ("Application Manager", Category::Organization),
        ("Case Management System", Category::Organization),
        ("the grants portal", Category::Organization),
        ("Brindle Jobs Portal", Category::Organization),
        ("AppStore", Category::Organization),
        ("the App Store", Category::Organization),
        ("Google Play", Category::Organization),
        ("GooglePlay", Category::Organization),
        ("USAJOBS", Category::Organization),
        ("Grants.gov", Category::Organization),
        ("CM/ECF", Category::Organization),
        ("myHMCTS", Category::Organization),
        ("OASys", Category::Organization),
        ("WHO.INT", Category::Organization),
        // Capitals only for the abbreviations that are names too.
        ("the SEA", Category::Organization),
        ("FIRs", Category::Organization),
        // A head word ends a name with no legal form after it: the
        // issue's choice (#488, review of #543).
        ("America Online", Category::Organization),
    ] {
        assert!(!pack.identifying(text, category), "{text}");
    }
    for (text, category) in [
        // A legal form keeps a company, whatever its words.
        ("Northvale Ledger Services LLC", Category::Organization),
        ("Acme Program Ltd", Category::Organization),
        ("Harrow Services Ltd", Category::Organization),
        ("Timber Trade Online LLC", Category::Organization),
        ("Crew Ltd", Category::Organization),
        (
            "Quillon Freight LLC, a Wyoming limited liability company",
            Category::Organization,
        ),
        ("Copper Dusk Facilities Ltd", Category::Organization),
        // A name of its own keeps one without a legal form.
        ("Northvale Ledger Services", Category::Organization),
        ("Harrow Floor Services", Category::Organization),
        ("Plan International", Category::Organization),
        ("Standard Chartered", Category::Organization),
        ("Acme Systems", Category::Organization),
        ("Acme Corporation", Category::Organization),
        ("Brindle Trust", Category::Organization),
        ("Crew Clothing", Category::Organization),
        ("Acme Fund SICAV", Category::Organization),
        ("VAT Advisers Quill", Category::Organization),
        ("Apple", Category::Organization),
        ("Google", Category::Organization),
        ("Brindle Online Group", Category::Organization),
        ("Bramble Purl Surveys Ltd", Category::Organization),
        // People.
        ("Ossian Crew", Category::Person),
        ("Lea Brindle", Category::Person),
        ("Paul Program", Category::Person),
        ("Collier", Category::Person),
        // Review of #543: a legal form keeps a head word, a subject that
        // is no field of law keeps a name, a name before a legal form in
        // words stays, and so does a word that only starts like an app
        // platform; "Lea" is a name.
        ("ООО «Bright Paths Program»", Category::Organization),
        ("Meals on Wheels America", Category::Organization),
        ("Meals on Wheels", Category::Organization),
        (
            "Northvale Ledger limited liability company",
            Category::Organization,
        ),
        ("App Storehouse", Category::Organization),
        ("Lea", Category::Person),
        ("Lea", Category::Organization),
    ] {
        assert!(pack.identifying(text, category), "{text}");
    }
}

/// Common words keep a person named so beside a title or their full
/// name (#501, #481): "Ms Crew", "Lea Brindle" then "Lea".
#[test]
fn generic_words_stay_a_persons_name_in_context() {
    let source = "Ms Crew met Lea Brindle; Lea signed.";
    // Each anchor's first word, at the anchor's place.
    let entities: Vec<crate::RawEntity> = [
        ("Ms Crew", "Crew", "person"),
        ("Lea Brindle", "Lea Brindle", "person"),
        ("Lea signed", "Lea", "person"),
    ]
    .into_iter()
    .map(|(anchor, text, label)| {
        let start = source.find(anchor).unwrap() + anchor.find(text).unwrap();
        crate::RawEntity {
            start,
            end: start + text.len(),
            label: label.into(),
            score: 0.8,
            text: text.into(),
        }
    })
    .collect();
    let found: Vec<(String, String)> = crate::recognition::proposals(source, &entities)
        .unwrap()
        .into_iter()
        .map(|span| (source[span.range].to_owned(), span.category))
        .collect();
    assert_eq!(
        found,
        pairs(&[
            ("Crew", "PERSON"),
            ("Lea Brindle", "PERSON"),
            ("Lea", "PERSON"),
        ])
    );
}

/// Probe rounds 1 and 2 (#385): only complete roles go, in either
/// model category. Do not use an arbitrary job head as a span veto.
#[test]
fn english_probe_roles_are_no_names() {
    for role in [
        "Debtor",
        "Trustee",
        "Chapter 7 Trustee",
        "Chapter 11 Trustee",
        "First Defendant",
        "Defendants",
        "Second Claimant",
        "Claimant",
        "Party",
        "Bankrupt Party",
        "Plaintiff(s)",
        "Affiliate",
        "Agents",
        "Lead Agent",
        "Dealer Covered Person",
        "Sponsor Staff",
        "Operating Principal",
        "Recipient",
        "Discloser",
        "Franchisee",
        "Subscriber",
        "Judges",
        "Chairmen",
        "Deputies",
        "Secretaries",
        "Masters",
        "Recorders",
        "Circuit Judges",
        "Circuit Judge(s)",
        "UNITED STATES DISTRICT JUDGE",
        "Sergeant",
        "SHERIFF",
        "UNDERSHERIFF",
        "Monsieur",
        "King of Sweden",
        "Esquire",
        "Team Leader",
        "Chief Investigator",
        "Bureau Safety Manager",
        "Chair",
        "Director",
        "Accident Investigation Advisor",
        "Program Manager",
        "Vice President of Infrastructure",
        "CxO",
        "CEO",
        "Supervisors",
        "Fire Marshal",
        "Officer",
        "TREASURER",
        "Participant",
        "Participants",
        "other Participant",
        "originating Participants",
        "host Participant's",
        "Head of Delegation",
        "HOD",
        "Chairman",
        "INTERNATIONAL AGREEMENTS SPECIALIST",
        "Appellees",
        "Appellants",
        "Attorneys for Plaintiffs",
        "Magistrate Judge",
        "Notary Public",
        "User",
        "Division Administrator",
        "Administrative Adviser",
    ] {
        for label in ["person", "organization"] {
            // Listed capitalized surnames are deliberately
            // retained as PERSON; their lowercase role readings still go.
            let person_name =
                ["Masters", "Sergeant", "SHERIFF", "Chair", "Chairman"].contains(&role);
            let expected = if label == "person" && person_name {
                pairs(&[(role, "PERSON")])
            } else {
                Vec::new()
            };
            assert_eq!(shown(role, &[(role, label)]), expected, "{label}: {role}");
            let lower = role.to_lowercase();
            assert!(
                shown(&lower, &[(&lower, label)]).is_empty(),
                "{label}: {lower}"
            );
        }
    }
}

/// Russian roles include oblique cases, plurals and multiword offices
/// (#385), never a wildcard that would also take an officer's name.
#[test]
fn russian_probe_roles_are_no_names() {
    for role in [
        "Собрание кредиторов",
        "Истец",
        "Истца",
        "Ответчику",
        "Заемщика",
        "Заёмщиком",
        "Заимодавец",
        "Заимодавца",
        "Заимодавцу",
        "Заимодавцем",
        "Заимодавце",
        "Займодавец",
        "Лицо",
        "Лица",
        "Лицами",
        "Несовершеннолетний ребенок",
        "Несовершеннолетнего ребёнка",
        "Несовершеннолетнему ребёнку",
        "Несовершеннолетним ребенком",
        "Руководитель",
        "Руководителя",
        "Руководителю",
        "Руководителем",
        "Руководителе",
        "Заместитель руководителя",
        "Заместителя главного бухгалтера",
        "Заместителю главного бухгалтера",
        "Уполномоченный",
        "Уполномоченного",
        "Предприниматель",
        "Предпринимателя",
        "Предпринимателю",
        "Предпринимателем",
        "Предпринимателе",
        "Правообладатель",
        "Правообладателя",
        "Правообладателю",
        "Правообладателем",
        "Правообладателе",
        "Должник",
        "Должника",
        "Должнику",
        "Должником",
        "Должнике",
        "Должник холост",
        "Кассатор",
        "Кассатора",
        "Кассатором",
        "Заявитель",
        "Заявителя",
        "Заявителю",
        "Заявителем",
        "Заявителе",
        "конкурсный управляющий",
        "конкурсного управляющего",
        "конкурсному управляющему",
        "финансовому управляющему",
        "финансовым управляющим",
        "арбитражного управляющего",
        "арбитражном управляющем",
        "Уполномоченный по защите прав предпринимателей",
        "Уполномоченному по защите прав предпринимателей",
        "Индивидуальный предприниматель",
        "ИП",
    ] {
        for label in ["person", "organization"] {
            assert!(shown(role, &[(role, label)]).is_empty(), "{label}: {role}");
        }
    }
}

/// Complete edge phrases are safe to trim; a role-like surname on
/// either edge is retained. ИП is outside an entrepreneur's name
/// (guide rule 1, #484 item 5).
#[test]
fn probe_role_edges_leave_the_name() {
    for (source, expected) in [
        ("Chief Investigator Ms Edda Brannock", "Edda Brannock"),
        ("Team Leader Edda Brannock", "Edda Brannock"),
        ("Edda Brannock, Division Administrator", "Edda Brannock"),
        (
            "Vice President of Infrastructure Edda Brannock",
            "Edda Brannock",
        ),
        ("Monsieur Ivo Penrow", "Ivo Penrow"),
        ("Chapter 7 Trustee Ivo Penrow", "Ivo Penrow"),
        ("First Defendant Edda Brannock", "Edda Brannock"),
        ("Attorneys for Plaintiffs Ivo Penrow", "Ivo Penrow"),
        (
            "Уполномоченный по защите прав предпринимателей Лада Брюн",
            "Лада Брюн",
        ),
        ("ИП Пахомцев И. В.", "Пахомцев И. В."),
        (
            "Индивидуальный предприниматель Пахомцев И. В.",
            "Пахомцев И. В.",
        ),
        (
            "конкурсного управляющего Пахомцева И. В.",
            "Пахомцева И. В.",
        ),
        (
            "финансовому управляющему Пахомцеву И. В.",
            "Пахомцеву И. В.",
        ),
        (
            "Пахомцевым И. В., арбитражным управляющим",
            "Пахомцевым И. В.",
        ),
        ("Заместитель главного бухгалтера Лада Брюн", "Лада Брюн"),
    ] {
        assert_eq!(
            shown(source, &[(source, "person")]),
            pairs(&[(expected, "PERSON")]),
            "{source}"
        );
    }
    for (source, expected) in [
        (
            "Team Leader, Brannock Services Ltd",
            "Brannock Services Ltd",
        ),
        (
            "Brannock Services Ltd, Chief Investigator",
            "Brannock Services Ltd",
        ),
        ("конкурсного управляющего: ООО «Листолёт»", "ООО «Листолёт»"),
    ] {
        assert_eq!(
            shown(source, &[(source, "organization")]),
            pairs(&[(expected, "ORG")]),
            "{source}"
        );
    }
}

#[test]
fn standalone_role_modifiers_keep_person_and_organization_names() {
    for modifier in [
        "VICE",
        "Deputy",
        "Acting",
        "Assistant",
        "Senior",
        "Chief",
        "Lead",
        "Head",
        "General",
        "Principal",
        "Executive",
        "First",
        "Technical",
        "Managing",
        "Junior",
        "Independent",
        "Administrative",
        "Заместитель",
        "Главный",
        "Старший",
        "Ведущий",
        "Генеральный",
        "Независимый",
        "Конкурсный",
        "Финансовый",
        "Арбитражный",
        "Несовершеннолетний",
    ] {
        assert!(!pack().never(modifier), "{modifier}");
        for (label, category) in [("person", "PERSON"), ("organization", "ORG")] {
            assert_eq!(
                shown(modifier, &[(modifier, label)]),
                pairs(&[(modifier, category)]),
                "{label}: {modifier}"
            );
        }
        let initialed = format!("R. {modifier}");
        assert_eq!(
            shown(&initialed, &[(&initialed, "person")]),
            pairs(&[(&initialed, "PERSON")]),
            "{initialed}"
        );
    }
    for role in [
        "Vice President",
        "Deputy Director",
        "Acting Director",
        "Assistant Director",
        "Senior Officer",
        "Chief Investigator",
        "Lead Agent",
        "Head of Delegation",
        "General Counsel",
        "Principal Investigator",
        "Executive Director",
        "Заместитель руководителя",
        "Главный бухгалтер",
        "Генеральный директор",
    ] {
        for label in ["person", "organization"] {
            assert!(shown(role, &[(role, label)]).is_empty(), "{label}: {role}");
        }
    }
}

#[test]
fn explicit_capacity_definitions_are_roles_but_principal_alone_stays() {
    for source in [
        "The terms Principal and Attorney describe capacities in the specimen.",
        "The term Principal describes a capacity.",
    ] {
        assert!(
            shown(source, &[("Principal", "person")]).is_empty(),
            "{source}"
        );
    }
    for source in [
        "Principal",
        "The terms Principal and Attorney identify two people.",
        "The terms Principal and Penrow describe capacities.",
    ] {
        assert_eq!(
            shown(source, &[("Principal", "person")]),
            pairs(&[("Principal", "PERSON")]),
            "{source}"
        );
    }
}

#[test]
fn role_modifiers_without_a_role_head_keep_organization_names() {
    for name in ["General Accident", "Head First", "First General"] {
        assert_eq!(
            shown(name, &[(name, "organization")]),
            pairs(&[(name, "ORG")]),
            "{name}"
        );
    }
}

#[test]
fn unlisted_capitalized_roles_and_plurals_are_not_surnames() {
    for (source, role) in [
        ("Defendants filed a motion.", "Defendants"),
        ("Plaintiffs", "Plaintiffs"),
        ("Tenants", "Tenants"),
        ("Directors", "Directors"),
        ("Specialist", "Specialist"),
        ("Notary", "Notary"),
        ("Esquire", "Esquire"),
        ("Recorder", "Recorder"),
        ("Treasurer", "Treasurer"),
        ("President", "President"),
    ] {
        assert!(shown(source, &[(role, "person")]).is_empty(), "{source}");
    }
    for name in ["Masters", "Stewards", "Judge", "Clerk", "Chair"] {
        assert_eq!(
            shown(name, &[(name, "person")]),
            pairs(&[(name, "PERSON")]),
            "{name}"
        );
    }
}

#[test]
fn cross_provider_review_verified_safe_names_stay() {
    for name in [
        "Vice Media",
        "VICE",
        "Officer & Co",
        "Trustee Ltd",
        "Leader Group",
        "General Electric",
        "Master Builders Ltd",
        "ООО «Директор»",
        "ООО «Кредитор»",
        "ООО «Руководитель»",
        "Trustees of Princeton University",
        "Chief Executive Officer of Acme Ltd",
    ] {
        assert_eq!(
            shown(name, &[(name, "organization")]),
            pairs(&[(name, "ORG")]),
            "{name}"
        );
    }
    for (source, name) in [
        ("Судья", "Судья"),
        ("Начальник", "Начальник"),
        ("Кузнецов", "Кузнецов"),
        ("Должников", "Должников"),
        // The title comes off a full name (guide rule 1, #387), and off a
        // lone surname too (#382).
        ("Judge Edda Brannock", "Edda Brannock"),
        ("Sheriff Brannock", "Brannock"),
        ("John Smith, Chief", "John Smith, Chief"),
        ("ИП ФИО3 К-Л.М.", "К-Л.М."),
    ] {
        assert_eq!(
            shown(source, &[(source, "person")]),
            pairs(&[(name, "PERSON")]),
            "{source}"
        );
    }
}

/// Russian form numbers, classifier codes, standards, a form's barcode
/// line and waybill numbers are no identifiers; bank details, tax,
/// registration and passport numbers and phones beside them stay
/// (guide rules 6 and 8, issue #486).
#[test]
fn russian_form_numbers_and_codes_are_no_identifiers() {
    let source = "   7000  2034\nИНН\n\n2990  2026  КПП  Стр. 0 0 2\n\n\
        Форма по КНД 1111501   Форма № Р11001\n\n\
        Платёжное поручение, форма по ОКУД 0401060, ОКВЭД 47.11, 47.19, \
        ОКТМО 45382000, КБК 18210102010011000110, код ТН ВЭД 8471300000.\n\n\
        Помещение соответствует СП 2.13130.2020, ГОСТ Р 7.0.97-2016 и СНиП 2.01.07-85*.\n\n\
        Груз принят по ТН № 47 от 03.02.2026, ТТН № 112 и товарной накладной № 58, \
        счёт-фактура № 123/4.\n\n\
        Получатель: ООО «Вега», ИНН 7707083893, КПП 770701001, ОКПО 12345678, \
        р/с 40702810900000001234 в банке, БИК 044525225, к/с 30101810400000000225, \
        расчётный счёт № 40702810500000009876, паспорт 4510 123456, \
        тел. 8-800-555-35-35, +7 (495) 123-45-67.\n";
    let raw = [
        ("7000  2034", "tax_id"),
        ("2990  2026", "bank_account"),
        ("1111501", "tax_id"),
        ("Р11001", "tax_id"),
        ("0401060", "bank_account"),
        ("47.11", "tax_id"),
        ("45382000", "tax_id"),
        ("18210102010011000110", "bank_account"),
        ("8471300000", "tax_id"),
        ("2.13130.2020", "bank_account"),
        ("7.0.97-2016", "tax_id"),
        ("2.01.07-85", "tax_id"),
        ("ТН № 47", "bank_account"),
        ("112", "bank_account"),
        ("58", "bank_account"),
        ("123/4", "tax_id"),
        ("7707083893", "tax_id"),
        ("770701001", "tax_id"),
        ("12345678", "tax_id"),
        ("р/с 40702810900000001234", "bank_account"),
        ("044525225", "bank_account"),
        ("к/с 30101810400000000225", "bank_account"),
        ("расчётный счёт № 40702810500000009876", "bank_account"),
        ("4510 123456", "passport_number"),
        ("8-800-555-35-35", "phone_number"),
        ("+7 (495) 123-45-67", "phone_number"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("7707083893", "TAX"),
            ("770701001", "TAX"),
            ("12345678", "TAX"),
            ("40702810900000001234", "BANK"),
            ("044525225", "BANK"),
            ("30101810400000000225", "BANK"),
            ("40702810500000009876", "BANK"),
            ("4510 123456", "IDENTITY"),
            ("8-800-555-35-35", "PHONE"),
            ("+7 (495) 123-45-67", "PHONE"),
        ])
    );
    // A joint venture's name after «СП» stays; so does a number after
    // a barcode-like group that is no barcode line.
    assert_eq!(
        shown(
            "Арендатор: СП «Восход», тел. 4951 2345 678.",
            &[
                ("СП «Восход»", "organization"),
                ("4951 2345 678", "phone_number")
            ]
        ),
        pairs(&[("СП «Восход»", "ORG"), ("4951 2345 678", "PHONE")])
    );
    // An account typed with one space on its own line is no barcode,
    // in either language.
    for source in [
        "Account number:\n1234 5678\n",
        "Номер счёта:\n1234 5678\nИНН\n",
    ] {
        assert_eq!(
            shown(source, &[("1234 5678", "bank_account")]),
            pairs(&[("1234 5678", "BANK")]),
            "{source:?}"
        );
    }
}

/// English patent and trademark schedules, labelled patent and form
/// numbers, industry codes, a link's query token, a page footer and a
/// toll-free prefix are no identifiers; an IBAN, a VAT number, a
/// company number, a toll-free number and a phone stay (guide rules 6
/// and 8, issue #486).
#[test]
fn english_patent_form_and_page_numbers_are_no_identifiers() {
    let source = "COUNTRY  APP NO.  FILING DATE  PATENT NUMBER  ISSUE DATE\n\
        AU  2099241803  30-Apr-29  2099241803  26-Sep-33\n\
        DE  DE 60 2090 031 448.0  29-Nov-31  2 424 911  23-Mar-36\n\
        EP  9739191.6  30-Apr-29  2286018  11-Jun-34\n\
        US  12/432,845  30-Apr-29  8,420,710  16-Apr-33\n\
        US  SIGNATURE SCRAPE  Registered  86920111  25-Feb-36  5187924\n\
        CA  SIGNATURE SCRAPE  Published  1769731  26-Feb-36\n\n\
        Patent US 99/123,456 (AU 2099000111), trademark Reg. No. 9876543, \
        form STD009X.UK, NAICS 999999; see https://example.invalid/p?srsltid=AbCd_123xYz \
        or call the toll-free \"800\" number.\n\nFinal PLZ\n\n   -12-\n\nDraft QRT\n   -13-\n\n\
        NAICS:\n335931\n\n\
        IBAN DE89 3704 0044 0532 0130 00, VAT No. GB 123 4567 89, \
        Company Registration No. 01234567, toll-free 1-800-555-0199, \
        phone +1 (202) 555-0164.\n";
    let raw = [
        ("AU", "organization"),
        ("2099241803", "tax_id"),
        ("DE 60 2090 031 448.0", "bank_account"),
        ("2 424 911", "tax_id"),
        ("9739191.6", "tax_id"),
        ("12/432,845", "tax_id"),
        ("8,420,710", "tax_id"),
        ("86920111", "national_id_number"),
        ("1769731", "national_id_number"),
        ("99/123,456", "tax_id"),
        ("2099000111", "tax_id"),
        ("9876543", "national_id_number"),
        ("STD009X.UK", "national_id_number"),
        ("999999", "tax_id"),
        ("AbCd_123xYz", "tax_id"),
        ("800", "phone_number"),
        ("PLZ\n\n   -12-", "phone_number"),
        ("QRT\n   -13-", "phone_number"),
        ("-12-", "phone_number"),
        ("335931", "tax_id"),
        ("IBAN DE89 3704 0044 0532 0130 00", "bank_account"),
        ("GB 123 4567 89", "tax_id"),
        ("01234567", "national_id_number"),
        ("1-800-555-0199", "phone_number"),
        ("+1 (202) 555-0164", "phone_number"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("https://example.invalid/p?srsltid=AbCd_123xYz", "URL"),
            ("DE89 3704 0044 0532 0130 00", "BANK"),
            ("GB 123 4567 89", "TAX"),
            ("01234567", "IDENTITY"),
            ("1-800-555-0199", "PHONE"),
            ("+1 (202) 555-0164", "PHONE"),
        ])
    );
    // Out of a schedule's row, a company's table (a status column too)
    // and a sentence opening "US" keep their values, and so do a query's
    // values of digits alone, of one case or without digits (inside a
    // whole link they are its URL's, rule 7). A row ends on its own
    // line: the line after it keeps its name and passport.
    let source = "| UK | Acme Holdings Ltd | 01234567 | 01/02/2030 |\n\
        | UK | Bryn Holdings Ltd | Registered | 07654321 | 01/03/2030 |\n\n\
        US law applies to Vela Quist from 30-Apr-29, SSN 123-45-6789.\n\n\
        Query: inn=7799000011&doc=AB1234567&who=JaneMorgan.\n\n\
        AU  2099241803  30-Apr-29   \n\
        Contact: Jane Morgan, passport RM6841932, acct 2099000111223.\n";
    let raw = [
        ("Acme Holdings Ltd", "organization"),
        ("01234567", "national_id_number"),
        ("Bryn Holdings Ltd", "organization"),
        ("07654321", "national_id_number"),
        ("Vela Quist", "person"),
        ("123-45-6789", "tax_id"),
        ("7799000011", "tax_id"),
        ("AB1234567", "passport_number"),
        ("JaneMorgan", "person"),
        ("2099241803", "tax_id"),
        ("Jane Morgan", "person"),
        ("RM6841932", "passport_number"),
        ("2099000111223", "bank_account"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[
            ("Acme Holdings Ltd", "ORG"),
            ("01234567", "IDENTITY"),
            ("Bryn Holdings Ltd", "ORG"),
            ("07654321", "IDENTITY"),
            ("Vela Quist", "PERSON"),
            // An SSN is IDENTITY (#381).
            ("123-45-6789", "IDENTITY"),
            ("7799000011", "TAX"),
            ("AB1234567", "IDENTITY"),
            ("JaneMorgan", "PERSON"),
            ("Jane Morgan", "PERSON"),
            ("RM6841932", "IDENTITY"),
            ("2099000111223", "BANK"),
        ])
    );
}

/// The number shapes that are no tax, bank, identity or phone value
/// alone, and the real values beside them (issue #486).
#[test]
fn grouped_numbers_guids_and_tokens_are_no_identifiers() {
    let keeps = |text: &str, category| pack().identifying(text, category);
    for (text, category) in [
        ("8,420,710", Category::Tax),
        ("12/432,845", Category::Bank),
        ("1,250,000.00", Category::Identity),
        ("2,000", Category::Phone),
        ("b539f4a9-4e5f-4abb-bec6-aae430c254f2", Category::Tax),
        ("B539F4A9-4E5F-4ABB-BEC6-AAE430C254F2", Category::Bank),
        (
            "AfmBOorJnJ4Ys9vL_zXDXNKRcwv2xdd4zUKdwjsQuHUWB4mbpigOS4O3",
            Category::Tax,
        ),
        ("AbcD_123xyzQ", Category::Phone),
        ("STD001Y2K.UK", Category::Identity),
        ("PLZ\n\n   -44-", Category::Phone),
        ("-44-", Category::Phone),
        ("– 12 –", Category::Phone),
        ("4510\r\n\r\n123456", Category::Identity),
    ] {
        assert!(!keeps(text, category), "{text:?}");
    }
    for (text, category) in [
        ("12345.67", Category::Bank),
        ("4510 123456", Category::Identity),
        ("GB29 NWBK 6016 1331 9268 19", Category::Bank),
        ("77l12345O7", Category::Tax),
        ("4510\n123456", Category::Identity),
        ("555-12-34", Category::Phone),
        ("8-800-555-35-35", Category::Phone),
        ("RM6841932", Category::Identity),
        ("GB123456789", Category::Tax),
    ] {
        assert!(keeps(text, category), "{text:?}");
    }
}

/// #375 review n2: each review input keeps what origin/main proposed,
/// its value and every repeat of it.
#[test]
fn short_numbers_and_records_keep_real_values() {
    // Source, the model's spans (text, label), what is proposed.
    type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a [(&'a str, &'a str)]);
    let cases: &[Case] = &[
        // P1-1: a sort code in a table, after a newline or a hyphenated label, in prose.
        (
            "| Sort code | 20-00-00 |\n| Account number | 12345678 |",
            &[("20-00-00", "bank_account"), ("12345678", "bank_account")],
            &[("20-00-00", "BANK"), ("12345678", "BANK")],
        ),
        (
            "| Sort code | 20-00-00 |",
            &[("20-00-00", "routing_number")],
            &[("20-00-00", "BANK")],
        ),
        (
            "Sort code: 20-00-00",
            &[("20-00-00", "bank_account")],
            &[("20-00-00", "BANK")],
        ),
        (
            "Sort code:\n20-00-00",
            &[("20-00-00", "bank_account")],
            &[("20-00-00", "BANK")],
        ),
        (
            "Sort-code 20-00-00, account 12345678.",
            &[("20-00-00", "bank_account")],
            &[("20-00-00", "BANK")],
        ),
        (
            "Pay into Mr Penrow's account at Barclays, 20-00-00, 12345678.",
            &[("20-00-00", "bank_account")],
            &[("20-00-00", "BANK")],
        ),
        // P1-2: dotted phones, unlabelled, in a table, after «тел./факс».
        (
            "Связаться с Ивановым: (495) 123.45.67",
            &[("(495) 123.45.67", "phone_number")],
            &[("(495) 123.45.67", "PHONE")],
        ),
        (
            "Иванов И. И., +7 495 123.45.67",
            &[("+7 495 123.45.67", "phone_number")],
            &[("+7 495 123.45.67", "PHONE")],
        ),
        (
            "Иванов И. И., 8.912.345.67.89",
            &[("8.912.345.67.89", "phone_number")],
            &[("8.912.345.67.89", "PHONE")],
        ),
        (
            "| Иванов И. И. | (495) 123.45.67 |",
            &[("(495) 123.45.67", "phone_number")],
            &[("(495) 123.45.67", "PHONE")],
        ),
        (
            "Contact Mme Durand on 01.23.45.67.89.",
            &[("01.23.45.67.89", "phone_number")],
            &[("01.23.45.67.89", "PHONE")],
        ),
        (
            "Mr Penrow, 07700.900.123",
            &[("07700.900.123", "phone_number")],
            &[("07700.900.123", "PHONE")],
        ),
        (
            "Иванов И. И., тел./факс (495) 123.45.67",
            &[("(495) 123.45.67", "phone_number")],
            &[("(495) 123.45.67", "PHONE")],
        ),
        // P2-2: short values reach their repeats, labelled or not.
        (
            "Phone: 6121. Phone: 6121.",
            &[("6121", "phone_number")],
            &[("6121", "PHONE"), ("6121", "PHONE")],
        ),
        (
            "тел. 6121; тел.: 6121",
            &[("6121", "phone_number")],
            &[("6121", "PHONE"), ("6121", "PHONE")],
        ),
        (
            "тел. 6121. Звоните по телефону 6121.",
            &[("6121", "phone_number")],
            &[("6121", "PHONE"), ("6121", "PHONE")],
        ),
        (
            "PIN: 4821. The PIN 4821 was shared.",
            &[("4821", "passport_number")],
            &[("4821", "IDENTITY"), ("4821", "IDENTITY")],
        ),
        (
            "| Удостоверение | 9917 |\n| Подпись адвоката (удостоверение 9917) |",
            &[("9917", "national_id_number")],
            &[("9917", "IDENTITY"), ("9917", "IDENTITY")],
        ),
        (
            "Account No. 1234; account 1234 again",
            &[("1234", "bank_account")],
            &[("1234", "BANK"), ("1234", "BANK")],
        ),
        (
            "ИНН 1234 (опечатка); ИНН 1234",
            &[("1234", "tax_id")],
            &[("1234", "TAX"), ("1234", "TAX")],
        ),
        // P2-3: short phones after ordinary labels; an extension and its repeat.
        (
            "Phone No. 6121",
            &[("6121", "phone_number")],
            &[("6121", "PHONE")],
        ),
        (
            "Tel. No.: 6121",
            &[("6121", "phone_number")],
            &[("6121", "PHONE")],
        ),
        (
            "Telephone extension 4521",
            &[("4521", "phone_number")],
            &[("4521", "PHONE")],
        ),
        (
            "Extension: 4521",
            &[("4521", "phone_number")],
            &[("4521", "PHONE")],
        ),
        (
            "Mr Penrow, ext. 4521",
            &[("4521", "phone_number")],
            &[("4521", "PHONE")],
        ),
        (
            "Звонить по телефону 6121.",
            &[("6121", "phone_number")],
            &[("6121", "PHONE")],
        ),
        (
            "внутренний 6121",
            &[("6121", "phone_number")],
            &[("6121", "PHONE")],
        ),
        (
            "| Телефон | 6121 |",
            &[("6121", "phone_number")],
            &[("6121", "PHONE")],
        ),
        (
            "+7 (495) 123-45-67, доб. 6121",
            &[
                ("+7 (495) 123-45-67", "phone_number"),
                ("6121", "phone_number"),
            ],
            &[("+7 (495) 123-45-67", "PHONE"), ("6121", "PHONE")],
        ),
        (
            "Phone: 020 7946 0958 x207; ask for x207 again",
            &[("x207", "phone_number")],
            &[("x207", "PHONE"), ("x207", "PHONE")],
        ),
        // P2-4: SWIFT/BIC codes.
        (
            "SWIFT/BIC: NWBKGB2L",
            &[("NWBKGB2L", "bank_account")],
            &[("NWBKGB2L", "BANK")],
        ),
        (
            "SWIFT code DEUTDEFF500",
            &[("DEUTDEFF500", "bank_account")],
            &[("DEUTDEFF500", "BANK")],
        ),
        // P2-5: masked and partial card and account numbers.
        (
            "The fee was charged to Ms Penrow's card ending 4242.",
            &[("4242", "bank_account")],
            &[("4242", "BANK")],
        ),
        (
            "Visa **** **** **** 4242",
            &[("**** **** **** 4242", "bank_account")],
            &[("**** **** **** 4242", "BANK")],
        ),
        (
            "Account ending ****1234",
            &[("****1234", "bank_account")],
            &[("****1234", "BANK")],
        ),
        (
            "Acct: ****1234",
            &[("****1234", "bank_account")],
            &[("****1234", "BANK")],
        ),
        (
            "Номер карты: **** 1234",
            &[("**** 1234", "bank_account")],
            &[("**** 1234", "BANK")],
        ),
        // P2-6: seven-digit accounts.
        (
            "| Лицевой счёт | 1234567 |",
            &[("1234567", "bank_account")],
            &[("1234567", "BANK")],
        ),
        (
            "| Account | 1234567 |",
            &[("1234567", "bank_account")],
            &[("1234567", "BANK")],
        ),
        (
            "л/с 1234567",
            &[("1234567", "bank_account")],
            &[("1234567", "BANK")],
        ),
        // P2-7: phones beside or inside a Windows path.
        (
            r"Saved as C:\Matters\Penrow\letter.doc - then call Penrow on 020 7946 0958 before sending reply.pdf",
            &[("020 7946 0958", "phone_number")],
            &[("020 7946 0958", "PHONE")],
        ),
        (
            r"Файл D:\Дела\Иванов\иск.docx отправлен Иванову, 8 912 345-67-89, вместе с копией скан.pdf",
            &[("8 912 345-67-89", "phone_number")],
            &[("8 912 345-67-89", "PHONE")],
        ),
        (
            r"W:\Penrow\contacts 020 7946 0958.txt",
            &[("020 7946 0958", "phone_number")],
            &[("020 7946 0958", "PHONE")],
        ),
        // P2-8: a lone "R" or "×" in an address or a passport.
        (
            "Ms Penrow lives at 1500 R Street NW.",
            &[("1500 R Street NW", "address")],
            &[("1500 R Street NW", "ADDRESS")],
        ),
        (
            "Flat 12, Block R 3, Larch Estate, Bristol BS6 9ZY",
            &[(
                "Flat 12, Block R 3, Larch Estate, Bristol BS6 9ZY",
                "address",
            )],
            &[(
                "Flat 12, Block R 3, Larch Estate, Bristol BS6 9ZY",
                "ADDRESS",
            )],
        ),
        (
            "Apartment 4, Building R 7, Larch Park, London E1 6AN",
            &[(
                "Apartment 4, Building R 7, Larch Park, London E1 6AN",
                "address",
            )],
            &[(
                "Apartment 4, Building R 7, Larch Park, London E1 6AN",
                "ADDRESS",
            )],
        ),
        (
            "Ms Penrow lives at 1500 R Street NW, Washington, DC 20009.",
            &[("1500 R Street NW, Washington, DC 20009", "address")],
            &[("1500 R Street NW, Washington, DC 20009", "ADDRESS")],
        ),
        (
            "Flat 2, 14 R Road, Bristol BS6 9ZY",
            &[("Flat 2, 14 R Road, Bristol BS6 9ZY", "address")],
            &[("Flat 2, 14 R Road, Bristol BS6 9ZY", "ADDRESS")],
        ),
        (
            "Passport 12 R 345678",
            &[("12 R 345678", "passport_number")],
            &[("12 R 345678", "IDENTITY")],
        ),
        (
            "Lot 7 × 3, Larch Road, Bristol BS6 9ZY",
            &[("Lot 7 × 3, Larch Road, Bristol BS6 9ZY", "address")],
            &[("Lot 7 × 3, Larch Road, Bristol BS6 9ZY", "ADDRESS")],
        ),
        // P2-9: a notary's licence and an administrative-offence protocol.
        (
            "Нотариус Брюн Л. И., лицензия № 0099911",
            &[("0099911", "national_id_number")],
            &[("0099911", "IDENTITY")],
        ),
        // An administrative-offence protocol number is CASE (#484 item 7,
        // #364's label rule).
        (
            "протокол об административном правонарушении № 0199999",
            &[("0199999", "national_id_number")],
            &[("0199999", "CASE")],
        ),
        // P3-1: a list after a record number stops before a phone or an ИНН;
        // a licence is no record.
        (
            "по договорам №№ 15, 16, 89123456789",
            &[("89123456789", "phone_number")],
            &[("89123456789", "PHONE")],
        ),
        (
            "договор № 15,7701234567",
            &[("7701234567", "tax_id")],
            &[("7701234567", "TAX")],
        ),
        (
            "лицензия ИП-7701234567",
            &[("7701234567", "tax_id")],
            &[("7701234567", "TAX")],
        ),
        // P3-2: a local phone after «т.» before a city.
        (
            "Иванов И.И., т. 234567, г. Энск",
            &[("234567", "phone_number")],
            &[("234567", "PHONE")],
        ),
        // P3-3: an IBAN split across lines or table cells.
        (
            "IBAN: GB29 NWBK\n6016 1331 9268 19",
            &[("GB29 NWBK", "iban"), ("6016 1331 9268 19", "iban")],
            &[("GB29 NWBK", "BANK"), ("6016 1331 9268 19", "BANK")],
        ),
        (
            "| GB29 NWBK | 6016 1331 9268 19 |",
            &[("GB29 NWBK", "iban"), ("6016 1331 9268 19", "iban")],
            &[("GB29 NWBK", "BANK"), ("6016 1331 9268 19", "BANK")],
        ),
    ];
    for (source, raw, expected) in cases {
        assert_eq!(shown(source, raw), pairs(expected), "{source}");
    }
}

/// #375: the issue's false hits still go where a narrow rule can tell
/// them from a real value: short line, header and route numbers, plot
/// ticks, drawing dimensions, a file number in a document path, Russian
/// record numbers and a postcode before a redacted address.
#[test]
fn short_numbers_and_records_are_no_bank_phone_or_tax() {
    // The issue's synthetic reproduction: only the labelled phone stays.
    let source = "[021] Пояснительная записка, тел. 6121. The Brindle Alternative \
                  Segment 3 route is set out in Paragraphs 1 through 24.";
    assert_eq!(
        shown(
            source,
            &[
                ("021", "phone_number"),
                ("6121", "phone_number"),
                ("Brindle Alternative Segment 3", "bank_account"),
                ("24", "bank_account"),
            ]
        ),
        pairs(&[("6121", "PHONE")])
    );
    for value in [
        "24",
        "300",
        "Eielson Alternative Segment 3",
        "Segments 1 & 2",
        "abcdefghijk12345678",
    ] {
        assert_eq!(shown(value, &[(value, "bank_account")]), [], "{value}");
    }
    for value in ["021", "[021]", "1.981.96", "1.98.96"] {
        assert_eq!(shown(value, &[(value, "phone_number")]), [], "{value}");
    }
    // A label lifts the counts and the plot-tick veto.
    for (source, value, label, category) in [
        ("Phone: 1.981.96", "1.981.96", "phone_number", "PHONE"),
        ("Tel. 021", "021", "phone_number", "PHONE"),
        ("ext. 207", "207", "phone_number", "PHONE"),
        ("доб. 207", "207", "phone_number", "PHONE"),
        ("Account No. 24", "24", "bank_account", "BANK"),
    ] {
        assert_eq!(
            shown(source, &[(value, label)]),
            pairs(&[(value, category)]),
            "{source}"
        );
    }
    // Drawing dimensions, but no number beside a lone "R" or "×".
    for value in ["76×6.4×914", "76 × 6.4 × 914", "6.4 R to inside"] {
        for label in ["bank_account", "phone_number", "tax_id", "address"] {
            assert_eq!(shown(value, &[(value, label)]), [], "{value}: {label}");
        }
    }
    assert_eq!(
        shown(
            "Detail 76×6.4×914; contact 020 7946 0958",
            &[("020 7946 0958", "phone_number")]
        ),
        pairs(&[("020 7946 0958", "PHONE")])
    );
    // A file number in a document path; the path's other text stays.
    for (source, value) in [
        (r"W:\Penrow\09-F-0999 final.doc", "09-F-0999"),
        (r"W:\Penrow\final 09-F-0999.doc", "09-F-0999"),
        (r"W:\Penrow\Drafts\09-F-0999.pdf", "0999"),
    ] {
        assert_eq!(shown(source, &[(value, "phone_number")]), [], "{source}");
    }
    assert_eq!(
        shown(
            "Phone: 555-01-99; file W:\\Penrow\\note.doc",
            &[("555-01-99", "phone_number")]
        ),
        pairs(&[("555-01-99", "PHONE")])
    );
    // An administrative-offence protocol number is no phone but CASE
    // (#484 item 7).
    assert_eq!(
        shown(
            "протокол об административном правонарушении № 0199999",
            &[("0199999", "phone_number")]
        ),
        pairs(&[("0199999", "CASE")])
    );
    // Russian record numbers, list members of the first one's form too.
    for (source, value) in [
        ("Договор № 12345678", "12345678"),
        (
            "договорами об ипотеке № ДИ-999999/2031/00009",
            "ДИ-999999/2031/00009",
        ),
        (
            "договорами об ипотеке № ДИ-999999/2031/00009, ДИ-999998/2031/00010",
            "ДИ-999998/2031/00010",
        ),
        ("договоров от 01.02.2031 № 12345678", "12345678"),
        ("по проектам №№ 1199999, 119999Е", "119999Е"),
        ("по проектам №№ 1152744, 118577Е, 118578Е", "118578Е"),
        ("сообщение № 9999999", "9999999"),
        ("объявление № 13838487", "13838487"),
        ("протокол № 0199999", "0199999"),
        ("по ордеру № 009999", "009999"),
        (
            "свидетельства о депонировании произведения № 023-014748",
            "023-014748",
        ),
        (
            "учетный номер в ЕРКНМ 20999999999999999999",
            "20999999999999999999",
        ),
    ] {
        for label in ["phone_number", "bank_account", "tax_id"] {
            assert_eq!(shown(source, &[(value, label)]), [], "{source}: {label}");
        }
        // A labelled value after the record stays.
        for (tail, pii, label, category) in [
            (
                "; телефон +7 (999) 555-01-23",
                "+7 (999) 555-01-23",
                "phone_number",
                "PHONE",
            ),
            ("; ИНН 7709999999", "7709999999", "tax_id", "TAX"),
            (
                "; р/с 40702810900000004711",
                "40702810900000004711",
                "bank_account",
                "BANK",
            ),
        ] {
            let source = format!("{source}{tail}");
            assert!(
                shown(&source, &[(pii, label)]).contains(&(pii.into(), category.into())),
                "{source}"
            );
        }
    }
    // A record word cannot reach over a label to a later number sign.
    for (source, value, label, category) in [
        ("договор телефон 6121 № 9", "6121", "phone_number", "PHONE"),
        (
            "свидетельство телефон 6121 № 9",
            "6121",
            "phone_number",
            "PHONE",
        ),
        (
            "по ордеру № 12 удостоверение № 9917",
            "9917",
            "national_id_number",
            "IDENTITY",
        ),
    ] {
        assert!(
            shown(source, &[(value, label)]).contains(&(value.into(), category.into())),
            "{source}"
        );
    }
    // A postcode before a redacted address or «г.»; not after «т.».
    for source in [
        "адрес регистрации: 999999, <...>",
        "999999, г. Энск",
        "Иванов, 234567, г. Энск",
    ] {
        let value = &source[source.find(|c: char| c.is_ascii_digit()).unwrap()..][..6];
        let spans = shown(source, &[(value, "phone_number")]);
        assert!(
            spans.iter().all(|(_, category)| category != "PHONE"),
            "{source}: {spans:?}"
        );
    }
    assert!(
        shown("тел. 999999, г. Энск", &[("999999", "phone_number")])
            .contains(&("999999".into(), "PHONE".into()))
    );
}

/// Money, including OCR-joined balance-sheet cells and fragments cut
/// at their decimal comma, is never TAX, BANK or PHONE (#547).
#[test]
fn money_and_its_fragments_are_no_identifiers() {
    for (source, fragment) in [
        ("Остаток: 4 588 762,0323 058 712,74 руб.", "0323 058 712"),
        ("Остаток: 4 588 762,0323 058 712,74 руб.", "762,0323 058"),
        ("Остаток: 4 588 762,0323 058 712,74 руб.", "4 588 762"),
        ("Итого 12 345,67942838 306,12.", "942838 306"),
        ("Итого 12 345,670321 448 312,74.", "0321 448 312"),
        ("Бюджет: 164 353 783 тыс. руб.", "164 353 783"),
        ("Бюджет: 12 345,05 8 443 789 778,00.", "05 8 443 789 778"),
        ("Выплачено 135 130 руб. 86 коп.", "86"),
        ("Выплачено 135 130 руб. 86 коп.", "135 130"),
        ("Итого 714157,31.", "714157"),
        ("Итого 23 707 750,02.", "23 707 750"),
        ("Итого 2903,02.", "2903"),
        ("Итого 714157.31.", "714157"),
        ("Итого 23\u{a0}707\u{202f}750,02.", "23\u{a0}707\u{202f}750"),
        ("Итого 23&nbsp;707&#32;750,02.", "23&nbsp;707&#32;750"),
        ("Итого 164 353 783 млн рублей.", "164 353 783"),
        ("Итого 164 353 783 €.", "164 353 783"),
        ("Сумма, руб.: 164 353 783.", "164 353 783"),
        ("The fund lost $350 billion.", "$350 billion"),
        ("The fund lost $350 billion.", "350"),
        ("The fund lost $11.2 billion a year.", "$11.2 billion"),
        ("The fund lost $11.2 billion a year.", "11.2"),
        ("The fund lost £350 million.", "350 million"),
        ("The fund lost €350 million.", "350"),
        ("The fund lost 350 billion dollars.", "350 billion"),
        ("Tax ID: 999991111 received 350 billion.", "350 billion"),
        ("Пени: ((0,06x159548,05) х2).", "06x159548"),
        ("Пени: (0,05х199999,05) ×2.", "05х199999"),
        ("Пени: (0,05*199999,05) *2.", "05*199999"),
        (
            "Взыскать 1 199999 (один миллион сто девяносто девять тысяч девятьсот девяносто девять) рублей.",
            "1 199999",
        ),
        (
            "Взыскать 1 129731 (один миллион сто двадцать девять тысяч семьсот тридцать один) рубль.",
            "1 129731",
        ),
    ] {
        for label in ["tax_id", "bank_account", "phone_number"] {
            assert_eq!(
                shown(source, &[(fragment, label)]),
                [],
                "{source}: {label} {fragment}"
            );
        }
    }
    // A labelled identifier earlier in the sentence must not exempt
    // a later amount; the context rule still proposes the account.
    assert_eq!(
        shown(
            "Bank account: 12345678 funded 350 million.",
            &[("350 million", "bank_account")]
        ),
        [("12345678".into(), "BANK".into())]
    );
}

/// A column gap ends the amount's token group, even without a label (#547).
#[test]
fn amount_guards_keep_neighbouring_columns() {
    assert_eq!(
        shown(
            "Account     Balance\n12345678    1000.00",
            &[("12345678", "bank_account"), ("1000.00", "bank_account")]
        ),
        pairs(&[("12345678", "BANK")])
    );
    for gap in ["    ", "  ", "\t", "\u{a0}\u{a0}"] {
        for (value, label, category) in [
            ("12345678", "bank_account", "BANK"),
            ("7709999999", "tax_id", "TAX"),
            ("2025550199", "phone_number", "PHONE"),
        ] {
            for amount in [
                "1000.00",
                "714157,31",
                "350 dollars.",
                "$350",
                "350 (триста пятьдесят) рублей.",
            ] {
                let source = format!("{value}{gap}{amount}");
                assert_eq!(
                    shown(&source, &[(value, label)]),
                    pairs(&[(value, category)]),
                    "{source:?}"
                );
            }
            // A currency in the previous column is not this value's cue.
            let source = format!("350 dollars{gap}{value}");
            assert_eq!(
                shown(&source, &[(value, label)]),
                pairs(&[(value, category)]),
                "{source:?}"
            );
        }
    }
}

/// An identifier label exempts its own value, not the next column (#547).
#[test]
fn amount_label_exemptions_end_at_column_gaps() {
    for gap in ["    ", "  ", "\t", "\u{a0}\u{a0}"] {
        for (prefix, value, amount, fragment, label, category) in [
            (
                "Bank account: ",
                "12345678",
                "350 dollars.",
                "350",
                "bank_account",
                "BANK",
            ),
            (
                "ИНН ",
                "7709999999",
                "714157,31 руб.",
                "714157",
                "tax_id",
                "TAX",
            ),
            (
                "Tax ID: ",
                "999991111",
                "350 dollars.",
                "350",
                "tax_id",
                "TAX",
            ),
            (
                "Счёт: ",
                "12345678",
                "350 руб.",
                "350",
                "bank_account",
                "BANK",
            ),
            (
                "Phone: ",
                "2025550199",
                "350 dollars.",
                "350",
                "phone_number",
                "PHONE",
            ),
            (
                "Телефон: ",
                "79990000123",
                "350 руб.",
                "350",
                "phone_number",
                "PHONE",
            ),
        ] {
            let source = format!("{prefix}{value}{gap}{amount}");
            assert_eq!(
                shown(&source, &[(value, label), (fragment, label)]),
                pairs(&[(value, category)]),
                "{source:?}"
            );
        }
        // Twenty digits in separate columns do not make one bank account.
        let source = format!("12345678{gap}123456789012 dollars.");
        assert_eq!(
            shown(
                &source,
                &[
                    ("12345678", "bank_account"),
                    ("123456789012", "bank_account")
                ]
            ),
            pairs(&[("12345678", "BANK")]),
            "{source:?}"
        );
    }
}

/// No amount cue means no reason to discard an identifier. Labels and
/// twenty-digit accounts win even with monetary context beside them.
#[test]
fn amount_guards_keep_identifiers_and_names() {
    for (source, value, label, category) in [
        // Single-space OCR columns must not join identifiers to money.
        (
            "Иванов И.И. 770912345678 15 000 руб.",
            "770912345678",
            "tax_id",
            "TAX",
        ),
        (
            "ООО «Ромашка» 7709999999 1 000,00",
            "7709999999",
            "tax_id",
            "TAX",
        ),
        ("Сумма 1 000,00 7709999999", "7709999999", "tax_id", "TAX"),
        (
            "Петров П.П. 8 916 123 45 67 1 500 руб.",
            "8 916 123 45 67",
            "phone_number",
            "PHONE",
        ),
        (
            "Карта 4276 3801 2345 6789 500 руб.",
            "4276 3801 2345 6789",
            "bank_account",
            "BANK",
        ),
        (
            "Сумма 1 000,00 4081781009991000431",
            "4081781009991000431",
            "bank_account",
            "BANK",
        ),
        // The passport's label decides its category over the model's
        // guess (#381).
        (
            "Паспорт 4506 123456 01.02.2020",
            "4506 123456",
            "tax_id",
            "IDENTITY",
        ),
        ("770912345678 01.02.2020", "770912345678", "tax_id", "TAX"),
        (
            "Взыскать 500 руб. 7709999999",
            "7709999999",
            "tax_id",
            "TAX",
        ),
        (
            "Оплачено 500 руб. 8 916 123 45 67",
            "8 916 123 45 67",
            "phone_number",
            "PHONE",
        ),
        (
            "Mobile: 202 555 0199 $500",
            "202 555 0199",
            "phone_number",
            "PHONE",
        ),
        (
            "Call 020 7946 0958 £0.10",
            "020 7946 0958",
            "phone_number",
            "PHONE",
        ),
        // An SSN's label makes it IDENTITY (#381).
        ("SSN 123 45 6789 $500", "123 45 6789", "tax_id", "IDENTITY"),
        ("UTR 12345 67890 £500", "12345 67890", "tax_id", "TAX"),
        (
            "IBAN DE89 3704 0044 0532 0130 00 € 500",
            "3704 0044 0532 0130 00",
            "bank_account",
            "BANK",
        ),
        (
            "ИНН/КПП 7709999999/770901001 500 руб.",
            "770901001",
            "tax_id",
            "TAX",
        ),
        (
            "ИНН/КПП 7709999999/770901001 500 руб.",
            "7709999999",
            "tax_id",
            "TAX",
        ),
        (
            "моб. 8 916 123 45 67 500 руб.",
            "8 916 123 45 67",
            "phone_number",
            "PHONE",
        ),
        (
            "Routing 021000021 $500",
            "021000021",
            "bank_account",
            "BANK",
        ),
        // Working labels in the review remain working beside money.
        (
            "тел. 8 (495) 123-45-67 500 руб.",
            "8 (495) 123-45-67",
            "phone_number",
            "PHONE",
        ),
        (
            "Account No. 12 345 678 $500",
            "12 345 678",
            "bank_account",
            "BANK",
        ),
        (
            "р/с 40702810900000004711 500 руб.",
            "40702810900000004711",
            "bank_account",
            "BANK",
        ),
        (
            "Phone 202.555.0199 $500",
            "202.555.0199",
            "phone_number",
            "PHONE",
        ),
        // Dates with a decimal-looking prefix are not amount evidence.
        ("770912345678 01.02,2020", "770912345678", "tax_id", "TAX"),
        // Written-out amounts have the same integer boundaries.
        (
            "770912345678 500 (пятьсот) рублей.",
            "770912345678",
            "tax_id",
            "TAX",
        ),
        ("Номер: 164 353 783.", "164 353 783", "tax_id", "TAX"),
        ("Номер: 942838 306.", "942838 306", "bank_account", "BANK"),
        (
            "Номер: 0321 448 312.",
            "0321 448 312",
            "phone_number",
            "PHONE",
        ),
        (
            "Номер: 1 199999 (основной) рублей не содержит.",
            "1 199999",
            "tax_id",
            "TAX",
        ),
        (
            "Счёт: 40702810900000004711 руб.",
            "40702810900000004711",
            "bank_account",
            "BANK",
        ),
        (
            "40702 810 9 0000 0004711 руб.",
            "40702 810 9 0000 0004711",
            "bank_account",
            "BANK",
        ),
        ("ИНН 7709999999,02 руб.", "7709999999", "tax_id", "TAX"),
        ("ОГРН 1199900000011 руб.", "1199900000011", "tax_id", "TAX"),
        ("КПП 779901001 руб.", "779901001", "tax_id", "TAX"),
        ("БИК 049999999 руб.", "049999999", "bank_account", "BANK"),
        ("account: 12345.67.", "12345.67", "bank_account", "BANK"),
        ("Счёт: 12345.67 руб.", "12345.67", "bank_account", "BANK"),
        (
            "Телефон: 79990000123 руб.",
            "79990000123",
            "phone_number",
            "PHONE",
        ),
        (
            "Phone: 2025550199 dollars.",
            "2025550199",
            "phone_number",
            "PHONE",
        ),
        (
            "Bank account: 12345678 dollars.",
            "12345678",
            "bank_account",
            "BANK",
        ),
        (
            "Tax ID: 999991111,02 dollars.",
            "999991111",
            "tax_id",
            "TAX",
        ),
        (
            "Tax ID: 999 991 111 dollars.",
            "999 991 111",
            "tax_id",
            "TAX",
        ),
        (
            "Bank account: 1234 5678 dollars.",
            "1234 5678",
            "bank_account",
            "BANK",
        ),
        (
            "Phone: +1 (202) 555-0199 dollars.",
            "+1 (202) 555-0199",
            "phone_number",
            "PHONE",
        ),
        ("ИНН 770 999 9999 руб.", "770 999 9999", "tax_id", "TAX"),
        ("Счёт: 1234 5678 руб.", "1234 5678", "bank_account", "BANK"),
        (
            "Телефон: +7 (999) 555-01-23 руб.",
            "+7 (999) 555-01-23",
            "phone_number",
            "PHONE",
        ),
        (
            "ИНН 7709999999; остаток 714157,31 руб.",
            "7709999999",
            "tax_id",
            "TAX",
        ),
        (
            "Остаток 714157,31 руб.; БИК 049999999.",
            "049999999",
            "bank_account",
            "BANK",
        ),
        (
            "Остаток 714157,31 руб.; телефон +7 (999) 555-01-23.",
            "+7 (999) 555-01-23",
            "phone_number",
            "PHONE",
        ),
        (
            "1 199999 рублей перечислены ООО «Кедровый бор».",
            "ООО «Кедровый бор»",
            "organization",
            "ORG",
        ),
        (
            "£350 million paid to Maren Quill.",
            "Maren Quill",
            "person",
            "PERSON",
        ),
    ] {
        assert_eq!(
            shown(source, &[(value, label)]),
            [(value.into(), category.into())],
            "{source}"
        );
    }
}

/// A total label cannot establish where an OCR column ends (#547, round 2 P1).
#[test]
fn amount_totals_keep_the_next_columns_identifiers() {
    for gap in [" ", "  ", "\t"] {
        for (header, value, label, category) in [
            ("Сумма  КПП", "770 901 001", "tax_id", "TAX"),
            ("Сумма  Routing", "021 000 021", "bank_account", "BANK"),
        ] {
            for source in [
                format!("{header}\nИтого 500{gap}{value}"),
                format!("Итого 500{gap}{value}"),
            ] {
                assert_eq!(
                    shown(&source, &[(value, label)]),
                    pairs(&[(value, category)]),
                    "{source:?}"
                );
            }
        }
    }
}

/// Grouped identifiers cannot borrow a neighbour's amount cue (#547, round 2 P1).
#[test]
fn amount_guards_keep_grouped_identifiers_beside_money() {
    for (value, label, category) in [
        ("770 901 001", "tax_id", "TAX"),
        ("1 199 900 000 011", "tax_id", "TAX"),
        ("021 000 021", "bank_account", "BANK"),
        ("202 555 019", "phone_number", "PHONE"),
    ] {
        for source in [
            format!("{value} 500 руб."),
            format!("500 руб. {value}"),
            format!("$500 {value}"),
            format!("{value} 500,00"),
            format!("{value} 500.00"),
            format!("{value} 500 (пятьсот) рублей."),
            format!("{value} 500 €"),
        ] {
            assert_eq!(
                shown(&source, &[(value, label)]),
                pairs(&[(value, category)]),
                "{source:?}"
            );
        }
    }
    // The review's literal OCR rows have header labels on another line.
    for (source, value) in [
        (
            "Организация  КПП  Сумма\nООО «Ромашка» 770 901 001 500 руб.",
            "770 901 001",
        ),
        (
            "Организация  ОГРН  Сумма\nООО «Ромашка» 1 199 900 000 011 500 руб.",
            "1 199 900 000 011",
        ),
    ] {
        assert_eq!(
            shown(source, &[(value, "tax_id")]),
            pairs(&[(value, "TAX")])
        );
    }
}

/// Multiword and slash-spaced labels protect only their values (#547, round 2 P2).
#[test]
fn amount_guards_keep_multiword_and_paired_labels() {
    for (source, value, label, category) in [
        (
            "Routing number: 021000021 dollars.",
            "021000021",
            "bank_account",
            "BANK",
        ),
        (
            "ABA routing number: 021000021 dollars.",
            "021000021",
            "bank_account",
            "BANK",
        ),
        (
            "Mobile number: 2025550199 dollars.",
            "2025550199",
            "phone_number",
            "PHONE",
        ),
        (
            "ИНН / КПП 7709999999 / 770901001 руб.",
            "770901001",
            "tax_id",
            "TAX",
        ),
        (
            "ИНН / КПП 7709999999 / 770901001 руб.",
            "7709999999",
            "tax_id",
            "TAX",
        ),
    ] {
        assert_eq!(
            shown(source, &[(value, label)]),
            pairs(&[(value, category)]),
            "{source}"
        );
        // Neither a later amount in prose nor another table column is exempt.
        for separator in ["; paid ", "  ", "\t"] {
            let source = format!("{source}{separator}714157,31");
            assert_eq!(
                shown(&source, &[(value, label), ("714157", label)]),
                pairs(&[(value, category)]),
                "{source:?}"
            );
        }
    }
}

/// Labels win even when the identifier itself has an amount's shape.
#[test]
fn amount_guards_keep_additional_labelled_values() {
    for (source, value, label, category) in [
        // Existing conservative account-label exception, acknowledged in review.
        ("на счёт 500 руб.", "500", "bank_account", "BANK"),
        (
            "ИНН/КПП 7709999999/770901001 руб.",
            "770901001",
            "tax_id",
            "TAX",
        ),
        (
            "моб. 123 456 789 руб.",
            "123 456 789",
            "phone_number",
            "PHONE",
        ),
        (
            "сот. 123 456 789 руб.",
            "123 456 789",
            "phone_number",
            "PHONE",
        ),
        (
            "мобильный: 123 456 789 руб.",
            "123 456 789",
            "phone_number",
            "PHONE",
        ),
        (
            "Mobile: 202 555 019 dollars.",
            "202 555 019",
            "phone_number",
            "PHONE",
        ),
        (
            "Cell: 202 555 019 dollars.",
            "202 555 019",
            "phone_number",
            "PHONE",
        ),
        (
            "Routing 021000021 dollars.",
            "021000021",
            "bank_account",
            "BANK",
        ),
        (
            "ABA: 021000021 dollars.",
            "021000021",
            "bank_account",
            "BANK",
        ),
        // An SSN's label makes it IDENTITY (#381).
        (
            "SSN 123 456 789 dollars.",
            "123 456 789",
            "tax_id",
            "IDENTITY",
        ),
        ("UTR 123 456 789 dollars.", "123 456 789", "tax_id", "TAX"),
        // The passport's label decides its category over the model's
        // guess (#381).
        (
            "Паспорт 4506 123456 руб.",
            "4506 123456",
            "tax_id",
            "IDENTITY",
        ),
    ] {
        assert_eq!(
            shown(source, &[(value, label)]),
            pairs(&[(value, category)]),
            "{source}"
        );
    }
}

/// Context searches stay local and preserve UTF-8 boundaries on long OCR lines.
#[test]
fn amount_guards_use_bounded_context() {
    for (value, label, category) in [
        ("7709999999", "tax_id", "TAX"),
        ("12345678", "bank_account", "BANK"),
        ("2025550199", "phone_number", "PHONE"),
    ] {
        let source = format!(
            "{} {value} 500 руб. {}",
            "ё ".repeat(1000),
            "ё ".repeat(1000)
        );
        assert_eq!(
            shown(&source, &[(value, label), ("500", label)]),
            pairs(&[(value, category)])
        );
    }
    let pack = Pack::load(&[(
        "test",
        r#"
            # Distant context is insufficient to discard a possible identifier.
            [[shape]]
            category = "TAX"
            forbid_in = ['price: .*\d+ units']
        "#,
    )])
    .unwrap();
    let source = format!("price: {} 123 units", "ё ".repeat(1000));
    let at = source.find("123").unwrap();
    assert!(pack.identifying_at(&source, at..at + 3, "123", Category::Tax));
}

/// The contextual shape extension uses the pack's existing match
/// boundaries and validates its regexes just like span patterns.
#[test]
fn contextual_shapes_have_bounded_ranges_and_checked_patterns() {
    let pack = Pack::load(&[(
        "test",
        r#"
            [[shape]]
            category = "TAX"
            forbid_in = ['(?P<lead>price: )\d+(?P<tail> units)']
        "#,
    )])
    .unwrap();
    let source = "price: 123 units; 456";
    assert!(!pack.identifying_at(source, 7..10, "123", Category::Tax));
    assert!(pack.identifying_at(source, 18..21, "456", Category::Tax));
    let pack = Pack::load(&[(
        "test",
        r#"
            [[shape]]
            category = "TAX"
            forbid = ['^666$']
            forbid_in = ['\d+ units']
            unless_in = ['ID: \d+ units']
        "#,
    )])
    .unwrap();
    assert!(pack.identifying_at("ID: 123 units", 4..7, "123", Category::Tax));
    assert!(!pack.identifying_at("ID: 666 units", 4..7, "666", Category::Tax));
    for field in ["forbid_in", "unless_in"] {
        for pattern in ["x*", "(", "(?P<unknown>x)"] {
            let toml = format!("[[shape]]\ncategory = 'TAX'\n{field} = ['{pattern}']");
            assert!(Pack::load(&[("test", &toml)]).is_err(), "{toml}");
        }
    }
}
