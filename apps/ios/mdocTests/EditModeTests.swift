import SwiftUI
import UIKit
import XCTest
@testable import mdoc

/// Edit mode with Show Markdown on (ADR 0109, slice 2), on the real core:
/// typing reaches the core through the text view's typing path as "Typing"
/// steps in the one undo line, the view's string stays the core's (and the
/// saved file's), an edited alias hints once, and the clipboard and edit
/// menu keep text inside mdoc.
@MainActor
final class EditModeTests: XCTestCase {
    private static let text = "# Agreement\n\nMaya Bennett signs first.\n\nThe tenant, Maya Bennett, pays rent.\n"

    private struct Harness {
        let session: any mdoc.DocumentSession
        let model: DocumentViewModel
        let window: UIWindow
        let view: CandidateTextView
        let coordinator: MarkdownTextView.Coordinator

        var shown: String { view.textStorage.string }
    }

    private func harness(text: String = text) throws -> Harness {
        let (core, storage) = try Fixture.core()
        let session = try core.library.importDocument(.text(text), title: "Agreement", into: .newCase(name: "Bennett"))
        _ = try session.recognize(observer: TestObserver())
        _ = try session.propose([SampleDocument.Name(wording: "Maya Bennett", category: .person)])
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let view = CandidateTextView()
        view.frame = window.bounds
        window.addSubview(view)
        window.isHidden = false
        let coordinator = MarkdownTextView.Coordinator(parent: textView(model))
        coordinator.attach(to: view)
        view.delegate = coordinator
        let harness = Harness(session: session, model: model, window: window, view: view, coordinator: coordinator)
        show(harness)
        return harness
    }

    private func textView(_ model: DocumentViewModel) -> MarkdownTextView {
        MarkdownTextView(
            snapshot: model.snapshot, selected: nil, rendered: false,
            onCandidate: { _ in }, onReplace: { _ in }, onReviewNext: {},
            edit: model.isEditing ? EditActions(model) : nil)
    }

    /// What SwiftUI does when the model changes.
    private func show(_ harness: Harness) {
        harness.coordinator.update(harness.view, with: textView(harness.model), dynamicTypeSize: .large, colorScheme: .light)
        harness.view.layoutIfNeeded()
    }

    private func edit(_ harness: Harness) {
        harness.model.enterEditMode()
        XCTAssertTrue(harness.model.isEditing)
        show(harness)
        XCTAssertTrue(harness.view.isEditable)
    }

    /// One change as UIKit makes it: announced, applied, then reported.
    private func change(_ harness: Harness, _ range: NSRange, _ text: String, announced: Bool = true) {
        let view = harness.view
        if announced {
            XCTAssertTrue(harness.coordinator.textView(view, shouldChangeTextIn: range, replacementText: text))
        }
        view.textStorage.replaceCharacters(in: range, with: text)
        view.selectedRange = NSRange(location: range.location + (text as NSString).length, length: 0)
        harness.coordinator.textViewDidChange(view)
    }

    /// Types `text` a character at a time at `location`.
    private func type(_ harness: Harness, _ text: String, at location: Int) {
        var at = location
        for character in text {
            let typed = String(character)
            change(harness, NSRange(location: at, length: 0), typed)
            at += (typed as NSString).length
        }
    }

    private func coreText(_ harness: Harness) -> String {
        harness.session.snapshot().markdown
    }

    // MARK: Typing and undo

