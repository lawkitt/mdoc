//! OCR damage and look-alike letters.
use crate::rules::test_support::{pairs, shown, shown_at};

/// #549: common Russian words in Latin or Greek look-alike letters are
/// no names, the issue's synthetic reproduction first: «Ha» before a
/// date («На»), «Bce» («Все»), «T.π.» («т.п.»), and words mixing both
/// scripts («yл.», «Pуб.», a Latin y and P).
#[test]
fn russian_words_in_look_alike_letters_are_no_names() {
    let source = "Ha 01.01.2031 r. Bce расходы учтены, и T.π. Подпись: \
        Пахомцева И.А. Управляющий.";
    assert_eq!(
        shown(
            source,
            &[
                ("Ha", "organization"),
                ("Bce", "organization"),
                ("T.π.", "person"),
                ("r.", "organization"),
                ("Пахомцева И.А.", "person"),
            ]
        ),
        pairs(&[("Пахомцева И.А.", "PERSON")])
    );
    for (source, span, label) in [
        (
            "Ha основании договора стороны пришли к соглашению.",
            "Ha",
            "organization",
        ),
        ("Ha 01.02.2031 r. проведена проверка", "Ha", "person"),
        ("Bcё имущество передано по акту.", "Bcё", "organization"),
        (
            "Проживает: г. Энск, yл. Лесная, д. 5.",
            "yл.",
            "organization",
        ),
        (
            "Оплата услуг связи и почты, Pуб. 1500",
            "Pуб.",
            "organization",
        ),
        (
            "Ведомость, расходы и T.n. приложены к акту.",
            "T.n.",
            "person",
        ),
    ] {
        assert!(
            shown(source, &[(span, label)]).is_empty(),
            "{source}: {span}"
        );
    }
}

/// #549 guards: a real name with a letter of its own stays, in Russian
/// text or not: a Cyrillic name with a Latin typo («Ивaнов»), a Latin
/// name ("Smith", "Hans"), a company in Latin letters after its legal
/// form («ООО «Abc»», «ООО «Ha»»), an initial alone, a word in
/// capitals ("BCE"), a name after a title, one the document writes
/// beside a Latin name ("Ha Jin") and look-alikes in English text.
#[test]
fn real_names_with_look_alike_letters_stay() {
    let source = "Акт составлен Ивaновым Петром, ООО «Abc» и Smith, Hans.";
    assert_eq!(
        shown(
            source,
            &[
                ("Ивaновым Петром", "person"),
                ("ООО «Abc»", "organization"),
                ("Smith", "person"),
                ("Hans", "person"),
            ]
        ),
        pairs(&[
            ("Ивaновым Петром", "PERSON"),
            ("ООО «Abc»", "ORG"),
            ("Smith", "PERSON"),
            ("Hans", "PERSON"),
        ])
    );
    for (source, span, n, label, expected) in [
        (
            "Поставщик ООО «Ha» передал товар покупателю.",
            "ООО «Ha»",
            0,
            "organization",
            "ООО «Ha»",
        ),
        (
            "Поставщик ООО «Ha» передал товар покупателю.",
            "Ha",
            0,
            "organization",
            "ООО «Ha»",
        ),
        (
            "Акт подписал гражданин Ivanov, а также C. по доверенности.",
            "C.",
            0,
            "person",
            "C.",
        ),
        (
            "Акционер BCE передал акции по договору купли-продажи.",
            "BCE",
            0,
            "organization",
            "BCE",
        ),
        (
            "Заявление подала г-жа Ha по месту жительства.",
            "Ha",
            0,
            "person",
            "Ha",
        ),
        (
            "Ha Jin прибыл в город. Позже Ha подписал акт приёма.",
            "Ha",
            1,
            "person",
            "Ha",
        ),
        (
            "Ha said the contract was signed on 1 March 2031 in London.",
            "Ha",
            0,
            "person",
            "Ha",
        ),
    ] {
        let found = shown_at(source, &[(span, n, label)]);
        assert!(
            found.iter().any(|(text, _)| text == expected),
            "{source}: {span} {found:?}"
        );
    }
}

