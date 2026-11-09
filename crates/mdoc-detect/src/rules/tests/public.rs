//! Courts, public bodies and public sites.
use super::*;
use crate::rules::load::category;
use crate::rules::public::unquoted;
use crate::rules::test_support::{fixed, pairs, proposed_orgs, shown};
use mdoc_pii::Recognizer;

/// Courts, public bodies, state institutions and laws in their case
/// forms and abbreviations are no organizations (#378), nor is a
/// country, region or city alone or as a body's tail (#478); companies,
/// banks and a company named after the country stay.
#[test]
fn public_bodies_laws_and_places_are_no_organizations() {
    let pack = pack();
    for text in [
        // Courts.
        "Арбитражный суд Северной области",
        "Арбитражного суда Северо-Западного округа",
        "Верховного Суда Российской Федерации",
        "АС Северной области",
        "ФАС Восточного округа",
        "14 ААС",
        "Пленума ВС РФ",
        "ВС РФ",
        "Судебная коллегия",
        "Коллегия",
        // Ministries, services, agencies and their territorial units.
        "ФНС России",
        "МИФНС России № 9",
        "Федеральной налоговой службы",
        "ГУ МВД России по Северной области",
        "Отдел МВД России по Ольховскому району",
        "ГИАЗ ОМВД России",
        "Ольховского ГОСП УФССП",
        "отдела судебных приставов по Центральному округу",
        "Минтруда России",
        "Управлением Росреестра по Северной области",
        "Правительства Северной области",
        "Мэрию муниципального образования",
        "ГБ МСЭ",
        // State institutions.
        "ГКУ города Ольховска",
        "Федерального казенного учреждения",
        "государственного бюджетного учреждения",
        // Codes and laws.
        "Гражданского кодекса Российской Федерации",
        "Арбитражного процессуального кодекса",
        "Кодекса Российской Федерации об административных правонарушениях",
        "Гражданский кодекс РФ",
        "АПК РФ",
        "КоАП РФ",
        "Федерального закона",
        "Федеральным законом",
        "Конституции Российской Федерации",
        // Places, alone or as a tail.
        "РОССИЙСКОЙ ФЕДЕРАЦИИ",
        "Российской Федерации",
        "Российская Федерация",
        "России",
        "субъектов Российской Федерации",
        "Именем Российской Федерации",
        "Северной области",
        "Кедровской Республики",
        "Ольховскому району",
        "Ольховскому краю",
        "городу Ольховску",
        "г. Ольховска",
        "ПГТ Ольховка",
        "Ольховского муниципального района Северной области",
    ] {
        assert!(!pack.identifying(text, Category::Organization), "{text}");
    }
    for text in [
        "ООО «Россия-Трейд»",
        "ООО «Россия»",
        "Банк «Россия»",
        "Сбербанк России",
        "ПАО Сбербанк",
        "АО «Северный банк»",
        "Российский союз автостраховщиков",
        "ООО «Северная область Строй»",
        "ИП Ветлин, г. Ольховск",
        "Федерация футбола России",
        "коллегия адвокатов «Елагин и партнёры»",
        "ООО «Отделстрой»",
        "ООО «Судак»",
        "Ромашка-Плюс",
        // A name in quotes is a name, whatever words it has.
        "Банк «Город»",
        "Банк «Северный край»",
    ] {
        assert!(pack.identifying(text, Category::Organization), "{text}");
    }
    for text in [
        "ФКЗ РФ",
        "ФЗ",
        "ФЗ № 127-ФЗ",
        "Федерального закона от 12.03.2026 № 418-ФЗ",
        "ГКУ «Дирекция парков»",
        "Суд «Северный»",
        "«Арбитражный суд Северной области»",
    ] {
        assert!(!pack.identifying(text, Category::Organization), "{text}");
    }
    assert_eq!(
        unquoted("ГКУ «Центр «Северный» снабжения» и суд"),
        "ГКУ   и суд"
    );
    assert_eq!(unquoted("Банк \"Город\", суд «Север"), "Банк  , суд «Север");
    // Gone whole, wherever the model's span starts: an institution's
    // quoted name with it, a court's tail.
    assert!(
        fixed(
            "Ответчик: ГКУ города Ольховска «Дирекция парков».",
            &[
                ("ГКУ города Ольховска", Category::Organization),
                ("Дирекция парков", Category::Organization),
            ]
        )
        .is_empty()
    );
    for source in [
        "третье лицо — Федеральное казенное учреждение «Центр снабжения».",
        "третье лицо — ГБУ Северной области «Центр снабжения».",
        "третье лицо — ГБУЗ Республики Сосновия «Центр снабжения».",
        "третье лицо — ГКУ г. Ольховска «Центр снабжения».",
        "третье лицо — ГБУ города Нижнего Ольховска «Центр снабжения».",
        "третье лицо — ГКУ Северо-Ольховского автономного округа «Центр снабжения».",
    ] {
        assert!(
            fixed(source, &[("Центр снабжения", Category::Organization)]).is_empty(),
            "{source}"
        );
    }
    // Never across another name: a company's legal form ends the
    // institution's reach.
    assert_eq!(
        fixed(
            "ГКУ и ООО «Ромашка-Плюс»",
            &[("Ромашка-Плюс", Category::Organization)]
        ),
        ["ООО «Ромашка-Плюс»"]
    );
    assert_eq!(
        fixed(
            "Именем Российской Федерации Арбитражный суд Северной области \
             рассмотрел иск ООО «Ромашка-Плюс».",
            &[
                ("Российской Федерации", Category::Organization),
                ("Северной области", Category::Organization),
                ("Ромашка-Плюс", Category::Organization),
            ]
        ),
        ["ООО «Ромашка-Плюс»"]
    );
}

