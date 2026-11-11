import SwiftUI
import UIKit
import XCTest
@testable import mdoc

/// How far the text scrolls to the name under review (ADR 0098, amended
/// 2026-11-11): a name that shows whole between the bars leaves the text
/// where it is, any other scrolls just into view, and a decision that
/// changes the text keeps a visible name under review where it is on
/// screen. The bar's steps run as `DocumentViewModel` runs them, over the
/// real `ReviewState`, coordinator and text view.
@MainActor
final class ReviewScrollTests: XCTestCase {
    /// A made-up form a few screens long: five tables, three wider than the
    /// screen, a task list, and names in cells and paragraphs.
    private static let form = """
        # Анкета заявителя на получение разрешения

        Настоящая анкета заполнена заявителем собственноручно. Все сведения в ней вымышлены и служат только для проверки отображения длинного документа с таблицами.

        ## 1. Сведения о заявителе

        | Поле | Значение |
        | --- | --- |
        | Фамилия, имя, отчество | Орлов Тимофей Сергеевич |
        | Дата рождения | 14.03.1987 |
        | Место рождения | город Верхнереченск, Заозёрная область |
        | Гражданство | Республика Северия |
        | Телефон | +7 900 555-01-42 |
        | Электронная почта | t.orlov@example.com |

        Заявитель Орлов Тимофей Сергеевич подтверждает, что указанные выше сведения верны, и обязуется сообщить об их изменении в течение десяти рабочих дней.

        ## 2. Документы, удостоверяющие личность

        | Вид документа | Серия и номер | Кем выдан | Дата выдачи | Срок действия | Примечание |
        | --- | --- | --- | --- | --- | --- |
        | Паспорт гражданина | 45 07 123456 | Отделом по вопросам миграции Верхнереченска | 02.04.2007 | бессрочно | Основной документ |
        | Заграничный паспорт | 71 4455667 | Управлением по вопросам миграции Заозёрной области | 19.08.2019 | 19.08.2029 | Предъявлен в оригинале |
        | Водительское удостоверение | 99 12 345678 | Отделением регистрации Верхнереченска | 23.11.2015 | 23.11.2025 | Категории B, C |

        Копии документов приложены к анкете и заверены сотрудником приёмной Лебедевой Мариной Андреевной.

        ## 3. Члены семьи

        Заявитель проживает совместно с членами семьи, перечисленными ниже. Сведения о несовершеннолетних детях вносятся со слов заявителя.

        | Степень родства | Фамилия, имя, отчество | Дата рождения | Место работы или учёбы | Телефон | Электронная почта | Адрес проживания |
        | --- | --- | --- | --- | --- | --- | --- |
        | Супруга | Орлова Вероника Павловна | 30.09.1989 | Общество «Северный маяк», бухгалтер | +7 900 555-01-77 | v.orlova@example.com | Верхнереченск, улица Сосновая, дом 12, квартира 48 |
        | Сын | Орлов Глеб Тимофеевич | 05.06.2014 | Гимназия номер 3, пятый класс | нет | нет | Верхнереченск, улица Сосновая, дом 12, квартира 48 |
        | Мать | Орлова Зинаида Фёдоровна | 11.01.1958 | На пенсии | +7 900 555-01-13 | нет | Посёлок Ключевой, улица Речная, дом 7 |

        ## 4. Трудовая деятельность

        Сведения о работе за последние десять лет указываются в обратном порядке, начиная с последнего места работы. Периоды без работы поясняются отдельно.

        - [x] Трудовая книжка предъявлена
        - [x] Справка с последнего места работы приложена
        - [ ] Сведения о доходах за прошлый год приложены
        - [ ] Согласие работодателя получено

        | Период | Организация | Должность | Руководитель |
        | --- | --- | --- | --- |
        | 2019 — настоящее время | Общество «Заозёрные линии» | Ведущий инженер | Громов Аркадий Ильич |
        | 2014 — 2019 | Общество «Северный маяк» | Инженер | Лебедева Марина Андреевна |
        | 2009 — 2014 | Верхнереченский завод приборов | Техник | Громов Аркадий Ильич |

        Руководитель по последнему месту работы, Громов Аркадий Ильич, готов подтвердить сведения по телефону +7 900 555-01-90 или по адресу a.gromov@example.com.

        ## 5. Сведения о поездках

        За последние пять лет заявитель выезжал за пределы страны три раза. Цели поездок и принимающие лица указаны в таблице.

        | Страна | Период | Цель | Принимающее лицо | Контакт принимающего лица | Комментарий |
        | --- | --- | --- | --- | --- | --- |
        | Королевство Вестмар | 03.05.2021 — 17.05.2021 | Обучение | Хельга Нордстрём | h.nordstrom@example.com | Курсы повышения квалификации, оплачены работодателем |
        | Республика Ладения | 12.10.2022 — 20.10.2022 | Командировка | Томаш Велич | +420 555 010 203 | Пусконаладочные работы на объекте заказчика |
        | Королевство Вестмар | 01.07.2023 — 14.07.2023 | Отдых | Хельга Нордстрём | h.nordstrom@example.com | Частная поездка с семьёй |

        ### Пояснения к поездкам

        Во время командировки в Республику Ладению заявитель работал вместе с Томашем Величем. Обратный билет был куплен заранее, выезд состоялся 20.10.2022 без задержек.

        ## 6. Заявление

        Я, Орлов Тимофей Сергеевич, прошу выдать мне разрешение сроком на три года. С порядком рассмотрения анкеты ознакомлен. О решении прошу сообщить по адресу t.orlov@example.com или по телефону +7 900 555-01-42.

        1. Анкету принял сотрудник приёмной: Лебедева Марина Андреевна.
        2. Дата приёма: 09.02.2024.
        3. Регистрационный номер: АН-2024-000318.

        Подпись заявителя: ____________ Орлов Т. С.

        Подпись сотрудника: ____________ Лебедева М. А.

        ## 7. Памятка заявителю

        Срок рассмотрения анкеты составляет до тридцати рабочих дней. При необходимости дополнительной проверки срок может быть продлён, о чём заявителю сообщают письменно. Заявитель вправе отозвать анкету в любое время до принятия решения.

        Вопросы о ходе рассмотрения можно задать по телефону приёмной +7 900 555-01-00 в рабочие дни. Обращения по электронной почте priemnaya@example.com рассматриваются в течение трёх рабочих дней.

        Все имена, адреса и номера в этом документе вымышлены.
        """