/// #549: an English scan's OCR-damaged words are no names: "PagelD"
/// (a "PageID" stamp), "IntereStee", "(LIU name of company)" ("Full"),
/// "Branwick Freight LIZ" ("LLC"), "V.S. agriculture" and
/// "Netberlaads", and a public body whose "U" OCR lost ("S. Depertuent
/// of Agriculture").
#[test]
fn english_ocr_damaged_words_are_no_names() {
    for (source, span, label) in [
        ("PageID 99; see PagelD 99.", "PagelD", "organization"),
        ("Exhibit 12, Page1D 4 of 9", "Page1D", "person"),
        (
            "Parties IntereStee in the outcome must file.",
            "IntereStee",
            "person",
        ),
        ("Signature (LIU name of company) and title", "LIU", "person"),
        (
            "Contractor: Branwick Freight LIZ, a Wyoming company.",
            "LIZ",
            "organization",
        ),
        (
            "Prepared for V.S. agriculture, the yields rose.",
            "V.S. agriculture",
            "organization",
        ),
        (
            "Prepared for U.S. agriculture, the yields rose.",
            "U.S. agriculture",
            "organization",
        ),
        (
            "Seed came from Netberlaads in 1931.",
            "Netberlaads",
            "organization",
        ),
        (
            "Issued by the S. Depertuent of Agriculture in 1931.",
            "S. Depertuent of Agriculture",
            "person",
        ),
    ] {
        let found = shown(source, &[(span, label)]);
        assert!(found.is_empty(), "{source}: {span} {found:?}");
    }
}

/// #549 guards: English names with "l" and "I" stay ("Ilya", "Lily",
/// "Liu", "Liz"), and so do "U.S. Bank", initials alone ("V.S."), a
/// name ending in "d" ("McLeod") and a company of common words.
#[test]
fn english_names_like_ocr_damage_stay() {
    for (source, span, label, category) in [
        (
            "Witness Ilya Penrow signed the form.",
            "Ilya Penrow",
            "person",
            "PERSON",
        ),
        (
            "Witness Lily Brannock signed the form.",
            "Lily Brannock",
            "person",
            "PERSON",
        ),
        (
            "Signed by Wei Liu on 3 March 2031.",
            "Wei Liu",
            "person",
            "PERSON",
        ),
        ("Signed by LIU on 3 March 2031.", "LIU", "person", "PERSON"),
        ("Ask Liz about the delivery.", "Liz", "person", "PERSON"),
        ("LIZ signed the agreement.", "LIZ", "organization", "ORG"),
        (
            "The account is held at U.S. Bank in Ohio.",
            "U.S. Bank",
            "organization",
            "ORG",
        ),
        ("Signed by V.S. at the office.", "V.S.", "person", "PERSON"),
        (
            "Signed by David McLeod in 2031.",
            "David McLeod",
            "person",
            "PERSON",
        ),
        (
            "Signed by Interested Parties Ltd in 2031.",
            "Interested Parties Ltd",
            "organization",
            "ORG",
        ),
    ] {
        assert_eq!(
            shown(source, &[(span, label)]),
            pairs(&[(span, category)]),
            "{source}"
        );
    }
}

