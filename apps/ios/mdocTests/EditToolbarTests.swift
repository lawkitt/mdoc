import MdocFFI
import UIKit
import XCTest
@testable import mdoc

/// Edit mode's toolbar, formatting keyboard, links and shortcuts (ADR 0109,
/// slice 4), on the real core: each button is one core edit and one
/// "Formatting" undo step, the view's string stays the core's, and the
/// buttons show on where the caret is in what they make and off where they
/// cannot apply.
@MainActor
final class EditToolbarTests: XCTestCase {
    static let text = """
        # Lease

        The tenant pays **monthly** rent to [Riverside](https://example.com).

        - first
        - second

        ```
        code
        ```

        Last line.
        """

    struct Harness {
        let session: any mdoc.DocumentSession
        let model: DocumentViewModel
        let window: UIWindow
        let view: CandidateTextView
        let coordinator: MarkdownTextView.Coordinator
        let rendered: Bool

        var shown: String { view.textStorage.string }
        var string: NSString { view.textStorage.string as NSString }
        var toolbar: EditToolbarModel { coordinator.toolbar }
    }

    func harness(text: String = text, rendered: Bool = true) throws -> Harness {
        let (core, storage) = try Fixture.core()
        let session = try core.library.importDocument(.text(text), title: "Lease", into: .newCase(name: "Lease"))
        _ = try session.recognize(observer: TestObserver())
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let view = CandidateTextView()
        view.frame = window.bounds
        window.addSubview(view)
        window.isHidden = false
        let coordinator = MarkdownTextView.Coordinator(parent: LivePreviewTests.textView(model, rendered: rendered))
        coordinator.attach(to: view)
        view.delegate = coordinator
        let harness = Harness(
            session: session, model: model, window: window, view: view, coordinator: coordinator, rendered: rendered)
        show(harness)
        model.enterEditMode()
        show(harness)
        XCTAssertTrue(view.isEditMode)
        return harness
    }

    func show(_ harness: Harness) {
        harness.coordinator.update(
            harness.view, with: LivePreviewTests.textView(harness.model, rendered: harness.rendered),
            dynamicTypeSize: .large, colorScheme: .light)
        harness.view.layoutIfNeeded()
    }

    /// The reader puts the caret (or a selection) at `range`; the toolbar
    /// asks the core.
    func select(_ harness: Harness, _ range: NSRange) {
        harness.view.selectedRange = range
        harness.coordinator.textViewDidChangeSelection(harness.view)
        harness.coordinator.refreshToolbar(harness.view)
    }

    func caret(_ harness: Harness, at needle: String, offset: Int = 0) {
        select(harness, NSRange(location: harness.string.range(of: needle).location + offset, length: 0))
    }

