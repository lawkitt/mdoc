//! People: initials, particles, surname-first names and entrepreneurs.
use super::*;
use crate::rules::load::Form;
use crate::rules::test_support::{fixed, pairs, proposed_marks, shown, shown_at};

/// #403: a surname's lowercase particle joins the model's surname, in
/// Russian and English, chained, glued, with the given names, initials
/// or a patronymic around it, never across a line; a particle that is
/// also a word («да», «фон», "van", "ten") only after a name's word, a
/// title or an introducing word.
#[test]
fn a_surnames_particle_joins_the_name() {
    for (source, model, expected) in [
        (
            "В журнале выдачи заказ записан на фамилию де Лаваль.",
            &["Лаваль"][..],
            &["де Лаваль"][..],
        ),
        (
            "Я, Генриетта Яновна де Лаваль, уполномочиваю представителя.",
            &["Лаваль"],
            &["Генриетта Яновна де Лаваль"],
        ),
        (
            "Я, Генриетта Яновна де Лаваль, уполномочиваю представителя.",
            &["Генриетта Яновна", "Лаваль"],
            &["Генриетта Яновна де Лаваль"],
        ),
        (
            "Решение согласовано с Генриеттой Яновной де Лаваль отдельно.",
            &["Лаваль"],
            &["Генриеттой Яновной де Лаваль"],
        ),
        (
            "Иск подан против де Лаваль Генриетты Яновны в мае.",
            &["Лаваль Генриетты Яновны"],
            &["де Лаваль Генриетты Яновны"],
        ),
        (
            "Опись передаётся Г. Я. де Лаваль лично.",
            &["Лаваль"],
            &["Г. Я. де Лаваль"],
        ),
        (
            "Договор подписал барон фон дер Пален.",
            &["Пален"],
            &["фон дер Пален"],
        ),
        ("Ответчик ван Хорн возражал.", &["Хорн"], &["ван Хорн"]),
        (
            "Доверенность выдана гражданке да Силва.",
            &["Силва"],
            &["да Силва"],
        ),
        ("Передать О. фон Пален лично.", &["Пален"], &["фон Пален"]),
        (
            "Явился мушкетёр д'Артаньян.",
            &["Артаньян"],
            &["д'Артаньян"],
        ),
        ("Свидетель аль-Хадид показал.", &["Хадид"], &["аль-Хадид"]),
        (
            "The deed was signed by van der Berg today.",
            &["Berg"],
            &["van der Berg"],
        ),
        ("Dear Ms de Vries, thank you.", &["Vries"], &["de Vries"]),
        (
            "Witness: J. R. van der Berg, notary.",
            &["Berg"],
            &["J. R. van der Berg"],
        ),
        ("Dear Ms ten Brink, thank you.", &["Brink"], &["ten Brink"]),
        ("Ms von Stein signed.", &["Stein"], &["von Stein"]),
        ("Ms Leonor de la Cruz attended.", &["Cruz"], &["de la Cruz"]),
        (
            "The letter came from Valéry Giscard d'Estaing himself.",
            &["Estaing"],
            &["d'Estaing"],
        ),
        // Written lowercase, a particle joins at a line's or a
        // sentence's start too.
        ("де Лаваль подписала опись.", &["Лаваль"], &["де Лаваль"]),
        (
            "Опись подписана. де Лаваль согласна.",
            &["Лаваль"],
            &["де Лаваль"],
        ),
        ("van der Berg signed.", &["Berg"], &["van der Berg"]),
        // A particle that is also a word stays out after a lowercase
        // word or a line's break, and no particle joins across a line.
        ("Он сказал да Петрову.", &["Петрову"], &["Петрову"]),
        (
            "Всё это создаёт фон Петровой.",
            &["Петровой"],
            &["Петровой"],
        ),
        (
            "Всё это создаёт\nфон Петровой.",
            &["Петровой"],
            &["Петровой"],
        ),
        ("Заказ на фамилию де\nЛаваль.", &["Лаваль"], &["Лаваль"]),
        ("The van Smith rented broke down.", &["Smith"], &["Smith"]),
        (
            "The delivery van Smith rented broke down.",
            &["Smith"],
            &["Smith"],
        ),
        ("It took ten Smith employees.", &["Smith"], &["Smith"]),
        // Nor after a bare capitalised word (#403 review): «да» joins
        // two names, «фон» is a noun, "von" a German preposition.
        (
            "Пришли Петров да Сидоров.",
            &["Петров", "Сидоров"],
            &["Петров", "Сидоров"],
        ),
        (
            "Сказка про Ивана да Марью.",
            &["Ивана", "Марью"],
            &["Ивана", "Марью"],
        ),
        ("Шумовой фон Петрова не смущал.", &["Петрова"], &["Петрова"]),
        ("Отто фон Пален прибыл.", &["Пален"], &["Пален"]),
        (
            "Das Schreiben von Schmidt liegt vor.",
            &["Schmidt"],
            &["Schmidt"],
        ),
        (
            "The claim was heard by Hans ten Brink yesterday.",
            &["Brink"],
            &["Brink"],
        ),
    ] {
        assert_eq!(proposed_people(source, model), expected, "{source:?}");
    }
}

/// The person proposals for the model's `people` spans in `source`,
/// each found after the one before it, as the user would be shown them.
fn proposed_people_in_order(source: &str, people: &[&str]) -> Vec<String> {
    let model: Vec<_> = people.iter().map(|text| ("person", *text)).collect();
    proposed_marks(source, &model)
        .into_iter()
        .filter_map(|mark| Some(mark.strip_prefix("PERSON⟦")?.strip_suffix('⟧')?.to_owned()))
        .collect()
}

/// #414: a surname-first name is one span, comma included, whether the
/// model gave the surname alone or the surname and the rest apart: with
/// initials after the comma anywhere, with a given name only where a
/// name stands alone (a line's start, a label, a table's cell).
#[test]
fn a_surname_first_name_is_one_span() {
    let references = "References: Penrow, J. L. 1971. Ostrand, D. J., and \
                      Brannock, E. E. 1969. Draycott,D.P. 1970.";
    let whole = [
        "Penrow, J. L.",
        "Ostrand, D. J.",
        "Brannock, E. E.",
        "Draycott,D.P.",
    ];
    for (source, model, expected) in [
        // The surname alone, the initials left out.
        (
            references,
            &["Penrow", "Ostrand", "Brannock", "Draycott"][..],
            &whole[..],
        ),
        // Split in two spans at the comma.
        (
            references,
            &[
                "Penrow", "J. L.", "Ostrand", "D. J.", "Brannock", "E. E.", "Draycott", "D.P.",
            ],
            &whole,
        ),
        (
            "Employee: VALECROFT, Jorin P.\nFor the employer: Siora Dennick",
            &["VALECROFT", "Jorin P.", "Siora Dennick"],
            &["VALECROFT, Jorin P.", "Siora Dennick"],
        ),
        (
            "**Employee:** VALECROFT, Jorin P.\n",
            &["VALECROFT"],
            &["VALECROFT, Jorin P."],
        ),
        (
            "Clark, R. L. 1970. Soils.\nHerder, Mike. 1999. Field notes.\n",
            &["Clark", "Herder"],
            &["Clark, R. L.", "Herder, Mike"],
        ),
        (
            "| Author | Year |\n|---|---|\n| Alexander, L. J. | 1971 |\n",
            &["Alexander", "L. J."],
            &["Alexander, L. J."],
        ),
        (
            "Cited: Hardy, T. B. (1968); Schumann, P. B. et al. 1970.",
            &["Hardy", "Schumann"],
            &["Hardy, T. B.", "Schumann, P. B."],
        ),
        // "and" is never crossed: two people (#489).
        (
            "Morvilius, M. and R. Fridell. 1968. Notes.",
            &["Morvilius", "R. Fridell"],
            &["Morvilius, M.", "R. Fridell"],
        ),
        (
            "Литература: Иванов, И. И. 1999. Петров,П.П. 2001.",
            &["Иванов", "Петров"],
            &["Иванов, И. И.", "Петров,П.П."],
        ),
        (
            "Литература: Иванов, И. И. 1999. Петров,П.П. 2001.",
            &["Иванов", "И. И.", "Петров", "П.П."],
            &["Иванов, И. И.", "Петров,П.П."],
        ),
        (
            "Авторы: Иванов, И. И. и Петров, П. П., 1999.",
            &["Иванов", "Петров"],
            &["Иванов, И. И.", "Петров, П. П."],
        ),
        (
            "- Истец, фамилия, имя и отчество: Маркович, Савелий Игоревич.\n",
            &["Маркович", "Савелий Игоревич"],
            &["Маркович, Савелий Игоревич"],
        ),
        (
            "1. Маркович, Савелий Игоревич; 2. Ветлина, Анна Олеговна.",
            &["Маркович", "Ветлина"],
            &["Маркович, Савелий Игоревич", "Ветлина, Анна Олеговна"],
        ),
        // #414 review (P3-2): one "J." before a reference's year.
        ("Brown, J. (2020). Title.", &["Brown"], &["Brown, J."]),
        ("Smith, J. 2020. Title.", &["Smith"], &["Smith, J."]),
        // A given name with a middle initial; a given name alone
        // before a reference's year in brackets.
        (
            "Employee: Smith, John P.\n",
            &["Smith"],
            &["Smith, John P."],
        ),
        (
            "Herder, Mike (1999). Field notes.",
            &["Herder"],
            &["Herder, Mike"],
        ),
        // Spaced initials are an author's, glued post-nominals are not.
        (
            "Alexander, L. J. 1971.",
            &["Alexander"],
            &["Alexander, L. J."],
        ),
        (
            "Present: Smith, A. B., C. D. Brown\n",
            &["Smith", "C. D. Brown"],
            &["Smith, A. B.", "C. D. Brown"],
        ),
        (
            "Свидетели: Иванов, Пётр Петрович; Петров, Иван Иванович.\n",
            &["Иванов", "Пётр Петрович", "Петров", "Иван Иванович"],
            &["Иванов, Пётр Петрович", "Петров, Иван Иванович"],
        ),
    ] {
        assert_eq!(
            proposed_people_in_order(source, model),
            expected,
            "{source:?} {model:?}"
        );
    }
}

