//! Span fixes.
use super::*;
use crate::rules::test_support::{fixed, pairs, proposed_marks, proposed_orgs, shown};

#[test]
fn span_fixes_trim_titles_keep_legal_forms_and_end_before_the_period() {
    // Titles and post-nominals off a person (#382); initials keep theirs.
    assert_eq!(
        fixed(
            "Ms Bennett met Dr. Ysolde Qhenn, KC and Jorin P.",
            &[
                ("Ms Bennett", Category::Person),
                ("Dr. Ysolde Qhenn, KC", Category::Person),
                ("Jorin P.", Category::Person),
            ]
        ),
        ["Bennett", "Ysolde Qhenn", "Jorin P."]
    );
    assert_eq!(
        fixed(
            "Г-жа Павлова пришла.",
            &[("Г-жа Павлова", Category::Person)]
        ),
        ["Павлова"]
    );
    // A title alone keeps no span.
    assert!(fixed("Dear Ms,", &[("Ms", Category::Person)]).is_empty());
    // An organization's legal form and quotes (#374), not the period.
    assert_eq!(
        fixed(
            "Истец: ООО «Северный Вектор», ответчик АО \"Ёлка\". Signed by Acme Ltd.",
            &[
                ("Северный Вектор", Category::Organization),
                ("Ёлка", Category::Organization),
                ("Acme Ltd.", Category::Organization),
            ]
        ),
        ["ООО «Северный Вектор»", "АО \"Ёлка\"", "Acme Ltd"]
    );
    // Quotes without a legal form, and a person, stay as they are.
    assert_eq!(
        fixed(
            "Роман «Кедровый свет» и «Вера Снегирь»",
            &[("Вера Снегирь", Category::Person)]
        ),
        ["Вера Снегирь"]
    );
    assert_eq!(
        fixed("журнал «Север»", &[("Север", Category::Organization)]),
        ["Север"]
    );
}

#[test]
fn an_initial_keeps_its_period_a_sentence_loses_it() {
    assert_eq!(
        fixed(
            "Исполнитель: Студия Белявская О. М. Заказчик: ООО «Ромашка».",
            &[
                ("Студия Белявская О. М.", Category::Organization),
                ("ООО «Ромашка».", Category::Organization),
            ]
        ),
        ["Студия Белявская О. М.", "ООО «Ромашка»"]
    );
    assert_eq!(
        fixed(
            "Исполнитель: Студия Белявская О.М.. Подрядчик: Альфа-Строй.",
            &[
                ("Студия Белявская О.М..", Category::Organization),
                ("Альфа-Строй.", Category::Organization),
            ]
        ),
        ["Студия Белявская О.М.", "Альфа-Строй"]
    );
    assert_eq!(
        fixed(
            "Prepared by Smith J. for Northbridge Legal Ltd.",
            &[
                ("Smith J.", Category::Organization),
                ("Northbridge Legal Ltd.", Category::Organization),
            ]
        ),
        ["Smith J.", "Northbridge Legal Ltd"]
    );
}

