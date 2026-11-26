import XCTest

/// Restore with mdoc on a phone (#1091 Decisions 20 and 37, Vault &
/// Projects item 10), as the maintainer checks it by hand: Copy for AI in
/// mdoc, then a reply shared as text from Notes to the "mdoc" share
/// extension, which restores with the place of the last Copy for AI and
/// saves the reply; back in mdoc, the Reply panes list the replies,
/// restored. mdoc's own Share sends a file, which the text-only extension
/// does not take, so the text comes from Notes. The scratch notes it makes
/// carry this run's title and are deleted. Device only: the Simulator has
/// no Notes.
/// `just ios-device-test <udid>` runs it.
final class DeviceRestoreUITests: XCTestCase {
    private static let notesBundle = "com.apple.mobilenotes"
    private static let project = "Restore check"
    private static let projectDocument = "Project letter"
    private static let unfiledDocument = "Unfiled note"
    /// This run's scratch notes start with it, so the cleanup never takes
    /// a note of the phone's owner.
    private let scratchTitle = "mdoc restore check \(UUID().uuidString.prefix(8))"
    private var madeNote = false

    override func setUpWithError() throws {
        continueAfterFailure = false
        #if targetEnvironment(simulator)
            throw XCTSkip("device only: the Simulator has no Notes")
        #endif
    }

    @MainActor
    func testRepliesSharedFromNotesRestoreWithThePlaceSharedLast() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_RESTORE"] = "1"
        app.launch()
        let notes = XCUIApplication(bundleIdentifier: Self.notesBundle)
        // A teardown block, not defer: a failure stops the test without
        // running defer, which left the scratch notes behind.
        addTeardownBlock { @MainActor [weak self] in
            self?.deleteScratchNotes(XCUIApplication(bundleIdentifier: Self.notesBundle))
        }

        // The Project's document: Copy for AI, then a reply from Notes.
        let projectAlias = try copyForAI(Self.projectDocument, in: app)
        let projectReply = "Project reply: write to \(projectAlias) today."
        shareFromNotes(projectReply, notes: notes, expecting: "Project: \(Self.project)")

        // The Unfiled document: the extension now defaults to it.
        app.activate()
        let unfiledAlias = try copyForAI(Self.unfiledDocument, in: app)
        let unfiledReply = "Unfiled reply: ask \(unfiledAlias) first."
        shareFromNotes(unfiledReply, notes: notes, expecting: "Document: \(Self.unfiledDocument)")

        // Back in mdoc: each Reply pane lists its reply, restored.
        app.activate()
        openReplyPane(Self.unfiledDocument, in: app)
        expectRestored("Unfiled reply: ask ann.baker@example.com first.", in: app)
        openReplyPane(Self.projectDocument, in: app)
        expectRestored("Project reply: write to maya.ellison@example.com today.", in: app)
        // The Project's page lists it under Replies, and shows it restored.
        expectOnProjectPage(app)
        openReplyPane(Self.projectDocument, in: app)
        expectRestored("Project reply: write to maya.ellison@example.com today.", in: app)

        // A second reply for the Project, pasted in mdoc: the pane's menu
        // switches between them, and Remove deletes the one shown. Copied
        // in mdoc: after a copy in Notes, a test's tap on Paste brought up
        // iOS's "Allow Paste", which the system's Paste control exists to
        // spare a person's tap.
        MdocUITest.pasteReply("Pasted reply for \(projectAlias).", in: app)
        expectRestored("Pasted reply for maya.ellison@example.com.", in: app)
        let picker = app.buttons["reply-picker"].firstMatch
        XCTAssertTrue(picker.waitForExistence(timeout: MdocUITest.timeout), "no menu of saved replies")
        picker.tap()
        let other = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Reply to \(Self.project)")).firstMatch
        XCTAssertTrue(other.waitForExistence(timeout: MdocUITest.timeout), dump(app, "reply-menu"))
        other.tap()
        expectRestored("Project reply: write to maya.ellison@example.com today.", in: app)