/// #414 guards: mid-prose, a list of surnames, a place, a judge's or
/// a lawyer's post-nominal, a role, the next person's initials, a line
/// or a word before the comma that is no tagged surname keep a
/// surname-first growth off: the model's spans stay as they are.
#[test]
fn a_surname_first_name_needs_its_context() {
    for (source, model) in [
        // A given name needs a name's own place, never mid-prose.
        ("I met Smith, John said hello.", &["Smith"][..]),
        ("I met Smith, John said hello.", &["Smith", "John"]),
        // A list of surnames.
        (
            "Smith, Jones and Brown appeared.",
            &["Smith", "Jones", "Brown"],
        ),
        ("Smith, Jones, Brown\n", &["Smith", "Jones", "Brown"]),
        ("Smith, Jones & Co\n", &["Smith"]),
        // A city's country or state.
        ("Paris, France\n", &["Paris"]),
        ("Albany, New York\n", &["Albany"]),
        ("Austin, Texas; Reno, Nevada", &["Austin", "Reno"]),
        ("Washington, D.C. 20001", &["Washington"]),
        ("Offices: Albany, N.Y. 12207", &["Albany"]),
        // A judge's post-nominal is a title (#730).
        ("Brown, J., concurring.", &["Brown"]),
        ("Roberts, C.J., delivered the opinion.", &["Roberts"]),
        // Initials before a capitalised word are its person's.
        ("Smith, J. R. Brown signed.", &["Smith", "J. R. Brown"]),
        ("Ветлин, Н. О. Иванов", &["Ветлин", "Н. О. Иванов"]),
        ("On June 5, J. Smith signed.", &["J. Smith"]),
        // Never across a line.
        ("Penrow,\nJ. L. 1971.", &["Penrow"]),
        (
            "Employee: VALECROFT,\nJorin P.\n",
            &["VALECROFT", "Jorin P."],
        ),
        (
            "Истец: Маркович,\nСавелий Игоревич.",
            &["Маркович", "Савелий Игоревич"],
        ),
        // Mid-prose in Russian too.
        (
            "Вызвали Петрова, Ивана Ивановича допросили.",
            &["Петрова", "Ивана Ивановича"],
        ),
        ("Город: Москва, Р. Ф.\n", &["Москва"]),
        // A company's form, a greeting or a sentence's adverb before
        // the comma is no surname.
        ("Acme Inc., J. R. 1971.", &["J. R."]),
        ("Thanks, J. R.\n", &["J. R."]),
        ("However, J. R., the witness, left.", &["J. R."]),
        // #414 review (P1-1): two surnames, a citation's two authors.
        ("Smith, Jones\n", &["Smith", "Jones"][..]),
        ("Witnesses: Harlow, Brennan\n", &["Harlow", "Brennan"][..]),
        (
            "Present: Harlow, Brennan; Oakes, Pell.\n",
            &["Harlow", "Brennan", "Oakes", "Pell"][..],
        ),
        ("| Harlow, Brennan |\n", &["Harlow", "Brennan"][..]),
        (
            "(Brown, 2001; Smith, Jones, 1999)",
            &["Brown", "Smith", "Jones"][..],
        ),
        (
            "Smith, A. B. Jones, C. D. Brown\n",
            &["Smith", "A. B. Jones", "C. D. Brown"][..],
        ),
        // P2-2: a judge's or a lawyer's post-nominal, glued, is no initials.
        ("Smith, J., dissenting.", &["Smith"][..]),
        ("Roe, C.J., concurring.", &["Roe"][..]),
        ("Denning, L.J., said the rule was plain.", &["Denning"][..]),
        ("Harlow, P.J., concurring.", &["Harlow"][..]),
        ("Harlow, J.A., for the court.", &["Harlow"][..]),
        ("HARLOW, J.S.C.\n", &["HARLOW"][..]),
        ("Harlow, A.J., dissenting.", &["Harlow"][..]),
        ("Denning, M.R., in Harlow v Pell.", &["Denning"][..]),
        ("Harlow, Q.C., for the claimant.", &["Harlow"][..]),
        ("Harlow, K.C., for the claimant.", &["Harlow"][..]),
        ("Harlow, M.D., testified.", &["Harlow"][..]),
        ("Harlow, J.D.\n", &["Harlow"][..]),
        ("Harlow, S.J., Senior Judge.", &["Harlow"][..]),
        ("Harlow, D.J.\n", &["Harlow"][..]),
        ("Harlow and Pell, JJ., dissenting.", &["Harlow", "Pell"][..]),
        ("Roe, C. J., concurring.", &["Roe"][..]),
        ("Doe, Inc. filed.", &["Doe"][..]),
        ("Harlow, L.L.P.\n", &["Harlow"][..]),
        ("Harlow, P.A.\n", &["Harlow"][..]),
        ("Harlow, P.C., counsel.", &["Harlow"][..]),
        ("HARLOW, P.J.\n", &["HARLOW"][..]),
        // P2-4: a place's state, province or country.
        ("Springfield, IL\n", &["Springfield"][..]),
        ("Paris, TX 75460", &["Paris"][..]),
        ("London, UK\n", &["London"][..]),
        ("Moscow, RU\n", &["Moscow"][..]),
        ("Washington, DC 20001", &["Washington"][..]),
        ("Newark, N.J. 07102", &["Newark"][..]),
        ("Wilson, N.C. 27893", &["Wilson"][..]),
        ("Florence, S.C.\n", &["Florence"][..]),
        ("Victoria, B.C.\n", &["Victoria"][..]),
        ("Darwin, N.T. 0800", &["Darwin"][..]),
        ("Hamilton, Ontario\n", &["Hamilton"][..]),
        ("Seattle, Washington\n", &["Seattle"][..]),
        ("Halifax, Nova Scotia\n", &["Halifax"][..]),
        ("Address: Limassol, Cyprus\n", &["Limassol"][..]),
        ("Vancouver, British Columbia\n", &["Vancouver"][..]),
        ("Seoul, Korea\n", &["Seoul"][..]),
        ("Washington, D.C.\n", &["Washington"][..]),
        ("Jackson, Mississippi\n", &["Jackson"][..]),
        // P2-1: with the initials alone tagged, the word before the comma
        // never joins them.
        ("On Monday, J. Smith left.", &["J. Smith"][..]),
        ("On Monday, A. B. and C. D. met.", &["A. B.", "C. D."][..]),
        ("In January, A. B. and C. D. met.", &["A. B.", "C. D."][..]),
        ("In Paris, A. B. and C. D. met.", &["A. B.", "C. D."][..]),
        ("Thereafter, J.R., a minor, moved.", &["J.R."][..]),
        ("Later, A. B., the witness, left.", &["A. B."][..]),
        ("Respectfully, A. B.\n", &["A. B."][..]),
        ("Signed, A. B.\n", &["A. B."][..]),
        ("Yours, A. B.\n", &["A. B."][..]),
        ("Indeed, A. B., agreed.", &["A. B."][..]),
        ("Signed: Harlow, June 5, 2020\n", &["Harlow"][..]),
        ("Dear Mr Harlow,\n", &["Harlow"][..]),
        ("Однако, И. И., возразил.", &["И. И."][..]),
        ("Спасибо, И. И.\n", &["И. И."][..]),
        ("Судья, И. И.\n", &["И. И."][..]),
        ("ООО, И. И.\n", &["И. И."][..]),
        ("See Penrow, J. L. 5 times.", &["Penrow"][..]),
        // P1-1: a role, a suffix or a given name alone after the comma.
        ("Plaintiff, J. Doe, appeared.", &["J. Doe"][..]),
        ("Plaintiff, A. B., appeared.", &["A. B."][..]),
        ("Defendant, Mr. Smith, appeared.", &["Smith"][..]),
        ("By: Harlow, President\n", &["Harlow"][..]),
        (
            "Attendees: Harlow, Director; Pell, Manager.\n",
            &["Harlow", "Pell"][..],
        ),
        ("cc: Harlow, Esq.\n", &["Harlow"][..]),
        ("Re: Harlow, Deceased\n", &["Harlow"][..]),
        ("Signed: Harlow, Trustee\n", &["Harlow"][..]),
        ("Witness, A. B.\n", &["A. B."][..]),
        ("Signed: Harlow, Jr.\n", &["Harlow"][..]),
        ("| Harlow, Partner |\n", &["Harlow"][..]),
        ("| Smith, John |\n", &["Smith"][..]),
        ("| Smith, John |\n", &["Smith", "John"][..]),
        ("Author: Smith, John\n", &["Smith"][..]),
        ("Re: Smith, John\n", &["Smith"][..]),
        ("Alexander, L. J. 1971.", &["L. J."][..]),
        ("Employee: VALECROFT, Jorin P.\n", &["Jorin P."][..]),
        ("Witnesses: HARLOW, BRENNAN\n", &["HARLOW", "BRENNAN"][..]),
        // The review's real-model run: an opinion's judges and a
        // minor's initials.
        (
            "HARLOW, P.J., delivered the opinion of the court. Brennan, J.A., \
             and Oakes, A.J., concurred.",
            &["HARLOW", "Brennan", "Oakes"],
        ),
        (
            "Thereafter, J.R., a minor, moved to Newark, N.J. 07102 with his \
             mother. In January, J.R. and D.M. testified. Later, J.R., the \
             child, returned.",
            &["J.R.", "J.R.", "D.M.", "J.R."],
        ),
    ] {
        assert_eq!(proposed_people_in_order(source, model), model, "{source:?}");
    }
    // An all-caps signature's growth (#387) takes no `unless_over`
    // word after the model's span: only a `tail` match checks there.
    assert_eq!(
        proposed_people_in_order("JOHN Q. STATES, Magistrate\n", &["JOHN"]),
        ["JOHN Q. STATES"]
    );
}