/// A surname takes in two initials beside it (#391); a sentence's
/// period, a comma, a line or one initial alone keeps them apart.
#[test]
fn initials_beside_a_surname_join_it() {
    for (source, model, expected) in [
        ("Ответчик Ветлин Н. О. возражал", "Ветлин", "Ветлин Н. О."),
        (
            "Заявитель: Ветлин Н. О.. Объяснения",
            "Ветлин",
            "Ветлин Н. О.",
        ),
        ("Подписал Ветлин Н.О., истец", "Ветлин", "Ветлин Н.О."),
        ("Нотариус Л. И. Брюн удостоверила", "Брюн", "Л. И. Брюн"),
        ("Подпись: О’Шеннон А. П.", "О’Шеннон", "О’Шеннон А. П."),
        ("Истец Ли-Вельт Л. А. пришла", "Ли-Вельт", "Ли-Вельт Л. А."),
        ("Signed by J. R. Quill on Monday", "Quill", "J. R. Quill"),
        (
            "Signed by J.R. O’Brien-Hale",
            "O’Brien-Hale",
            "J.R. O’Brien-Hale",
        ),
        // A sentence's period, a comma and a line end the name.
        ("Ветлин. Н. О. Иванов", "Ветлин", "Ветлин"),
        ("Ветлин, Н. О. Иванов", "Ветлин", "Ветлин"),
        ("Ветлин\nН. О. Иванов", "Ветлин", "Ветлин"),
        // One initial is not enough to join.
        ("Ветлин Н. пришёл", "Ветлин", "Ветлин"),
        ("under Part A. Smith said", "Smith", "Smith"),
        // A judge's "J" after an English surname is a title.
        ("Smith J. R. held", "Smith", "Smith"),
        // A role word before the initials does not take them.
        ("Ответчик Н. О. Ветлин", "Ветлин", "Н. О. Ветлин"),
    ] {
        assert_eq!(
            fixed(source, &[(model, Category::Person)]),
            [expected],
            "{source}"
        );
    }
    // Two people side by side keep their own spans.
    assert_eq!(
        fixed(
            "Ветлин Н. О. Брюн Л. И.",
            &[("Ветлин", Category::Person), ("Брюн", Category::Person)]
        ),
        ["Ветлин Н. О.", "Брюн Л. И."]
    );
    // Initials the model gave apart join the surname after them, not a
    // role word before them.
    assert_eq!(
        fixed(
            "Ответчик Н. О. Ветлин",
            &[("Н. О.", Category::Person), ("Ветлин", Category::Person)]
        ),
        ["Н. О. Ветлин", "Н. О. Ветлин"]
    );
    assert_eq!(
        fixed("Истец Н. О.", &[("Н. О.", Category::Person)]),
        ["Н. О."]
    );
    // After the initials a role word is a surname.
    assert_eq!(
        fixed("Signed by J. R. Clerk.", &[("J. R.", Category::Person)]),
        ["J. R. Clerk"]
    );
    // An organization is not a person.
    assert_eq!(
        fixed("ИП Ветлин Н. О.", &[("ИП Ветлин", Category::Organization)]),
        ["ИП Ветлин"]
    );
}

/// A company's name in nested quotes is one span, from its legal form
/// to its last closing quote, however the model cut it: split at the
/// inner quote, on the inner name alone or short of the outer closing
/// quote; straight quotes too, the legal form spelled out in any case
/// form and letter case (guide rule 2, #557).
#[test]
fn a_name_in_nested_quotes_is_one_span() {
    for (source, model, expected) in [
        // The issue's synthetic reproduction, split at both inner quotes.
        (
            "Иск ООО «Правовая группа «Кедровый щит»» к обществу с ограниченной ответственностью \"Группа компаний \"Северный мост\"\".",
            &[
                "ООО «Правовая группа",
                "Кедровый щит",
                "обществу с ограниченной ответственностью",
                "Северный мост",
            ][..],
            &[
                "ООО «Правовая группа «Кедровый щит»»",
                "обществу с ограниченной ответственностью \"Группа компаний \"Северный мост\"\"",
            ][..],
        ),
        // The inner names alone.
        (
            "Иск ООО «Правовая группа «Кедровый щит»» к обществу с ограниченной ответственностью \"Группа компаний \"Северный мост\"\".",
            &["Кедровый щит", "Северный мост"][..],
            &[
                "ООО «Правовая группа «Кедровый щит»»",
                "обществу с ограниченной ответственностью \"Группа компаний \"Северный мост\"\"",
            ][..],
        ),
        // The legal form and the inner name, the middle words left out.
        (
            "Иск Общества с ограниченной ответственностью «Правовая группа «Кедровый щит»» удовлетворить.",
            &["Общества с ограниченной ответственностью", "Кедровый щит"][..],
            &["Общества с ограниченной ответственностью «Правовая группа «Кедровый щит»»"][..],
        ),
        // Split at the inner straight quote.
        (
            "Взыскать с общества с ограниченной ответственностью \"Группа компаний \"Северный мост\"\" долг.",
            &["Группа компаний", "Северный мост"][..],
            &["общества с ограниченной ответственностью \"Группа компаний \"Северный мост\"\""][..],
        ),
        // All capitals, the inner name alone.
        (
            "ОБЩЕСТВО С ОГРАНИЧЕННОЙ ОТВЕТСТВЕННОСТЬЮ \"ГРУППА КОМПАНИЙ \"СЕВЕРНЫЙ МОСТ\"\" обратилось в суд.",
            &["СЕВЕРНЫЙ МОСТ"][..],
            &["ОБЩЕСТВО С ОГРАНИЧЕННОЙ ОТВЕТСТВЕННОСТЬЮ \"ГРУППА КОМПАНИЙ \"СЕВЕРНЫЙ МОСТ\"\""][..],
        ),
        // The outer closing quote left outside.
        (
            "Истец ООО «Правовая группа «Кедровый щит»» обратился в суд.",
            &["ООО «Правовая группа «Кедровый щит»"][..],
            &["ООО «Правовая группа «Кедровый щит»»"][..],
        ),
        // A joint-stock company spelled out, and straight quotes inside
        // guillemets.
        (
            "Ответчик публичное акционерное общество «Лизинговая компания «Развитие»» и ООО «Группа \"Кедр\"».",
            &["Развитие", "Кедр"][..],
            &[
                "публичное акционерное общество «Лизинговая компания «Развитие»»",
                "ООО «Группа \"Кедр\"»",
            ][..],
        ),
        // Without a legal form, a split name is one inside its quotes.
        (
            "Лизингодатель «Лизинговая компания «Развитие»» передал предмет.",
            &["Лизинговая компания", "Развитие"][..],
            &["Лизинговая компания «Развитие»"][..],
        ),
        (
            "Лизингодатель «Лизинговая компания «Развитие»» передал предмет.",
            &["«Лизинговая компания «Развитие»"][..],
            &["«Лизинговая компания «Развитие»»"][..],
        ),
    ] {
        assert_eq!(proposed_orgs(source, model), expected, "{source}");
    }
}