/// English courts, agencies, departments, offices, laws and codes are
/// no organizations and no people (#378), nor is a country, state or
/// city (#478); companies with their legal form, and private bodies that
/// share a word with them, stay.
#[test]
fn english_public_bodies_laws_and_places_are_no_organizations() {
    let pack = pack();
    let org = |text: &str| pack.identifying(text, Category::Organization);
    let person = |text: &str| pack.identifying(text, Category::Person);
    for text in [
        // Departments, offices, agencies and bureaus.
        "Department of Justice",
        "U.S. Department of Labor",
        "Dep't of Labor",
        "Department for Work and Pensions",
        "Ashmoor County Probation Department",
        "Office of the Attorney General",
        "Home Office",
        "Clerk's Office",
        "Agency for International Development",
        "National Security Agency",
        "Bureau of Prisons",
        "Census Bureau",
        // Commissions, legislatures, services and councils.
        "Federal Election Commission",
        "Securities and Exchange Commission",
        "Commission on Civil Rights",
        "National Labor Relations Board",
        "Board of Education",
        "Ashmoor County Board of Supervisors",
        "Congress",
        "the Legislature",
        "Senate Judiciary Committee",
        "Social Security Administration",
        "Internal Revenue Service",
        "Crown Prosecution Service",
        "Ashmoor City Council",
        "HM Treasury",
        "HM Land Registry",
        "His Majesty's Revenue and Customs",
        // Short names.
        "IRS",
        "the IRS",
        "U.S. FDA",
        "CMA",
        "EEOC",
        "OHRP",
        "SEC",
        "EAT",
        "the ET",
        // Prosecutors and ministers.
        "Attorney General",
        "Attorney-General's Office",
        "U.S. Attorney",
        "District Attorney",
        "Secretary of State",
        // Courts by circuit, district and division.
        "1st Cir",
        "9th Cir.",
        "Ninth Circuit",
        "Federal Circuit",
        "Northern District of Ashmoria",
        "Chancery Division",
        "King's Bench Division",
        // Laws, codes and rules.
        "Gaming Act 2031",
        "Biometric Information Privacy Act",
        "the First Amendment",
        "Data Protection Regulations",
        "Bankruptcy Code",
        "Code of Federal Regulations",
        "18 U.S.C.",
        "Civil Procedure Rules",
        "Federal Rules of Evidence",
        // Places alone, together, and after "State of" and the like.
        "Pakistan",
        "North Macedonia",
        "Republic of Korea",
        "Illinois",
        "Massachusetts",
        "United States",
        "United States of America",
        "the United Kingdom",
        "U.S.",
        "U.S",
        "USA",
        "Boston",
        "New York",
        "State",
        "States",
        "Federal",
        "Georgia v. Alabama",
        "Washington, D.C.",
        "State of Ashmoria",
        "STATE OF ASHMORIA",
        "Commonwealth of Brindlemoor",
        "People of the State of Ashmoria",
        "Town of Brindle",
        "District of Columbia",
        "Ashmoor County",
    ] {
        assert!(!org(text), "{text}");
    }
    for text in [
        // A legal form keeps a company, whatever public or place word
        // it has; none cuts it.
        "Bank of America, N.A.",
        "State Street Corporation",
        "Georgia-Pacific LLC",
        "Georgia-Pacific",
        "Federal Express Corp.",
        "Federal Express",
        "Quillon Freight LLC",
        "Brindle Court Ltd",
        "Ashmoor Department Store Ltd",
        "State of Ashmoria Holdings LLC",
        "Congress Street Partners LLP",
        // Private bodies that share a word with a public one.
        "Northbridge Department Store",
        "Acme Talent Agency",
        "Cedar Arts Council",
        "Law Offices of Quill & Venn",
        "New York Life",
        "Boston Scientific",
        "Ohio Valley Freight",
        "United Airlines",
        "Washington Post",
        "Delaware North",
        // Short names and words of public bodies that companies share
        // (review of #515).
        "CMA CGM",
        "SEC Newgate",
        "Commission Junction",
        "The Republic of Tea",
    ] {
        assert!(org(text), "{text}");
    }
    for text in [
        "HMRC",
        "Gaming Act 1968",
        "Biometric Information Privacy Act",
        "Ashmoor County Probation Department",
        "Department of Justice",
        "Federal Election Commission",
        "High Court of Justice",
        "State of Ashmoria",
        "Town of Plymouth",
        "Ninth Circuit",
        "Chancery Division",
        "Attorney General",
        "Bankruptcy Code",
        "National Labor Relations Board",
        "IRS",
    ] {
        assert!(!person(text), "{text}");
    }
    for text in [
        "Georgia Quill",
        "Virginia Ashby",
        "Washington Vale",
        "Sec. Brindle",
        "Justice Brindle",
        "Attorney General Brindle",
        "Pakistan",
    ] {
        assert!(person(text), "{text}");
    }
    // A place's name after "Town of" grows to the place and goes, as an
    // organization or a person; the other side of a caption stays.
    assert_eq!(
        fixed(
            "The Town of Brindle (Illinois) sued; Town of Plymouth v. Orrin Quill.",
            &[
                ("Brindle", Category::Organization),
                ("Illinois", Category::Organization),
                ("Plymouth", Category::Person),
                ("Orrin Quill", Category::Person),
            ]
        ),
        ["Orrin Quill"]
    );
    // An abbreviation defined beside a law or a public body is the law
    // or the body wherever it stands; a company's stays.
    assert_eq!(
        fixed(
            "Under the Biometric Information Privacy Act, 740 ILCS 14/1 (“BIPA”), \
             and the Equal Employment Opportunity Commission (EEOC) rules, \
             Quillon Freight LLC (“QF”) answered. BIPA applies; the EEOC \
             agreed; BIPA’s terms bind QF.",
            &[
                ("BIPA", Category::Organization),
                ("EEOC", Category::Person),
                ("BIPA’s", Category::Person),
                ("QF", Category::Organization),
            ]
        ),
        ["QF"]
    );
    // A title wrapped across a line, LF or CRLF.
    for source in [
        "the Biometric Information
Privacy Act (“BIPA”). BIPA applies.",
        "the Biometric Information
Privacy Act (“BIPA”). BIPA applies.",
    ] {
        let at = source.rfind("BIPA").unwrap();
        let mut detections = vec![Detection {
            range: at..at + 4,
            category: Category::Organization,
            score: 0.9,
            recognizer: Recognizer::Model,
        }];
        pack.fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
        assert!(detections.is_empty(), "{source:?}");
    }
    // An abbreviation the text also defines for something else is
    // ambiguous and stays.
    let source = "the Data Protection Act (“DPA”); DPA Microphones (“DPA”) supplied                       it. DPA invoiced us.";
    let at = source.rfind("DPA").unwrap();
    let mut detections = vec![Detection {
        range: at..at + 3,
        category: Category::Organization,
        score: 0.9,
        recognizer: Recognizer::Model,
    }];
    pack.fix_spans(source, &mdoc_pii::view::View::new(source), &mut detections);
    assert_eq!(detections.len(), 1);
}