    private static let names: [SampleDocument.Name] = [
        "Орлов Тимофей Сергеевич", "Орлова Вероника Павловна", "Орлов Глеб Тимофеевич", "Орлова Зинаида Фёдоровна",
        "Лебедева Марина Андреевна", "Лебедевой Мариной Андреевной", "Громов Аркадий Ильич", "Хельга Нордстрём",
        "Томаш Велич", "Томашем Величем",
    ].map { .init(wording: $0, category: .person) } + [
        "Общество «Северный маяк»", "Общество «Заозёрные линии»",
    ].map { .init(wording: $0, category: .organization) }

    /// The bars the screen's safe area gives the text: the navigation bar
    /// and the docked review bar.
    private static let topBar: CGFloat = 48
    private static let bar: CGFloat = 190

    private enum Action {
        case replace, keep, replaceThis, next, previous, replaceInstead, keepInstead, nextUndecided
    }

    /// One step of the bar: the name it ends on, where that name was on
    /// screen before (nil when not wholly in view) and where it is after.
    private struct Step {
        let action: Action
        let name: String
        let before: CGFloat?
        let after: CGFloat?
        let inView: Bool
    }

    @MainActor
    private final class Review {
        let session: any mdoc.DocumentSession
        let window: UIWindow
        let view: CandidateTextView
        let coordinator: MarkdownTextView.Coordinator
        var state: ReviewState
        var snapshot: DocumentSnapshot

        init() throws {
            let (core, _) = try Fixture.core()
            session = try core.library.importDocument(.text(ReviewScrollTests.form), title: "Form", into: .newCase(name: "Form"))
            _ = try session.recognize(observer: TestObserver())
            snapshot = try session.propose(ReviewScrollTests.names)
            state = ReviewState(snapshot)
            window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
            view = CandidateTextView()
            view.isEditable = false
            view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 32, right: 12)
            view.frame = window.bounds
            view.contentInset = UIEdgeInsets(top: ReviewScrollTests.topBar, left: 0, bottom: ReviewScrollTests.bar, right: 0)
            window.addSubview(view)
            window.isHidden = false
            view.layoutIfNeeded()
            coordinator = MarkdownTextView.Coordinator(parent: Self.textView(snapshot, state))
            coordinator.attach(to: view)
            view.delegate = coordinator
            show()
            state.begin()
            show()
            settle()
        }

        private static func textView(_ snapshot: DocumentSnapshot, _ state: ReviewState) -> MarkdownTextView {
            MarkdownTextView(
                snapshot: snapshot, selected: state.reviewing, scoped: state.scopedMentions, reveal: state.reveal,
                onCandidate: { _ in }, onReplace: { _ in }, onReviewNext: {})
        }

