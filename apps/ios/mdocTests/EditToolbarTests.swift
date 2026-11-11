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

    /// `scanned`: the names are found and the first is replaced, so the
    /// text holds an alias and a proposed name.
    func harness(text: String = text, rendered: Bool = true, scanned: Bool = false) throws -> Harness {
        let (core, storage) = try Fixture.core()
        let session = try core.library.importDocument(.text(text), title: "Lease", into: .newCase(name: "Lease"))
        _ = try session.recognize(observer: TestObserver())
        if scanned, let first = try session.scan(observer: TestObserver()).candidates.first {
            _ = try session.decide(first.id, .replace, scope: .this)
        }
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

    /// The toolbar is as tall as its buttons and margins wherever it is not
    /// at the screen's bottom, and at the bottom, with a hardware keyboard,
    /// it clears the home indicator. (`rc.7`'s gap above the keyboard came
    /// from the keyboard's own window: `EditToolbarUITests` measures it.)
    func testTheToolbarLeavesHomeIndicatorRoomOnlyAtTheScreensBottom() throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let window = UIWindow(windowScene: scene)
        window.isHidden = false
        let indicator = window.safeAreaInsets.bottom
        try XCTSkipIf(indicator == 0, "a device with no home indicator")
        let host = EditToolbarHost(model: EditToolbarModel())
        func height(at y: CGFloat) -> CGFloat {
            // Twice: the first pass finds the toolbar's height, the second
            // lays it out at that height.
            for _ in 0..<2 {
                host.frame = CGRect(x: 0, y: y - host.intrinsicContentSize.height, width: window.bounds.width, height: host.intrinsicContentSize.height)
                host.setNeedsLayout()
                window.layoutIfNeeded()
            }
            return host.intrinsicContentSize.height
        }
        window.addSubview(host)
        let own = Size.target + Space.xxs + Space.xs
        XCTAssertEqual(height(at: window.bounds.midY), own, accuracy: 0.5, "above a keyboard")
        XCTAssertEqual(height(at: window.bounds.maxY), own + indicator, accuracy: 0.5, "at the screen's bottom")
        XCTAssertEqual(height(at: window.bounds.midY), own, accuracy: 0.5, "and back")
    }

    /// On a landscape phone the toolbar's buttons stay out of the notch
    /// and the home indicator's side: the toolbar fills the host's safe
    /// area, not its bounds.
    func testTheToolbarKeepsToTheSafeAreaAtTheSides() throws {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let window = UIWindow(windowScene: scene)
        let controller = UIViewController()
        controller.additionalSafeAreaInsets = UIEdgeInsets(top: 0, left: 62, bottom: 0, right: 48)
        window.rootViewController = controller
        window.isHidden = false
        let host = EditToolbarHost(model: EditToolbarModel())
        host.frame = CGRect(x: 0, y: 200, width: controller.view.bounds.width, height: 60)
        controller.view.addSubview(host)
        window.layoutIfNeeded()
        let sides = host.safeAreaInsets
        XCTAssertGreaterThanOrEqual(sides.left, 62)
        XCTAssertGreaterThanOrEqual(sides.right, 48)
        let toolbar = try XCTUnwrap(host.subviews.first).frame
        XCTAssertEqual(toolbar.minX, sides.left, accuracy: 0.5)
        XCTAssertEqual(toolbar.maxX, host.bounds.width - sides.right, accuracy: 0.5)
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

    func testAaSwapsInTheFormattingKeyboardAndBack() throws {
        let harness = try harness()
        harness.toolbar.setFormatting(true)
        XCTAssertTrue(harness.toolbar.formatting)
        XCTAssertTrue(harness.view.inputView is FormattingKeyboardHost)
        // Aa again: no key on the keyboard does it (ADR 0110).
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

    // MARK: Block commands keep names and tables

    /// A list or task made of several paragraphs edits their line prefixes
    /// only, so the names in them stay candidates, in the core and in the
    /// view, and a table below stays on its own lines.
    func testListAndTaskKeepNamesAndTheTableBelow() throws {
        let text = """
            # Page 1

            Write to anna.smith@example.com about the lease.

            A

            Then to john.doe@example.org, long enough to wrap onto a second line of the screen.

            ## Details of the document

            | Name | Files |
            | --- | --- |
            | Passport | 1 |

            Last line.
            """
        let harness = try harness(text: text, scanned: true)
        let names = harness.session.snapshot().candidates.map(\.original).sorted()
        XCTAssertEqual(names, ["anna.smith@example.com", "john.doe@example.org"])
        XCTAssertEqual(harness.session.snapshot().candidates.filter { $0.state == .replaced }.count, 1)
        XCTAssertFalse(harness.shown.contains("anna.smith@example.com"), "an alias in the text")
        func check(_ what: String) throws {
            harness.view.layoutIfNeeded()
            XCTAssertEqual(harness.session.snapshot().candidates.map(\.original).sorted(), names, "\(what): the core's")
            let shown = harness.coordinator.livePreview.candidates
            XCTAssertEqual(shown.map(\.original).sorted(), names, "\(what): the view's")
            let map = UTF8Map(harness.shown)
            for candidate in shown {
                XCTAssertEqual(
                    harness.string.substring(with: map.range(of: candidate.range)),
                    candidate.state == .replaced ? candidate.alias : candidate.original, what)
            }
            XCTAssertEqual(coreText(harness), harness.shown, what)
            // The grid sits on its table's lines, below the heading.
            let grid = try XCTUnwrap(harness.view.tableViews.first, what)
            XCTAssertEqual(grid.table.anchor, harness.string.range(of: "| Name").location, "\(what): the grid's anchor")
            let heading = harness.string.range(of: "Details of the document")
            let from = try XCTUnwrap(harness.view.position(from: harness.view.beginningOfDocument, offset: heading.location))
            let to = try XCTUnwrap(harness.view.position(from: from, offset: heading.length))
            let line = harness.view.firstRect(for: try XCTUnwrap(harness.view.textRange(from: from, to: to)))
            XCTAssertGreaterThanOrEqual(grid.frame.minY, line.maxY - 1, "\(what): the grid is below the heading")
        }
        func selectParagraphs() {
            let start = harness.string.range(of: "Write to").location
            let end = NSMaxRange(harness.string.range(of: "of the screen."))
            select(harness, NSRange(location: start, length: end - start))
        }
        // Checked as the command leaves the view, before anything shows the
        // core's snapshot again.
        func step(_ command: EditCommand) {
            let sent = harness.coordinator.bridge.sent
            run(harness, command)
            XCTAssertEqual(harness.coordinator.bridge.sent, sent + 1, "one core edit")
        }
        try check("as opened")
        selectParagraphs()
        step(.task)
        XCTAssertTrue(harness.shown.contains(" about the lease.\n- [ ] A\n- [ ] Then to"), harness.shown)
        XCTAssertTrue(harness.shown.contains("- [ ] Write to "), harness.shown)
        try check("tasks")
        step(.list)
        XCTAssertTrue(harness.shown.contains(" about the lease.\n- A\n- Then to"), harness.shown)
        XCTAssertTrue(harness.shown.contains("- Write to "), harness.shown)
        try check("a list")
        step(.list)
        XCTAssertTrue(harness.shown.contains("lease.\n\nA\n\nThen to"), harness.shown)
        try check("paragraphs again")
        // One paragraph, the caret in it.
        caret(harness, at: "about the lease")
        step(.task)
        try check("one task")
        // Bold over the alias and the words after it, in the task: only
        // the markers are edited, on and off.
        func selectFromTheAlias() throws {
            let alias = try XCTUnwrap(harness.coordinator.livePreview.candidates.first { $0.state == .replaced })
            let start = UTF8Map(harness.shown).range(of: alias.range).location
            let end = NSMaxRange(harness.string.range(of: "about the lease"))
            select(harness, NSRange(location: start, length: end - start))
        }
        try selectFromTheAlias()
        step(.format(.bold))
        XCTAssertTrue(harness.shown.contains(" about the lease**."), harness.shown)
        try check("bold")
        try selectFromTheAlias()
        step(.format(.bold))
        XCTAssertFalse(harness.shown.contains("**"), harness.shown)
        try check("bold off")
        caret(harness, at: "about the lease")
        step(.turnInto(.heading2))
        try check("a heading")
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