/// #403 review: a glued particle keeps a surname, but "al-Qaeda" and
/// «аль-Каида», in any case form, are no person.
#[test]
fn al_qaeda_is_no_person() {
    for (source, model) in [
        ("Funds went to al-Qaeda last year.", "al-Qaeda"),
        ("Средства ушли аль-Каиде в прошлом году.", "аль-Каиде"),
        ("Член аль-Каиды задержан.", "аль-Каиды"),
    ] {
        assert!(proposed_people(source, &[model]).is_empty(), "{source:?}");
    }
    assert_eq!(
        proposed_people("Свидетель аль-Хадид показал.", &["аль-Хадид"]),
        ["аль-Хадид"]
    );
}

/// #403: a surname with its particle repeats with it, inflected too.
#[test]
fn a_surnames_particle_repeats_with_it() {
    let source = "Я, Генриетта Яновна де Лаваль, уполномочиваю. Решение \
        согласовано с Генриеттой Яновной де Лаваль. Заказ записан на фамилию \
        де Лаваль.";
    assert_eq!(
        proposed_people(source, &["Лаваль"]),
        [
            "Генриетта Яновна де Лаваль",
            "Генриеттой Яновной де Лаваль",
            "де Лаваль"
        ]
    );
}

/// #404: a short two-part name after a word that introduces a person
/// is PERSON with no model span; a country after «гражданин», an office
/// after «господин» or "Mr", and a longer name are not this rule's.
#[test]
fn a_short_name_after_an_introducing_word_is_a_person() {
    for (source, expected) in [
        (
            "Я уполномочиваю гражданина Лю Бо получить экземпляры.",
            &["Лю Бо"][..],
        ),
        ("Передать гражданке Ким Ир, проживающей рядом.", &["Ким Ир"]),
        ("Направить господину Ли Вэй.", &["Ли Вэй"]),
        ("I authorise Mr Li Na to collect the copies.", &["Li Na"]),
        ("Ms Ko Un, the agent, signed.", &["Ko Un"]),
        ("паспорт гражданина Российской Федерации серии 45", &[]),
        ("гражданки Республики Казахстан, проживающей", &[]),
        ("гражданина Украины Петренко, проживающего", &[]),
        ("Направить господину Председателю Суда.", &[]),
        ("Mr Justice Ashdale held that the claim failed.", &[]),
        // Words of five letters or more are no short name (#404
        // review): a country, an office, a form's heading, a suffix.
        ("Он был гражданином Советского Союза.", &[]),
        ("гражданке Канады Браун, проживающей", &[]),
        ("Гражданин Петров Подпись", &[]),
        ("Направить господину Мэру Москвы.", &[]),
        ("Mr Smith Said: no.", &[]),
        ("Ms Jones Counsel, for the claimant", &[]),
        ("Mr Inspector Morse arrived.", &[]),
        // Short suffixes are skipped.
        ("Mr Bo Esq, for the claimant", &[]),
        ("Mr Li Jr, the son", &[]),
        ("Ms Li QC, for the claimant", &[]),
    ] {
        assert_eq!(proposed_people(source, &[]), expected, "{source:?}");
    }
    // Only a gap: a model span over the match keeps its own wording,
    // and a wrong word beside it never spreads (#404 review).
    assert_eq!(
        proposed_people(
            "Договор заключён с гражданином Великобритании Смитом.",
            &["Смитом"]
        ),
        ["Смитом"]
    );
    assert_eq!(
        proposed_people("Я уполномочиваю гражданина Лю Бо.", &["Бо"]),
        ["Бо"]
    );
    assert_eq!(
        proposed_people("I authorise Mr Li Na to act.", &["Li Na"]),
        ["Li Na"]
    );
    // A longer name is left to the model: the rule takes no part of it.
    let source = "уполномочиваю гражданина Иванова Ивана Ивановича получить";
    assert!(proposed_people(source, &[]).is_empty(), "{source:?}");
    let source = "I authorise Mr John Paul Smith to collect.";
    assert!(proposed_people(source, &[]).is_empty(), "{source:?}");
}