/// Nested quotes join no more than one name (#557): two companies, a
/// defined term after one, a document's title, a quotation that
/// mentions a company, an inner name without a legal form, a quote left
/// open, a line between, quotes nested deeper and English prose stay
/// as they were.
#[test]
fn nested_quotes_join_no_more_than_one_name() {
    for (source, model, expected) in [
        (
            "Истец ООО «Кедр» и АО «Север» заключили договор.",
            &["Кедр", "Север"][..],
            &["ООО «Кедр»", "АО «Север»"][..],
        ),
        // The same companies in a quotation.
        (
            "Суд указал: «ООО «Кедр» и АО «Север»» — стороны договора.",
            &["Кедр", "Север"][..],
            &["ООО «Кедр»", "АО «Север»"][..],
        ),
        (
            "Суд указал: «ООО «Кедр» и АО «Север»» — стороны договора.",
            &["ООО «Кедр»", "АО «Север»"][..],
            &["ООО «Кедр»", "АО «Север»"][..],
        ),
        (
            "Истец ООО «Кедр» (далее — «Общество») обратился в суд.",
            &["Кедр"][..],
            &["ООО «Кедр»"][..],
        ),
        (
            "ООО «Кедр» заключило договор «О поставке «Север»» с АО «Ель».",
            &["Кедр", "Ель"][..],
            &["ООО «Кедр»", "АО «Ель»"][..],
        ),
        (
            "Суд указал: «долг ООО «Кедр»» погашен.",
            &["Кедр"][..],
            &["ООО «Кедр»"][..],
        ),
        (
            "Он работал в «Группе «Кедр»» три года.",
            &["Кедр"][..],
            &["Кедр"][..],
        ),
        (
            "Компания \"Кедр\" ответила \"нет\" на претензию.",
            &["Кедр"][..],
            &["Кедр"][..],
        ),
        // The outer quote never closes.
        (
            "Истец ООО «Правовая группа «Кедровый щит» обратился в суд.",
            &["Кедровый щит"][..],
            &["Кедровый щит"][..],
        ),
        // A line between the quotes.
        (
            "Истец ООО «Правовая группа
«Кедровый щит»» обратился в суд.",
            &["Кедровый щит"][..],
            &["Кедровый щит"][..],
        ),
        // Three levels.
        (
            "Истец ООО «Альфа «Бета «Кедр»»» обратился в суд.",
            &["Кедр"][..],
            &["Кедр"][..],
        ),
        (
            "He called it \"the \"Acme\" deal\" at Acme Ltd.",
            &["Acme"][..],
            &["Acme", "Acme"][..],
        ),
    ] {
        assert_eq!(proposed_orgs(source, model), expected, "{source}");
    }
}

