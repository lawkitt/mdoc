import MdocFFI
import UIKit
import XCTest
@testable import mdoc

/// A selection copied by hand (#1209, Vault & Projects item 41): the menu
/// each check of the core gives, the marking, the conversion from the
/// text view's UTF-16 ranges to the core's UTF-8 ones, and the model's
/// copies on the real core.
@MainActor
final class SelectionCopyTests: XCTestCase {
    private static let email = "ann.baker@example.com"
    private static let letter = "# Site visit\n\nWrite to \(email) about the roof.\n\nTwo tiles are cracked."

    // MARK: The menu and the marking

    func testEachCheckGivesItsMenu() {
        XCTAssertEqual(SelectionCopy.items(for: .clean), [.copy])
        XCTAssertEqual(
            SelectionCopy.items(for: .notClean(proposals: 2, changedSinceScan: false, knownNames: 0)),
            [.replaceAndCopy(2), .copyWithNames])
        // Proposals win over the rest: Replace N and copy comes first.
        XCTAssertEqual(
            SelectionCopy.items(for: .notClean(proposals: 1, changedSinceScan: true, knownNames: 3)),
            [.replaceAndCopy(1), .copyWithNames])
        XCTAssertEqual(SelectionCopy.items(for: .notClean(proposals: 0, changedSinceScan: true, knownNames: 0)), [.copyWithNames])
        XCTAssertEqual(SelectionCopy.items(for: .notClean(proposals: 0, changedSinceScan: false, knownNames: 1)), [.copyWithNames])
        XCTAssertEqual(SelectionCopy.items(for: .neverScanned), [.copyWithNames])
        // A check that failed still copies, with names.
        XCTAssertEqual(SelectionCopy.items(for: nil), [.copyWithNames])
    }

    func testTheItemsWords() {
        XCTAssertEqual(SelectionCopy.Item.copy.title, "Copy")
        XCTAssertEqual(SelectionCopy.Item.replaceAndCopy(3).title, "Replace 3 and copy")
        XCTAssertEqual(SelectionCopy.Item.copyWithNames.title, "Copy with names")
    }

    func testOnlyACleanCheckCopiesUnmarked() {
        XCTAssertFalse(SelectionCopy.marked(.clean))
        XCTAssertTrue(SelectionCopy.marked(.notClean(proposals: 0, changedSinceScan: true, knownNames: 0)))
        XCTAssertTrue(SelectionCopy.marked(.notClean(proposals: 0, changedSinceScan: false, knownNames: 1)))
        XCTAssertTrue(SelectionCopy.marked(.notClean(proposals: 1, changedSinceScan: false, knownNames: 0)))
        XCTAssertTrue(SelectionCopy.marked(.neverScanned))
        XCTAssertTrue(SelectionCopy.marked(nil))
    }

    func testAMarkedCopyIsLocalAndExpiresAndACleanOneIsNot() throws {
        let pasteboard = try XCTUnwrap(UIPasteboard(name: UIPasteboard.Name("mdoc-tests-\(UUID().uuidString)"), create: true))
        defer { UIPasteboard.remove(withName: pasteboard.name) }
        SelectionCopy.write("Ann Baker", marked: false, to: pasteboard)
        XCTAssertEqual(pasteboard.string, "Ann Baker")
        SelectionCopy.write("Ann Baker", marked: true, to: pasteboard)
        XCTAssertEqual(pasteboard.string, "Ann Baker", "a marked copy pastes")
        // The marking is the restored reply's: written as if eleven
        // minutes ago, its ten minutes are over.
        RestoredPasteboard.copy("Ann Baker", to: pasteboard, now: .now.addingTimeInterval(-11 * 60))
        XCTAssertNil(pasteboard.string)
    }

    // MARK: UTF-16 to UTF-8