/// Probe round 2 of #378 in Russian: courts and public bodies go as
/// people too, and so do the pension and social funds, their offices,
/// the central bank and more short forms; a surname with a body's
/// stem, a titled or initialled person and companies stay.
#[test]
fn russian_public_bodies_are_no_people_and_funds_no_organizations() {
    let pack = pack();
    let org = |text: &str| pack.identifying(text, Category::Organization);
    let person = |text: &str| pack.identifying(text, Category::Person);
    for text in [
        "Арбитражный суд Республики Сосновия",
        "Арбитражный суд Северной области",
        "Министерства обороны Российской Федерации",
        "Министерства",
        "Администрации г. Ольховска",
        "Государственное учреждение - Управление Пенсионного фонда \
         Российской Федерации по Ольховскому району",
        "Минобороны России",
        "Минфина России",
        "Президента Российской Федерации",
        "Гражданского кодекса Российской Федерации",
        "Суд",
    ] {
        assert!(!person(text), "{text}");
    }
    for text in [
        "Пенсионный фонд Российской Федерации",
        "Пенсионного фонда РФ",
        "Фонда пенсионного и социального страхования Российской Федерации",
        "Фонда социального страхования",
        "Фонда социального страхованияРоссийской Федерации",
        "Социальный фонд России",
        "бюджет Фонда",
        "Фонда",
        "УПФР",
        "ОПФР по Северной области",
        "ОСФР",
        "УФК по Северной области",
        "ГУ",
        "Регистрационно-лицензионной палатой",
        "Президента Российской Федерации",
        "ЦБ РФ",
        "Банк России",
        "Центрального банка Российской Федерации",
        "ВАС РФ",
        "Президиума ВАС РФ",
        "АС Ольховска и Северной обл",
        "АС Северной области",
        "ЛК РФ",
        "Депнедра",
        "ФГАУ «Жилкомплекс»",
        "Вооруженных Сил Российской Федерации",
        "Налоговые органы",
        "гражданским законодательством Российской Федерации",
    ] {
        assert!(!org(text), "{text}");
    }
    for text in [
        "Судов",
        "Суднев",
        "Судина",
        "Приставко",
        "Приставкина",
        "Законов",
        "Палатова",
        "Суд Н. О.",
        "Н.О. Суд",
        "Гу",
        "Иван Суднев",
    ] {
        assert!(person(text), "{text}");
    }
    for text in [
        "ООО «Фонд Север»",
        "Фонд «Север»",
        "Благотворительный фонд «Север»",
        "Сбербанк России",
        "Банк «Россия»",
        "АС Групп",
        "ООО «Палата»",
    ] {
        assert!(org(text), "{text}");
    }
    // A title keeps a surname that is a court's word.
    assert_eq!(
        fixed("Г-н Суд пояснил.", &[("Суд", Category::Person)]),
        ["Суд"]
    );
    // A state institution's quoted name, as a person, goes with it.
    assert!(
        fixed(
            "жалобу ГБУ СО «Кедровый дом-интернат»",
            &[("Кедровый дом-интернат", Category::Person)]
        )
        .is_empty()
    );
}

/// Probe round 2 of #378: a span cut out of a public body's name goes,
/// read with the words beside it; a person beside a body's word stays.
#[test]
fn a_span_cut_out_of_a_public_body_goes() {
    for (source, span, category) in [
        (
            "жалобу Министерства финансов Российской Федерации",
            "финансов Российской Федерации",
            Category::Organization,
        ),
        (
            "жалобу Министерства обороны Российской Федерации",
            "обороны Российской Федерации",
            Category::Person,
        ),
        (
            "the Ashmoor County Sheriff’s Office arrested him",
            "Ashmoor County Sheriff’",
            Category::Organization,
        ),
        (
            "the Bureau of Oceans, Environment, and Science",
            "Bureau",
            Category::Organization,
        ),
        (
            "the Office of the Attorney General",
            "Office",
            Category::Person,
        ),
    ] {
        assert!(fixed(source, &[(span, category)]).is_empty(), "{source}");
    }
    for (source, span, category) in [
        (
            "начальник Отдела Ветлина Н. О. пояснила",
            "Ветлина Н. О.",
            Category::Person,
        ),
        (
            "Ms Quill of the Department of Labor",
            "Quill",
            Category::Person,
        ),
        ("Quill’s Office", "Quill", Category::Person),
        (
            "Quillon Freight LLC of the Bureau of Prisons",
            "Quillon Freight LLC",
            Category::Organization,
        ),
    ] {
        assert_eq!(fixed(source, &[(span, category)]), [span], "{source}");
    }
}