/// The person proposals for the model's `people` spans in `source`
/// (each its first occurrence), as the user would be shown them.
fn proposed_people(source: &str, people: &[&str]) -> Vec<String> {
    let raw: Vec<_> = people
        .iter()
        .map(|text| {
            let start = source.find(text).unwrap();
            crate::RawEntity {
                start,
                end: start + text.len(),
                label: "person".into(),
                score: 0.9,
                text: (*text).into(),
            }
        })
        .collect();
    crate::recognition::proposals(source, &raw)
        .unwrap()
        .into_iter()
        .filter(|span| span.category == "PERSON")
        .map(|span| source[span.range].to_owned())
        .collect()
}

/// One initial before a surname joins it after a title, "by", "v",
/// "per", "and" after a word or a judge's "J,", with its period or
/// without; an all-caps given name joins "X. SURNAME" in a signature or
/// caption position (#387). A list letter, a section letter, a sentence's
/// "A", the pronoun "I", a comma, a colon, "to" or "(" before the letter,
/// a role and a signature's verb do not.
#[test]
fn one_initial_or_an_all_caps_given_name_joins_a_surname() {
    for (source, model, expected) in [
        (
            "Before Judge K Ashdale and DORIAN P. WELK, Magistrate.",
            &["Ashdale", "P. WELK"][..],
            &["K Ashdale", "DORIAN P. WELK"][..],
        ),
        (
            "The order of Judge V Davies stands.",
            &["Davies"],
            &["V Davies"],
        ),
        (
            "Heard by Judge K. Ashdale today.",
            &["Ashdale"],
            &["K. Ashdale"],
        ),
        (
            "Signed by K Ashdale on Monday.",
            &["Ashdale"],
            &["K Ashdale"],
        ),
        ("Dear Ms A Brannock,", &["Brannock"], &["A Brannock"]),
        (
            "Smith v A Jones [2020] EWHC 1",
            &["Smith", "Jones"],
            &["Smith", "A Jones"],
        ),
        (
            "per K Ashdale-Brown",
            &["Ashdale-Brown"],
            &["K Ashdale-Brown"],
        ),
        (
            "Before Judge K Ashdale, L Brown and M Green.",
            &["Ashdale", "Brown", "Green"],
            &["K Ashdale", "Brown", "M Green"],
        ),
        (
            "Before: Smith J, K Brown and Green LJ",
            &["Smith", "Brown", "Green"],
            &["Smith", "K Brown", "Green"],
        ),
        (
            "/s/ DORIAN P. WELK\nMagistrate",
            &["P. WELK"],
            &["DORIAN P. WELK"],
        ),
        ("DORIAN P. WELK, Magistrate", &["WELK"], &["DORIAN P. WELK"]),
        ("SIGNED P. WELK\n", &["P. WELK"], &["P. WELK"]),
        // A section, list or answer letter, a sentence's article and the
        // pronoun stay out.
        ("under Part B Smith said", &["Smith"], &["Smith"]),
        ("See Exhibit A Smith for the plan.", &["Smith"], &["Smith"]),
        ("A Smith attended the site.", &["Smith"], &["Smith"]),
        ("It ended. A Smith attended.", &["Smith"], &["Smith"]),
        ("B. Smith\nC. Jones", &["Smith"], &["Smith"]),
        ("Then I Penrow it.", &["Penrow"], &["Penrow"]),
        (
            "See Schedule A and B Jones for the plan.",
            &["Jones"],
            &["Jones"],
        ),
        (
            "In Annex A, B Smith states the terms.",
            &["Smith"],
            &["Smith"],
        ),
        (
            "the holders of Class A and B Smith shares",
            &["Smith"],
            &["Smith"],
        ),
        ("Answer: A Smith attended.", &["Smith"], &["Smith"]),
        ("Amendments to A Jones Trust deed", &["Jones"], &["Jones"]),
        ("the witness ( B Smith ) said", &["Smith"], &["Smith"]),
        ("Q. Who signed it? A. Smith did.", &["Smith"], &["Smith"]),
        (
            "Present: Ivo Penrow, K Ashdale.",
            &["Ivo Penrow", "Ashdale"],
            &["Ivo Penrow", "Ashdale"],
        ),
        // A role, a heading, a caption's party word, a short word, a
        // "V." or a mid-line word before "X. SURNAME".
        ("JUDGE P. WELK presiding.", &["P. WELK"], &["P. WELK"]),
        ("BY P. WELK, counsel.", &["P. WELK"], &["P. WELK"]),
        ("Mixed Case P. WELK here.", &["P. WELK"], &["P. WELK"]),
        ("EXHIBIT A. WELK DECLARATION", &["WELK"], &["WELK"]),
        ("SCHEDULE B. WELK", &["WELK"], &["WELK"]),
        ("COUNT I. WELK", &["WELK"], &["WELK"]),
        ("COMMONWEALTH V. WELK", &["WELK"], &["WELK"]),
        ("PETITIONER J. DOE", &["J. DOE"], &["J. DOE"]),
        (
            "SMITH V. WELK, No. 21-cv-104",
            &["SMITH", "WELK"],
            &["SMITH", "WELK"],
        ),
        ("A note on DORIAN P. WELK here", &["P. WELK"], &["P. WELK"]),
        // A model's own name keeps its start.
        (
            "Witness: HON WAI KEUNG",
            &["HON WAI KEUNG"],
            &["HON WAI KEUNG"],
        ),
        (
            "Witness: Hon Wai Keung",
            &["Hon Wai Keung"],
            &["Hon Wai Keung"],
        ),
        // A judge's title the model took in comes off before a name: an
        // initial and surname, a given name and surname, or a lone surname
        // (#382); a role surname alone or after a given name stays.
        (
            "Before Judge K Ashdale and DORIAN P. WELK, Magistrate.",
            &["Judge K Ashdale", "DORIAN P. WELK"],
            &["K Ashdale", "DORIAN P. WELK"],
        ),
        (
            "Heard by Justice Mary Ashdale.",
            &["Justice Mary Ashdale"],
            &["Mary Ashdale"],
        ),
        (
            "Heard by Hon. K Ashdale.",
            &["Hon. K Ashdale"],
            &["K Ashdale"],
        ),
        ("Judge", &["Judge"], &["Judge"]),
        ("Mary Judge agreed.", &["Mary Judge"], &["Mary Judge"]),
        ("Then Mr Judge agreed.", &["Mr Judge"], &["Judge"]),
        ("Judge Reinhold agreed.", &["Judge Reinhold"], &["Reinhold"]),
    ] {
        assert_eq!(proposed_people(source, model), expected, "{source}");
    }
    // A company's span is never grown into, whatever the initial.
    for (source, company) in [
        ("ACME LLC V. WELK, No. 21-cv-104", "ACME LLC"),
        ("ACME CORP V. WELK, No. 21-cv-104", "ACME CORP"),
        ("PELLOW HOLDINGS INC V. WELK", "PELLOW HOLDINGS INC"),
        ("/s/ ACME LLC P. WELK", "ACME LLC"),
        ("/s/ ACMECO P. WELK", "ACMECO"),
    ] {
        let start = source.find(company).unwrap();
        let welk = source.find("WELK").unwrap();
        let raw = [
            crate::RawEntity {
                start,
                end: start + company.len(),
                label: "organization".into(),
                score: 0.9,
                text: company.into(),
            },
            crate::RawEntity {
                start: welk,
                end: welk + 4,
                label: "person".into(),
                score: 0.9,
                text: "WELK".into(),
            },
        ];
        let shown: Vec<_> = crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .map(|span| (span.category, source[span.range].to_owned()))
            .collect();
        assert_eq!(
            shown,
            [
                ("ORG".to_owned(), company.to_owned()),
                ("PERSON".to_owned(), "WELK".to_owned())
            ],
            "{source}"
        );
    }
}