    func testRangesConvertThroughNonASCIIAndEmoji() throws {
        let text = "Иван 👩🏽‍⚖️ met Ann — «ok»."
        let offsets = TextOffsets(text)
        let shown = text as NSString
        for words in ["Иван", "👩🏽‍⚖️", "Ann", "— «ok»", "Иван 👩🏽‍⚖️ met"] {
            let range = shown.range(of: words)
            let source = SelectionCopy.sourceRange(range, in: offsets)
            let bytes = try XCTUnwrap(text.range(of: words)).utf8Offsets(in: text)
            XCTAssertEqual(source, bytes, words)
            XCTAssertEqual(SelectionCopy.text(source, in: text), words)
            XCTAssertEqual(offsets.nsRange(source), range, "\(words) round trips")
        }
        // An end inside an emoji's surrogate pair moves to the scalar's end.
        let judge = shown.range(of: "👩🏽‍⚖️")
        let half = SelectionCopy.sourceRange(NSRange(location: judge.location, length: 1), in: offsets)
        XCTAssertEqual(SelectionCopy.text(half, in: text), "👩")
    }

    func testACellsRangeIsInTheDocumentsOffsets() {
        let text = "| Name | Ёлка |\n| --- | --- |\n| Ann | Пётр |"
        let offsets = TextOffsets(text)
        let cell = (text as NSString).range(of: "Пётр")
        let local = NSRange(location: 0, length: cell.length)
        let source = SelectionCopy.sourceRange(local, base: cell.location, in: offsets)
        XCTAssertEqual(SelectionCopy.text(source, in: text), "Пётр")
    }

    func testTextRefusesARangeOffACharacter() {
        let text = "Ёлка"
        XCTAssertEqual(SelectionCopy.text(SourceRange(start: 0, end: 2), in: text), "Ё")
        XCTAssertNil(SelectionCopy.text(SourceRange(start: 1, end: 2), in: text), "inside Ё")
        XCTAssertNil(SelectionCopy.text(SourceRange(start: 0, end: 99), in: text), "past the end")
        XCTAssertEqual(SelectionCopy.text(SourceRange(start: 8, end: 8), in: text), "")
    }

    // MARK: The model, on the real core

    /// `letter` imported, recognized and, unless `scanned` is false,
    /// scanned; at `memory`.
    private func model(scanned: Bool = true, memory: UInt64 = Fixture.belowFloor) throws -> DocumentViewModel {
        let (core, storage) = try Fixture.core(memory: memory)
        let session = try core.library.importDocument(.text(Self.letter), title: "Site visit", into: nil)
        _ = try session.recognize(observer: TestObserver())
        if scanned { _ = try session.scan(observer: TestObserver()) }
        return DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
    }

    private func range(of words: String, in model: DocumentViewModel) throws -> SourceRange {
        try XCTUnwrap(model.snapshot.markdown.range(of: words)).utf8Offsets(in: model.snapshot.markdown)
    }

    /// What a copy wrote: the text and whether it was marked.
    @MainActor
    private final class Written {
        var copies: [(text: String, marked: Bool)] = []
        var write: DocumentViewModel.CopyWriter { { [self] in copies.append(($0, $1)) } }
    }

    func testACleanSelectionCopiesUnmarked() async throws {
        let model = try model()
        let tiles = try range(of: "Two tiles are cracked.", in: model)
        let check = await model.checkSelection(tiles)
        XCTAssertEqual(check, .clean)
        let written = Written()
        await model.copySelection("Two tiles are cracked.", at: tiles, write: written.write).value
        XCTAssertEqual(written.copies.map(\.text), ["Two tiles are cracked."])
        XCTAssertEqual(written.copies.map(\.marked), [false])
    }

    func testAProposalOffersReplaceAndCopyWhichCopiesTheAliasWhole() async throws {
        let model = try model()
        XCTAssertEqual(model.snapshot.pendingCount, 1)
        // Cut into the email: the core checks it as the whole word.
        let cut = try range(of: "Write to ann.ba", in: model)
        let check = await model.checkSelection(cut)
        XCTAssertEqual(check, .notClean(proposals: 1, changedSinceScan: false, knownNames: 0))
        XCTAssertEqual(SelectionCopy.items(for: check).first, .replaceAndCopy(1))

        // Copy with names: as written, marked.
        let written = Written()
        await model.copySelection("Write to ann.ba", at: cut, write: written.write).value
        XCTAssertEqual(written.copies.last?.marked, true)

        let undoBefore = model.snapshot.undoStep
        await model.replaceAndCopy(cut, in: model.snapshot.textRevision, write: written.write)?.value
        XCTAssertEqual(model.snapshot.pendingCount, 0, "the email replaced")
        XCTAssertNotEqual(model.snapshot.undoStep, undoBefore, "one undo step")
        XCTAssertFalse(model.snapshot.markdown.contains(Self.email))
        let copy = try XCTUnwrap(written.copies.last)
        XCTAssertTrue(copy.text.hasPrefix("Write to EMAIL_1"), copy.text)
        XCTAssertFalse(copy.text.contains("ann.ba"))
        XCTAssertFalse(copy.marked, "replaced, the selection is clean")
    }

