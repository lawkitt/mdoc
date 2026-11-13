import XCTest
@testable import mdoc

/// The app shell (ADR 0104, ROADMAP iOS 17): one navigator, one
/// presentation, the window's layout, the open documents' models, Delete
/// with Undo, the launch configuration and the preferences.
@MainActor
final class AppShellTests: XCTestCase {
    private var seeded: Seeded!

    private func app() throws -> AppModel {
        seeded = try Fixture.seeded()
        return Fixture.app(seeded)
    }

    // MARK: Navigation

    /// A drawer starts closed and a standing column shows: a window that
    /// narrows opens nothing over the screen (#675), and opening something
    /// in a wide window keeps the column.
    func testTheSidebarFollowsTheLayout() {
        let navigator = Navigator()
        navigator.sidebarLayoutChanged(stands: true)
        XCTAssertTrue(navigator.showsSidebar)
        navigator.show(.section(.cases))
        XCTAssertTrue(navigator.showsSidebar, "the standing column stays")
        navigator.sidebarLayoutChanged(stands: false)
        XCTAssertFalse(navigator.showsSidebar, "the drawer starts closed")
        navigator.showsSidebar = true
        navigator.show(.start)
        XCTAssertFalse(navigator.showsSidebar, "the drawer closes on a destination")
    }

    /// ⌘K shows the sidebar wherever it is and waits for it to take the
    /// search; a column hidden in a wide window shows again.
    func testSearchShowsAHiddenColumn() {
        let navigator = Navigator()
        navigator.sidebarLayoutChanged(stands: true)
        navigator.showsSidebar = false
        navigator.requestSearch()
        XCTAssertTrue(navigator.showsSidebar)
        XCTAssertTrue(navigator.takeSearchRequest())
    }

    func testDestinationsAreOneValue() throws {
        let app = try app()
        app.open(seeded.letter)
        XCTAssertEqual(app.navigator.destination, .document(seeded.letter))
        XCTAssertNil(app.navigator.section)
        app.open(.cases)
        XCTAssertEqual(app.navigator.destination, .section(.cases))
        XCTAssertNil(app.navigator.openDocument)
        app.showStart()
        XCTAssertEqual(app.navigator.destination, .start)
    }

    // MARK: Presentation

    /// One thing is presented at a time: a newer presentation replaces the
    /// older, and dismissing an older one leaves the newer alone.
    func testOnePresentationAtATime() {
        let navigator = Navigator()
        navigator.present(.settings)
        let settings = navigator.presentation?.id
        navigator.present(.intake(.paste))
        XCTAssertEqual(navigator.presentation?.id, Presentation.intake(.paste).id)
        navigator.dismiss(settings)
        XCTAssertNotNil(navigator.presentation, "a stale dismissal leaves the newer one")
        navigator.dismiss()
        XCTAssertNil(navigator.presentation)
    }

    /// Opening a document closes Settings over it.
    func testOpeningADocumentClosesSettings() throws {
        let app = try app()
        app.navigator.present(.settings)
        app.open(seeded.letter)
        XCTAssertNil(app.navigator.presentation)
    }

    /// Something brought in waits in the import sheet; adding it opens the
    /// document and closes the sheet. A failure presents the error.
    func testAFileWaitsForItsTitleThenOpens() throws {
        let app = try app()
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).pdf")
        try FileManager.default.copyItem(at: Fixture.repository.appendingPathComponent("tests/fixtures/import/text.pdf"), to: file)
        app.receiveFile(file)
        guard case .importing(let pending) = app.navigator.presentation else {
            return XCTFail("no import sheet: \(String(describing: app.navigator.presentation))")
        }
        let model = try XCTUnwrap(app.finishImport(pending, title: "Lease", into: .newCase(name: "Lease")))
        XCTAssertNil(app.navigator.presentation)
        XCTAssertEqual(app.navigator.openDocument, model.id)
        XCTAssertTrue(app.openModel === model)

