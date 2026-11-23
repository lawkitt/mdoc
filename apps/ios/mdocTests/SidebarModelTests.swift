import XCTest
@testable import mdoc

/// The sidebar's navigation and its document commands (ADR 0102), over the
/// real core and the seeded library: a document, a section's page or Start
/// in the main area; Rename and Move to case with Undo.
@MainActor
final class SidebarModelTests: XCTestCase {
    private var seeded: Seeded!

    private func app() throws -> AppModel {
        let seeded = try Fixture.seeded()
        self.seeded = seeded
        return AppModel(
            core: seeded.core, storage: seeded.storage,
            background: NoBackgroundContinuation(), downloader: FakeModelDownloader(seconds: 0),
            network: FakeNetwork(.wifi), preferences: Fixture.preferences())
    }

    /// The error the app presents, if it presents one.
    private func error(_ app: AppModel) -> String? {
        if case .error(let message) = app.navigator.presentation { message } else { nil }
    }

    func testOpeningADocumentClosesTheSidebarAndLeavesTheSection() throws {
        let app = try app()
        app.open(.projects)
        XCTAssertEqual(app.navigator.section, .projects)
        app.navigator.showsSidebar = true
        app.open(seeded.letter)
        XCTAssertNil(app.navigator.section, "a document replaces the section's page")
        XCTAssertFalse(app.navigator.showsSidebar)
        app.showStart()
        XCTAssertNil(app.navigator.openDocument)
        XCTAssertNil(app.navigator.section)
        XCTAssertEqual(app.navigator.destination, .start)
    }

    func testOpeningAnotherDocumentReleasesTheDocxUnlock() async throws {
        let app = try app()
        let source = try Fixture.copy("tests/fixtures/locked-office/word-locked.docx", into: seeded.storage)
        let locked = try seeded.core.library.importDocument(
            .file(source, .docx), title: "Locked", into: seeded.ellison)
        app.open(locked.id)
        let model = try XCTUnwrap(app.openModel)
        await model.recognize()
        XCTAssertEqual(model.passwordPrompt, .needed)
        await model.unlock(password: "Petrov-2026")
        XCTAssertNotNil(model.unlockedDocx)

        app.open(seeded.letter)
        XCTAssertEqual(app.navigator.openDocument, seeded.letter)
        XCTAssertNil(model.unlockedDocx)
    }

    func testSearchOpensTheSidebar() throws {
        let app = try app()
        app.requestSearch()
        XCTAssertTrue(app.navigator.showsSidebar)
        XCTAssertTrue(app.navigator.searchPending)
    }

    /// ⌘K (#675): a sidebar that takes the request later still focuses its
    /// search, once.
    func testSearchWaitsForTheSidebarOnce() throws {
        let app = try app()
        app.requestSearch()
        XCTAssertTrue(app.navigator.takeSearchRequest(), "the sidebar focuses its search")
        XCTAssertFalse(app.navigator.takeSearchRequest(), "only once")
        XCTAssertFalse(app.navigator.searchPending)
    }

    /// The Documents row lists every document, whatever case was tapped
    /// before (#676).
    func testTheDocumentsRowClearsAProjectsFilter() throws {
        let app = try app()
        app.openProject(seeded.harbour)
        XCTAssertEqual(app.navigator.section, .documents)
        XCTAssertEqual(app.library.projectFilter, seeded.harbour)
        app.open(.documents)
        XCTAssertNil(app.library.projectFilter)
        XCTAssertEqual(app.library.shownDocuments.count, app.library.documents.count)
    }

    /// Move to case › New case… names the case as every new case is named
    /// (#677): trimmed, a blank name refused, a taken one numbered.
    func testMoveToANewProjectTrimsAndNumbersTheName() throws {
        let app = try app()
        let before = app.library.projects.count
        app.move(seeded.letter, to: .newProject(name: "   "))
        XCTAssertEqual(app.library.projects.count, before, "a blank name makes no case")
        XCTAssertNil(app.toast)
        app.move(seeded.letter, to: .newProject(name: "  Harbour Street lease "))
        let moved = try XCTUnwrap(app.library.document(seeded.letter)?.projectID)
        XCTAssertEqual(app.library.projects.first { $0.id == moved }?.name, "Harbour Street lease 2")
        XCTAssertEqual(app.library.projects.map(\.name).filter { $0 == "Harbour Street lease" }.count, 1)
    }

    func testContinueIsTheNewestDocumentWithNamesLeft() throws {
        let app = try app()
        let reviewing = app.library.documents.filter { $0.phase == .reviewing }
        XCTAssertEqual(app.library.continueDocument?.id, reviewing.max { $0.updated < $1.updated }?.id)
    }

    func testRenameShowsInTheList() throws {
        let app = try app()
        app.rename(seeded.letter, to: "Letter: before claim")
        XCTAssertEqual(app.library.document(seeded.letter)?.title, "Letter: before claim")
        XCTAssertNil(error(app))
    }

    /// An import not yet read (the tenancy scan) can be neither renamed nor
    /// moved: each says why, and the document stays as it was.
    func testAnImportNotYetReadRefusesRenameAndMove() throws {
        let app = try app()
        let before = try XCTUnwrap(app.library.document(seeded.tenancy))
        app.rename(seeded.tenancy, to: "Lease")
        XCTAssertEqual(error(app), "Rename the document once it is read.")
        app.navigator.dismiss()
        app.move(seeded.tenancy, to: .existing(seeded.ellison))
        XCTAssertNotNil(error(app))
        XCTAssertNil(app.toast)
        let after = try XCTUnwrap(app.library.document(seeded.tenancy))
        XCTAssertEqual(after.title, before.title)
        XCTAssertEqual(after.projectID, seeded.harbour)
    }

    func testMoveOffersUndoThatMovesBack() throws {
        let app = try app()
        let before = app.library.document(seeded.letter)?.projectID
        app.move(seeded.letter, to: .existing(seeded.harbour))
        XCTAssertEqual(app.library.document(seeded.letter)?.projectID, seeded.harbour)
        let toast = try XCTUnwrap(app.toast)
        XCTAssertTrue(toast.text.contains("Moved to"))
        toast.action?()
        XCTAssertEqual(app.library.document(seeded.letter)?.projectID, before)
    }

    func testRecentProjectsLeaveOutTheDocumentsOwn() throws {
        let app = try app()
        let own = app.library.document(seeded.letter)?.projectID
        XCTAssertFalse(app.library.recentProjects(excluding: own).contains { $0.id == own })
        XCTAssertLessThanOrEqual(app.library.recentProjects(excluding: own).count, 5)
    }
}