    func testAReplaceFromOlderTextReplacesNothing() async throws {
        let model = try model()
        let cut = try range(of: Self.email, in: model)
        let revision = model.snapshot.textRevision
        model.replaceAllPending()
        XCTAssertNotEqual(model.snapshot.textRevision, revision)
        let written = Written()
        XCTAssertNil(model.replaceAndCopy(cut, in: revision, write: written.write))
        XCTAssertEqual(model.errorMessage, "The document changed. Select the text again.")
        XCTAssertTrue(written.copies.isEmpty)
    }

    func testAKeptNameCopiesWithNamesMarked() async throws {
        let model = try model()
        let email = try XCTUnwrap(model.snapshot.candidates.first)
        XCTAssertTrue(model.decide(email.id, .keep, scope: .all))
        let line = try range(of: "Write to \(Self.email)", in: model)
        let check = await model.checkSelection(line)
        XCTAssertEqual(check, .notClean(proposals: 0, changedSinceScan: false, knownNames: 1))
        XCTAssertEqual(SelectionCopy.items(for: check), [.copyWithNames])
        let written = Written()
        await model.copySelection("Write to \(Self.email)", at: line, write: written.write).value
        XCTAssertEqual(written.copies.last?.marked, true)
    }

    func testADocumentNeverScannedCopiesWithNamesMarked() async throws {
        let model = try model(scanned: false, memory: Fixture.detectorTier)
        let tiles = try range(of: "Two tiles", in: model)
        let check = await model.checkSelection(tiles)
        XCTAssertEqual(check, .neverScanned)
        XCTAssertEqual(SelectionCopy.items(for: check), [.copyWithNames])
        let written = Written()
        await model.copySelection("Two tiles", at: tiles, write: written.write).value
        XCTAssertEqual(written.copies.last?.marked, true)
    }

    /// The check is about the bytes copied: text the core no longer holds
    /// there (edit mode's typing, an undo) copies marked.
    func testACopyOfOtherTextThanTheCoresIsMarked() async throws {
        let model = try model()
        let tiles = try range(of: "Two tiles are cracked.", in: model)
        let written = Written()
        await model.copySelection("Two tales are cracked.", at: tiles, write: written.write).value
        XCTAssertEqual(written.copies.last?.marked, true)
    }

    /// A newer copy wins over one still being checked.
    func testOnlyTheLatestCopyIsWritten() async throws {
        let model = try model()
        let tiles = try range(of: "Two tiles are cracked.", in: model)
        let written = Written()
        let first = model.copySelection("Two tiles are cracked.", at: tiles, write: written.write)
        let second = model.copySelection("Two tiles", at: nil, write: written.write)
        await first.value
        await second.value
        XCTAssertEqual(written.copies.map(\.text), ["Two tiles"])
    }

    /// Anything else on the clipboard meanwhile (a Cut) wins too.
    func testAClipboardChangeWinsOverACopyBeingChecked() throws {
        let pasteboard = try XCTUnwrap(UIPasteboard(name: UIPasteboard.Name("mdoc-tests-\(UUID().uuidString)"), create: true))
        defer { UIPasteboard.remove(withName: pasteboard.name) }
        let first = SelectionCopy.Request(pasteboard: pasteboard)
        XCTAssertTrue(first.isLatest)
        let second = SelectionCopy.Request(pasteboard: pasteboard)
        XCTAssertFalse(first.isLatest, "a later copy")
        XCTAssertTrue(second.isLatest)
        pasteboard.string = "cut"
        XCTAssertFalse(second.isLatest, "a cut meanwhile")
    }

