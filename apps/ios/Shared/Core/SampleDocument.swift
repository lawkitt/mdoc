import Foundation

/// The sample document behind "Try a sample" (ADR 0099): a short contract
/// with made-up names, in English and Russian, chosen by the phone's
/// language. The UI tests and previews use the same text, so screenshots of
/// it are safe to share. Every name, address and number is invented: the
/// phone numbers are in Ofcom's drama range and an all-zero Russian
/// number, the emails use the reserved example.com domain.
struct SampleDocument: Sendable, Equatable {
    struct Name: Sendable, Equatable {
        var wording: String
        var category: Category
    }

    var title: String
    var text: String
    /// What the sample's review proposes, so it works before the name
    /// finder is downloaded.
    var names: [Name]

    /// Russian when the phone's first language is Russian, English
    /// otherwise.
    static func forLanguages(_ languages: [String] = Locale.preferredLanguages) -> SampleDocument {
        languages.first.map { $0.hasPrefix("ru") } == true ? russian : english
    }

    static let english = SampleDocument(
        title: "Sample: services agreement",
        text: """
            # Services agreement

            **Between:** Riverside Holdings Ltd, 14 Quayside Walk, Exeter EX1 9ZZ \
            ("the Client")
            **And:** Maya Bennett, trading as Bennett Design, 22 Orchard Row, \
            Exeter EX4 9ZZ ("the Designer")
            **Date:** 3 November 2026

            ## 1. The work

            The Designer will redesign the Client's offices at 3 Mill Lane, \
            Taunton TA1 9ZZ. Ms Bennett will report to Daniel Okafor, the \
            Client's facilities manager.

            ## 2. Fees

            The Client will pay £18,500 in two parts to the account below. \
            Riverside Holdings Ltd may withhold the second part until \
            Mr Okafor signs off the work.

            | Item | Amount |
            |---|---|
            | Deposit | £5,000 |
            | On completion | £13,500 |

            ## 3. Contact

            Questions go to maya.bennett@example.com or 01632 960 418.

            Signed for Riverside Holdings Ltd by Daniel Okafor.
            Signed by Maya Bennett.
            """,
        names: [
            Name(wording: "Riverside Holdings Ltd", category: .organization),
            Name(wording: "14 Quayside Walk, Exeter EX1 9ZZ", category: .address),
            Name(wording: "Maya Bennett", category: .person),
            Name(wording: "Bennett Design", category: .organization),
            Name(wording: "22 Orchard Row, Exeter EX4 9ZZ", category: .address),
            Name(wording: "3 Mill Lane, Taunton TA1 9ZZ", category: .address),
            Name(wording: "Ms Bennett", category: .person),
            Name(wording: "Daniel Okafor", category: .person),
            Name(wording: "Mr Okafor", category: .person),
            Name(wording: "maya.bennett@example.com", category: .email),
            Name(wording: "01632 960 418", category: .phone),
        ])

    static let russian = SampleDocument(
        title: "Образец: договор оказания услуг",
        text: """
            # Договор оказания услуг

            **Заказчик:** ООО «Речная гавань», г. Тверь, ул. Набережная, д. 14
            **Исполнитель:** Беннет Майя Андреевна, ИП, г. Тверь, \
            ул. Садовая, д. 22
            **Дата:** 3 ноября 2026 г.

            ## 1. Предмет договора

            Исполнитель обновляет дизайн офиса Заказчика по адресу \
            г. Торжок, ул. Мельничная, д. 3. Майя Беннет согласует работу \
            с Даниилом Окафоровым, руководителем хозяйственной службы Заказчика.

            ## 2. Стоимость

            Заказчик оплачивает 1 850 000 рублей двумя частями. \
            ООО «Речная гавань» вправе удержать вторую часть, пока \
            Д. Окафоров не подпишет акт.

            | Этап | Сумма |
            |---|---|
            | Аванс | 500 000 ₽ |
            | По завершении | 1 350 000 ₽ |

            ## 3. Контакты

            Вопросы направляйте на maya.bennett@example.com или по телефону \
            +7 900 000-00-00.

            От Заказчика: Даниил Окафоров.
            Исполнитель: Майя Беннет.
            """,
        names: [
            Name(wording: "ООО «Речная гавань»", category: .organization),
            Name(wording: "г. Тверь, ул. Набережная, д. 14", category: .address),
            Name(wording: "Беннет Майя Андреевна", category: .person),
            Name(wording: "г. Тверь, ул. Садовая, д. 22", category: .address),
            Name(wording: "г. Торжок, ул. Мельничная, д. 3", category: .address),
            Name(wording: "Майя Беннет", category: .person),
            Name(wording: "Даниилом Окафоровым", category: .person),
            Name(wording: "Д. Окафоров", category: .person),
            Name(wording: "Даниил Окафоров", category: .person),
            Name(wording: "maya.bennett@example.com", category: .email),
            Name(wording: "+7 900 000-00-00", category: .phone),
        ])
}