#[test]
fn entrepreneur_prefix_trim_preserves_the_surname_ip() {
    for source in [
        "Свидетель Ип Ман подписал заявление",
        "Свидетель Ип Ман дал показания.",
    ] {
        assert_eq!(
            shown(source, &[("Ип Ман", "person")]),
            pairs(&[("Ип Ман", "PERSON")]),
            "{source}"
        );
    }
    // Only the uppercase status at the start comes off. A surname at
    // the end and a name after the status must both survive.
    for (source, name) in [
        ("ИП Ип Ман", "Ип Ман"),
        ("ИП Ман Ип", "Ман Ип"),
        ("Ман ИП", "Ман ИП"),
        // A role phrase at the end comes off too (#385).
        ("Ман индивидуальный предприниматель", "Ман"),
        ("ИП Ветлин", "Ветлин"),
        ("индивидуального предпринимателя Ип Ман", "Ип Ман"),
        ("ИНДИВИДУАЛЬНЫЙ ПРЕДПРИНИМАТЕЛЬ Ип Ман", "Ип Ман"),
        ("от ИП Ип Ман", "Ип Ман"),
        ("от Ип Ман", "от Ип Ман"),
    ] {
        let raw = crate::RawEntity {
            start: 0,
            end: source.len(),
            label: "person".into(),
            score: 0.8,
            text: source.into(),
        };
        let corrected: Vec<_> = crate::correct(source, &[raw])
            .into_iter()
            .map(|d| (source[d.range].to_owned(), d.category.token().to_owned()))
            .collect();
        assert_eq!(corrected, pairs(&[(name, "PERSON")]), "{source}");
    }
}

#[test]
fn entrepreneur_name_only_org_yields_to_person() {
    for (source, name) in [
        ("ИП Ветлин Н. О.", "Ветлин Н. О."),
        (
            "индивидуальный предприниматель Ветлин Н. О.",
            "Ветлин Н. О.",
        ),
        (
            "индивидуальному предпринимателю Ветлину Н.О.",
            "Ветлину Н.О.",
        ),
        (
            "индивидуальный предприниматель Брюн Лидия Игоревна",
            "Брюн Лидия Игоревна",
        ),
    ] {
        assert_eq!(
            shown(source, &[(name, "organization")]),
            pairs(&[(name, "PERSON")]),
            "{source}"
        );
    }
    // With no entrepreneur status, this rule has no evidence to
    // override an organization. Quoted trading names stay as well.
    assert_eq!(
        shown("Ветлин Н. О.", &[("Ветлин Н. О.", "organization")]),
        pairs(&[("Ветлин Н. О.", "ORG")])
    );
    let source = "ИП Ветлин Н. О., магазин «Кедрач»";
    assert_eq!(
        shown(
            source,
            &[
                ("Ветлин Н. О.", "organization"),
                ("«Кедрач»", "organization")
            ]
        ),
        pairs(&[("Ветлин Н. О.", "PERSON"), ("«Кедрач»", "ORG")])
    );
}

#[test]
fn entrepreneur_full_names_in_capitals_are_people() {
    for (source, name) in [
        ("ИП МОХОВ ЛЕВ РОМАНОВИЧ", "МОХОВ ЛЕВ РОМАНОВИЧ"),
        ("ИП ЛЕВ РОМАНОВИЧ МОХОВ", "ЛЕВ РОМАНОВИЧ МОХОВ"),
        ("ИП МОХОВА ЛИДИЯ ИГОРЕВНА", "МОХОВА ЛИДИЯ ИГОРЕВНА"),
        ("ИП МОХОВ-БРЮН ЛЕВ РОМАНОВИЧ", "МОХОВ-БРЮН ЛЕВ РОМАНОВИЧ"),
        (
            "индивидуального предпринимателя МОХОВА ЛЬВА РОМАНОВИЧА",
            "МОХОВА ЛЬВА РОМАНОВИЧА",
        ),
        (
            "ИНДИВИДУАЛЬНЫЙ ПРЕДПРИНИМАТЕЛЬ МОХОВ ЛЕВ РОМАНОВИЧ",
            "МОХОВ ЛЕВ РОМАНОВИЧ",
        ),
    ] {
        assert!(
            !pack().identifying(source, Category::Organization),
            "{source}"
        );
        for raw in [
            vec![],
            vec![(source, "organization")],
            vec![(name, "organization")],
            vec![(source, "person")],
        ] {
            assert_eq!(
                shown(source, &raw),
                pairs(&[(name, "PERSON")]),
                "{source}: {raw:?}"
            );
        }
    }
}

/// A surname in capitals before a name and patronymic in title case,
/// and the name in every case form, «-ичом» and «-ичем» among them
/// (the Mac hub's review of #582, issue #562).
#[test]
fn entrepreneur_mixed_case_and_case_forms_are_people() {
    for (source, name) in [
        ("ИП МОХОВ Лев Романович", "МОХОВ Лев Романович"),
        ("ИП Лев Романович МОХОВ", "Лев Романович МОХОВ"),
        (
            "индивидуальным предпринимателем Моховым Ильёй Ильичом",
            "Моховым Ильёй Ильичом",
        ),
        (
            "индивидуальным предпринимателем МОХОВЫМ ИЛЬЁЙ ИЛЬИЧОМ",
            "МОХОВЫМ ИЛЬЁЙ ИЛЬИЧОМ",
        ),
        (
            "индивидуальным предпринимателем Моховым Львом Романовичем",
            "Моховым Львом Романовичем",
        ),
        (
            "индивидуальным предпринимателем Ильёй Ильичом Моховым",
            "Ильёй Ильичом Моховым",
        ),
        (
            "индивидуальному предпринимателю Мохову Илье Ильичу",
            "Мохову Илье Ильичу",
        ),
        (
            "об индивидуальном предпринимателе Мохове Илье Ильиче",
            "Мохове Илье Ильиче",
        ),
        (
            "индивидуального предпринимателя Мохова Ильи Ильича",
            "Мохова Ильи Ильича",
        ),
        (
            "индивидуальным предпринимателем Моховой Лидией Игоревной",
            "Моховой Лидией Игоревной",
        ),
        (
            "индивидуальным предпринимателем Брюн Анной Сергеевной",
            "Брюн Анной Сергеевной",
        ),
        (
            "индивидуальным предпринимателем Моховой Ниной Ильиничной",
            "Моховой Ниной Ильиничной",
        ),
        (
            "индивидуального предпринимателя Моховой Нины Ильиничны",
            "Моховой Нины Ильиничны",
        ),
        (
            "индивидуальному предпринимателю Моховой Лидии Игоревне",
            "Моховой Лидии Игоревне",
        ),
        (
            "на индивидуального предпринимателя Мохову Лидию Игоревну",
            "Мохову Лидию Игоревну",
        ),
    ] {
        assert!(
            !pack().identifying(source, Category::Organization),
            "{source}"
        );
        for raw in [
            vec![],
            vec![(source, "organization")],
            vec![(name, "organization")],
            vec![(source, "person")],
        ] {
            assert_eq!(
                shown(source, &raw),
                pairs(&[(name, "PERSON")]),
                "{source}: {raw:?}"
            );
        }
    }
}

