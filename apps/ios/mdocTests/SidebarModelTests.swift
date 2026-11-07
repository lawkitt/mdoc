import XCTest
@testable import mdoc

/// The sidebar's navigation and its document commands (ADR 0102), over the
/// fake core: a document, a section's page or Start in the main area;
/// Rename and Move to case with Undo.
@MainActor
final class SidebarModelTests: XCTestCase {
    private func app() throws -> AppModel {
        AppModel(
            core: Fixture.core(), storage: try Fixture.temporaryStorage(),
            background: NoBackgroundContinuation(), downloader: FakeModelDownloader(seconds: 0),
            network: FakeNetwork(.wifi))
    }

    func testOpeningADocumentClosesTheSidebarAndLeavesTheSection() throws {
        let app = try app()
        app.open(.cases)
        XCTAssertEqual(app.section, .cases)
        app.showsSidebar = true
        app.selection = Fixture.letter
        XCTAssertNil(app.section, "a document replaces the section's page")
        XCTAssertFalse(app.showsSidebar)
        app.showStart()
        XCTAssertNil(app.selection)
        XCTAssertNil(app.section)
    }

    func testSearchOpensTheSidebar() throws {
        let app = try app()
        let before = app.searchRequest
        app.requestSearch()
        XCTAssertTrue(app.showsSidebar)
        XCTAssertEqual(app.searchRequest, before + 1)
    }

    func testContinueIsTheNewestDocumentWithNamesLeft() throws {
        let app = try app()
        let reviewing = app.home.documents.filter { $0.phase == .reviewing }
        XCTAssertEqual(app.continueDocument?.id, reviewing.max { $0.updated < $1.updated }?.id)
    }

    func testRenameShowsInTheList() throws {
        let app = try app()
        app.rename(Fixture.letter, to: "Letter: before claim")
        XCTAssertEqual(app.home.documents.first { $0.id == Fixture.letter }?.title, "Letter: before claim")
        XCTAssertNil(app.home.errorMessage)
    }

    func testMoveOffersUndoThatMovesBack() throws {
        let app = try app()
        let before = app.home.documents.first { $0.id == Fixture.letter }?.caseID
        app.move(Fixture.letter, to: .existing(Fixture.harbour))
        XCTAssertEqual(app.home.documents.first { $0.id == Fixture.letter }?.caseID, Fixture.harbour)
        let toast = try XCTUnwrap(app.toast)
        XCTAssertTrue(toast.text.contains("Moved to"))
        toast.action?()
        XCTAssertEqual(app.home.documents.first { $0.id == Fixture.letter }?.caseID, before)
    }

    func testRecentCasesLeaveOutTheDocumentsOwn() throws {
        let app = try app()
        let own = app.home.documents.first { $0.id == Fixture.letter }?.caseID
        XCTAssertFalse(app.recentCases(excluding: own).contains { $0.id == own })
        XCTAssertLessThanOrEqual(app.recentCases(excluding: own).count, 5)
    }
}
