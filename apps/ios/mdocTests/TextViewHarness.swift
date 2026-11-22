import UIKit
import XCTest
@testable import mdoc

/// The editing tests' one text-view harness (#1076, seam 2): a document on
/// the real core, its model, and the text view and coordinator SwiftUI
/// makes for it, in a phone-sized window. `ReviewScrollTests` and the
/// read-only `MarkdownTextViewTests` keep their own setup.
@MainActor
struct TextViewHarness {
    let session: any mdoc.DocumentSession
    let model: DocumentViewModel
    let window: UIWindow
    let view: CandidateTextView
    let coordinator: MarkdownTextView.Coordinator
    /// Show Markdown off: the live preview.
    let rendered: Bool

    var shown: String { view.textStorage.string }
    var string: NSString { view.textStorage.string as NSString }
    var toolbar: EditToolbarModel { coordinator.keyboard.toolbar }

    /// The view SwiftUI makes from the model now.
    static func textView(_ model: DocumentViewModel, rendered: Bool) -> MarkdownTextView {
        MarkdownTextView(
            snapshot: model.snapshot, selected: nil, rendered: rendered,
            onCandidate: { _ in }, onReplace: { _ in }, onReviewNext: {},
            edit: model.isEditing ? EditActions(model) : nil)
    }
}

/// A test class that drives a `TextViewHarness` as UIKit and SwiftUI do:
/// `show` is SwiftUI's update, `change` one keystroke.
@MainActor
protocol TextViewHarnessTests: XCTestCase {}

extension TextViewHarnessTests {
    typealias Harness = TextViewHarness

    /// `text` imported into a new case and recognized, `prepare` run on the
    /// session before the model reads it, and shown; in edit mode when
    /// `editing`.
    func textViewHarness(
        text: String, title: String, caseName: String, rendered: Bool, editing: Bool,
        prepare: (any mdoc.DocumentSession) throws -> Void = { _ in }
    ) throws -> Harness {
        let (core, storage) = try Fixture.core()
        let session = try core.library.importDocument(.text(text), title: title, into: .newCase(name: caseName))
        _ = try session.recognize(observer: TestObserver())
        try prepare(session)
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let view = CandidateTextView()
        view.frame = window.bounds
        window.addSubview(view)
        window.isHidden = false
        let coordinator = MarkdownTextView.Coordinator(parent: Harness.textView(model, rendered: rendered))
        coordinator.attach(to: view)
        view.delegate = coordinator
        let harness = Harness(
            session: session, model: model, window: window, view: view, coordinator: coordinator, rendered: rendered)
        show(harness)
        if editing {
            model.enterEditMode()
            show(harness)
        }
        return harness
    }

    /// What SwiftUI does when the model changes; `inViewUpdate` as from
    /// SwiftUI's own update of the view.
    func show(_ harness: Harness, rendered: Bool? = nil, inViewUpdate: Bool = false) {
        harness.coordinator.update(
            harness.view, with: Harness.textView(harness.model, rendered: rendered ?? harness.rendered),
            dynamicTypeSize: .large, colorScheme: .light, inViewUpdate: inViewUpdate)
        harness.view.layoutIfNeeded()
    }

    /// The reader puts the caret at `location`.
    func caret(_ harness: Harness, at location: Int) {
        harness.view.selectedRange = NSRange(location: location, length: 0)
        harness.coordinator.textViewDidChangeSelection(harness.view)
    }

    /// The reader puts the caret (or a selection) at `range`; the toolbar
    /// asks the core.
    func select(_ harness: Harness, _ range: NSRange) {
        harness.view.selectedRange = range
        harness.coordinator.textViewDidChangeSelection(harness.view)
        harness.coordinator.refreshToolbar(harness.view)
    }

    /// The caret `offset` past the first `needle`; the toolbar asks the core.
    func caret(_ harness: Harness, at needle: String, offset: Int = 0) {
        select(harness, NSRange(location: harness.string.range(of: needle).location + offset, length: 0))
    }

    /// One change as UIKit makes it: announced, applied, then reported. A
    /// structural key (Return, ⌫ or ⌦ of one character) the editor core
    /// made itself is refused, UIKit applies nothing, and this is false.
    @discardableResult
    func change(_ harness: Harness, _ range: NSRange, _ text: String, announced: Bool = true) -> Bool {
        let view = harness.view
        if announced, !harness.coordinator.textView(view, shouldChangeTextIn: range, replacementText: text) {
            return false
        }
        view.textStorage.replaceCharacters(in: range, with: text)
        view.selectedRange = NSRange(location: range.location + (text as NSString).length, length: 0)
        harness.coordinator.textViewDidChange(view)
        return true
    }

    /// Types `text` a character at a time at the caret.
    func type(_ harness: Harness, _ text: String) {
        for character in text {
            change(harness, harness.view.selectedRange, String(character))
        }
    }

    /// Types `text` a character at a time at `location`.
    func type(_ harness: Harness, _ text: String, at location: Int) {
        var at = location
        for character in text {
            let typed = String(character)
            change(harness, NSRange(location: at, length: 0), typed)
            at += (typed as NSString).length
        }
    }

    /// ⌫ as UIKit sends it: the character before the caret.
    func backspace(_ harness: Harness) {
        let caret = harness.view.selectedRange.location
        change(harness, harness.string.rangeOfComposedCharacterSequence(at: caret - 1), "")
    }

    func coreText(_ harness: Harness) -> String {
        harness.session.snapshot().markdown
    }

    /// `act` is one core edit and one Formatting step: the view's string is
    /// the core's, Undo takes back exactly it, Redo brings it again.
    func assertOneStep(
        _ harness: Harness, file: StaticString = #filePath, line: UInt = #line, _ act: () -> Void
    ) {
        let before = harness.shown
        let sent = harness.coordinator.bridge.sent
        act()
        let after = harness.shown
        let selection = harness.view.selectedRange
        // The command's own selection again, once undone and redone.
        defer { select(harness, selection) }
        XCTAssertNotEqual(after, before, "nothing changed", file: file, line: line)
        XCTAssertEqual(harness.coordinator.bridge.sent, sent + 1, "one core edit", file: file, line: line)
        XCTAssertEqual(coreText(harness), after, file: file, line: line)
        XCTAssertEqual(harness.session.snapshot().undoStep?.label, "Formatting", file: file, line: line)
        harness.model.undoTapped()
        show(harness)
        XCTAssertEqual(harness.shown, before, "one undo step", file: file, line: line)
        harness.model.redoTapped()
        show(harness)
        XCTAssertEqual(harness.shown, after, file: file, line: line)
    }
}