/// The review of probe round 2 of #378: a surname with a body's stem,
/// a particle after a body's word, a non-state fund, a university, a
/// private name with an agency's letters and a company's abbreviation
/// after a law stay; an institution named after a person goes.
#[test]
fn names_and_private_bodies_beside_public_ones_stay() {
    let pack = pack();
    let org = |text: &str| pack.identifying(text, Category::Organization);
    let person = |text: &str| pack.identifying(text, Category::Person);
    // A surname with a court's stem, whatever its ending; the court's
    // own forms still go.
    for text in ["Николае Судине", "Иван Судник", "Судине", "Судник"]
    {
        assert!(person(text), "{text}");
    }
    for text in ["Суда", "Судом", "Судебной коллегии", "Законом", "Палатой"]
    {
        assert!(!person(text), "{text}");
    }
    // A lowercase particle opens the person's name, not the body's.
    assert_eq!(
        fixed(
            "начальник Отдела де Лаваль пояснил",
            &[("де Лаваль", Category::Person)]
        ),
        ["де Лаваль"]
    );
    // Non-state funds, universities and colleges are no public bodies
    // (#484), whoever owns them.
    for text in [
        "Негосударственный пенсионный фонд «Кедр»",
        "Негосударственного пенсионного фонда «Кедр»",
        "НПФ «Кедр»",
        "ФГБОУ ВО «Северный государственный университет»",
        "Северный государственный университет",
        "ГБПОУ «Ольховский колледж»",
        "Ashmoor State University",
        "University of Ashmoor",
        "Ashmoor County Community College",
    ] {
        assert!(org(text), "{text}");
    }
    assert_eq!(
        fixed(
            "ректор ФГБОУ ВО «Северный государственный университет» сообщил",
            &[(
                "Северный государственный университет",
                Category::Organization
            )]
        ),
        ["ФГБОУ ВО «Северный государственный университет»"]
    );
    // An agency's letters or a grand jury inside a private name.
    assert!(person("Nato Venn"));
    assert!(org("NATO Logistics"));
    assert!(org("Grand Jury Games"));
    assert!(!org("the Ashmoor County Grand Jury"));
    // Prose after a law's year is no citation: the company's own
    // abbreviation stays.
    assert_eq!(
        fixed(
            "Ashmoor Freight Safety Act, 2026, applies to Ashmoor Freight Services \
             (hereinafter “AFS”). AFS wrote.",
            &[("AFS", Category::Organization)]
        ),
        ["AFS"]
    );
    // Only a name of initials and capitalized words is a person's: an
    // institution named after one, or a lone period, spares nothing.
    assert!(pack.public_body("ГБУ имени Н. О. Ветлина", Category::Person, false));
    assert!(pack.public_body("Суд .", Category::Person, false));
    assert!(!pack.public_body("Суд Н. О.", Category::Person, false));
    // A body's own address and phone are marked like anyone's (#553).
    let source = "Арбитражный суд Северной области: 190000, г. Ольховск, ул. Лесная, \
                  д. 1, тел. +7 (812) 555-01-01";
    assert_eq!(
        fixed(
            source,
            &[
                ("190000, г. Ольховск, ул. Лесная, д. 1", Category::Address),
                ("+7 (812) 555-01-01", Category::Phone),
            ]
        ),
        [
            "190000, г. Ольховск, ул. Лесная, д. 1",
            "+7 (812) 555-01-01"
        ]
    );
}

/// The second review of probe round 2 of #378: a sentence's end stops
/// a citation, an uppercase surname keeps its initials, only a
/// university or college itself is private, a body's word keeps all
/// its case forms, and a grand jury and an agency's organ go with
/// what follows them.
#[test]
fn second_review_of_public_bodies_round_two() {
    let pack = pack();
    let org = |text: &str| pack.identifying(text, Category::Organization);
    let person = |text: &str| pack.identifying(text, Category::Person);
    // A sentence's end is no citation, and neither is "Act." before
    // the next sentence's capitals: the company keeps its "AFS".
    for source in [
        "Ashmoor Freight Safety Act, 2026. ASHMOOR FREIGHT SERVICES \
         (hereinafter “AFS”). AFS wrote.",
        "Ashmoor Freight Safety Act. ASHMOOR FREIGHT SERVICES (hereinafter \
         “AFS”). AFS wrote.",
    ] {
        assert_eq!(
            fixed(source, &[("AFS", Category::Organization)]),
            ["AFS"],
            "{source}"
        );
    }
    // An uppercase surname with initials is a person; an institution
    // abbreviation with initials is not.
    assert_eq!(
        fixed("Подпись: СУД Н. О.", &[("СУД Н. О.", Category::Person)]),
        ["СУД Н. О."]
    );
    assert!(pack.public_body("ГБУ Н. О.", Category::Person, false));
    // Only a span that names a university or college itself is
    // private; the Electoral College is public.
    for text in [
        "Ministry of Colleges and Universities",
        "ГБУ «Университетская больница»",
        "Министерство науки и университетов",
        "Electoral College",
        "the Electoral College",
        "College of Policing",
    ] {
        assert!(!org(text), "{text}");
    }
    for text in [
        "ФГБОУ ВО «Северный государственный университет»",
        "Северного государственного университета",
        "University of Ashmoor",
        "Royal College of Surgeons",
        "Негосударственный пенсионный фонд «Кедр»",
    ] {
        assert!(org(text), "{text}");
    }
    // A body's word goes as a person in its case forms; a capitalized
    // surname with "-ов" stays.
    for text in [
        "Министерство финансов",
        "Министерства финансов",
        "Министерству финансов",
        "Правительство",
        "Правительства Северной области",
        "Правительством",
        "Управление",
        "Администрацией",
        "Прокуратуру",
        "Прокуратурою",
        "Росимущество",
        "Минстрой",
        "Законодательство",
        "Конституцию",
        "Кодексом",
    ] {
        assert!(!person(text), "{text}");
    }
    assert!(pack.public_body("судов общей юрисдикции", Category::Person, false));
    for text in ["Судов", "Иван Судов", "Законов", "Приставов"] {
        assert!(person(text), "{text}");
    }
    // A grand jury with its jurisdiction, and an agency's organ with
    // an adjective, go; private names stay.
    for text in [
        "Grand Jury of Ashmoor County",
        "the Grand Jury for the Southern District of Ashmoor",
        "NATO Parliamentary Assembly",
        "IMF Executive Board",
    ] {
        assert!(!org(text), "{text}");
    }
    for text in [
        "Grand Jury Games",
        "NATO Logistics",
        "Grand Jury Games of Ashmoor LLC",
    ] {
        assert!(org(text), "{text}");
    }
}