/// Nested quotes that are prose, a generic legal form before a
/// quotation, markup, a table's cells and over-long names are left as
/// they were before #557 (the pre-gate review's P1 and P2 inputs):
/// spans of quoted prose are no pieces of one name, a quotation opens
/// in lowercase or holds a legal form, and a growth over markup, a cell
/// border, a person outside the inner name or 1000 bytes would leave the
/// review no span.
#[test]
fn nested_quotes_leave_prose_markup_and_tables_alone() {
    // Past the 1000 bytes a growth may take: the spans as they were.
    let many = format!("Он сказал: «{}«Север»» вчера.", "Кедр ".repeat(120));
    let mut model = vec![("organization", "Кедр"); 120];
    model.push(("organization", "Север"));
    let mut expected = vec!["ORG⟦Кедр⟧"; 120];
    expected.push("ORG⟦Север⟧");
    assert_eq!(proposed_marks(&many, &model), expected);
    let long = format!(
        "Истец ООО «{}компания «Кедр»» обратился.",
        "Строительная ".repeat(90)
    );
    assert_eq!(
        proposed_marks(&long, &[("organization", "Кедр")]),
        ["ORG⟦Кедр⟧"]
    );
    for (source, model, expected) in [
        // two-inner-noform
        (
            "Свидетель показал: «деньги перевели «Кедру» и «Северу»» вчера.",
            &[("organization", "Кедру"), ("organization", "Северу")][..],
            &["ORG⟦Кедру⟧", "ORG⟦Северу⟧"][..],
        ),
        // two-inner-noform-p
        (
            "Свидетель показал: «Иванов И. И. перевёл деньги «Кедру» и «Северу»» вчера.",
            &[
                ("person", "Иванов И. И."),
                ("organization", "Кедру"),
                ("organization", "Северу"),
            ][..],
            &["PERSON⟦Иванов И. И.⟧", "ORG⟦Кедру⟧", "ORG⟦Северу⟧"][..],
        ),
        // isk-ip
        (
            "Суд рассмотрел «иск ООО Кедр к ИП Петрову «Север»» и отказал.",
            &[
                ("organization", "ООО Кедр"),
                ("person", "ИП Петрову"),
                ("organization", "Север"),
            ][..],
            &["ORG⟦ООО Кедр⟧", "PERSON⟦Петрову⟧", "ORG⟦Север⟧"][..],
        ),
        // short-quote-2orgs
        (
            "Пункт 2.1 договора гласит: «Продавец Кедр передаёт товар, гарантом выступает «Север»». Суд согласился.",
            &[("organization", "Кедр"), ("organization", "Север")][..],
            &["ORG⟦Кедр⟧", "ORG⟦Север⟧"][..],
        ),
        // long-quote-2orgs
        (
            "Пункт 2.1 договора гласит: «Продавец Кедр поставщик обязуется передать товар в собственность покупателя в сроки, установленные спецификацией, а покупатель обязуется принять и оплатить товар поставщик обязуется передать товар в собственность покупателя в сроки, установленные спецификацией, а покупатель обязуется принять и оплатить товар поставщик обязуется передать товар в собственность покупателя в сроки, установленные спецификацией, а покупатель обязуется принять и оплатить товар, при этом гарантом выступает «Север»». Суд согласился.",
            &[("organization", "Кедр"), ("organization", "Север")][..],
            &["ORG⟦Кедр⟧", "ORG⟦Север⟧"][..],
        ),
        // bold-inner
        (
            "Истец ООО «Правовая группа «**Кедр**»» обратился.",
            &[("organization", "Кедр")][..],
            &["ORG⟦Кедр⟧"][..],
        ),
        // link-inner
        (
            "Истец ООО «Правовая группа «[Кедр](https://kedr.example)»» обратился.",
            &[("organization", "Кедр")][..],
            &["ORG⟦Кедр⟧", "URL⟦https://kedr.example⟧"][..],
        ),
        // link-nourl
        (
            "Истец ООО «Правовая группа «[Кедр](/company/123)»» обратился.",
            &[("organization", "Кедр")][..],
            &["ORG⟦Кедр⟧"][..],
        ),
        // html
        (
            "Истец ООО «Правовая группа «<b>Кедр</b>»» обратился.",
            &[("organization", "Кедр")][..],
            &["ORG⟦Кедр⟧"][..],
        ),
        // cells-split
        (
            "| Истец: ООО «Правовая группа | «Кедровый щит»» |",
            &[
                ("organization", "Правовая группа"),
                ("organization", "Кедровый щит"),
            ][..],
            &["ORG⟦Правовая группа⟧", "ORG⟦Кедровый щит⟧"][..],
        ),
        // cells-split2
        (
            "| ООО «Правовая группа | «Кедровый щит»» |",
            &[("organization", "Кедровый щит")][..],
            &["ORG⟦Кедровый щит⟧"][..],
        ),
        // t3-pipes-oneorg
        (
            "| ООО \"Кедр | 2024 | \"Север\"\" |",
            &[("organization", "Север")][..],
            &["ORG⟦Север⟧"][..],
        ),
        // t3-pipes-noperson
        (
            "| ООО \"Кедр | Иванов | \"Север\"\" |",
            &[("organization", "Кедр"), ("organization", "Север")][..],
            &["ORG⟦Север⟧"][..],
        ),
        // md-table
        (
            "| ООО «Кедр «Юг» | Петров П. П. | АО «Север»» |",
            &[
                ("organization", "Кедр"),
                ("person", "Петров П. П."),
                ("organization", "Север"),
            ][..],
            &["ORG⟦Кедр⟧", "PERSON⟦Петров П. П.⟧", "ORG⟦АО «Север»⟧"][..],
        ),
        // md-table2
        (
            "| ООО \"Кедр | Петров П. П. | АО \"Север\"\" |",
            &[
                ("organization", "Кедр"),
                ("person", "Петров П. П."),
                ("organization", "Север"),
            ][..],
            &["PERSON⟦Петров П. П.⟧", "ORG⟦Север⟧"][..],
        ),
        // md-table3
        (
            "| ООО \"Кедр | Петров П. П. | \"Север\"\" |",
            &[
                ("organization", "Кедр"),
                ("person", "Петров П. П."),
                ("organization", "Север"),
            ][..],
            &["PERSON⟦Петров П. П.⟧", "ORG⟦Север⟧"][..],
        ),
        // t3-nopipes
        (
            "ООО \"Кедр, Петров П. П., \"Север\"\" и далее.",
            &[
                ("organization", "Кедр"),
                ("person", "Петров П. П."),
                ("organization", "Север"),
            ][..],
            &["ORG⟦ООО \"Кедр, Петров П. П., \"⟧", "ORG⟦Север⟧"][..],
        ),
        // ao-generic
        (
            "Ответчик полагает, что акционерное общество \"не вправе требовать оплаты по договору \"Север\"\".",
            &[("organization", "Север")][..],
            &["ORG⟦Север⟧"][..],
        ),
        // ao-generic-p
        (
            "Суд отметил, что акционерное общество «обязано выплатить Иванову И. И. премию «Лучший работник»» в срок.",
            &[
                ("person", "Иванову И. И."),
                ("organization", "Лучший работник"),
            ][..],
            &["PERSON⟦Иванову И. И.⟧", "ORG⟦Лучший работник⟧"][..],
        ),
        // ooo-generic-quote
        (
            "Истец утверждает, что ООО «не исполнило обязательств перед ИП Петровым П. П. по договору «Кедр»».",
            &[("person", "Петровым П. П."), ("organization", "Кедр")][..],
            &["PERSON⟦Петровым П. П.⟧", "ORG⟦Кедр⟧"][..],
        ),
        // ao-form-generic2
        (
            "Акционерное общество «Север» и акционерное общество «обязано, как сказано в «Положении»» платить.",
            &[("organization", "Север"), ("organization", "Положении")][..],
            &["ORG⟦Север⟧", "ORG⟦Положении⟧"][..],
        ),
    ] {
        assert_eq!(proposed_marks(source, model), expected, "{source}");
    }
}

