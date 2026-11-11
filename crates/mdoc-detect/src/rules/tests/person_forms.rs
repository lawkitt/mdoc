//! A person's surname or titled name proposed as an organization, words
//! glued onto a name, and Russian initials' forms (#503, #489, #793, #387).
use crate::rules::test_support::proposed_marks;

/// A case: its source, the model's spans (label, text, each found after
/// the one before) and what the user is shown.
type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a [&'a str]);

/// Each case against what the user is shown, all mismatches reported at
/// once.
fn check(cases: &[Case<'_>]) {
    let mut wrong = Vec::new();
    for (source, model, expected) in cases {
        let shown = proposed_marks(source, model);
        if shown != *expected {
            wrong.push(format!(
                "{source:?}\n  shown:    {shown:?}\n  expected: {expected:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

/// #503: a surname the model proposes as an organization is the person
/// where the document gives a person that surname: a person span, a title
/// before it there or anywhere, or a full name; and a full name proposed as
/// an organization is the person too. Every mention follows the wording.
#[test]
fn a_known_persons_surname_is_no_organization() {
    check(&[
        (
            "Talwen Ostrander said the plan failed. Later Ostrander confirmed it, and Ostrander left.",
            &[
                ("person", "Talwen Ostrander"),
                ("organization", "Ostrander"),
            ],
            &[
                "PERSON⟦Talwen Ostrander⟧",
                "PERSON⟦Ostrander⟧",
                "PERSON⟦Ostrander⟧",
            ],
        ),
        // The full name itself as an organization, later.
        (
            "Talwen Ostrander said the plan failed. The memo of Talwen Ostrander was late.",
            &[
                ("person", "Talwen Ostrander"),
                ("organization", "Talwen Ostrander"),
            ],
            &["PERSON⟦Talwen Ostrander⟧", "PERSON⟦Talwen Ostrander⟧"],
        ),
        // A signature in capitals.
        (
            "Dear Congressman Iver Stroud: we write again.\n\nSTROUD, Chair",
            &[("person", "Iver Stroud"), ("organization", "STROUD")],
            &["PERSON⟦Iver Stroud⟧", "PERSON⟦STROUD⟧"],
        ),
        // A title before the surname, here or elsewhere.
        (
            "Senator Quillfeather spoke first. Then Quillfeather sat down.",
            &[("organization", "Quillfeather")],
            &["PERSON⟦Quillfeather⟧", "PERSON⟦Quillfeather⟧"],
        ),
        (
            "The bill passed. Sen. Quillfeather voted for it.",
            &[("organization", "Quillfeather")],
            &["PERSON⟦Quillfeather⟧"],
        ),
        // The title inside the model's span.
        (
            "Dr. Pellow examined the claimant on 3 May.",
            &[("organization", "Dr. Pellow")],
            &["PERSON⟦Pellow⟧"],
        ),
        (
            "Dr Pellow examined the claimant on 3 May.",
            &[("organization", "Dr Pellow")],
            &["PERSON⟦Pellow⟧"],
        ),
        (
            "The hearing before Judge Marrow was short.",
            &[("organization", "Judge Marrow")],
            &["PERSON⟦Marrow⟧"],
        ),
        (
            "One report came from HHJ Penrow in March.",
            &[("organization", "HHJ Penrow")],
            &["PERSON⟦Penrow⟧"],
        ),
        // A defined short name right after the full name.
        (
            "The trustee, Petra Vandermeer (“Vandermeer”), will reply.",
            &[
                ("person", "Petra Vandermeer"),
                ("organization", "Vandermeer"),
            ],
            &["PERSON⟦Petra Vandermeer⟧", "PERSON⟦Vandermeer⟧"],
        ),
        // An address is no company: a state's code after a town's comma
        // reads as "Co" no more, so the titled surname is the person, and
        // its mentions follow; the town stays in the address.
        (
            "Senator Hartrow testified.\nMail: Hartrow, CO 80501.\nHartrow objected.",
            &[("organization", "Hartrow")],
            &[
                "PERSON⟦Hartrow⟧",
                "ADDRESS⟦Hartrow, CO 80501⟧",
                "PERSON⟦Hartrow⟧",
            ],
        ),
        // So is a state's code in brackets.
        (
            "Senator Hartrow testified.\nMailing location: (Hartrow, CO)\nHartrow objected.",
            &[("organization", "Hartrow")],
            &["PERSON⟦Hartrow⟧", "PERSON⟦Hartrow⟧", "PERSON⟦Hartrow⟧"],
        ),
    ]);
}

/// #503: surnames listed after a plural title are each a person, whether
/// the model gave each one or one span over the list.
#[test]
fn surnames_after_a_plural_title_are_people() {
    check(&[
        (
            "Senators Quillfeather, Brannock and Ostley voted against it.",
            &[
                ("organization", "Quillfeather"),
                ("organization", "Brannock"),
                ("organization", "Ostley"),
            ],
            &["PERSON⟦Quillfeather⟧", "PERSON⟦Brannock⟧", "PERSON⟦Ostley⟧"],
        ),
        (
            "Senators Quillfeather, Brannock and Ostley voted against it.",
            &[("organization", "Quillfeather, Brannock and Ostley")],
            &["PERSON⟦Quillfeather⟧", "PERSON⟦Brannock⟧", "PERSON⟦Ostley⟧"],
        ),
        (
            "Signatories: Quillfeather, Brannock, Ostley.",
            &[("organization", "Brannock")],
            &["PERSON⟦Brannock⟧"],
        ),
        (
            "The bill passed. Senators Quillfeather and Brannock objected.",
            &[("person", "Senators Quillfeather and Brannock")],
            &["PERSON⟦Quillfeather⟧", "PERSON⟦Brannock⟧"],
        ),
        // A person's span with a comma alone may be surname first.
        (
            "Witnesses: Brannock, Ivo gave evidence on oath.",
            &[("person", "Brannock, Ivo")],
            &["PERSON⟦Brannock, Ivo⟧"],
        ),
        // A list heading over a company's short name: the firm signs, so
        // the model's organization stays whole, as a lone surname would.
        (
            "Hartrow and Vensel LLP signed below.\nSignatories: Hartrow and Vensel",
            &[
                ("organization", "Hartrow and Vensel LLP"),
                ("organization", "Hartrow and Vensel"),
            ],
            &["ORG⟦Hartrow and Vensel LLP⟧", "ORG⟦Hartrow and Vensel⟧"],
        ),
        // A comma before the legal form: the list is still the firm's
        // short name, with no model span over the firm's whole name; the
        // wording spreads to the firm's name too.
        (
            "Hartrow and Vensel, LLP signed.\nSignatories: Hartrow and Vensel",
            &[("organization", "Hartrow and Vensel")],
            &["ORG⟦Hartrow and Vensel⟧", "ORG⟦Hartrow and Vensel⟧"],
        ),
        // The firm's address after its form's own comma leaves the form.
        (
            "Hartrow and Vensel, LLP, 18 Alder Road\nSignatories: Hartrow and Vensel",
            &[("organization", "Hartrow and Vensel")],
            &["ORG⟦Hartrow and Vensel⟧", "ORG⟦Hartrow and Vensel⟧"],
        ),
        // The model's span over the heading too: the firm's short name
        // stays one organization, the heading outside.
        (
            "Hartrow and Vensel LLP signed.\nSignatories: Hartrow and Vensel",
            &[
                ("organization", "Hartrow and Vensel LLP"),
                ("organization", "Signatories: Hartrow and Vensel"),
            ],
            &["ORG⟦Hartrow and Vensel LLP⟧", "ORG⟦Hartrow and Vensel⟧"],
        ),
        // A comma and "Co." still make a company's short name: that name
        // alone stays an organization.
        (
            "Hartrow, Co. signed.\nSignatories: Hartrow and Vensel",
            &[("organization", "Hartrow and Vensel")],
            &["ORG⟦Hartrow⟧", "ORG⟦Hartrow⟧", "PERSON⟦Vensel⟧"],
        ),
        // A company holding one surname of a people list keeps only that
        // name an organization: the rest of the list are people.
        (
            "Hartrow Holdings Ltd appealed.\nSenators Hartrow and Vensel opposed it.",
            &[
                ("organization", "Hartrow Holdings Ltd"),
                ("organization", "Hartrow and Vensel"),
            ],
            &[
                "ORG⟦Hartrow Holdings Ltd⟧",
                "ORG⟦Hartrow⟧",
                "PERSON⟦Vensel⟧",
            ],
        ),
        // No plural title: a company's list stays.
        (
            "Clients: Kestrel, Pellam and Quenby renewed.",
            &[("organization", "Kestrel")],
            &["ORG⟦Kestrel⟧"],
        ),
    ]);
}

/// #503: the guards. A company named after a person, with a legal form or
/// a body word, stays a company, and so does a bare surname such a
/// company of the document holds (its short name); a prize or an act named
/// after a person is no surname; a title alone and a party code stay
/// unmarked; an unknown one-word organization stays.
#[test]
fn a_company_named_after_a_person_stays_a_company() {
    check(&[
        (
            "Talwen Ostrander founded Ostrander Holdings Ltd in 2001.",
            &[
                ("person", "Talwen Ostrander"),
                ("organization", "Ostrander Holdings Ltd"),
            ],
            &["PERSON⟦Talwen Ostrander⟧", "ORG⟦Ostrander Holdings Ltd⟧"],
        ),
        (
            "Talwen Ostrander chairs the Ostrander Foundation board.",
            &[
                ("person", "Talwen Ostrander"),
                ("organization", "Ostrander Foundation"),
            ],
            &["PERSON⟦Talwen Ostrander⟧", "ORG⟦Ostrander Foundation⟧"],
        ),
        (
            "Talwen Ostrander left Ostrander Holdings Ltd. Ostrander then sold its shares.",
            &[
                ("person", "Talwen Ostrander"),
                ("organization", "Ostrander Holdings Ltd"),
                ("organization", "Ostrander"),
            ],
            &[
                "PERSON⟦Talwen Ostrander⟧",
                "ORG⟦Ostrander Holdings Ltd⟧",
                "ORG⟦Ostrander⟧",
            ],
        ),
        (
            "Talwen Pellow won the Pellow Prize twice.",
            &[
                ("person", "Talwen Pellow"),
                ("organization", "Pellow Prize"),
            ],
            &["PERSON⟦Talwen Pellow⟧", "ORG⟦Pellow Prize⟧"],
        ),
        (
            "The Kestrel board met. Kestrel then issued shares.",
            &[("organization", "Kestrel")],
            &["ORG⟦Kestrel⟧", "ORG⟦Kestrel⟧"],
        ),
        (
            "Mrs Edda Brannock (C2) gave evidence, and C2 was cross-examined.",
            &[("person", "Edda Brannock"), ("organization", "C2")],
            &["PERSON⟦Edda Brannock⟧"],
        ),
        // A surname the document writes with a legal form after it is the
        // company's short name.
        (
            "Edda Ostley wrote. Later Ostley paid, as Ostley Ltd must.",
            &[("person", "Edda Ostley"), ("organization", "Ostley")],
            &["PERSON⟦Edda Ostley⟧", "ORG⟦Ostley⟧", "ORG⟦Ostley⟧"],
        ),
    ]);
}

/// #503: a cited case's party stays as a citation leaves it, whichever
/// category the model gave the surname (#561 is separate).
#[test]
fn a_cited_partys_surname_keeps_todays_behaviour() {
    let source = "Talwen Pellow wrote to us. See Ostrand v. Pellow, 12 F.4th 345 (9th Cir. 2031).";
    let person = proposed_marks(source, &[("person", "Talwen Pellow"), ("person", "Pellow")]);
    let organization = proposed_marks(
        source,
        &[("person", "Talwen Pellow"), ("organization", "Pellow")],
    );
    assert_eq!(organization, person);
}

/// #503: a title alone is no person's name, whatever follows it elsewhere.
#[test]
fn a_title_alone_is_no_person() {
    let shown = proposed_marks(
        "The Senator spoke, and Senator Quillfeather answered.",
        &[("organization", "Senator")],
    );
    assert!(
        !shown
            .iter()
            .any(|mark| mark.contains("Senator") && mark.starts_with("PERSON")),
        "{shown:?}"
    );
}

/// #503: a noun glued after a person's name stays outside it, and a
/// post-nominal does too; as an organization, only where the document
/// names the person. A company's own name with a legal form stays.
#[test]
fn a_noun_after_a_persons_name_stays_outside() {
    check(&[
        (
            "Talwen Pellow spoke. The Pellow family agreed.",
            &[
                ("person", "Talwen Pellow"),
                ("organization", "Pellow family"),
            ],
            &["PERSON⟦Talwen Pellow⟧", "PERSON⟦Pellow⟧"],
        ),
        (
            "Talwen Pellow spoke. The Pellow family agreed.",
            &[("person", "Talwen Pellow"), ("person", "Pellow family")],
            &["PERSON⟦Talwen Pellow⟧", "PERSON⟦Pellow⟧"],
        ),
        (
            "Iver Ostrander ran, and the Ostrander campaign spent little.",
            &[
                ("person", "Iver Ostrander"),
                ("organization", "the Ostrander campaign"),
            ],
            &["PERSON⟦Iver Ostrander⟧", "PERSON⟦Ostrander⟧"],
        ),
        (
            "Dr. Pellow keeps the samples in the Pellow laboratory.",
            &[("person", "Pellow"), ("organization", "Pellow laboratory")],
            &["PERSON⟦Pellow⟧", "PERSON⟦Pellow⟧"],
        ),
        (
            "Professor Ostrander moved the Ostrander Lab to the north wing.",
            &[("person", "Ostrander"), ("organization", "Ostrander Lab")],
            &["PERSON⟦Ostrander⟧", "PERSON⟦Ostrander⟧"],
        ),
        (
            "The samples went to Ashby Kestrel Site for testing.",
            &[("person", "Ashby Kestrel Site")],
            &["PERSON⟦Ashby Kestrel⟧"],
        ),
        (
            "Report signed by Darren Holt M.D. on Friday.",
            &[("person", "Darren Holt M.D.")],
            &["PERSON⟦Darren Holt⟧"],
        ),
        (
            "Report signed by Darren Holt, Ph.D. on Friday.",
            &[("person", "Darren Holt, Ph.D.")],
            &["PERSON⟦Darren Holt⟧"],
        ),
        // No person of that name: no person is made of it.
        (
            "The Kestrel campaign spent little.",
            &[("organization", "Kestrel campaign")],
            &[],
        ),
        (
            "Ivo Kestrel joined Kestrel Labs Inc in May.",
            &[
                ("person", "Ivo Kestrel"),
                ("organization", "Kestrel Labs Inc"),
            ],
            &["PERSON⟦Ivo Kestrel⟧", "ORG⟦Kestrel Labs Inc⟧"],
        ),
    ]);
}

/// #503: junk before a name and a caption's given name: a title with a
/// pleading line number between, and an all-capitals given name the model
/// left out; a role there stays outside.
#[test]
fn a_captions_given_name_joins_and_a_line_number_goes() {
    check(&[
        (
            "the statement of former President 21 Quillfeather was read",
            &[("person", "former President 21 Quillfeather")],
            &["PERSON⟦Quillfeather⟧"],
        ),
        (
            "PLAINTIFF SIGNE ODEGAARD\nv.\nKESTREL HOLDINGS LLC",
            &[
                ("person", "ODEGAARD"),
                ("organization", "KESTREL HOLDINGS LLC"),
            ],
            &["PERSON⟦SIGNE ODEGAARD⟧", "ORG⟦KESTREL HOLDINGS LLC⟧"],
        ),
        (
            "JOEL VANCE, Defendant.",
            &[("person", "VANCE")],
            &["PERSON⟦JOEL VANCE⟧"],
        ),
        (
            "PLAINTIFF ODEGAARD\nDEPUTY VANCE",
            &[("person", "ODEGAARD"), ("person", "VANCE")],
            &["PERSON⟦ODEGAARD⟧", "PERSON⟦VANCE⟧"],
        ),
    ]);
}

/// #503, #793: Russian initials' forms: glued to the surname, after a
/// double surname, after a role, and a verb or preposition glued before
/// them outside, as a person or an organization.
#[test]
fn russian_initials_forms_are_the_name() {
    check(&[
        (
            "Расписку составила Т.Т.Колобова в тот же день.",
            &[("person", "Т.Т.")],
            &["PERSON⟦Т.Т.Колобова⟧"],
        ),
        (
            "Расписка: Ростов-Кедров Т.Т., подпись.",
            &[],
            &["PERSON⟦Ростов-Кедров Т.Т.⟧"],
        ),
        (
            "Дело рассмотрела судья Л. Н. Кедрина единолично.",
            &[],
            &["PERSON⟦Л. Н. Кедрина⟧"],
        ),
        (
            "Обязать Е. Р. вернуть ключи до 1 мая.",
            &[("person", "Обязать Е. Р.")],
            &["PERSON⟦Е. Р.⟧"],
        ),
        // The initials left grow to the surname after them.
        (
            "Обязать Е. Р. Тулину вернуть ключи.",
            &[("person", "Обязать Е. Р.")],
            &["PERSON⟦Е. Р. Тулину⟧"],
        ),
        (
            "Иск к Е. Р. удовлетворить полностью.",
            &[("organization", "к Е. Р.")],
            &["PERSON⟦Е. Р.⟧"],
        ),
        (
            "Иск к Е. Р. Тулиной удовлетворить.",
            &[("organization", "к Е. Р.")],
            &["PERSON⟦Е. Р. Тулиной⟧"],
        ),
        // A surname in -ть before initials is no verb: it stays after a
        // role, surname first and mid-sentence, and the model's
        // organization is never cut to the initials.
        (
            "Свидетель Белоть Е. Р. подписал.",
            &[("person", "Белоть Е. Р.")],
            &["PERSON⟦Белоть Е. Р.⟧"],
        ),
        (
            "Белоть Е. Р. подписал расписку.",
            &[("person", "Белоть Е. Р.")],
            &["PERSON⟦Белоть Е. Р.⟧"],
        ),
        (
            "Расписку вчера подписал Дремоть Е. Р. при свидетелях.",
            &[("person", "Е. Р.")],
            &["PERSON⟦Дремоть Е. Р.⟧"],
        ),
        (
            "Иск к Белоть Е. Р. удовлетворить.",
            &[("organization", "Белоть Е. Р.")],
            &["ORG⟦Белоть Е. Р.⟧"],
        ),
        // Any operative verb of a court order stays outside, whether the
        // model gave the initials alone, the verb with them as a person,
        // or as an organization.
        (
            "Выдать Е. Р. копию решения.",
            &[("person", "Е. Р.")],
            &["PERSON⟦Е. Р.⟧"],
        ),
        (
            "Выплатить Е. Р. Тулиной компенсацию.",
            &[("person", "Е. Р.")],
            &["PERSON⟦Е. Р. Тулиной⟧"],
        ),
        (
            "Возместить Е. Р. расходы.",
            &[("person", "Возместить Е. Р.")],
            &["PERSON⟦Е. Р.⟧"],
        ),
        (
            "Зачесть Е. Р. срок содержания.",
            &[("organization", "Зачесть Е. Р.")],
            &["PERSON⟦Е. Р.⟧"],
        ),
        // A place before initials is no double surname.
        ("г. Санкт-Петербург Т.Т. Колобова", &[], &[]),
    ]);
}

/// What the user is shown when the model gives `name` in `source` as a
/// person and as an organization at once, the organization scored higher.
fn both_labels(source: &str, name: &str) -> Vec<String> {
    let start = source.find(name).unwrap();
    let raw: Vec<crate::RawEntity> = [("person", 0.5), ("organization", 0.8)]
        .into_iter()
        .map(|(label, score)| crate::RawEntity {
            start,
            end: start + name.len(),
            label: label.into(),
            score,
            text: name.into(),
        })
        .collect();
    crate::recognition::proposals(source, &raw)
        .unwrap()
        .into_iter()
        .map(|span| format!("{}⟦{}⟧", span.category, &source[span.range]))
        .collect()
}

/// #503: a Russian surname with initials the model gave as a person and
/// an organization at once is the person (initials are a person's, guide
/// rule 1), before or after the surname, glued or spaced, a hyphenated
/// surname too. A company's name with initials-like letters stays a
/// company, and an organization alone stays as the model gave it (an
/// entrepreneur's rule decides that, `people.rs`).
#[test]
fn a_surname_with_initials_given_both_labels_is_a_person() {
    for (source, name) in [
        (
            "Акт подписал Корнев-Ясный Д.Д., а затем ушёл.",
            "Корнев-Ясный Д.Д.",
        ),
        (
            "Акт подписала Д. А. Мирошина в тот же день.",
            "Д. А. Мирошина",
        ),
        ("Акт подписала Д.А.Мирошина в тот же день.", "Д.А.Мирошина"),
    ] {
        assert_eq!(
            both_labels(source, name),
            [format!("PERSON⟦{name}⟧")],
            "{source}"
        );
    }
    // A company: its legal form and quotes keep it an organization.
    let source = "Поставщик ООО «Р.Р. Групп» отгрузил товар.";
    assert!(
        !both_labels(source, "Р.Р. Групп")
            .iter()
            .any(|shown| shown.starts_with("PERSON")),
        "{source}"
    );
    check(&[
        (
            "Акт подписал Корнев-Ясный Д.Д., а затем ушёл.",
            &[("organization", "Корнев-Ясный Д.Д.")],
            &["ORG⟦Корнев-Ясный Д.Д.⟧"],
        ),
        (
            "Поставщик ООО «Р.Р. Групп» отгрузил товар.",
            &[("organization", "ООО «Р.Р. Групп»")],
            &["ORG⟦ООО «Р.Р. Групп»⟧"],
        ),
        (
            "Поставщик ООО «Р.Р. Групп» отгрузил товар.",
            &[("organization", "Р.Р. Групп")],
            &["ORG⟦ООО «Р.Р. Групп»⟧"],
        ),
    ]);
}

/// #503: bare initials the model left unmarked are the person the
/// document names with those initials and a surname; initials no person of
/// the document has stay unmarked.
#[test]
fn bare_initials_of_a_known_person_are_the_person() {
    check(&[
        (
            "Обязать Д. А. Сухову вернуть ключи. Требования к Д. А. в остальной части оставить.",
            &[("person", "Д. А. Сухову")],
            &["PERSON⟦Д. А. Сухову⟧", "PERSON⟦Д. А.⟧"],
        ),
        (
            "Расписку дал Сухов Д.А., а позже к Д.А. претензий не было.",
            &[("person", "Сухов Д.А.")],
            &["PERSON⟦Сухов Д.А.⟧", "PERSON⟦Д.А.⟧"],
        ),
        // Other initials: no person of the document has them.
        (
            "Обязать Д. А. Сухову вернуть ключи. Требования к В. Г. оставить.",
            &[("person", "Д. А. Сухову")],
            &["PERSON⟦Д. А. Сухову⟧"],
        ),
        // No known person with initials: nothing is added.
        ("Требования к Д. А. оставить без рассмотрения.", &[], &[]),
    ]);
}

/// #793: the court order's operative verbs that come off before initials
/// are the same in the code's organization check (`OPERATIVE_VERBS`), the
/// Russian pack's PERSON trim and its initials join's `unless_over`.
#[test]
fn operative_verbs_agree_with_the_russian_pack() {
    let verbs = crate::rules::person_forms::OPERATIVE_VERBS;
    let pack: toml::Table =
        toml::from_str(include_str!("../../../rules/ru.toml")).expect("ru.toml");
    let spans = pack["span"].as_array().expect("spans");
    let trimmed = spans.iter().any(|span| {
        span.get("trim_start")
            .and_then(toml::Value::as_array)
            .is_some_and(|trims| {
                trims.iter().any(|trim| {
                    trim.as_str()
                        .is_some_and(|t| t.contains(&format!("(?:{verbs})")))
                })
            })
    });
    assert!(trimmed, "no PERSON trim_start lists OPERATIVE_VERBS");
    let lower: Vec<String> = verbs.split('|').map(str::to_lowercase).collect();
    let joined = spans.iter().any(|span| {
        span.get("unless_over")
            .and_then(toml::Value::as_array)
            .is_some_and(|words| {
                words
                    .iter()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
                    == lower
            })
    });
    assert!(joined, "no unless_over lists OPERATIVE_VERBS in lowercase");
}