/// Probe round 2 of #378 in English: more agencies and their units,
/// treaties, ordinances, rules and laws whose head is "Law" go, as an
/// organization or a person; companies and people stay.
#[test]
fn english_agencies_treaties_and_laws_of_probe_round_two() {
    let pack = pack();
    let org = |text: &str| pack.identifying(text, Category::Organization);
    let person = |text: &str| pack.identifying(text, Category::Person);
    for text in [
        "Customs",
        "U.S. Customs",
        "Customs and Border Protection",
        "Northvale Constabulary",
        "Elko County Jail",
        "TEXARKANA DIVISION",
        "Utah Division of Wildlife Resources",
        "the Board",
        "Central Office",
        "Secretary of State for the Home Department",
        "Director of Public Prosecutions",
        "Grand Jury",
        "Ashmoor District Registry",
        "ICAO",
        "ICAO Council",
        "NASA",
        "the IMF",
        "NATO",
        "USFWS",
        "Court’s",
        "CAFTA-DR",
        "Environmental Cooperation Agreement",
        "Dunmere Trade Agreement",
        "Environment Chapter",
        "Brussels I",
        "Lugano Conventions",
        "Treaty of Rome",
        "Philadelphia Fair Practices Ordinance",
        "Federal Rules of Bankruptcy Procedure",
        "Tex. R. App. P.",
        "Ashmoor Executive Law",
        "New York State Criminal Procedure Law",
        "Debtor and Creditor Law",
        "the FDCPA",
        "TCPA",
        "FLSA",
        "SB 9",
    ] {
        assert!(!org(text), "{text}");
    }
    for text in [
        "BLM",
        "Customs",
        "Northvale Constabulary",
        "Grand Jury",
        "Brussels I",
    ] {
        assert!(!person(text), "{text}");
    }
    for text in [
        "Customs Brokers Inc",
        "Treaty Oak Capital LLC",
        "Treaty Oak Capital",
        "Ashmoor Convention Center",
        "Agreement Solutions LLC",
        "Brussels Airlines",
        "Quill Tax Law",
        "Board Street Partners LLP",
        "NASA Federal Credit Union",
    ] {
        assert!(org(text), "{text}");
    }
    for text in ["Nasa Quill", "Treaty Quill"] {
        assert!(person(text), "{text}");
    }
    // An abbreviation defined after "hereinafter", or after a citation
    // with commas, is the law wherever it stands.
    assert_eq!(
        fixed(
            "the Ashmoor Freight Safety Act, 12 A.S.C. § 41, et seq. (hereinafter, \
             the “AFSA”) binds Quillon Freight LLC (“QF”). AFSA applies to QF.",
            &[
                ("AFSA", Category::Organization),
                ("QF", Category::Organization),
            ]
        ),
        ["QF"]
    );
}

/// Recheck the outstanding #378 probe shapes against the current pack,
/// as both categories the model used. Collect failures to see the gaps
/// together before extending the existing public-body rules.
#[test]
fn public_bodies_round_three_reported_names() {
    let mut missed = Vec::new();
    for text in [
        "HEALTH SUBCOMMITTEE",
        "OVERSIGHT SUMCOMMITTEE",
        "Federal Deposit Insurance Corporation",
        "EPA Regional Office",
        "Arizona Game and Fish Department",
        "FWS's Law Enforcement Office",
        "FBI Ashmoor Field Office",
        "Ashmoor State Experiment Station",
        "Northern Regional Research Laboratory",
        "Ashmoor Experiment Station",
        "Iova State Experiment Statione",
        "Georgia Experiment Ststion",
        "Northern Regional Reseerch Laboratory",
        "Fungicide-Herbicide Branch Registration Division",
        "Ashmoor County Dispatch",
        "Ashmoor County Medical Examiner",
        "Agricultural Research Service",
        "Soil Conservation Service",
        "Corps of Engineers",
        "Secretary of Defense",
        "Minister of Defence of the French Republic",
        "NATO Land Group 1",
        "Armaments Cooperation Division",
        "Security Authority",
        "National Archives",
        "Basel Committee",
        "International Monetary Fund",
        "Brindle County Police Pension System",
        "Fire and Police Employees' Retirement System of the City of Brindle",
        "Williamson Cnty. Appraisal Dist.",
        "Texas Health and Safety Code",
        "CAFTA-DR Secretariat",
        "ChD",
        "Арбитражный суд Энской области",
        "Министерства обороны Российской Федерации",
        "Пенсионного фонда Российской Федерации",
        "Фонда пенсионного и социального страхованияРоссийской Федерации",
        "ОПФР",
        "Регистрационно-лицензионной палатой",
    ] {
        for category in [Category::Organization, Category::Person] {
            if !fixed(text, &[(text, category)]).is_empty() {
                missed.push(format!("{category:?}: {text}"));
            }
        }
    }
    assert!(missed.is_empty(), "still proposed:\n{}", missed.join("\n"));
}

/// #378 cut spans and defined short names need the body's context;
/// adjacent personal names and private bodies must keep their spans.
#[test]
fn public_bodies_round_three_fragments() {
    let mut missed = Vec::new();
    for (source, span) in [
        ("High Court of Justice", "Justice"),
        ("the Law Enforcement Office", "Office"),
        (
            "Министерства\nфинансов Российской Федерации",
            "финансов Российской Федерации",
        ),
        (
            "Министерства\r\nфинансов Российской Федерации",
            "финансов Российской Федерации",
        ),
        ("бюджет Фонда", "Фонда"),
        ("МО МВД России «Энский»", "Энский"),
        ("Brindle Freight Safety\nAct", "Brindle Freight Safety"),
        (
            "Brindle County Police Pension System (“Brindle P&P”). Brindle P&P replied.",
            "Brindle P&P",
        ),
        (
            "Fire and Police Employees' Retirement System of the City of Baltimore (“Baltimore F&P”).",
            "Baltimore F&P",
        ),
        (
            "Fire and Police Employees' Retirement System of the City of Brindle",
            "Employees' Retirement System",
        ),
    ] {
        for category in [Category::Organization, Category::Person] {
            if !fixed(source, &[(span, category)]).is_empty() {
                missed.push(format!("{category:?}: {source} [span {span}]"));
            }
        }
    }
    assert!(missed.is_empty(), "still proposed:\n{}", missed.join("\n"));
}

/// The reach decision in #378's last comment (#484 item 4): statutory
/// corporations, enterprises, laboratories and regulators are bodies;
/// ownership alone never removes a university or a college.
#[test]
fn public_bodies_round_three_statutory_bodies() {
    let mut missed = Vec::new();
    for text in [
        "PBGC",
        "ARRC",
        "Brookhaven National Laboratory",
        "Brindle NHS Trust",
        "Brindle Port Authority",
        "Law Society of Brindle",
        "Росатом",
        "Роскосмос",
        "ГУП «Энский водоканал»",
        "МУП «Энский водоканал»",
    ] {
        for category in [Category::Organization, Category::Person] {
            if !fixed(text, &[(text, category)]).is_empty() {
                missed.push(format!("{category:?}: {text}"));
            }
        }
    }
    assert!(missed.is_empty(), "still proposed:\n{}", missed.join("\n"));
}

