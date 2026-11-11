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
        // role, surname first and mid-sentence.
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
        // A place before initials is no double surname.
        ("г. Санкт-Петербург Т.Т. Колобова", &[], &[]),
    ]);
}