        func show() {
            coordinator.update(view, with: Self.textView(snapshot, state), dynamicTypeSize: .large, colorScheme: .light)
            view.layoutIfNeeded()
        }

        /// Runs the run loop until the text has stopped scrolling.
        func settle() {
            let deadline = Date().addingTimeInterval(4)
            var last = view.contentOffset
            var still = Date()
            while Date() < deadline {
                RunLoop.main.run(until: Date().addingTimeInterval(0.02))
                if view.contentOffset != last {
                    last = view.contentOffset
                    still = Date()
                } else if Date().timeIntervalSince(still) > 0.3 {
                    return
                }
            }
        }

        /// Where the candidate is drawn now, in the text view's coordinates.
        func drawnRect(of candidate: Candidate) -> CGRect? {
            view.setNeedsLayout()
            view.layoutIfNeeded()
            return coordinator.place(of: TextOffsets(snapshot.markdown).nsRange(candidate.range), in: view, onlyShown: true)?.rect
        }

        /// The candidate's top below the top bar, when it shows whole
        /// between the bars and inside the view's width.
        func shownTop(of candidate: Candidate) -> CGFloat? {
            guard let rect = drawnRect(of: candidate) else { return nil }
            let top = view.contentOffset.y + ReviewScrollTests.topBar
            let bottom = view.contentOffset.y + view.bounds.height - ReviewScrollTests.bar
            guard rect.minY >= top - 0.5, rect.maxY <= bottom + 0.5, rect.minX >= 0, rect.maxX <= view.bounds.width else { return nil }
            return rect.minY - top
        }

        /// The reader scrolls the name under review to just below the top
        /// bar, so the names after it are in view too.
        func scrollTheNameNearTheTop() {
            guard let current = state.reviewedCandidate,
                  let rect = coordinator.place(of: TextOffsets(snapshot.markdown).nsRange(current.range), in: view)?.rect else { return }
            let highest = -ReviewScrollTests.topBar
            let lowest = max(highest, view.contentSize.height + ReviewScrollTests.bar - view.bounds.height)
            coordinator.scrollViewWillBeginDragging(view)
            view.setContentOffset(CGPoint(x: 0, y: min(max(rect.minY - ReviewScrollTests.topBar - 24, highest), lowest)), animated: false)
            view.layoutIfNeeded()
        }

        /// One action of the bar, as `DocumentViewModel` runs it, and the
        /// text until it has stopped scrolling.
        func act(_ action: Action) throws -> Step? {
            guard state.reviewedCandidate != nil else { return nil }
            var before: [CandidateID: CGFloat] = [:]
            for candidate in state.order.candidates {
                if let top = shownTop(of: candidate) { before[candidate.id] = top }
            }
            try perform(action)
            settle()
            guard let now = state.reviewedCandidate else { return nil }
            let after = shownTop(of: now)
            // Where it is, for a failure's message.
            let place = "\(drawnRect(of: now).map { "\($0)" } ?? "not laid out") in \(view.bounds)"
            return Step(
                action: action, name: "\(now.original) (\(now.state)) at \(place)", before: before[now.id], after: after,
                inView: after != nil)
        }

        /// The action alone, shown: the run loop has not turned, so a
        /// reveal it owes has not run, and a scroll on its way goes on.
        func perform(_ action: Action) throws {
            guard let current = state.reviewedCandidate else { return }
            let index = state.order.index(of: current.id)
            switch action {
            case .replace, .keep, .replaceThis:
                if action == .replaceThis { state.setScope(.this) }
                snapshot = try session.decide(current.id, action == .keep ? .keep : .replace, scope: state.scope)
                state.apply(snapshot)
                state.decided()
                _ = state.advance(from: index)
            case .replaceInstead, .keepInstead:
                snapshot = try session.decide(current.id, action == .replaceInstead ? .replace : .keep, scope: state.scope)
                state.apply(snapshot)
                state.decided()
                state.reselect(near: current, index: index)
            case .next: state.step(1)
            case .previous: state.step(-1)
            case .nextUndecided: _ = state.advance(from: index)
            }
            show()
        }

        /// The run loop for a moment: a reveal owed runs, a scroll gets
        /// part of its way.
        func turn(_ seconds: TimeInterval) {
            RunLoop.main.run(until: Date().addingTimeInterval(seconds))
        }