/// Gate review of #643: each reported name survives both span fixes
/// and the proposals the user actually sees, with its category intact.
#[test]
fn public_bodies_gate_review_keeps_names() {
    for (source, span, category) in [
        (
            "Employee: John Smith\nAct of acceptance of completed work",
            "John Smith",
            Category::Person,
        ),
        (
            "For National Grid\nAct of acceptance of completed work",
            "National Grid",
            Category::Organization,
        ),
        ("John Smith\nAct", "John Smith", Category::Person),
        (
            "Brindle Freight Safety\nAct of acceptance of completed work",
            "Brindle Freight Safety",
            Category::Organization,
        ),
        (
            "Employee: Brindle Freight Safety\nAct",
            "Brindle Freight Safety",
            Category::Organization,
        ),
        ("Department\njohn Smith", "john Smith", Category::Person),
        ("Ministry\njohn Smith", "john Smith", Category::Person),
        ("Ministry\r\njohn Smith", "john Smith", Category::Person),
        ("Department john Smith", "john Smith", Category::Person),
        (
            "Министерства\nиван Смирнов",
            "иван Смирнов",
            Category::Person,
        ),
        (
            "Сотрудник МВД России «Иван Смирнов» подписал заявление.",
            "Иван Смирнов",
            Category::Person,
        ),
        (
            "Сотрудник МВД России «Сокол» сообщил о встрече.",
            "Сокол",
            Category::Person,
        ),
        (
            "МВД России «Сокол» сообщил о встрече.",
            "Сокол",
            Category::Person,
        ),
        (
            "Сотрудник МО МВД России «Сокол» сообщил о встрече.",
            "Сокол",
            Category::Person,
        ),
    ] {
        assert_eq!(fixed(source, &[(span, category)]), [span], "{source}");
        let start = source.find(span).unwrap();
        let label = match category {
            Category::Person => "person",
            Category::Organization => "organization",
            _ => unreachable!(),
        };
        let proposed = crate::recognition::proposals(
            source,
            &[crate::RawEntity {
                start,
                end: start + span.len(),
                label: label.into(),
                score: 0.9,
                text: span.into(),
            }],
        )
        .unwrap();
        assert_eq!(
            proposed,
            [crate::recognition::Span {
                range: start..start + span.len(),
                category: if category == Category::Person {
                    "PERSON"
                } else {
                    "ORG"
                }
                .into(),
            }],
            "{source}"
        );
    }
    // An inner agency quotation cannot overrule the private company
    // enclosing it, whose name in nested quotes is one span (#557).
    for source in [
        "ООО «МВД России «Страж»» заключило договор.",
        "ООО «МО МВД России «Страж»» заключило договор.",
    ] {
        let company = source.strip_suffix(" заключило договор.").unwrap();
        assert_eq!(proposed_orgs(source, &["Страж"]), [company], "{source}");
    }
}

/// Private and party qualifiers on either side take precedence over
/// even a direct central-bank relation, in either model category.
#[test]
fn bank_of_england_gate_review_keeps_private_names() {
    let name = "Bank of England";
    for category in [Category::Organization, Category::Person] {
        for source in [
            "The Bank of England, a private Arkansas lender, financed a home in London.",
            "The Bank of England (Lender) serves UK customers.",
            "Private Bank of England discussed UK monetary policy.",
            "The Bank of England, a private bank, discussed monetary policy.",
            "Lender: The Bank of England is the British central bank.",
            "The Bank of England (Lender) is the British central bank.",
            "Party A: The Bank of England is the British central bank.",
            "The Bank of England (Party A) is the British central bank.",
            "Private: The Bank of England is the British central bank.",
            "The Bank of England, private, is the British central bank.",
            "The Bank of England\n(Lender) serves UK customers.",
        ] {
            assert_eq!(fixed(source, &[(name, category)]), [name], "{source}");
            let start = source.find(name).unwrap();
            let proposed = crate::recognition::proposals(
                source,
                &[crate::RawEntity {
                    start,
                    end: start + name.len(),
                    label: if category == Category::Person {
                        "person"
                    } else {
                        "organization"
                    }
                    .into(),
                    score: 0.9,
                    text: name.into(),
                }],
            )
            .unwrap();
            assert!(
                proposed
                    .iter()
                    .any(|span| span.range == (start..start + name.len())),
                "{source}: {proposed:?}"
            );
        }
    }
}

/// Institutional continuations still go when their syntax, rather
/// than the preceding bare office word, establishes the body's name.
#[test]
fn public_body_continuations_need_institutional_syntax() {
    for (source, span) in [
        ("Department\nof Justice", "of Justice"),
        ("Ministry\nof Defence", "of Defence"),
        (
            "Министерства\nфинансов Российской Федерации",
            "финансов Российской Федерации",
        ),
        (
            "Министерства\nобороны Российской Федерации",
            "обороны Российской Федерации",
        ),
        ("Brindle Freight Safety\nAct", "Brindle Freight Safety"),
        (
            "Biometric Information Privacy\r\nAct",
            "Biometric Information Privacy",
        ),
        ("Data Protection\nAct", "Data Protection"),
        ("МО МВД России «Энский»", "Энский"),
    ] {
        for category in [Category::Organization, Category::Person] {
            assert!(fixed(source, &[(span, category)]).is_empty(), "{source}");
        }
    }
}