    /// A check answered after its selection went is dropped.
    func testAnAnswerForASelectionGoneIsDropped() throws {
        var checks = SelectionChecks()
        let key = SelectionChecks.Key(revision: 1, range: SourceRange(start: 0, end: 4))
        let request = try XCTUnwrap(checks.ask(key))
        XCTAssertNil(checks.ask(key), "asked already")
        checks.forget()
        XCTAssertFalse(checks.answer(request, .clean))
        XCTAssertNil(checks.answer(for: key))
        let again = try XCTUnwrap(checks.ask(key))
        XCTAssertTrue(checks.answer(again, .neverScanned))
        XCTAssertEqual(checks.answer(for: key), .some(.neverScanned))
    }

    func testACopyWithNoRangeIsMarkedUnchecked() async throws {
        let model = try model()
        let written = Written()
        await model.copySelection("Two tiles", at: nil, write: written.write).value
        XCTAssertEqual(written.copies.last?.marked, true)
    }

    // MARK: The views

    /// The main text's Copy, ⌘C and VoiceOver's: allowed for a selection,
    /// and Share and Look Up never.
    func testTheReviewTextCopiesASelectionAndNothingElse() {
        let view = CandidateTextView()
        view.text = "Maya Ellison"
        XCTAssertFalse(view.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil), "nothing selected")
        view.selectedRange = NSRange(location: 0, length: 4)
        XCTAssertTrue(view.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
        XCTAssertFalse(view.canPerformAction(#selector(UIResponderStandardEditActions.cut(_:)), withSender: nil))
        XCTAssertFalse(view.canPerformAction(NSSelectorFromString("_share:"), withSender: nil))
        XCTAssertFalse(view.canPerformAction(NSSelectorFromString("_define:"), withSender: nil))
        XCTAssertFalse(view.canPerformAction(NSSelectorFromString("_translate:"), withSender: nil))
        XCTAssertTrue(view.canPerformAction(#selector(UIResponderStandardEditActions.selectAll(_:)), withSender: nil))
        var copied = 0
        view.onHandCopy = { copied += 1 }
        view.copy(nil)
        XCTAssertEqual(copied, 1, "⌘C goes through the hand copy")
    }

    /// A review cell copies through the review's hand copy; a restored
    /// reply's cell copies marked; neither shares.
    func testTableCellsCopyTheirOwnersWay() throws {
        let cell = CellTextView(usingTextLayoutManager: false)
        cell.text = "Maya Bennett"
        cell.selectedRange = NSRange(location: 0, length: 4)
        XCTAssertFalse(cell.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil), "no owner")
        var copied = 0
        cell.onCopy = { copied += 1 }
        XCTAssertTrue(cell.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
        cell.copy(nil)
        XCTAssertEqual(copied, 1)
        cell.onCopy = nil
        cell.allowsCopy = true
        XCTAssertTrue(cell.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
        for action in ["_share:", "_define:", "_translate:"] {
            XCTAssertFalse(cell.canPerformAction(NSSelectorFromString(action), withSender: nil), action)
        }
    }

    /// The restored reply (#1228): Copy and Select All only, in UIKit's
    /// words; Copy is marked.
    func testARestoredReplyKeepsCopyAndSelectAllOnly() {
        let view = RestoredReplyTextView()
        view.text = "Maya Bennett owes £4,200."
        view.selectedRange = NSRange(location: 0, length: 12)
        XCTAssertTrue(view.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
        for action in ["_share:", "_define:", "_translate:", "_searchWeb:"] {
            XCTAssertFalse(view.canPerformAction(NSSelectorFromString(action), withSender: nil), action)
        }
        XCTAssertFalse(view.canPerformAction(#selector(UIResponderStandardEditActions.cut(_:)), withSender: nil))
        let copy = UICommand(title: "Copy", action: #selector(UIResponderStandardEditActions.copy(_:)))
        let all = UICommand(title: "Select All", action: #selector(UIResponderStandardEditActions.selectAll(_:)))
        let lookUp = UICommand(title: "Look Up", action: NSSelectorFromString("_define:"))
        let translate = UICommand(title: "Translate", action: NSSelectorFromString("_translate:"))
        let search = UICommand(title: "Search Web", action: NSSelectorFromString("_searchWeb:"))
        let share = UICommand(title: "Share…", action: NSSelectorFromString("_share:"))
        let writing = UIAction(title: "Writing Tools") { _ in }
        let menu = RestoredReplyTextView.menu(from: [
            UIMenu(options: .displayInline, children: [copy]), all,
            UIMenu(options: .displayInline, children: [lookUp, translate, search]), share, writing,
        ])
        func titles(_ element: UIMenuElement) -> [String] {
            if let menu = element as? UIMenu { return menu.children.flatMap(titles) }
            return [element.title]
        }
        XCTAssertEqual(titles(menu), ["Copy", "Select All"])
    }
}

/// The review text's selection menu (#1209): Hide name and Select All
/// while the core checks the selection, then the copy items first.
@MainActor
final class SelectionMenuTests: XCTestCase, TextViewHarnessTests {
    private static let letter = "# Site visit\n\nWrite to ann.baker@example.com about the roof.\n\nTwo tiles are cracked."

    private func harness() throws -> Harness {
        try textViewHarness(
            text: Self.letter, title: "Site visit", projectName: "", rendered: true, editing: false,
            prepare: { _ = try $0.scan(observer: TestObserver()) })
    }

    private func titles(_ element: UIMenuElement?) -> [String] {
        guard let element else { return [] }
        if let menu = element as? UIMenu { return menu.children.flatMap(titles) }
        return [element.title]
    }

    /// The menu over `words` as the core is asked, and once it answered.
    private func menu(over words: String, in harness: Harness) async throws -> (before: [String], after: [String]) {
        let range = harness.string.range(of: words)
        harness.view.selectedRange = range
        let before = titles(harness.coordinator.textView(harness.view, editMenuForTextIn: range, suggestedActions: []))
        let source = SelectionCopy.sourceRange(range, in: TextOffsets(harness.shown))
        let key = SelectionChecks.Key(revision: harness.model.snapshot.textRevision, range: source)
        for _ in 0..<300 where harness.coordinator.selectionChecks.answer(for: key) == nil {
            try await Task.sleep(for: .milliseconds(10))
        }
        let after = titles(harness.coordinator.textView(harness.view, editMenuForTextIn: range, suggestedActions: []))
        return (before, after)
    }

    func testACleanSelectionsMenu() async throws {
        let harness = try harness()
        let menu = try await menu(over: "Two tiles", in: harness)
        XCTAssertEqual(menu.before, ["Hide name", "Select All"], "while the core checks")
        XCTAssertEqual(menu.after, ["Copy", "Hide name", "Select All"])
    }

    func testAProposalsMenu() async throws {
        let harness = try harness()
        let menu = try await menu(over: "Write to ann.baker@example.com", in: harness)
        XCTAssertEqual(menu.after, ["Replace 1 and copy", "Copy with names", "Hide name", "Select All"])
    }

    /// Hide name snaps a selection to whole words without its ends'
    /// punctuation; Copy takes the reader's own, the full stop included.
    func testCopyKeepsTheSelectionHideNameSnapped() async throws {
        let harness = try harness()
        let own = harness.string.range(of: "Two tiles are cracked.")
        let snapped = NSRange(location: own.location, length: own.length - 1)
        harness.coordinator.unsnapped = UnsnappedSelection(view: harness.view, original: own, snapped: snapped)
        harness.view.selectedRange = snapped
        XCTAssertNotNil(harness.coordinator.unsnapped, "the snap itself keeps it")
        UIPasteboard.general.items = []
        harness.view.copy(nil)
        for _ in 0..<300 where UIPasteboard.general.string == nil {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(UIPasteboard.general.string, "Two tiles are cracked.")
        // Another selection: the reader's own is forgotten.
        harness.view.selectedRange = NSRange(location: 0, length: 3)
        XCTAssertNil(harness.coordinator.unsnapped)
    }

    /// A new selection is asked about afresh once the old one went.
    func testAnAnswerIsForgottenWithTheSelection() async throws {
        let harness = try harness()
        _ = try await menu(over: "Two tiles", in: harness)
        harness.view.selectedRange = NSRange(location: 0, length: 0)
        XCTAssertNil(harness.coordinator.selectionChecks.answered)
    }
}

private extension Range where Bound == String.Index {
    /// The range in `text`'s UTF-8 bytes.
    func utf8Offsets(in text: String) -> SourceRange {
        SourceRange(
            start: text.utf8.distance(from: text.utf8.startIndex, to: lowerBound),
            end: text.utf8.distance(from: text.utf8.startIndex, to: upperBound))
    }
}