/// A long name in nested quotes is still one span, up to the 1000
/// bytes a growth may take; two quoted words with nothing between their
/// quotes are one name without a legal form too, and a person in the
/// last inner pair is part of the company's name (guide rule 2, #557,
/// as «Фирма «Петров П. П.»»).
#[test]
fn nested_quotes_join_long_names_and_names_inside() {
    let source = "Истец Общество с ограниченной ответственностью «Научно-производственное предприятие «Специальные технологии и оборудование для нефтегазовой промышленности Западной Сибири»» обратился.";
    let name = source
        .strip_prefix("Истец ")
        .and_then(|rest| rest.strip_suffix(" обратился."))
        .unwrap();
    assert!(name.len() > 300);
    assert_eq!(
        proposed_marks(
            source,
            &[(
                "organization",
                "Специальные технологии и оборудование для нефтегазовой промышленности Западной Сибири"
            )]
        ),
        [format!("ORG⟦{name}⟧")]
    );
    assert_eq!(
        proposed_marks(
            "ООО «Кедр» заключило договор «Север «Плюс»» с АО «Ель».",
            &[
                ("organization", "Кедр"),
                ("organization", "Север"),
                ("organization", "Плюс"),
                ("organization", "Ель"),
            ]
        ),
        ["ORG⟦ООО «Кедр»⟧", "ORG⟦Север «Плюс»⟧", "ORG⟦АО «Ель»⟧"]
    );
    for person in ["Петров", "ИП Петров"] {
        assert_eq!(
            proposed_marks(
                "Истец ООО «Кедр «ИП Петров»» обратился.",
                &[("organization", "Кедр"), ("person", person)]
            ),
            ["ORG⟦ООО «Кедр «ИП Петров»»⟧"],
            "{person}"
        );
    }
}