/// #549: a garbled token is no tax number ("t e43b7") and a run of one
/// digit no identity document's number ("2808888888R") without a
/// label; OCR-damaged numbers and other runs stay.
#[test]
fn garbled_tokens_are_no_tax_or_identity_numbers() {
    for (source, span, label) in [
        ("The hash t e43b7 was printed below.", "t e43b7", "tax_id"),
        (
            "Stamp 2808888888R on the page.",
            "2808888888R",
            "national_id_number",
        ),
        (
            "Stamp 8888888888R on the page.",
            "8888888888R",
            "passport_number",
        ),
    ] {
        assert!(
            shown(source, &[(span, label)]).is_empty(),
            "{source}: {span}"
        );
    }
    for (source, span, label, category) in [
        (
            "ИНН 77l12345O7 указан в договоре.",
            "77l12345O7",
            "tax_id",
            "TAX",
        ),
        (
            "Passport number 4510 123456 issued.",
            "4510 123456",
            "passport_number",
            "IDENTITY",
        ),
        (
            "Passport 4510 888888 issued.",
            "4510 888888",
            "passport_number",
            "IDENTITY",
        ),
        // A label keeps the issue's own shape, and runs without the
        // final capital or shorter ones stay (pre-gate review, #549).
        (
            "Passport number: 2808888888R",
            "2808888888R",
            "passport_number",
            "IDENTITY",
        ),
        (
            "Stamp 4500000001 on the page.",
            "4500000001",
            "passport_number",
            "IDENTITY",
        ),
        (
            "Stamp 10000000A on the page.",
            "10000000A",
            "passport_number",
            "IDENTITY",
        ),
        (
            "Stamp 70000000001 on the page.",
            "70000000001",
            "national_id_number",
            "IDENTITY",
        ),
    ] {
        assert_eq!(
            shown(source, &[(span, label)]),
            pairs(&[(span, category)]),
            "{source}"
        );
    }
}

/// #549 pre-gate review: only hex letters make a garbled tax token; an
/// OCR-damaged number after a stray letter keeps its value.
#[test]
fn a_stray_letter_before_a_damaged_tax_number_keeps_it() {
    let source = "ИНН n 7712345l07 указан в договоре.";
    let found = shown(source, &[("n 7712345l07", "tax_id")]);
    assert!(
        found
            .iter()
            .any(|(text, category)| text.contains("7712345l07") && category == "TAX"),
        "{found:?}"
    );
}

/// #549 pre-gate review: a lone Latin "Ha" stays a person where the
/// document names her: beside a Latin name in any script or case
/// ("Nguyễn Thị Ha", "HA THI LAN"), after a person's label or role in
/// its clause («Гражданка», «Свидетель», «Фамилия:», «Работник:»,
/// «гражданка Республики Корея»); and a quoted «Bce» is a company's
/// name.
#[test]
fn a_named_or_labelled_look_alike_word_stays() {
    for (source, raw, expected, category) in [
        (
            "Переводчик Nguyễn Thị Ha разъяснила права. Затем Ha подтвердила показания.",
            &[("Nguyễn Thị Ha", 0, "person"), ("Ha", 1, "person")][..],
            "Ha",
            "PERSON",
        ),
        (
            "Гражданка Ha пояснила суду, что договор подписан.",
            &[("Ha", 0, "person")][..],
            "Ha",
            "PERSON",
        ),
        (
            "Свидетель Ha пояснила, что расписку не видела.",
            &[("Ha", 0, "person")][..],
            "Ha",
            "PERSON",
        ),
        (
            "Анкета заявителя. Фамилия: Ha\nИмя: Lan\nДата рождения указана.",
            &[("Ha", 0, "person"), ("Lan", 0, "person")][..],
            "Ha",
            "PERSON",
        ),
        (
            "Работник: Ha, гражданин Вьетнама, принят на должность.",
            &[("Ha", 0, "person")][..],
            "Ha",
            "PERSON",
        ),
        (
            "В заседании гражданка Республики Корея Ha пояснила, что долг погашен.",
            &[("Ha", 0, "person")][..],
            "Ha",
            "PERSON",
        ),
        (
            "HA THI LAN разъяснила права. Подсудимый Ha вину признал.",
            &[("HA THI LAN", 0, "person"), ("Ha", 0, "person")][..],
            "Ha",
            "PERSON",
        ),
        (
            "Затем HA THI LAN подписала. Позже Ha уехала из города.",
            &[("HA THI LAN", 0, "person"), ("Ha", 0, "person")][..],
            "Ha",
            "PERSON",
        ),
        (
            "Поставщик «Bce» передал товар покупателю по накладной.",
            &[("Bce", 0, "organization")][..],
            "Bce",
            "ORG",
        ),
    ] {
        let found = shown_at(source, raw);
        assert!(
            found
                .iter()
                .any(|(text, found)| text.trim_matches(['«', '»']) == expected
                    && found == category),
            "{source}: {found:?}"
        );
    }
}