        app.buttons["reply-actions"].firstMatch.tap()
        let confirm = app.buttons["reply-remove-confirm"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: MdocUITest.timeout))
        confirm.tap()
        expectRestored("Pasted reply for maya.ellison@example.com.", in: app)
        XCTAssertTrue(
            app.buttons["reply-picker"].firstMatch.waitForNonExistence(timeout: MdocUITest.timeout),
            "one reply left: no menu")
    }

    // MARK: mdoc

    /// Opens `title` and taps Copy for AI; returns the alias its email
    /// took, as the text shows it.
    @MainActor
    private func copyForAI(_ title: String, in app: XCUIApplication) throws -> String {
        open(title, in: app)
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 60))
        let alias = try XCTUnwrap(
            MdocUITest.wait(timeout: 60) { Self.alias(in: text.value as? String) != nil }
                ? Self.alias(in: text.value as? String) : nil,
            "no alias in \(title): \(String(describing: text.value))")
        let copy = app.buttons["copy-for-ai"].firstMatch
        XCTAssertTrue(copy.waitForExistence(timeout: 60), dump(app, "no-copy-\(title)"))
        copy.tap()
        return alias
    }

    private static func alias(in text: String?) -> String? {
        guard let text, let range = text.range(of: "EMAIL_[0-9]+", options: .regularExpression) else { return nil }
        return String(text[range])
    }

    /// Opens `title` from the recent documents the sidebar's search lists.
    @MainActor
    private func open(_ title: String, in app: XCUIApplication) {
        let row = MdocUITest.listedRow(app, title: title)
        XCTAssertTrue(row.waitForExistence(timeout: 60), dump(app, "no-row-\(title)"))
        row.tap()
    }

    @MainActor
    private func openReplyPane(_ title: String, in app: XCUIApplication) {
        open(title, in: app)
        let reply = app.segmentedControls["pane-picker"].buttons["Reply"].firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: MdocUITest.timeout), dump(app, "no-reply-pane-\(title)"))
        reply.tap()
    }

    /// The Project's page, from its sidebar row: Replies lists the reply
    /// from Notes, and a tap shows it restored.
    @MainActor
    private func expectOnProjectPage(_ app: XCUIApplication) {
        MdocUITest.revealSidebar(app)
        let project = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'sidebar-project' AND label == %@", Self.project)).firstMatch
        XCTAssertTrue(project.waitForExistence(timeout: MdocUITest.timeout), dump(app, "no-project-row"))
        project.tap()
        let segments = app.segmentedControls["project-segments"]
        XCTAssertTrue(segments.waitForExistence(timeout: MdocUITest.timeout), dump(app, "no-project-page"))
        segments.buttons["Replies"].tap()
        let reply = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'reply-row' AND label BEGINSWITH %@", "Reply to \(Self.project)"))
            .firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: MdocUITest.timeout), dump(app, "no-project-reply"))
        reply.tap()
        expectRestored("Project reply: write to maya.ellison@example.com today.", in: app)
        app.buttons["Done"].firstMatch.tap()
    }

    @MainActor
    private func expectRestored(_ restored: String, in app: XCUIApplication) {
        let text = app.textViews["restored-text"].firstMatch
        XCTAssertTrue(
            text.wait(until: NSPredicate(format: "value CONTAINS %@", restored), timeout: MdocUITest.timeout),
            "not restored: \(restored); \(String(describing: text.value))")
    }

    // MARK: Notes and the extension

    /// A scratch note with `reply`, shared to "mdoc": the extension's
    /// picker shows `place`, then Save and Done.
    @MainActor
    private func shareFromNotes(_ reply: String, notes: XCUIApplication, expecting place: String) {
        newNote(reply, notes: notes)
        let share = Self.first(notes.buttons, labels: ["Share", "Compartir"], identifiers: ["share"])
        XCTAssertTrue(share.waitForExistence(timeout: MdocUITest.timeout), dump(notes, "notes-note"))
        share.tap()
        // iOS 26 asks first how to share a note: a copy.
        let copy = Self.first(notes.buttons, labels: ["Send Copy", "Enviar copia"], identifiers: [])
        if copy.waitForExistence(timeout: MdocUITest.probeTimeout) { copy.tap() }
        let mdoc = notes.descendants(matching: .any)
            .matching(NSPredicate(format: "label IN {'mdoc', 'Restore with mdoc'}"))
            .firstMatch
        XCTAssertTrue(mdoc.waitForExistence(timeout: MdocUITest.timeout), dump(notes, "share-sheet"))
        mdoc.tap()

        let picker = notes.descendants(matching: .any)["restore-project-picker"].firstMatch
        XCTAssertTrue(picker.waitForExistence(timeout: 30), dump(notes, "extension"))
        XCTAssertEqual(picker.label, place, "the extension's default place")
        let save = notes.buttons["restore-save"].firstMatch
        XCTAssertTrue(save.waitForExistence(timeout: MdocUITest.timeout))
        save.tap()
        XCTAssertTrue(
            save.wait(until: NSPredicate(format: "label == 'Saved'"), timeout: MdocUITest.timeout), dump(notes, "saved"))
        notes.buttons["Done"].firstMatch.tap()
        XCTAssertTrue(picker.waitForNonExistence(timeout: MdocUITest.timeout))
    }

    /// A scratch note with this run's title and `body`, left open with
    /// its text focused.
    @MainActor
    private func newNote(_ body: String, notes: XCUIApplication) {
        notes.activate()
        XCTAssertTrue(notes.wait(for: .runningForeground, timeout: MdocUITest.timeout))
        let compose = Self.first(
            notes.buttons, labels: ["New Note", "Nueva nota", "Compose", "Redactar"], identifiers: ["newNote", "compose"])
        XCTAssertTrue(compose.waitForExistence(timeout: MdocUITest.timeout), dump(notes, "notes-start"))
        madeNote = true
        compose.tap()
        let text = notes.textViews.firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: MdocUITest.timeout), dump(notes, "notes-new"))
        notes.typeText("\(scratchTitle)\n\(body)")
    }

    /// This run's scratch notes go, whatever the test left open in Notes;
    /// one that stays fails the run.
    @MainActor
    private func deleteScratchNotes(_ notes: XCUIApplication) {
        guard madeNote else { return }
        notes.activate()
        XCTAssertTrue(notes.wait(for: .runningForeground, timeout: MdocUITest.timeout), "Notes did not open to clean up")
        // A share sheet or the extension left open covers the note.
        let sheet = notes.otherElements["ActivityListView"].firstMatch
        for _ in 0..<2 where sheet.exists {
            sheet.swipeDown(velocity: .fast)
            _ = sheet.waitForNonExistence(timeout: MdocUITest.probeTimeout)
        }
        let note = notes.cells.matching(NSPredicate(format: "label BEGINSWITH %@", scratchTitle)).firstMatch
        // From a note, back leads to its list or to the folders, where the
        // first folder named Notes holds new notes.
        let editorBack = notes.navigationBars["ICTK2NoteEditorView"].buttons["BackButton"]
        let folder = notes.cells.matching(NSPredicate(format: "label BEGINSWITH 'Notas' OR label BEGINSWITH 'Notes'"))
            .firstMatch
        for _ in 0..<8 {
            if !note.waitForExistence(timeout: MdocUITest.probeTimeout) {
                if editorBack.exists {
                    editorBack.tap()
                } else if folder.exists {
                    folder.tap()
                } else {
                    break
                }
                continue
            }
            note.swipeLeft()
            let delete = Self.first(
                notes.buttons, labels: ["Delete", "Delete Note", "Eliminar", "Eliminar nota", "Trash", "Papelera"],
                identifiers: [])
            guard delete.waitForExistence(timeout: MdocUITest.probeTimeout) else { break }
            delete.tap()
            _ = note.waitForNonExistence(timeout: MdocUITest.probeTimeout)
        }
        XCTAssertFalse(note.exists, dump(notes, "scratch-note-left"))
    }

    private static func first(_ query: XCUIElementQuery, labels: [String], identifiers: [String]) -> XCUIElement {
        var predicates = [NSPredicate(format: "label IN %@", labels)]
        predicates += identifiers.map { NSPredicate(format: "identifier CONTAINS[c] %@", $0) }
        return query.matching(NSCompoundPredicate(orPredicateWithSubpredicates: predicates)).firstMatch
    }

    /// The tree as text, kept with the run, for a failure's message.
    @MainActor
    private func dump(_ app: XCUIApplication, _ name: String) -> String {
        let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
        let tree = XCTAttachment(string: app.debugDescription)
        tree.name = "\(name)-tree"
        tree.lifetime = .keepAlways
        add(tree)
        return name
    }
}