    func run(_ harness: Harness, _ command: EditCommand) {
        harness.toolbar.run(command)
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

    // MARK: The toolbar

    func testTheToolbarIsAboveTheKeyboardOnlyInEditMode() throws {
        let harness = try harness()
        XCTAssertTrue(harness.view.inputAccessoryView is EditToolbarHost)
        XCTAssertFalse(harness.view.inputAccessoryView?.canBecomeFirstResponder ?? true)
        harness.model.doneEditing()
        show(harness)
        XCTAssertNil(harness.view.inputAccessoryView)
        XCTAssertNil(harness.view.inputView)
    }

    func testBoldWrapsTheCaretsWordAndComesOffInsideBold() throws {
        let harness = try harness()
        caret(harness, at: "tenant", offset: 2)
        XCTAssertFalse(harness.toolbar.isOn(.format(.bold)))
        XCTAssertTrue(harness.toolbar.isEnabled(.format(.bold)))
        assertOneStep(harness) { run(harness, .format(.bold)) }
        XCTAssertTrue(harness.shown.contains("The **tenant** pays"))
        // The caret stayed in the word, now bold: Bold shows on.
        harness.coordinator.refreshToolbar(harness.view)
        XCTAssertTrue(harness.toolbar.isOn(.format(.bold)))
        // Inside bold, Bold takes its markers off.
        caret(harness, at: "monthly", offset: 3)
        XCTAssertTrue(harness.toolbar.isOn(.format(.bold)))
        assertOneStep(harness) { run(harness, .format(.bold)) }
        XCTAssertTrue(harness.shown.contains("pays monthly rent"))
    }

    func testItalicAndTheFormattingKeysWrapASelection() throws {
        let harness = try harness()
        for (command, wrapped) in [
            (EditCommand.format(.italic), "*rent*"), (.format(.strikethrough), "~~rent~~"),
            (.format(.code), "`rent`"), (.format(.highlight), "==rent=="),
        ] {
            let rent = harness.string.range(of: "rent")
            select(harness, rent)
            assertOneStep(harness) { run(harness, command) }
            XCTAssertTrue(harness.shown.contains(wrapped), "\(command): \(harness.shown)")
            // The same text stays selected and the key shows on: it toggles off.
            harness.coordinator.refreshToolbar(harness.view)
            XCTAssertTrue(harness.toolbar.isOn(command), "\(command)")
            run(harness, command)
            XCTAssertTrue(harness.shown.contains(" rent to"), "\(command): \(harness.shown)")
            XCTAssertEqual(coreText(harness), harness.shown)
        }
    }

    func testListAndTaskButtons() throws {
        let harness = try harness()
        caret(harness, at: "Last line.")
        XCTAssertFalse(harness.toolbar.isOn(.list))
        assertOneStep(harness) { run(harness, .list) }
        XCTAssertTrue(harness.shown.hasSuffix("- Last line."))
        harness.coordinator.refreshToolbar(harness.view)
        XCTAssertTrue(harness.toolbar.isOn(.list))
        assertOneStep(harness) { run(harness, .task) }
        XCTAssertTrue(harness.shown.hasSuffix("- [ ] Last line."))
        harness.coordinator.refreshToolbar(harness.view)
        XCTAssertTrue(harness.toolbar.isOn(.task))
        XCTAssertFalse(harness.toolbar.isOn(.list))
        XCTAssertEqual(harness.toolbar.taskValue, "Not checked")
        // On a task, the task button checks its box.
        assertOneStep(harness) { run(harness, .task) }
        XCTAssertTrue(harness.shown.hasSuffix("- [x] Last line."))
        harness.coordinator.refreshToolbar(harness.view)
        XCTAssertEqual(harness.toolbar.taskValue, "Checked")
    }

    func testIndentNestsUnderTheItemAboveAndOutdentTakesItBack() throws {
        let harness = try harness()
        caret(harness, at: "first", offset: 5)
        XCTAssertFalse(harness.toolbar.isEnabled(.indent), "nothing above the first item")
        XCTAssertFalse(harness.toolbar.isEnabled(.outdent))
        caret(harness, at: "second", offset: 6)
        XCTAssertTrue(harness.toolbar.isEnabled(.indent))
        assertOneStep(harness) { run(harness, .indent) }
        XCTAssertTrue(harness.shown.contains("- first\n    - second"))
        harness.coordinator.refreshToolbar(harness.view)
        XCTAssertTrue(harness.toolbar.isEnabled(.outdent))
        assertOneStep(harness) { run(harness, .outdent) }
        XCTAssertTrue(harness.shown.contains("- first\n- second"))
        // Not on a list item: indent is off (four spaces would make code).
        caret(harness, at: "Last line.")
        XCTAssertFalse(harness.toolbar.isEnabled(.indent))
    }

    func testFormatsAreOffInCodeAndUndoFollowsTheCore() throws {
        let harness = try harness(rendered: false)
        caret(harness, at: "code\n", offset: 2)
        XCTAssertFalse(harness.toolbar.isEnabled(.format(.bold)))
        XCTAssertFalse(harness.toolbar.isEnabled(.rule))
        XCTAssertTrue(harness.toolbar.isOn(.turnInto(.codeBlock)))
        // ⌘B there does nothing either: the core leaves code's text alone.
        let before = harness.shown, sent = harness.coordinator.bridge.sent
        harness.view.boldKey()
        XCTAssertEqual(harness.shown, before)
        XCTAssertEqual(harness.coordinator.bridge.sent, sent)
        XCTAssertFalse(harness.toolbar.canUndo)
        caret(harness, at: "Last")
        run(harness, .format(.bold))
        harness.coordinator.refreshToolbar(harness.view)
        XCTAssertTrue(harness.toolbar.canUndo)
        XCTAssertFalse(harness.toolbar.canRedo)
        harness.toolbar.undo()
        show(harness)
        XCTAssertTrue(harness.toolbar.canRedo)
    }

    // MARK: The formatting keyboard

    func testAaSwapsInTheFormattingKeyboardAndItsKeyBringsTheKeyboardBack() throws {
        let harness = try harness()
        harness.toolbar.setFormatting(true)
        XCTAssertTrue(harness.toolbar.formatting)
        XCTAssertTrue(harness.view.inputView is FormattingKeyboardHost)
        harness.toolbar.setFormatting(false)
        XCTAssertFalse(harness.toolbar.formatting)
        XCTAssertNil(harness.view.inputView)
        // Hide keyboard leaves edit mode on.
        harness.toolbar.setFormatting(true)
        harness.toolbar.hideKeyboard()
        XCTAssertNil(harness.view.inputView)
        XCTAssertFalse(harness.toolbar.formatting)
        XCTAssertTrue(harness.model.isEditing)
    }

    func testTurnIntoEachKindAsOneStepWithItsKeySelected() throws {
        let harness = try harness()
        let kinds: [(EditorBlockKind, String)] = [
            (.heading1, "# Last line."), (.heading2, "## Last line."), (.heading3, "### Last line."),
            (.quote, "> Last line."), (.callout, "> [!NOTE]"), (.codeBlock, "```\nLast line.\n```"),
        ]
        for (kind, made) in kinds {
            caret(harness, at: "Last line.", offset: 4)
            assertOneStep(harness) { run(harness, .turnInto(kind)) }
            XCTAssertTrue(harness.shown.contains(made), "\(kind): \(harness.shown)")
            caret(harness, at: "Last line.", offset: 4)
            XCTAssertTrue(harness.toolbar.isOn(.turnInto(kind)), "\(kind)")
            run(harness, .turnInto(.text))
            XCTAssertTrue(harness.shown.hasSuffix("\n\nLast line."), "\(kind) back to text: \(harness.shown)")
        }
    }

    func testRuleAndTableGoInAsBlocksOfTheirOwn() throws {
        let harness = try harness(rendered: false)
        caret(harness, at: "Last line.", offset: 10)
        assertOneStep(harness) { run(harness, .rule) }
        XCTAssertTrue(harness.shown.hasSuffix("Last line.\n\n---\n\n"), harness.shown)
        XCTAssertEqual(harness.view.selectedRange.location, harness.string.length)
        assertOneStep(harness) { run(harness, .insertTable) }
        XCTAssertTrue(harness.shown.hasSuffix("---\n\n|  |  |\n| --- | --- |\n|  |  |\n\n"), harness.shown)
        // With Show Markdown on the caret is in the first cell.
        let table = harness.string.range(of: "|  |  |").location
        XCTAssertEqual(harness.view.selectedRange.location, table + 2)
        harness.coordinator.refreshToolbar(harness.view)
        XCTAssertTrue(harness.toolbar.state?.inTable == true)
        XCTAssertFalse(harness.toolbar.isEnabled(.insertTable))
    }

    // MARK: Links

    func testTheCaretsLinkOffersOpenOnlyForTheWeb() throws {
        let harness = try harness(text: "See [Riverside](https://example.com) and [run](javascript:alert(1)).")
        var opened: [URL] = []
        harness.toolbar.open = { opened.append($0) }
        caret(harness, at: "Riverside", offset: 2)
        XCTAssertEqual(harness.toolbar.state?.link?.text, "Riverside")
        XCTAssertEqual(harness.toolbar.webURL, URL(string: "https://example.com"))
        caret(harness, at: "run", offset: 1)
        XCTAssertNotNil(harness.toolbar.state?.link)
        XCTAssertNil(harness.toolbar.webURL, "only http and https open")
        caret(harness, at: "See")
        XCTAssertNil(harness.toolbar.state?.link)
        XCTAssertTrue(opened.isEmpty, "nothing opens without a tap")
    }

    func testTheLinkSheetMakesALinkFromTheSelection() throws {
        let harness = try harness()
        select(harness, harness.string.range(of: "rent"))
        harness.toolbar.editLink()
        let draft = try XCTUnwrap(harness.model.linkDraft)
        XCTAssertEqual(draft.text, "rent")
        XCTAssertEqual(draft.url, "")
        XCTAssertFalse(draft.editing)
        assertOneStep(harness) { draft.finish("the rent", "https://example.org/rent") }
        XCTAssertTrue(harness.shown.contains("pays **monthly** [the rent](https://example.org/rent) to"), harness.shown)
    }

    func testTheLinkSheetEditsAndRemovesTheCaretsLink() throws {
        let harness = try harness(rendered: false)
        caret(harness, at: "Riverside", offset: 3)
        harness.view.boldKey()
        caret(harness, at: "Riverside", offset: 3)
        harness.view.linkKey()
        let draft = try XCTUnwrap(harness.model.linkDraft)
        XCTAssertEqual(draft.text, "**Riverside**")
        XCTAssertEqual(draft.url, "https://example.com")
        XCTAssertTrue(draft.editing)
        assertOneStep(harness) { draft.finish("Riverside Ltd", "https://riverside.example") }
        XCTAssertTrue(harness.shown.contains("[Riverside Ltd](https://riverside.example)."), harness.shown)
        // An empty URL removes the link and keeps its text.
        caret(harness, at: "Riverside Ltd", offset: 1)
        harness.toolbar.editLink()
        let removing = try XCTUnwrap(harness.model.linkDraft)
        assertOneStep(harness) { removing.finish(removing.text, "") }
        XCTAssertTrue(harness.shown.contains("rent to Riverside Ltd."), harness.shown)
    }

    // MARK: Hardware keys

    func testShortcutsAreListedAndRunTheSameCommands() throws {
        let harness = try harness()
        let titles = CandidateTextView.editKeyCommands.compactMap(\.discoverabilityTitle)
        XCTAssertEqual(titles, ["Bold", "Italic", "Link…", "Indent", "Outdent"])
        let tab = CandidateTextView.editKeyCommands.filter { $0.input == "\t" }
        XCTAssertEqual(tab.map(\.modifierFlags), [[], .shift])
        caret(harness, at: "tenant", offset: 1)
        assertOneStep(harness) { harness.view.italicKey() }
        XCTAssertTrue(harness.shown.contains("The *tenant* pays"))
        caret(harness, at: "second")
        assertOneStep(harness) { harness.view.indentKey() }
        assertOneStep(harness) { harness.view.outdentKey() }
        // Outside edit mode the shortcuts go.
        harness.model.doneEditing()
        show(harness)
        XCTAssertFalse(harness.view.keyCommands?.contains { $0.discoverabilityTitle == "Bold" } ?? false)
    }
}