    func testTypingIsOneUndoStepBesideDecisions() async throws {
        let harness = try harness()
        _ = try harness.session.replaceAllPending()
        harness.model.refresh()
        show(harness)
        edit(harness)
        XCTAssertNil(harness.model.notice)
        let end = (harness.shown as NSString).length
        type(harness, "Signed today.", at: end)
        XCTAssertEqual(coreText(harness), harness.shown)
        XCTAssertTrue(harness.shown.hasSuffix("Signed today."))
        XCTAssertTrue(harness.model.canUndo)

        // The pause's discovery brings the snapshot, which names the step.
        await harness.model.discoverNow()
        XCTAssertEqual(harness.model.snapshot.undoStep?.label, "Typing")
        show(harness)
        XCTAssertEqual(harness.shown, coreText(harness))

        // Undo takes the typing back as one step and names it; the view
        // follows in place. The next Undo is the Replace all.
        harness.model.undoTapped()
        XCTAssertEqual(harness.model.snapshot.redoStep?.label, "Typing")
        XCTAssertEqual(harness.model.notice, "Undid Typing")
        show(harness)
        XCTAssertEqual(harness.shown, coreText(harness))
        XCTAssertFalse(harness.shown.contains("Signed today."))
        XCTAssertTrue(harness.shown.contains("PERSON_1"))
        harness.model.undoTapped()
        show(harness)
        XCTAssertTrue(harness.shown.contains("Maya Bennett"))
        XCTAssertFalse(harness.model.isReviewing, "edit mode keeps the review bar closed")
        harness.model.redoTapped()
        harness.model.redoTapped()
        show(harness)
        XCTAssertTrue(harness.shown.hasSuffix("Signed today."))
        XCTAssertEqual(harness.shown, coreText(harness))
    }