/// #378 Q2: the Arkansas lender shares the central bank's name.
/// Only an article and a direct institutional relation disambiguate
/// it; geography and discussion of public policy are insufficient.
#[test]
fn bank_of_england_needs_public_context() {
    let name = "Bank of England";
    for category in [Category::Organization, Category::Person] {
        for source in [
            "Lender: Bank of England",
            "Bank of England",
            "The Bank of England supplied the loan",
            "Bank of England discussed Monetary Policy",
            "The Governor spoke. The Bank of England replied",
            "UK policy\n\nThe Bank of England supplied the loan",
            "The Bank of England Services Ltd discussed Monetary Policy",
            "The Bank of England discussed Governor",
            "The Bank of England discussed Monetary Policy",
            "The Bank of England discussed Threadneedle",
            "The Bank of England discussed Prudential Regulation",
            "The Bank of England discussed UK",
            "The Bank of England discussed United Kingdom",
            "The Bank of England discussed British",
            "The Bank of England discussed London",
        ] {
            let span = if source.contains("Services Ltd") {
                "Bank of England Services Ltd"
            } else {
                name
            };
            assert_eq!(fixed(source, &[(span, category)]), [span], "{source}");
        }
        for source in [
            "The Governor of the Bank of England replied",
            "The Bank of England's Monetary Policy Committee replied",
            "The Bank of England's Prudential Regulation Authority replied",
            "The Bank of England, the UK's central bank, replied",
            "The Bank of England is the British central bank",
        ] {
            assert!(fixed(source, &[(name, category)]).is_empty(), "{source}");
            for label in [
                "Lender",
                "Borrower",
                "Party",
                "Plaintiff",
                "Defendant",
                "Lenders",
                "Party A",
                "Lender (UK)",
            ] {
                for separator in [": ", ":\n"] {
                    let source = format!("{label}{separator}{source}");
                    // The article may be outside or inside the span.
                    let article = if source.contains("the Bank") {
                        "the"
                    } else {
                        "The"
                    };
                    for span in [name, &format!("{article} Bank of England")] {
                        assert_eq!(fixed(&source, &[(span, category)]), [span], "{source}");
                    }
                }
            }
        }
    }
}

/// #378 Q2: the production company's complete trading name is private;
/// an office needs its public parent inside the proposed span.
#[test]
fn field_office_needs_a_public_parent() {
    for category in [Category::Organization, Category::Person] {
        for (source, span) in [
            ("The Field Office supplied the film", "The Field Office"),
            ("The Field Office supplied the film", "Field Office"),
            ("The Ashmoor Field Office replied", "Ashmoor Field Office"),
            ("Ice Field Office supplied the film", "Ice Field Office"),
            ("The FBI contacted the Field Office", "Field Office"),
            (
                "FBI Field Office Ltd supplied the film",
                "FBI Field Office Ltd",
            ),
        ] {
            // An ORG ending in "Office" is a private department (#506)
            // and goes anyway; a legal form keeps it.
            if category == Category::Organization && span.ends_with("Office") {
                continue;
            }
            assert_eq!(fixed(source, &[(span, category)]), [span], "{source}");
        }
        for span in [
            "FBI Field Office",
            "FBI Ashmoor Field Office",
            "ICE Field Office",
            "USCIS Field Office",
            "Social Security Field Office",
            "Department of Labor Field Office",
            "Ashmoor Agency Field Office",
            "Ashmoor Bureau Field Office",
            "Ashmoor Service Field Office",
        ] {
            assert!(fixed(span, &[(span, category)]).is_empty(), "{span}");
        }
    }
}

/// #378 Q2: Foundation and Hospital are parts of NHS trust names.
#[test]
fn nhs_foundation_trusts_are_public_bodies() {
    for span in [
        "Oxleas NHS Foundation Trust",
        "Oxleas NHS Trust",
        "Oxleas NHS Foundation Trust Hospital",
        "NHS Foundation Trust",
        "NHS Trust",
        "NHS Foundation Trust Hospital",
        "Ashmoor and Brindle NHS Foundation Trust",
        "Ashmoor & Brindle NHS Foundation Trust",
        "Ashmoor NHS\nFoundation Trust",
        "Ashmoor NHS\r\nFoundation Trust",
        "Ashmoor and\nBrindle NHS Foundation Trust",
        "Ashmoor NHS Foundation\nTrust",
    ] {
        for category in [Category::Organization, Category::Person] {
            assert!(fixed(span, &[(span, category)]).is_empty(), "{span}");
        }
    }
    for private in [
        "Oxleas NHS Foundation Trust Services Ltd",
        "Ashmoor and Brindle NHS Foundation Trust Services Ltd",
        "Ashmoor & Brindle NHS\nFoundation Trust Services Ltd",
        "Ashmoor NHS\n\nFoundation Trust",
        "Ashmoor and Brindle NHS Foundation Trust Consulting",
    ] {
        for category in [Category::Organization, Category::Person] {
            assert_eq!(
                fixed(private, &[(private, category)]),
                [private],
                "{private}"
            );
        }
    }
}

/// The privacy side of #378: similar people, companies, universities,
/// non-state funds and the body's own identifiers remain marked.
#[test]
fn public_bodies_round_three_preserves_names_and_details() {
    for text in [
        "Quillon Freight LLC",
        "Bank of England Services Ltd",
        "Security Authority Ltd",
        "Basel Committee Consulting",
        "Research Laboratory Partners",
        "Field Office Supplies",
        "Ashmoor Experiment Station Ltd",
        "Experiment Station Productions",
        "Regional Research Laboratory Services",
        "NATO Logistics",
        "NASA Federal Credit Union",
        "Treaty Oak Capital",
        "Ashmoor County Community College",
        "ФГБОУ ВО «Энский государственный университет»",
        "Негосударственный пенсионный фонд «Кедровый бор»",
        "Благотворительный фонд «Кедровый бор»",
        "ООО «Палата»",
    ] {
        assert_eq!(
            fixed(text, &[(text, Category::Organization)]),
            [text],
            "{text}"
        );
    }
    for (source, span) in [
        ("Ms Justice wrote", "Justice"),
        ("Secretary of Defense Orrin Quill wrote", "Orrin Quill"),
        ("Minister of Defence Orrin Quill wrote", "Orrin Quill"),
        ("начальник Министерства де Лаваль", "де Лаваль"),
        ("Суд Н. О.", "Суд Н. О."),
        ("Иван Судов", "Иван Судов"),
        ("ГУ МВД России: Иван Ветлин", "Иван Ветлин"),
        ("Police met Orrin Quill (“Orrin Q”).", "Orrin Q"),
        (
            "Brindle County Police Pension System; Brindle Paper Partners (“Brindle P&P”).",
            "Brindle P&P",
        ),
    ] {
        assert_eq!(
            fixed(source, &[(span, Category::Person)]),
            [span],
            "{source}"
        );
    }
    // The same short name also defined for a company is ambiguous.
    assert_eq!(
        fixed(
            "Brindle County Police Pension System (“Brindle P&P”); \
             Brindle Paper Partners LLC (“Brindle P&P”).",
            &[("Brindle P&P", Category::Organization)],
        ),
        ["Brindle P&P"]
    );
    // A line wrap is context; a paragraph break or a sentence is not.
    assert_eq!(
        fixed(
            "Министерства\n\nФинансов Иван",
            &[("Финансов Иван", Category::Person)]
        ),
        ["Финансов Иван"]
    );
    assert_eq!(
        fixed(
            "Министерства\n\nфинансов Российской Федерации",
            &[("финансов Российской Федерации", Category::Organization)]
        ),
        ["финансов Российской Федерации"]
    );
    assert_eq!(
        fixed(
            "Treaty Oak Capital Agreement",
            &[("Treaty Oak Capital", Category::Organization)]
        ),
        ["Treaty Oak Capital"]
    );
    assert_eq!(
        fixed(
            "Quillon Freight LLC\nAct",
            &[("Quillon Freight LLC", Category::Organization)]
        ),
        ["Quillon Freight LLC"]
    );
    for (value, category) in [
        ("41 Larch Road, Ashmoor AS1 9ZZ", Category::Address),
        ("+1 (202) 555-0164", Category::Phone),
        ("office@quarry.invalid", Category::Email),
        ("7701234567", Category::Tax),
        ("40702810900000012345", Category::Bank),
    ] {
        let source = format!("Bureau of Quarry Safety / Министерство финансов: {value}");
        assert_eq!(fixed(&source, &[(value, category)]), [value]);
    }
}