/// An individual entrepreneur is a person, «ИП» or «индивидуальный
/// предприниматель» outside the name, in any case form; a trading name
/// in quotes stays an organization, and placeholders stay no one (guide
/// rule 1, the maintainer's decision on #484 items 5 and 8, issue #562).
#[test]
fn individual_entrepreneurs_are_people() {
    let pack = pack();
    for text in [
        "ИП Мохов Л. Р.",
        "ИП Мохов Л.Р.",
        "ИП Мохов Л. Р.,",
        "ИП МОХОВ Л.Р.",
        "ИП Л. Р. Мохов",
        "ИП Ветлину Н.О.",
        "ответчик ИП Мохов Л. Р.",
        "индивидуальный предприниматель Мохов Лев Романович",
        "Индивидуальный предприниматель Брюн Лидия Игоревна",
        "индивидуального предпринимателя Ветлина Н. О.",
        "индивидуальным предпринимателем Мохов Лев Романович",
        "ИП Лев Романович Мохов",
        "ИП Мохова Лидия Игоревна",
        "ИП Мохов-Брюн Л. Р.",
    ] {
        assert!(!pack.identifying(text, Category::Organization), "{text}");
    }
    for text in [
        "ИП Мохов Л. Р. магазин «Кедрач»",
        "ИП «Кедрач»",
        "ИП Мохов",
        "ИП Мохов и партнёры",
        "ООО «Мохов Л. Р.»",
        "Мохов Л. Р.",
    ] {
        assert!(pack.identifying(text, Category::Organization), "{text}");
    }
    let source = "Истец ИП Ветлин Н. О. и индивидуальный предприниматель Брюн Лидия Игоревна заключили договор с ООО «Север».";
    let expected = pairs(&[
        ("Ветлин Н. О.", "PERSON"),
        ("Брюн Лидия Игоревна", "PERSON"),
        ("ООО «Север»", "ORG"),
    ]);
    // The model's organizations, or nothing for the names.
    assert_eq!(
        shown(
            source,
            &[
                ("ИП Ветлин Н. О.", "organization"),
                (
                    "индивидуальный предприниматель Брюн Лидия Игоревна",
                    "organization"
                ),
                ("ООО «Север»", "organization"),
            ]
        ),
        expected
    );
    assert_eq!(shown(source, &[("ООО «Север»", "organization")]), expected);
    // Or as persons with the status inside the span.
    assert_eq!(
        shown(
            source,
            &[
                ("ИП Ветлин Н. О.", "person"),
                (
                    "индивидуальный предприниматель Брюн Лидия Игоревна",
                    "person"
                ),
                ("ООО «Север»", "organization"),
            ]
        ),
        expected
    );
    // Or the model had the names right.
    assert_eq!(
        shown(
            source,
            &[
                ("Ветлин Н. О.", "person"),
                ("Брюн Лидия Игоревна", "person"),
                ("ООО «Север»", "organization"),
            ]
        ),
        expected
    );
    // Case forms, and the initials without a space.
    let source = "Иск к индивидуальному предпринимателю Ветлину Н. О. и ИП Ветлину Н.О. отклонён; от индивидуального предпринимателя Мохова Льва Романовича поступил отзыв.";
    assert_eq!(
        shown(
            source,
            &[
                (
                    "индивидуальному предпринимателю Ветлину Н. О.",
                    "organization"
                ),
                ("ИП Ветлину Н.О.", "organization"),
                (
                    "индивидуального предпринимателя Мохова Льва Романовича",
                    "organization"
                ),
            ]
        ),
        pairs(&[
            ("Ветлину Н. О.", "PERSON"),
            ("Ветлину Н.О.", "PERSON"),
            ("Мохова Льва Романовича", "PERSON"),
        ])
    );
    // A shop sign in quotes is an organization; only the name changes.
    let source = "Исполнитель: ИП Мохов Л. Р., магазин «Кедрач».";
    assert_eq!(
        shown(
            source,
            &[
                ("ИП Мохов Л. Р.", "organization"),
                ("«Кедрач»", "organization")
            ]
        ),
        pairs(&[("Мохов Л. Р.", "PERSON"), ("«Кедрач»", "ORG")])
    );
    // The status alone, a company and a role are no names.
    let source = "Индивидуальный предприниматель вправе заключить договор. ИП Ромашка не явился.";
    assert_eq!(shown(source, &[]), pairs(&[]));
    // Placeholders stay no one, their real initials PERSON alone.
    let source = "Истец ИП ФИО1 и ИП ФИО3 А-Х.О. подали иск.";
    assert_eq!(
        shown(
            source,
            &[
                ("ИП ФИО1", "organization"),
                ("ИП ФИО3 А-Х.О.", "organization")
            ]
        ),
        pairs(&[("А-Х.О.", "PERSON")])
    );
}

/// #491: the role before bare initials, as the pack reads it: a whole
/// word just before them, the longest phrase first, «ИП» as written,
/// and whether only a person holds it; a legal form before them makes
/// them a company's, as a definition of a company's short name does;
/// abbreviations, public bodies and function words are never initials.
#[test]
fn initials_roles_and_companies_are_read_before_the_letters() {
    let pack = pack();
    let role = |start: usize, person: bool, three: bool| {
        Some(Role {
            start,
            person,
            three,
        })
    };
    let cases = [
        ("Он и потерпевший ", role("Он и ".len(), true, false)),
        ("**Осуждённому** ", role("**".len(), true, false)),
        ("Законный представитель ", role(0, false, false)),
        ("Ответчику ", role(0, false, false)),
        ("что ИП ", role("что ".len(), true, false)),
        ("гр. ", role(0, true, true)),
        ("Гражданке ", role(0, true, true)),
    ];
    for (before, expected) in cases {
        assert_eq!(pack.initials_role(before), expected, "{before}");
    }
    for before in [
        "Ип ",
        "потерпевший",
        "Потерпевшийся ",
        "непотерпевший ",
        "с ",
    ] {
        assert_eq!(pack.initials_role(before), None, "{before}");
    }
    for before in ["АО ", "в ООО «", "ПАО \"", "ООО", "НАО "] {
        assert!(pack.initials_company(before), "{before}");
    }
    for before in ["ИП ", "ЦАО ", "гр. ", ""] {
        assert!(!pack.initials_company(before), "{before}");
    }
    for word in ["ЖКХ", "НЕ", "ДПС", "МЧС", "ФМС", "УФК", "СУД"] {
        assert!(pack.initials_never(word), "{word}");
    }
    for word in ["КЛМ", "БАВ", "Не"] {
        assert!(!pack.initials_never(word), "{word}");
    }
    let defined = "ООО «Кедр Строй Сервис» (далее – КСС) и Иванов (далее – ИАВ).";
    assert!(pack.defined_company(defined, "КСС"));
    assert!(!pack.defined_company(defined, "ИАВ"));
    assert!(!pack.defined_company(defined, "ССК"));
    // The company ends the clause before the bracket: a legal form
    // last, or a quoted name a legal form or the quote opens.
    assert!(pack.defined_company("Кедр ООО (далее – КСС)", "КСС"));
    assert!(pack.defined_company("Поставщик, «Кедр Строй» (далее – КС)", "КС"));
    assert!(!pack.defined_company(
        "Иванов Иван Иванович, работник ООО «Ромашка» (далее – ИИИ).",
        "ИИИ"
    ));
    assert!(!pack.defined_company("ООО Кедр, Иванов (далее – ИАВ)", "ИАВ"));
}

#[test]
fn role_surnames_and_initials_survive_without_other_mentions() {
    for name in [
        "R. Masters",
        "Masters",
        "Stewards",
        "Master",
        "Steward",
        "Judge",
        "Marshal",
        "Sheriff",
        "Bishop",
        "Chancellor",
        "Clerk",
        "Mayor",
        "Dean",
        "Sergeant",
        "Banker",
        "Cook",
        "Porter",
        "Baker",
        "Taylor",
        "Mason",
        "Carter",
        "Fowler",
        "Судья",
        "Староста",
        "Голова",
        "Кузнец",
        "Бухгалтер",
    ] {
        assert_eq!(
            shown(name, &[(name, "person")]),
            pairs(&[(name, "PERSON")]),
            "{name}"
        );
    }
    // Exercise every role word/stem, rather than protecting only Masters.
    for entry in pack().never.iter().filter(|entry| entry.person_role) {
        for word in &entry.words {
            let mut chars = word.chars();
            let capitalized =
                chars.next().unwrap().to_uppercase().collect::<String>() + chars.as_str();
            for surname in [capitalized.clone(), format!("{capitalized}s")]
                .into_iter()
                .take(if entry.form == Form::Plural { 2 } else { 1 })
            {
                let name = format!("R. {surname}");
                assert_eq!(
                    shown(&name, &[(&name, "person")]),
                    pairs(&[(&name, "PERSON")]),
                    "{name}"
                );
            }
        }
    }
    for role in ["masters", "judges", "Judges", "the Masters", "the Judges"] {
        assert!(shown(role, &[(role, "person")]).is_empty(), "{role}");
    }
    for (source, detected) in [("the Masters", "Masters"), ("these Judges", "Judges")] {
        assert!(
            shown(source, &[(detected, "person")]).is_empty(),
            "{source}"
        );
    }
}