        /// The name under review shows whole and, a while later, has not
        /// moved.
        func assertTheNameRests(_ message: String, file: StaticString = #filePath, line: UInt = #line) throws {
            settle()
            let now = try XCTUnwrap(state.reviewedCandidate, message, file: file, line: line)
            let top = try XCTUnwrap(shownTop(of: now), "\(message): \(now.original) is in view", file: file, line: line)
            turn(0.6)
            XCTAssertEqual(shownTop(of: now) ?? -1, top, accuracy: 0.5, "\(message): \(now.original) has stopped", file: file, line: line)
        }
    }

    /// Every step ends with the name whole in view, and a name that was
    /// in view already has not moved.
    private func check(_ steps: [Step], visibleBefore least: Int, file: StaticString = #filePath, line: UInt = #line) {
        var stayed = 0
        for (number, step) in steps.enumerated() {
            XCTAssertTrue(step.inView, "step \(number + 1) \(step.action): \(step.name) is in view", file: file, line: line)
            guard let before = step.before, let after = step.after else { continue }
            stayed += 1
            XCTAssertEqual(
                after, before, accuracy: 1,
                "step \(number + 1) \(step.action): \(step.name) was in view and has not moved", file: file, line: line)
        }
        XCTAssertGreaterThanOrEqual(stayed, least, "steps onto a name already in view", file: file, line: line)
    }

    /// Swiping the card to the next name and back: a name in view leaves
    /// the text where it is (the top third pulled it up), and each name
    /// ends in view, to the last one and back.
    func testSwipingToANameInViewDoesNotMoveTheText() throws {
        let review = try Review()
        let count = review.state.order.candidates.count
        XCTAssertGreaterThanOrEqual(count, 16)
        var steps: [Step] = []
        for _ in 1..<count { try review.act(.next).map { steps.append($0) } }
        for _ in 1..<count { try review.act(.previous).map { steps.append($0) } }
        XCTAssertEqual(steps.count, 2 * (count - 1))
        check(steps, visibleBefore: 5)
    }

    /// Replace and Keep to the end, the name under review near the top of
    /// the view each time: the next name ends in view after each decision
    /// rebuilt the text, and one in view already stays put.
    func testDecidingKeepsTheNextNameInViewAndAVisibleOneInPlace() throws {
        for decisions: [Action] in [[.replace], [.keep], [.replaceThis, .replaceThis, .keep]] {
            let review = try Review()
            var steps: [Step] = []
            var turn = 0
            while review.state.isReviewing, turn < 40 {
                review.scrollTheNameNearTheTop()
                try review.act(decisions[turn % decisions.count]).map { steps.append($0) }
                turn += 1
            }
            XCTAssertFalse(review.state.isReviewing, "all reviewed")
            check(steps, visibleBefore: 2)
        }
    }

    /// Keep instead and Replace instead on the card just decided, then
    /// Next: the name keeps its place while its wording, and the lines
    /// above it, change.
    func testChangingADecidedNameKeepsItsPlace() throws {
        let review = try Review()
        var steps: [Step] = []
        for _ in 0..<8 {
            for action: Action in [.replaceThis, .previous, .keepInstead, .replaceInstead, .nextUndecided] {
                try review.act(action).map { steps.append($0) }
            }
        }
        // Keep instead and Replace instead stay on a name in view: 16 of them.
        check(steps, visibleBefore: 16)
    }

    /// Two swipes before the run loop turns: the reveal owed to the first
    /// name is the second's. It stayed the first's, and the name the bar
    /// was on ended off screen (review of #814).
    func testASecondSwipeBeforeTheRevealRunsWins() throws {
        let review = try Review()
        // Onto names further and further down, none in view from the one
        // before: each swipe alone scrolls.
        for swipes in [2, 3, 5] {
            for _ in 0..<swipes { try review.perform(.next) }
            XCTAssertNotNil(review.coordinator.pendingReveal, "the reveal has not run")
            try review.assertTheNameRests("\(swipes) swipes at once")
        }
        for _ in 0..<6 { try review.perform(.previous) }
        try review.assertTheNameRests("six swipes back at once")
    }

    /// A swipe to a distant name and, while the text is on its way there,
    /// a swipe back to the name still in view: the scroll stops, and the
    /// name stays in view. It went on to the name let go and carried the
    /// one under review off screen (review of #814).
    func testSwipingBackDuringTheScrollStopsIt() throws {
        let review = try Review()
        let view = review.view
        var checked = 0
        for _ in 0..<12 {
            guard let from = review.state.reviewedCandidate, review.shownTop(of: from) != nil else { break }
            let start = view.contentOffset.y
            try review.perform(.next)
            // Until the scroll is on its way: a name in view scrolls nothing.
            let deadline = Date().addingTimeInterval(0.25)
            while abs(view.contentOffset.y - start) < 4, Date() < deadline { review.turn(0.01) }
            let onItsWay = abs(view.contentOffset.y - start) >= 4
            if onItsWay, review.shownTop(of: from) != nil {
                try review.perform(.previous)
                let stopped = view.contentOffset.y
                try review.assertTheNameRests("back during the scroll")
                XCTAssertEqual(review.state.reviewing, from.id)
                XCTAssertEqual(view.contentOffset.y, stopped, accuracy: 40, "the scroll stopped about where the swipe back found it")
                checked += 1
                // On again, to try the next pair.
                _ = try review.act(.next)
            } else {
                review.settle()
            }
        }
        XCTAssertGreaterThanOrEqual(checked, 2, "swipes back while a scroll ran")
    }

    /// Taps and swipes faster than the text settles: several actions with
    /// the run loop not turning between them, or turning only part of a
    /// scroll, then the name the bar ends on is in view and stays.
    func testAnUnsettledWalkEndsWithTheNameInViewAndStill() throws {
        let bursts: [[Action]] = [
            [.next, .next, .previous], [.replace, .next], [.next, .keep, .previous], [.previous, .previous, .next, .next, .next],
            [.replaceThis, .previous, .keepInstead], [.replaceInstead, .nextUndecided, .next], [.keep, .replace, .previous, .next],
            [.next, .next, .next, .next], [.previous, .replaceInstead, .previous, .keepInstead, .nextUndecided],
        ]
        for pause: TimeInterval in [0, 0.05, 0.12] {
            let review = try Review()
            for (number, burst) in bursts.enumerated() where review.state.isReviewing {
                for action in burst where review.state.isReviewing {
                    try review.perform(action)
                    if pause > 0 { review.turn(pause) }
                }
                guard review.state.isReviewing else { break }
                try review.assertTheNameRests("burst \(number + 1), \(pause) s between actions")
            }
        }
    }

    /// The review bar grows (a hint line, the mentions pulled up) and the
    /// text view gets shorter: a name it would cover scrolls back into
    /// view, and one the reader had scrolled away from stays away.
    func testAGrowingBarKeepsAVisibleNameInView() throws {
        let review = try Review()
        let view = review.view
        // Onto a name the text had to scroll down to: it rests just above
        // the bar.
        var step: Step?
        for _ in 0..<4 { step = try review.act(.next) }
        let current = try XCTUnwrap(review.state.reviewedCandidate)
        let low = try XCTUnwrap(step?.after)
        XCTAssertGreaterThan(low, 400, "near the bottom of the view")

        view.frame.size.height -= 150
        view.layoutIfNeeded()
        review.settle()
        let bottom = view.contentOffset.y + view.bounds.height - ReviewScrollTests.bar
        let rect = try XCTUnwrap(review.drawnRect(of: current))
        XCTAssertLessThanOrEqual(rect.maxY, bottom + 0.5, "above the taller bar")
        XCTAssertGreaterThanOrEqual(rect.minY, view.contentOffset.y + ReviewScrollTests.topBar - 0.5)

        // Scrolled away by hand, the bar shrinks and grows again: the text
        // stays where the reader left it.
        review.coordinator.scrollViewWillBeginDragging(view)
        view.setContentOffset(CGPoint(x: 0, y: -ReviewScrollTests.topBar), animated: false)
        view.layoutIfNeeded()
        review.settle()
        for change: CGFloat in [150, -150] {
            view.frame.size.height += change
            view.layoutIfNeeded()
            review.settle()
            XCTAssertEqual(view.contentOffset.y, -ReviewScrollTests.topBar, accuracy: 0.5)
        }
    }

    /// The reader scrolled away and decides: the text comes to the next
    /// name, wherever it was.
    func testDecidingAfterScrollingAwayBringsTheNextNameIntoView() throws {
        let review = try Review()
        let view = review.view
        var steps: [Step] = []
        for turn in 0..<8 {
            let lowest = max(0, view.contentSize.height + ReviewScrollTests.bar - view.bounds.height)
            let away = min(max(view.contentOffset.y + (turn % 2 == 0 ? 1100 : -1100), -ReviewScrollTests.topBar), lowest)
            review.coordinator.scrollViewWillBeginDragging(view)
            view.setContentOffset(CGPoint(x: 0, y: away), animated: false)
            view.layoutIfNeeded()
            try review.act(turn % 2 == 0 ? .replaceThis : .replace).map { steps.append($0) }
        }
        check(steps, visibleBefore: 0)
    }
}