#[test]
fn list_letters_come_off_names_only_at_list_breaks() {
    let source = "a. Penrow represents and warrants; b. Brannock will attend; \
        d. Quillon Media Inc agrees to defend.\n(c) Ostrand replied.\ni. Larkfield replied.";
    assert_eq!(
        shown(
            source,
            &[
                ("a. Penrow", "person"),
                ("b. Brannock", "person"),
                ("d. Quillon Media Inc", "organization"),
                ("(c) Ostrand", "person"),
                ("i. Larkfield", "person")
            ]
        ),
        pairs(&[
            ("Penrow", "PERSON"),
            ("Brannock", "PERSON"),
            ("Quillon Media Inc", "ORG"),
            ("Ostrand", "PERSON"),
            ("Larkfield", "PERSON")
        ])
    );
    assert_eq!(
        shown(
            "A. Penrow met J. R. Ostrand and van Hoorn.",
            &[
                ("A. Penrow", "person"),
                ("J. R. Ostrand", "person"),
                ("van Hoorn", "person")
            ]
        ),
        pairs(&[
            ("A. Penrow", "PERSON"),
            ("J. R. Ostrand", "PERSON"),
            ("van Hoorn", "PERSON")
        ])
    );
    assert_eq!(
        fixed(
            "Met a. Penrow yesterday.",
            &[("a. Penrow", Category::Person)]
        ),
        ["a. Penrow"]
    );
    assert_eq!(
        fixed("  a. Penrow  ", &[("  a. Penrow  ", Category::Person)]),
        ["Penrow"]
    );
    for ending in [".", "!", "?", "…"] {
        let source = format!("The deed was signed{ending} j. Penrow witnessed it.");
        assert_eq!(
            shown(&source, &[("j. Penrow", "person")]),
            pairs(&[("j. Penrow", "PERSON")]),
            "{source}"
        );
    }
    for marker in ["ii.", "(iv)", "a)", "viii."] {
        for before in ["", "Attending: ", "First; ", "Signed.\n"] {
            for (name, label, category) in [
                ("Penrow", "person", "PERSON"),
                ("Quillon Media Inc", "organization", "ORG"),
            ] {
                let span = format!("{marker} {name}");
                let source = format!("{before}{span} replied.");
                assert_eq!(
                    shown(&source, &[(&span, label)]),
                    pairs(&[(name, category)])
                );
            }
        }
    }
}