    func testAMovedCaretStartsANewStep() throws {
        let harness = try harness()
        edit(harness)
        type(harness, "abc", at: 0)
        // Backspace, the caret moved by the reader, backspace: two steps,
        // where two backspaces in a row would be one.
        change(harness, NSRange(location: 2, length: 1), "")
        harness.view.selectedRange = NSRange(location: 1, length: 0)
        harness.coordinator.textViewDidChangeSelection(harness.view)
        change(harness, NSRange(location: 0, length: 1), "")
        XCTAssertEqual(coreText(harness), "b" + Self.text)
        harness.model.undoTapped()
        show(harness)
        XCTAssertEqual(harness.shown, "ab" + Self.text, "the delete after the caret moved is its own step")
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testEditingAnAliasHintsOnce() throws {
        let harness = try harness()
        _ = try harness.session.replaceAllPending()
        harness.model.refresh()
        show(harness)
        edit(harness)
        let alias = (harness.shown as NSString).range(of: "PERSON_1")
        change(harness, NSRange(location: alias.location + 2, length: 1), "")
        XCTAssertEqual(harness.model.notice, "You edited an alias: that mention no longer restores.")
        harness.model.clearNotice()
        let second = (harness.shown as NSString).range(of: "PERSON_1")
        change(harness, NSRange(location: second.location, length: 1), "")
        XCTAssertNil(harness.model.notice, "the hint shows for the first broken alias only")
    }

    func testAChangeUIKitDidNotAnnounceStillReachesTheCore() throws {
        let harness = try harness()
        edit(harness)
        // A committed IME word, a find replacement or a drop.
        change(harness, NSRange(location: 2, length: 9), "Договор", announced: false)
        XCTAssertEqual(coreText(harness), harness.shown)
        // Announced edits that do not give the view's text are not trusted.
        let view = harness.view
        XCTAssertTrue(harness.coordinator.textView(view, shouldChangeTextIn: NSRange(location: 0, length: 0), replacementText: "x"))
        view.textStorage.replaceCharacters(in: NSRange(location: 3, length: 0), with: "😀")
        harness.coordinator.textViewDidChange(view)
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testEditWaitsForAJobAndDoneLeavesEditMode() async throws {
        let harness = try harness()
        let slow = DocumentViewModel(
            session: SlowJobs(harness.session, seconds: 30), jobs: LongJobRunner(background: NoBackgroundContinuation()),
            storage: try Fixture.temporaryStorage())
        let job = Task { await slow.findNames() }
        while slow.runningJob == nil { await Task.yield() }
        XCTAssertEqual(slow.editBlockedReason, "Finding names. Edit when it finishes.")
        slow.enterEditMode()
        XCTAssertFalse(slow.isEditing)
        XCTAssertEqual(slow.notice, slow.editBlockedReason)
        slow.cancelJob()
        await job.value

        edit(harness)
        type(harness, "x", at: 0)
        harness.model.doneEditing()
        XCTAssertFalse(harness.model.isEditing)
        show(harness)
        XCTAssertFalse(harness.view.isEditable)
        XCTAssertEqual(harness.shown, "x" + Self.text)
    }

    // MARK: Review findings (PR #766)

    /// Done with text still marked commits it and sends it to the core
    /// before review reads the core's text.
    func testDoneCommitsMarkedText() throws {
        let harness = try harness()
        edit(harness)
        let view = harness.view
        XCTAssertTrue(view.becomeFirstResponder())
        view.selectedRange = NSRange(location: 0, length: 0)
        view.setMarkedText("日本", selectedRange: NSRange(location: 2, length: 0))
        try XCTSkipIf(view.markedTextRange == nil, "this host does not mark text")
        harness.coordinator.textViewDidChange(view)
        XCTAssertFalse(coreText(harness).contains("日本"), "marked text never reaches the core")
        harness.model.doneEditing()
        show(harness)
        XCTAssertTrue(coreText(harness).hasPrefix("日本"))
        XCTAssertEqual(harness.shown, coreText(harness))
    }

    /// After typing a multi-byte character, review maps selections with
    /// the typed text's offsets, not the ones from before the typing.
    func testReviewOffsetsFollowTypedText() async throws {
        let harness = try harness()
        edit(harness)
        type(harness, "😀", at: 0)
        await harness.model.discoverNow()
        show(harness)
        harness.model.doneEditing()
        show(harness)
        let shown = harness.shown as NSString
        let signs = shown.range(of: "signs")
        let source = harness.coordinator.offsets.sourceRange(signs)
        let bytes = Array(coreText(harness).utf8)
        XCTAssertEqual(String(decoding: bytes[source.start..<source.end], as: UTF8.self), "signs")
    }

    /// A document emptied by editing stays editable, before and after Done.
    func testAnEmptiedDocumentStaysEditable() async throws {
        let harness = try harness()
        edit(harness)
        change(harness, NSRange(location: 0, length: (harness.shown as NSString).length), "")
        await harness.model.discoverNow()
        XCTAssertEqual(harness.model.snapshot.markdown, "")
        XCTAssertTrue(harness.model.hasEditableText)
        type(harness, "New", at: 0)
        harness.model.doneEditing()
        XCTAssertEqual(coreText(harness), "New")
        harness.model.enterEditMode()
        XCTAssertTrue(harness.model.isEditing)
    }

    /// A restyle after a typing pause keeps where typing left the caret: a
    /// move after it still ends the step, so Undo takes back one delete.
    func testACaretMoveAfterARestyleStillEndsTheStep() async throws {
        let harness = try harness()
        edit(harness)
        // A delete before the names moves them: the pause restyles.
        change(harness, NSRange(location: 2, length: 1), "")
        let rebuilds = harness.coordinator.rebuilds
        await harness.model.discoverNow()
        show(harness)
        XCTAssertGreaterThan(harness.coordinator.rebuilds, rebuilds, "the moved names restyle")
        let end = (harness.shown as NSString).length
        harness.view.selectedRange = NSRange(location: end, length: 0)
        harness.coordinator.textViewDidChangeSelection(harness.view)
        change(harness, NSRange(location: end - 1, length: 1), "")
        harness.model.undoTapped()
        show(harness)
        var expected = Self.text
        expected.remove(at: expected.index(expected.startIndex, offsetBy: 2))
        XCTAssertEqual(harness.shown, expected, "only the second delete is undone")
    }

    // MARK: The typing fuzz test (ADR 0109's acceptance)

    /// Random typing, deletes, pastes, unannounced changes, undo and redo:
    /// after each, the core's text is the view's, and each save writes
    /// exactly the view's string.
    func testTypingFuzzKeepsTheSavedBytesTheViewsString() throws {
        let harness = try harness()
        edit(harness)
        var random = SeededRandom(seed: 0x6d646f63)
        let pieces = ["a", "e", " ", "\n", "Maya", "é", "Ж", "😀", "👩‍⚖️", "**", "- ", "| x |", "PERSON_1"]
        for step in 1...400 {
            let text = harness.shown as NSString
            let length = text.length
            func boundary(_ value: Int) -> Int {
                guard value < length else { return length }
                return text.rangeOfComposedCharacterSequence(at: value).location
            }
            let start = boundary(random.next(below: length + 1))
            let end = boundary(min(length, start + random.next(below: 6)))
            switch random.next(below: 10) {
            case 0...4:
                type(harness, pieces[random.next(below: pieces.count)], at: start)
            case 5, 6:
                if end > start { change(harness, NSRange(location: start, length: end - start), "") }
            case 7:
                change(harness, NSRange(location: start, length: end - start), pieces.shuffled(using: &random).prefix(3).joined(),
                       announced: random.next(below: 2) == 0)
            case 8:
                harness.model.undoTapped()
                show(harness)
            default:
                harness.model.redoTapped()
                show(harness)
            }
            XCTAssertEqual(coreText(harness), harness.shown, "step \(step)")
            if step % 50 == 0 {
                try harness.session.saveChanges()
                let path = try XCTUnwrap((harness.session as? RealSession)?.path)
                let saved = try Data(contentsOf: URL(fileURLWithPath: path))
                XCTAssertEqual(saved, Data(harness.shown.utf8), "saved bytes at step \(step)")
            }
        }
    }

    // MARK: Find and replace

    /// The find bar's Replace and Replace All are UIKit's own storage edits,
    /// which no delegate call announces: each reaches the core as an edit
    /// of its own, so an alias between two matches keeps restoring.
    func testFindBarReplacementsAreCoreEdits() throws {
        let harness = try harness(text: "The rent is due. Maya Bennett pays the rent.\n")
        _ = try harness.session.replaceAllPending()
        harness.model.refresh()
        show(harness)
        edit(harness)
        let storage = harness.view.textStorage
        for match in ["rent", "rent"] {
            // Last match first, as a replace-all keeps earlier places.
            let range = (storage.string as NSString).range(of: match, options: .backwards)
            storage.replaceCharacters(in: range, with: "fee")
        }
        harness.view.onEdited?()
        XCTAssertEqual(harness.shown, "The fee is due. PERSON_1 pays the fee.\n")
        XCTAssertEqual(coreText(harness), harness.shown)
        XCTAssertEqual(harness.session.snapshot().counts?.replaced, 1, "the alias between the matches still restores")
    }

    // MARK: Clipboard and menu

    func testCutAndPasteStayInMdoc() throws {
        let harness = try harness()
        edit(harness)
        let own = UIPasteboard.withUniqueName()
        let system = UIPasteboard.withUniqueName()
        let clipboard = InAppClipboard(pasteboard: own, system: system)
        clipboard.write("Maya")
        XCTAssertEqual(clipboard.read(), "Maya")
        XCTAssertNil(system.string, "a copy never reaches the system clipboard")
        system.string = "From another app"
        XCTAssertEqual(clipboard.read(), "From another app", "a newer copy elsewhere pastes")
        system.setData(Data("<h1>Title</h1><p>Body</p>".utf8), forPasteboardType: "public.html")
        XCTAssertEqual(clipboard.read()?.hasPrefix("# Title"), true, "rich text pastes as the core's Markdown")

        // Cut through the view: the in-app pasteboard has it, the core lost it.
        let range = (harness.shown as NSString).range(of: "first")
        harness.view.selectedRange = range
        harness.view.cutSelection()
        XCTAssertEqual(InAppClipboard.shared.pasteboard.string, "first")
        XCTAssertFalse(harness.shown.contains("signs first"))
        XCTAssertEqual(coreText(harness), harness.shown)
        harness.view.selectedRange = NSRange(location: 0, length: 0)
        harness.view.pasteClipboard()
        XCTAssertTrue(harness.shown.hasPrefix("first"))
        XCTAssertEqual(coreText(harness), harness.shown)
    }

    func testTheEditMenuHasCutCopyPasteAndSelectAllOnly() throws {
        let cut = UICommand(title: "Cut", action: #selector(UIResponderStandardEditActions.cut(_:)))
        let copy = UICommand(title: "Copy", action: #selector(UIResponderStandardEditActions.copy(_:)))
        let paste = UICommand(title: "Paste", action: #selector(UIResponderStandardEditActions.paste(_:)))
        let all = UICommand(title: "Select All", action: #selector(UIResponderStandardEditActions.selectAll(_:)))
        let lookUp = UICommand(title: "Look Up", action: NSSelectorFromString("_define:"))
        let share = UICommand(title: "Share…", action: NSSelectorFromString("_share:"))
        let bold = UICommand(title: "Bold", action: #selector(UIResponderStandardEditActions.toggleBoldface(_:)))
        let writing = UIAction(title: "Writing Tools") { _ in }
        let menu = CandidateTextView.editMenu(from: [
            UIMenu(options: .displayInline, children: [cut, copy, paste]), all, lookUp, share,
            UIMenu(title: "Format", children: [bold]), writing,
        ])
        func titles(_ element: UIMenuElement) -> [String] {
            if let menu = element as? UIMenu { return menu.children.flatMap(titles) }
            return [element.title]
        }
        XCTAssertEqual(titles(menu), ["Cut", "Copy", "Paste", "Select All"])

        let harness = try harness()
        edit(harness)
        harness.view.selectedRange = NSRange(location: 0, length: 3)
        XCTAssertTrue(harness.view.canPerformAction(#selector(UIResponderStandardEditActions.copy(_:)), withSender: nil))
        XCTAssertFalse(harness.view.canPerformAction(NSSelectorFromString("_define:"), withSender: nil))
        XCTAssertFalse(harness.view.canPerformAction(#selector(UIResponderStandardEditActions.toggleBoldface(_:)), withSender: nil))
        XCTAssertTrue(harness.view.undoManager is CoreUndoManager, "UIKit's own typing undo never runs")
        XCTAssertEqual(harness.view.autocorrectionType, .yes)
        XCTAssertEqual(harness.view.spellCheckingType, .yes)
        XCTAssertEqual(harness.view.smartQuotesType, .no)
        XCTAssertEqual(harness.view.smartDashesType, .no)
    }

    func testTheDifferenceIsOneStretchAndKeepsSurrogatePairs() {
        XCTAssertNil(TypingBridge.difference(from: "same", to: "same"))
        let inserted = TypingBridge.difference(from: "ab", to: "a😀b")
        XCTAssertEqual(inserted?.range, NSRange(location: 1, length: 0))
        XCTAssertEqual(inserted?.text, "😀")
        // Two emoji sharing a lead surrogate: the change takes the pair.
        let swapped = TypingBridge.difference(from: "😀", to: "😁")
        XCTAssertEqual(swapped?.range, NSRange(location: 0, length: 2))
        XCTAssertEqual(swapped?.text, "😁")
        XCTAssertEqual(TypingBridge.sourceRange(NSRange(location: 1, length: 2), in: "é😀x"), SourceRange(start: 2, end: 6))
    }
}

/// A small deterministic generator, so a failing fuzz run repeats.
struct SeededRandom: RandomNumberGenerator {
    private var state: UInt64

    init(seed: UInt64) { state = seed }

    mutating func next() -> UInt64 {
        state = state &* 6_364_136_223_846_793_005 &+ 1_442_695_040_888_963_407
        return state >> 33 ^ state
    }

    mutating func next(below bound: Int) -> Int {
        bound <= 1 ? 0 : Int(next() % UInt64(bound))
    }
}