/// Exercise the complete correction/review path on the new synthetic
/// documents with injected model spans; no model snapshot is written.
#[test]
fn public_bodies_round_three_fixtures_with_injected_spans() {
    use crate::recognition::{Fixture, proposals};
    for (lang, extra) in [
        (
            "en",
            vec![
                ("Ashmoor Field Office", Category::Organization),
                ("Northern Regional Research Laboratory", Category::Person),
                ("Justice", Category::Person),
                ("Law Enforcement Office", Category::Organization),
                ("Brindle P&P", Category::Person),
                ("Brindle Freight Safety", Category::Organization),
            ],
        ),
        (
            "ru",
            vec![
                ("МО МВД России «Энский»", Category::Organization),
                ("Энский»", Category::Person),
                ("финансов Российской Федерации", Category::Organization),
                ("МУП «Энский водоканал»", Category::Person),
            ],
        ),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../tests/fixtures/recognition/{lang}/judgment/probe-{lang}-r3-public-bodies-r3.md"
        ));
        let fixture = Fixture::read(&path).unwrap();
        fixture.validate().unwrap();
        fixture.validate_layout().unwrap();
        let raw = |range: std::ops::Range<usize>, category: Category| {
            let label = crate::FAMILIES
                .iter()
                .flat_map(|family| family.iter())
                .find(|(_, value)| *value == category)
                .unwrap()
                .0;
            crate::RawEntity {
                start: range.start,
                end: range.end,
                label: label.into(),
                score: 0.9,
                text: fixture.source[range].to_owned(),
            }
        };
        let mut entities: Vec<_> = fixture
            .gold
            .iter()
            .map(|span| raw(span.range.clone(), category(&span.category).unwrap()))
            .collect();
        for (needle, category) in extra {
            assert!(fixture.source.contains(needle), "{lang}: {needle}");
            for (at, _) in fixture.source.match_indices(needle) {
                entities.push(raw(at..at + needle.len(), category));
            }
        }
        let mut proposed = proposals(&fixture.source, &entities).unwrap();
        // "Ashmoor Field Office" goes as a private department (#506),
        // as the gold has it.
        let mut gold = fixture.gold;
        proposed.sort();
        gold.sort();
        assert_eq!(proposed, gold, "{lang}");
    }
}

/// A public-law company is a public body, by its form or its name
/// (guide rule 8, #484 item 4); companies stay.
#[test]
fn a_public_law_company_is_a_public_body() {
    let source = "Сведения предоставила публично-правовая компания «Роскадастр», договор \
        заключён с ППК «Фонд развития территорий», филиал Роскадастра ответил, а \
        ООО «Кадастр-Сервис» и АО «Фонд Сибири» подписали акт.";
    let raw = [
        ("Роскадастр", "organization"),
        ("Фонд развития территорий", "organization"),
        ("Роскадастра", "organization"),
        ("Кадастр-Сервис", "organization"),
        ("Фонд Сибири", "organization"),
    ];
    assert_eq!(
        shown(source, &raw),
        pairs(&[("ООО «Кадастр-Сервис»", "ORG"), ("АО «Фонд Сибири»", "ORG")])
    );
    for (text, category) in [
        ("ППК «Роскадастр»", Category::Organization),
        (
            "публично-правовой компании «Фонд развития территорий»",
            Category::Organization,
        ),
        ("Роскадастру", Category::Person),
    ] {
        assert!(!pack().identifying(text, category), "{text}");
    }
    // The company's quoted name as a person is no one either.
    let source = "Договор заключён с ППК «Фонд развития территорий».";
    assert_eq!(
        shown(source, &[("Фонд развития территорий", "person")]),
        pairs(&[])
    );
}

#[test]
fn private_departments_are_not_organizations_but_company_forms_and_people_stay() {
    for name in [
        "Legal Team",
        "Sales Team",
        "Data Protection Officer",
        "Planning Inspector",
        "Acme Legal Team",
        "Acme Office",
        "Acme Department",
        "Acme Committee",
        "Acme Board",
        "Acme Unit",
        "Acme Division",
        "Acme Desk",
        "Ashmoor Advisory Board",
    ] {
        assert!(shown(name, &[(name, "organization")]).is_empty(), "{name}");
    }
    for name in [
        "Officer & Co",
        "Leader Group",
        "Trustee Ltd",
        "Acme Legal Team Ltd",
        "ООО «Acme Office»",
    ] {
        assert_eq!(
            shown(name, &[(name, "organization")]),
            pairs(&[(name, "ORG")]),
            "{name}"
        );
    }
    let name = "Brannock Officer";
    assert_eq!(shown(name, &[(name, "person")]), pairs(&[(name, "PERSON")]));
}

#[test]
fn public_bodies_misclassified_as_people_still_go_after_role_changes() {
    for body in [
        "Министерство финансов",
        "Арбитражный суд города Москвы",
        "Department of Justice",
    ] {
        assert!(shown(body, &[(body, "person")]).is_empty(), "{body}");
    }
}