#[test]
fn of_inside_an_organization_name_does_not_separate_a_role() {
    for company in [
        "Notary Public of America, Inc",
        "Notary Public of Quillon Ltd",
    ] {
        assert_eq!(
            shown(company, &[(company, "organization")]),
            pairs(&[(company, "ORG")]),
            "{company}"
        );
    }
}

#[test]
fn organization_roles_need_separators_and_a_name_remainder() {
    for company in [
        "Notary Public Underwriters, Inc",
        "Team Leader Brannock Services Ltd",
        "конкурсного управляющего ООО «Листолёт»",
        "Notary Public, LLC",
    ] {
        assert_eq!(
            shown(company, &[(company, "organization")]),
            pairs(&[(company, "ORG")]),
            "{company}"
        );
    }
    for form in [
        "Inc", "LLC", "Ltd", "Corp", "Co.", "PLC", "LLP", "ООО", "АО",
    ] {
        let company = format!("Notary Public Underwriters, {form}");
        let expected = company.trim_end_matches('.');
        assert_eq!(
            shown(&company, &[(&company, "organization")]),
            pairs(&[(expected, "ORG")]),
            "{company}"
        );
    }
    for (source, name) in [
        ("Trustee for Quillon Ltd", "Quillon Ltd"),
        (
            "Trustee for the Brannock Family Trust",
            "Brannock Family Trust",
        ),
        ("Chief Investigator, Quillon Ltd", "Quillon Ltd"),
        ("Notary Public — Quillon Ltd", "Quillon Ltd"),
        ("Notary Public - Quillon Ltd", "Quillon Ltd"),
        ("Notary Public: Quillon Ltd", "Quillon Ltd"),
        ("Quillon Ltd, Notary Public", "Quillon Ltd"),
        ("Quillon Ltd — Notary Public", "Quillon Ltd"),
    ] {
        assert_eq!(
            shown(source, &[(source, "organization")]),
            pairs(&[(name, "ORG")]),
            "{source}"
        );
    }
}

#[test]
fn comma_after_leading_person_role_comes_off_with_the_role() {
    for source in [
        "Chief Investigator, John Smith",
        "Chief Investigator,John Smith",
        "Team Leader, John Smith",
        "Заместитель руководителя, Иван Петров",
    ] {
        let name = source.split_once(',').unwrap().1.trim_start();
        assert_eq!(
            shown(source, &[(source, "person")]),
            pairs(&[(name, "PERSON")]),
            "{source}"
        );
    }
}

/// #489: a no-break space read through a wrong Latin-1/UTF-8 round trip is
/// "Â" glued to the word before it; it comes off a person's or a company's
/// end, with a possessive and what follows. "Â" opening a word is a letter,
/// and one before another name word is the space between names.
#[test]
fn a_misdecoded_a_circumflex_comes_off_a_name() {
    for (source, model, label, expected, category) in [
        (
            "RosalindÂ came.",
            "RosalindÂ",
            "person",
            "Rosalind",
            "PERSON",
        ),
        (
            "Dr. OstrovaÂ’s labÂ is open.",
            "OstrovaÂ",
            "person",
            "Ostrova",
            "PERSON",
        ),
        (
            "Dr. OstrovaÂ’s labÂ is open.",
            "OstrovaÂ’s labÂ",
            "person",
            "Ostrova",
            "PERSON",
        ),
        (
            "Quillon FreightÂ ships.",
            "Quillon FreightÂ",
            "organization",
            "Quillon Freight",
            "ORG",
        ),
        (
            "Âgé Ostrova came.",
            "Âgé Ostrova",
            "person",
            "Âgé Ostrova",
            "PERSON",
        ),
        (
            "RosalindÂ Ostrova came.",
            "RosalindÂ Ostrova",
            "person",
            "RosalindÂ Ostrova",
            "PERSON",
        ),
    ] {
        assert_eq!(
            shown(source, &[(model, label)]),
            pairs(&[(expected, category)]),
            "{model}"
        );
    }
}