#[test]
fn role_surnames_after_said_and_possessives_survive() {
    // Only Masters is supplied: no other name seed can rescue it.
    for source in [
        "Willis said Masters did not author these two notes",
        "Willis testified that Masters did not sign.",
    ] {
        assert_eq!(
            shown(source, &[("Masters", "person")]),
            pairs(&[("Masters", "PERSON")]),
            "{source}"
        );
    }
    for before in [
        "said", "her", "his", "their", "its", "our", "my", "your", "a", "an", "this", "that",
        "этот",
    ] {
        let source = format!("{before} Masters");
        assert_eq!(
            shown(&source, &[("Masters", "person")]),
            pairs(&[("Masters", "PERSON")]),
            "{source}"
        );
    }
    for before in ["the", "these", "those", "such", "данный", "указанный"] {
        let source = format!("{before} Masters");
        assert!(
            shown(&source, &[("Masters", "person")]).is_empty(),
            "{source}"
        );
        for separator in [", ", ": ", " — "] {
            let source = format!("{before}{separator}Masters");
            assert_eq!(
                shown(&source, &[("Masters", "person")]),
                pairs(&[("Masters", "PERSON")]),
                "{source}"
            );
        }
        let source = format!("{before} spoke to Masters");
        assert_eq!(
            shown(&source, &[("Masters", "person")]),
            pairs(&[("Masters", "PERSON")]),
            "{source}"
        );
    }
}

/// A role can also be a surname. Titles, initials, full names and
/// later surname-only mentions must keep that real name (#385).
#[test]
fn probe_role_surnames_stay_names() {
    for name in [
        "Edda Leader",
        "Ivo Chair",
        "Edda Trustee",
        "Ivo Esquire",
        "Edda Investigator",
        "Ivo Participant",
        "Edda Clerk",
        "Лада Должник",
        "Иво Кассатор",
        "Лада Руководитель",
        "Иво Уполномоченный",
        "Лада Предприниматель",
        "Иво Правообладатель",
        "Подрядчиков",
        "Директоров",
        "Кредиторов",
        "Руководителев",
    ] {
        assert_eq!(
            shown(name, &[(name, "person")]),
            pairs(&[(name, "PERSON")]),
            "{name}"
        );
    }
    for (source, name) in [
        ("Mr Leader", "Leader"),
        ("Ms Trustee", "Trustee"),
        ("Dr. Chair", "Chair"),
        ("Ms Participant", "Participant"),
        ("Mr Team Leader", "Team Leader"),
        ("Ms Chief Investigator", "Chief Investigator"),
        ("господин Должник", "Должник"),
        ("госпожа Руководитель", "Руководитель"),
        ("господин Предприниматель", "Предприниматель"),
        ("госпожа Кредиторов", "Кредиторов"),
    ] {
        for detected in [source, name] {
            assert_eq!(
                shown(source, &[(detected, "person")]),
                pairs(&[(name, "PERSON")]),
                "{detected}"
            );
        }
    }
    for (source, full, surname) in [
        ("Edda Leader met Leader", "Edda Leader", "Leader"),
        ("Иво Должник встретил Должник", "Иво Должник", "Должник"),
    ] {
        let raw: Vec<crate::RawEntity> = [
            (source.find(full).unwrap(), full),
            (source.rfind(surname).unwrap(), surname),
        ]
        .into_iter()
        .map(|(start, text)| crate::RawEntity {
            start,
            end: start + text.len(),
            label: "person".into(),
            score: 0.8,
            text: text.into(),
        })
        .collect();
        let found: Vec<(String, String)> = crate::recognition::proposals(source, &raw)
            .unwrap()
            .into_iter()
            .map(|span| (source[span.range].to_owned(), span.category))
            .collect();
        assert_eq!(
            found,
            pairs(&[(full, "PERSON"), (surname, "PERSON")]),
            "{source}"
        );
    }
    for name in ["J. R. Leader", "Должник И. В."] {
        assert_eq!(
            shown(name, &[(name, "person")]),
            pairs(&[(name, "PERSON")]),
            "{name}"
        );
    }
    // A role inside a longer model span cannot veto the whole name; a
    // role the pack knows comes off it (#382).
    for (source, name) in [
        ("Trustee Edda Brannock", "Edda Brannock"),
        ("Руководитель Лада Брюн", "Руководитель Лада Брюн"),
    ] {
        assert_eq!(
            shown(source, &[(source, "person")]),
            pairs(&[(name, "PERSON")]),
            "{source}"
        );
    }
    for name in ["Brannock Leader", "Trustee Brannock Ltd"] {
        assert_eq!(
            shown(name, &[(name, "organization")]),
            pairs(&[(name, "ORG")]),
            "{name}"
        );
    }
}

/// #382: a title, rank, office or party role the model took into a
/// person's span comes off it, chained, with a determiner or "late" or
/// "former" before it, in mixed case or capitals, before a lone surname
/// too; in Russian a court officer's or a party's role in its case forms.
#[test]
fn a_role_before_a_name_comes_off() {
    for (source, model, expected) in [
        (
            "Defendant HOLLOWAY struck him.",
            "Defendant HOLLOWAY",
            "HOLLOWAY",
        ),
        (
            "Then Defendant Holloway struck him.",
            "Defendant Holloway",
            "Holloway",
        ),
        (
            "Then Plaintiff Ostrander fell.",
            "Plaintiff Ostrander",
            "Ostrander",
        ),
        ("Then Deputy Calloway came.", "Deputy Calloway", "Calloway"),
        ("Then DEPUTY CALLOWAY came.", "DEPUTY CALLOWAY", "CALLOWAY"),
        (
            "Then Officer Calloway came.",
            "Officer Calloway",
            "Calloway",
        ),
        (
            "Then officer Calloway came.",
            "officer Calloway",
            "Calloway",
        ),
        ("Then Sgt. Calloway came.", "Sgt. Calloway", "Calloway"),
        (
            "Then Colonel Wendell P. Ashgrove came.",
            "Colonel Wendell P. Ashgrove",
            "Wendell P. Ashgrove",
        ),
        ("Then Senator Pruett spoke.", "Senator Pruett", "Pruett"),
        (
            "Before Employment Judge Pemberton sitting alone.",
            "Employment Judge Pemberton",
            "Pemberton",
        ),
        (
            "Before Employment Judge R Pemberton sitting alone.",
            "Employment Judge R Pemberton",
            "R Pemberton",
        ),
        (
            "BEFORE HIS HONOUR JUDGE TALBOT\n",
            "HIS HONOUR JUDGE TALBOT",
            "TALBOT",
        ),
        (
            "BEFORE HER HONOUR JUDGE TALBOT\n",
            "HER HONOUR JUDGE TALBOT",
            "TALBOT",
        ),
        ("Heard by HHJ Talbot today.", "HHJ Talbot", "Talbot"),
        ("BEFORE JUDGE TALBOT\n", "JUDGE TALBOT", "TALBOT"),
        ("BEFORE JUSTICE MARLOWE\n", "JUSTICE MARLOWE", "MARLOWE"),
        (
            "Heard by Justice Marlowe today.",
            "Justice Marlowe",
            "Marlowe",
        ),
        (
            "Heard by Mr Justice Marlowe today.",
            "Mr Justice Marlowe",
            "Marlowe",
        ),
        (
            "Heard by Deputy District Judge Rowe.",
            "Deputy District Judge Rowe",
            "Rowe",
        ),
        (
            "As former President Hargrove said.",
            "former President Hargrove",
            "Hargrove",
        ),
        (
            "As the late Mr Hargrove said.",
            "the late Mr Hargrove",
            "Hargrove",
        ),
        (
            "Протокол вела с/з Мухиным П.Е. в зале.",
            "с/з Мухиным П.Е.",
            "Мухиным П.Е.",
        ),
        ("Дело рассмотрел судья Мухин один.", "судья Мухин", "Мухин"),
        (
            "Судья Мухин П.Е. огласил решение.",
            "Судья Мухин П.Е.",
            "Мухин П.Е.",
        ),
        (
            "при секретаре судебного заседания Мухиной М.С. дело слушалось",
            "секретаре судебного заседания Мухиной М.С.",
            "Мухиной М.С.",
        ),
        (
            "допрошен старший лейтенант полиции Мухин",
            "старший лейтенант полиции Мухин",
            "Мухин",
        ),
        ("Истец Мухин пояснил.", "Истец Мухин", "Мухин"),
    ] {
        assert_eq!(
            shown(source, &[(model, "person")]),
            pairs(&[(expected, "PERSON")]),
            "{source}"
        );
    }
    // A role alone, a role after a given name or a title and a surname
    // made of a role's stem stay as they were; a role after a role is a
    // role phrase, no one's name, and neither is a team's name.
    for (source, model, expected) in [
        ("Mary Judge agreed.", "Mary Judge", &["Mary Judge"][..]),
        ("Then Mr Officer spoke.", "Mr Officer", &["Officer"]),
        ("Then Mr Judge agreed.", "Mr Judge", &["Judge"]),
        ("The Magistrate Judge ruled.", "Magistrate Judge", &[]),
        ("The Deputy Director left.", "Deputy Director", &[]),
        ("The Justice League met.", "Justice League", &[]),
        (
            "Прокуроров Иван пояснил.",
            "Прокуроров Иван",
            &["Прокуроров Иван"],
        ),
        ("Майоров Иван пояснил.", "Майоров Иван", &["Майоров Иван"]),
    ] {
        let found: Vec<String> = shown(source, &[(model, "person")])
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        assert_eq!(found, expected, "{source}");
    }
    // A doubled word is a title and its surname: the span is the second.
    assert_eq!(
        fixed("Judge Judge ruled.", &[("Judge Judge", Category::Person)]),
        ["Judge"]
    );
}

