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

    /// A Project's row opens its page, never a filtered list; All Documents
    /// lists every document, and Unfiled those in no Project (#1091
    /// Decisions 33 and 34).
    func testAProjectsRowOpensItsPage() throws {
        let app = try app()
        app.navigator.showsSidebar = true
        app.openProject(seeded.harbour)
        XCTAssertEqual(app.navigator.destination, .project(seeded.harbour))
        XCTAssertNil(app.navigator.section)
        XCTAssertFalse(app.navigator.showsSidebar)
        XCTAssertEqual(app.library.documents(in: seeded.harbour).map(\.id), [seeded.tenancy])

        app.open(.documents)
        XCTAssertNil(app.navigator.project)
        XCTAssertEqual(app.library.shownDocuments.count, app.library.documents.count)
        app.open(.unfiled)
        XCTAssertTrue(app.library.unfiled.isEmpty, "every seeded document is in a Project")

        // A Project that is gone opens nothing.
        app.openProject(ProjectID(rawValue: String(repeating: "0", count: 32)))
        XCTAssertEqual(app.navigator.destination, .section(.unfiled))
    }

    /// The sidebar's Projects: the first five in the Vault's order, pinned
    /// first, then most recently updated (#1091 Decision 28); All Projects
    /// has the rest.
    func testTheSidebarShowsFiveProjects() throws {
        let app = try app()
        for name in ["One", "Two", "Three", "Four", "Five"] {
            app.newProject(named: name)
        }
        XCTAssertEqual(app.library.projects.count, 7)
        XCTAssertEqual(app.library.topProjects.map(\.id), Array(app.library.projects.prefix(5)).map(\.id))
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