        app.receiveFile(FileManager.default.temporaryDirectory.appendingPathComponent("notes.xyz"))
        guard case .error = app.navigator.presentation else {
            return XCTFail("no error for an unknown file")
        }
    }

    // MARK: Document models

    /// A model is made when its document opens, and goes once another
    /// opens; the window's undo steps come back with the document (#672).
    func testModelsAreMadeOnOpenAndEvicted() throws {
        let app = try app()
        XCTAssertNil(app.documents.model(for: seeded.letter), "nothing is made before it opens")
        app.open(seeded.letter)
        let letter = try XCTUnwrap(app.openModel)
        letter.systemUndoSteps = 2
        app.open(seeded.quotation)
        XCTAssertNil(app.documents.model(for: seeded.letter), "a document no longer open lets its model go")
        XCTAssertEqual(app.documents.ids, [seeded.quotation])
        app.open(seeded.letter)
        let again = try XCTUnwrap(app.openModel)
        XCTAssertFalse(again === letter)
        XCTAssertEqual(again.systemUndoSteps, 2, "its window steps come back")
    }

    /// A document waiting for its model's download keeps its model when
    /// another opens: Download & continue holds it and runs its job (review
    /// of #691).
    func testAModelWaitingForSetupIsKept() async throws {
        let seeded = try Fixture.seeded(memory: Fixture.detectorTier)
        let app = Fixture.app(seeded)
        app.open(seeded.quotation)
        let waiting = try XCTUnwrap(app.openModel)
        await waiting.findNames()
        XCTAssertTrue(waiting.needsSetup)
        app.open(seeded.letter)
        XCTAssertTrue(app.documents.model(for: seeded.quotation) === waiting)
        app.open(seeded.quotation)
        XCTAssertTrue(app.openModel === waiting, "the same model, with its setup card")
    }

    /// Share from a menu makes a model for a document not open, and lets
    /// it go again.
    func testShareFromAMenuKeepsNoModel() throws {
        let app = try app()
        XCTAssertNotNil(app.shareItems(for: seeded.quotation))
        XCTAssertTrue(app.documents.ids.isEmpty)
    }

    // MARK: Layout

    func testTheLayoutComesFromTheWindow() {
        XCTAssertEqual(DocumentLayout(sizeClass: .compact, width: 1300), .compact)
        XCTAssertEqual(DocumentLayout(sizeClass: .regular, width: 1032), .narrow)
        XCTAssertEqual(DocumentLayout(sizeClass: .regular, width: 1376), .wide)
        XCTAssertEqual(DocumentLayout(sizeClass: .regular, width: 0), .wide, "before the width is known")
        XCTAssertTrue(DocumentLayout.wide.sidebarStands)
        XCTAssertFalse(DocumentLayout.narrow.sidebarStands, "a narrow iPad slides the sidebar over (Q13)")
        XCTAssertFalse(DocumentLayout.compact.sidebarStands)
        XCTAssertEqual(DocumentLayout.compact.identities(hasRightPane: true), .drawer)
        XCTAssertEqual(DocumentLayout.narrow.identities(hasRightPane: true), .column)
        XCTAssertEqual(DocumentLayout.narrow.identities(hasRightPane: false), .inspector)
        XCTAssertEqual(DocumentLayout.wide.identities(hasRightPane: true), .inspector)
    }

    /// A document with a right pane (the quotation with a reply): landscape
    /// shows Identities in the inspector; rotating to portrait puts the
    /// pane in the right column, and Identities there replace it on
    /// request; back to landscape, the inspector again.
    func testRotationMovesIdentitiesBetweenInspectorAndColumn() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.quotation)
        model.restore(reply: "Thanks.")
        XCTAssertTrue(model.hasRightPane)
        model.show(.text)
        model.windowChanged(to: .wide)
        XCTAssertEqual(model.identitiesPlacement, .inspector)
        XCTAssertTrue(model.identitiesShowing, "a wide window opens the inspector")
        model.windowChanged(to: .narrow)
        XCTAssertEqual(model.identitiesPlacement, .column)
        XCTAssertFalse(model.identitiesShowing, "portrait shows the right pane first")
        model.showIdentities(true)
        XCTAssertTrue(model.identitiesShowing)
        model.windowChanged(to: .wide)
        XCTAssertEqual(model.identitiesPlacement, .inspector)
        XCTAssertTrue(model.identitiesShowing)
    }

    /// A narrowing window (a split iPad going compact) closes nothing it
    /// did not open and opens no drawer; widening again shows the inspector.
    func testANarrowingWindowOpensNoDrawer() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        model.windowChanged(to: .wide)
        XCTAssertTrue(model.wantsIdentities)
        model.windowChanged(to: .compact)
        XCTAssertEqual(model.identitiesPlacement, .drawer)
        XCTAssertFalse(model.identitiesDrawerOpen)
        XCTAssertFalse(model.identitiesShowing)
        model.showIdentities(true)
        XCTAssertTrue(model.identitiesDrawerOpen)
        model.windowChanged(to: .wide)
        XCTAssertFalse(model.identitiesDrawerOpen, "the drawer closes as the window widens")
        XCTAssertTrue(model.identitiesShowing, "the inspector shows")
        model.windowChanged(to: .compact)
        XCTAssertFalse(model.identitiesShowing, "and narrowing again keeps the text in view (#674)")
    }

    /// In a narrow window a restored reply takes the right column from
    /// Identities, and Identities there send the bar back to the text.
    func testTheReplyPaneAndIdentitiesShareTheColumn() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.quotation)
        model.windowChanged(to: .narrow)
        XCTAssertEqual(model.identitiesPlacement, .inspector, "no right pane yet")
        XCTAssertTrue(model.identitiesShowing)
        model.restore(reply: "Thanks.")
        XCTAssertEqual(model.pane, .reply)
        XCTAssertEqual(model.identitiesPlacement, .column)
        XCTAssertFalse(model.identitiesShowing, "the reply shows over the list")
        model.showIdentities(true)
        XCTAssertEqual(model.pane, .text, "Identities in the column: the bar acts on the text")
        XCTAssertTrue(model.identitiesShowing)
        model.show(.reply)
        XCTAssertFalse(model.identitiesShowing)
        model.clearReply()
        XCTAssertEqual(model.identitiesPlacement, .inspector)
        XCTAssertTrue(model.identitiesShowing, "no right pane: the inspector again")
    }

    /// Identities in the narrow column give way to edit mode and come back
    /// on Done, the bar on the text (ADR 0110); a pane chosen while editing
    /// keeps the column after Done.
    func testIdentitiesInTheColumnComeBackOnDone() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.quotation)
        model.windowChanged(to: .narrow)
        model.restore(reply: "Thanks.")
        model.showIdentities(true)
        model.enterEditMode()
        XCTAssertFalse(model.identitiesShowing, "the column gives way to the original")
        model.doneEditing()
        XCTAssertTrue(model.identitiesShowing, "back on Done")
        XCTAssertEqual(model.pane, .text, "the bar acts on the text, not the hidden reply")

        model.enterEditMode()
        model.show(.reply)
        model.doneEditing()
        XCTAssertFalse(model.identitiesShowing, "the reply chosen while editing stays")
        XCTAssertEqual(model.pane, .reply)
    }

    /// A document waiting for recognition opens without the inspector.
    func testADocumentWaitingForRecognitionOpensWithoutTheInspector() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.tenancy)
        model.windowChanged(to: .wide)
        XCTAssertFalse(model.identitiesShowing)
    }

    // MARK: Delete with Undo (Q17)

    /// Delete hides the document at once, closes it if open, and offers
    /// Undo; Undo puts it back in place, open again.
    func testDeleteOffersUndoThatRestoresInPlace() throws {
        let app = try app()
        let order = app.library.documents.map(\.id)
        app.open(seeded.letter)
        app.delete(seeded.letter)
        XCTAssertFalse(app.library.documents.contains { $0.id == seeded.letter })
        XCTAssertEqual(app.navigator.destination, .start)
        let toast = try XCTUnwrap(app.toast)
        XCTAssertTrue(toast.text.contains(Letters.letterTitle))
        Toast(message: toast) { app.toastEnded(toast.id) }.perform()
        XCTAssertEqual(app.library.documents.map(\.id), order, "back in place")
        XCTAssertEqual(app.navigator.openDocument, seeded.letter, "open again")
        XCTAssertNoThrow(try seeded.library.session(for: seeded.letter))
        XCTAssertNil(app.toast)
    }

    /// The document is deleted when its toast ends.
    func testDeleteHappensWhenTheToastEnds() throws {
        let app = try app()
        app.delete(seeded.letter)
        XCTAssertTrue(seeded.library.snapshot().documents.contains { $0.id == seeded.letter }, "not yet")
        app.toastEnded(try XCTUnwrap(app.toast).id)
        XCTAssertFalse(seeded.library.snapshot().documents.contains { $0.id == seeded.letter })
        XCTAssertFalse(app.library.documents.contains { $0.id == seeded.letter })
        XCTAssertNil(app.toast)
    }

    /// Leaving the foreground deletes it at once, as does another toast
    /// taking the place of Delete's.
    func testDeleteHappensWhenMdocLeavesOrAnotherToastShows() throws {
        let app = try app()
        app.delete(seeded.letter)
        app.leftForeground()
        XCTAssertFalse(seeded.library.snapshot().documents.contains { $0.id == seeded.letter })
        app.delete(seeded.quotation)
        app.show(ToastMessage(text: "Copied"))
        XCTAssertFalse(seeded.library.snapshot().documents.contains { $0.id == seeded.quotation })
        app.undoDelete()
        XCTAssertFalse(app.library.documents.contains { $0.id == seeded.quotation }, "too late for Undo")
    }

    /// Once the delete happened, its toast goes: no Undo that undoes nothing
    /// (review of #691).
    func testTheToastGoesWithTheDelete() throws {
        let app = try app()
        app.delete(seeded.letter)
        XCTAssertNotNil(app.toast)
        app.leftForeground()
        XCTAssertNil(app.toast)
    }

    /// Undo puts the document back, but opens it again only if nothing
    /// else opened meanwhile (review of #691).
    func testUndoLeavesWhatOpenedSince() throws {
        let app = try app()
        app.open(seeded.letter)
        app.delete(seeded.letter)
        app.open(seeded.quotation)
        app.undoDelete()
        XCTAssertEqual(app.navigator.openDocument, seeded.quotation)
        XCTAssertTrue(app.library.documents.contains { $0.id == seeded.letter })
    }

    // MARK: Launch and preferences

    func testTheLaunchIsReadOnce() {
        let normal = LaunchConfiguration(arguments: ["mdoc"], environment: ["MDOC_SEED_FILE": "/x.docx"])
        XCTAssertFalse(normal.isUITesting)
        XCTAssertNil(normal.seedFile, "only UI tests seed")
        let testing = LaunchConfiguration(
            arguments: ["mdoc", LaunchConfiguration.uiTestingArgument],
            environment: ["MDOC_SEED_FILE": "/x.docx", "MDOC_REAL_DOWNLOADS": "1", "MDOC_UI_DETECTOR_TIER": "1"])
        XCTAssertTrue(testing.isUITesting)
        XCTAssertEqual(testing.seedFile, "/x.docx")
        XCTAssertTrue(testing.realDownloads)
        XCTAssertTrue(testing.detectorTier)
        XCTAssertEqual(testing.physicalMemory, ProcessInfo.processInfo.physicalMemory)
        let belowFloor = LaunchConfiguration(arguments: [LaunchConfiguration.uiTestingArgument], environment: [:])
        XCTAssertEqual(belowFloor.physicalMemory, LaunchConfiguration.belowFloorMemory)
    }

    /// One store for every preference; UI tests keep theirs apart and clear
    /// them; what the app kept in its own defaults moves in once.
    func testPreferencesKeepOneStore() throws {
        let suite = "mdoc-tests-\(UUID().uuidString)"
        let store = try XCTUnwrap(UserDefaults(suiteName: suite))
        let user = Preferences(store: store)
        let testing = Preferences(store: store, testing: true)
        user.theme = .dark
        user.haptics = false
        XCTAssertEqual(testing.theme, .automatic, "a UI test never sees the user's choice")
        XCTAssertTrue(testing.haptics)
        testing.serif = true
        testing.reset()
        XCTAssertFalse(testing.serif)
        XCTAssertEqual(user.theme, .dark, "a UI test's reset keeps the user's")

        let old = try XCTUnwrap(UserDefaults(suiteName: "\(suite)-old"))
        old.set(true, forKey: Preferences.Key.swipeLearned.rawValue)
        old.set(true, forKey: Preferences.Key.setupCardDismissed.rawValue)
        old.set(true, forKey: Preferences.Key.haptics.rawValue)
        user.moveIn(from: old)
        XCTAssertTrue(user.isLearned(.swipeLearned))
        XCTAssertTrue(user.setupCardDismissed)
        XCTAssertFalse(user.haptics, "a value already in the store wins")
        XCTAssertNil(old.object(forKey: Preferences.Key.swipeLearned.rawValue))
        user.resetTips()
        XCTAssertFalse(user.isLearned(.swipeLearned))
    }
}