/// #382, item 2: a common role word ("Defendant", "Judge") is a name only
/// where a person span holds it beside a name; a role the trim took off
/// unlocks no bare mention of it.
#[test]
fn a_trimmed_role_unlocks_no_bare_role() {
    for (source, model, expected) in [
        (
            "Employment Judge Pemberton heard the claim. Then the Judge said no.",
            &[("Employment Judge Pemberton", 0), ("Judge", 1)][..],
            &["Pemberton"][..],
        ),
        (
            "Defendant HOLLOWAY struck him. Defendant then fled.",
            &[("Defendant HOLLOWAY", 0), ("Defendant", 1)],
            &["HOLLOWAY"],
        ),
        (
            "Истец Мухин подал иск. Истец пояснил суду.",
            &[("Истец Мухин", 0), ("Истец", 1)],
            &["Мухин"],
        ),
    ] {
        let raw: Vec<(&str, usize, &str)> =
            model.iter().map(|&(text, n)| (text, n, "person")).collect();
        let found: Vec<String> = shown_at(source, &raw)
            .into_iter()
            .map(|(text, category)| {
                assert_eq!(category, "PERSON");
                text
            })
            .collect();
        assert_eq!(found, expected, "{source}");
    }
    // A name beside the word still unlocks it ("Dawn French").
    let source = "Ivo Defendant signed. Defendant agreed.";
    let found: Vec<String> = shown_at(
        source,
        &[("Ivo Defendant", 0, "person"), ("Defendant", 1, "person")],
    )
    .into_iter()
    .map(|(text, _)| text)
    .collect();
    assert_eq!(found, ["Ivo Defendant", "Defendant"]);
}

/// A pleading's text, its lines numbered 1 to `lines` inline as an import
/// keeps them, with `line` in place of line `at`'s text.
fn pleading(lines: u8, at: u8, line: &str) -> String {
    (1..=lines)
        .map(|n| {
            if n == at {
                line.to_owned()
            } else {
                format!(
                    "{n} The parties met on the date set out in the schedule to this complaint."
                )
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// #382: a pleading's line number between two parts of a name is inside
/// it, as the guide's annotators keep it: "DESMOND 4 ARKWRIGHT" is one span,
/// and its forms spread. Only in a pleading, and only a line number: a
/// number between two names elsewhere keeps them apart.
#[test]
fn a_pleading_line_number_inside_a_name_joins_it() {
    let mut source = pleading(12, 3, "3 Plaintiff DESMOND 4 ARKWRIGHT alleges as follows.");
    source = source.replacen("4 The parties", "The parties", 1);
    source.push_str(" Later Ms. Arkwright testified.");
    let found: Vec<String> = shown_at(
        &source,
        &[("DESMOND", 0, "person"), ("ARKWRIGHT", 0, "person")],
    )
    .into_iter()
    .map(|(text, _)| text)
    .collect();
    assert_eq!(found, ["DESMOND 4 ARKWRIGHT", "Arkwright"]);
    // Prose, and a number that is no line's in a pleading.
    let prose = "The list names Smith 2 Jones and others.";
    let in_pleading = pleading(
        12,
        8,
        "8 Witnesses: Smith 2 Jones and others were heard on the day.",
    );
    // Its 12 line numbers, without the list's "2".
    assert_eq!(crate::rules::people::line_numbers(&in_pleading).len(), 12);
    for source in [prose, in_pleading.as_str()] {
        let found: Vec<String> =
            shown_at(source, &[("Smith", 0, "person"), ("Jones", 0, "person")])
                .into_iter()
                .map(|(text, _)| text)
                .collect();
        assert_eq!(found, ["Smith", "Jones"], "{source}");
    }
    // A short run of numbers is no pleading's.
    assert!(crate::rules::people::line_numbers("1 a 2 b 3 c 4 d").is_empty());
    assert_eq!(
        crate::rules::people::line_numbers(&pleading(12, 0, "")).len(),
        12
    );
}

/// What the review shows for the model's person spans `model` (each text
/// with its place from 0) in `source`, texts only.
fn people_shown(source: &str, model: &[(&str, usize)]) -> Vec<String> {
    let raw: Vec<(&str, usize, &str)> =
        model.iter().map(|&(text, n)| (text, n, "person")).collect();
    shown_at(source, &raw)
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

/// #382 review: a line end between the parts keeps both, as the review
/// takes no wrapped name with prose after it; a roster's numbered entries
/// are whole names, each its own span, on one line or on several.
#[test]
fn a_line_number_joins_no_wrapped_name_and_no_roster() {
    let mut source = pleading(
        12,
        3,
        "3 Plaintiff DESMOND\n4 ARKWRIGHT alleges as follows.",
    );
    source = source.replacen("4 The parties", "The parties", 1);
    assert_eq!(
        people_shown(&source, &[("DESMOND", 0), ("ARKWRIGHT", 0)]),
        ["DESMOND", "ARKWRIGHT"]
    );
    let given = [
        "Ivo", "Edda", "Rhea", "Oren", "Lida", "Bram", "Tova", "Quin", "Sela", "Arno", "Fenn",
        "Juno",
    ];
    let surnames = [
        "Ashgrove",
        "Brannock",
        "Calloway",
        "Draycott",
        "Elsworth",
        "Fairlie",
        "Gorran",
        "Hollins",
        "Isherwood",
        "Jessop",
        "Kettering",
        "Larkfield",
    ];
    let full: Vec<String> = given
        .iter()
        .zip(surnames)
        .map(|(given, surname)| format!("{given} {surname}"))
        .collect();
    for (names, separator) in [
        (full.clone(), " "),
        (full.clone(), "\n"),
        (given.iter().map(|name| (*name).to_owned()).collect(), " "),
    ] {
        let roster: String = names
            .iter()
            .enumerate()
            .map(|(n, name)| format!("{} {name}", n + 1))
            .collect::<Vec<_>>()
            .join(separator);
        assert_eq!(crate::rules::people::line_numbers(&roster).len(), 12);
        let model: Vec<(&str, usize)> = names.iter().map(|name| (name.as_str(), 0)).collect();
        assert_eq!(people_shown(&roster, &model), names, "{roster}");
    }
}

/// #382 review: a role alone before a pleading's line number joins no
/// name, so it unlocks no bare mention of the role either.
#[test]
fn a_role_before_a_line_number_joins_no_name() {
    let mut source = pleading(12, 3, "3 Defendant 4 Holloway struck him without warning.");
    source = source.replacen("4 The parties", "The parties", 1);
    source.push_str(" Defendant then fled.");
    assert_eq!(
        people_shown(
            &source,
            &[("Defendant", 0), ("Holloway", 0), ("Defendant", 1)]
        ),
        ["Holloway"]
    );
}
