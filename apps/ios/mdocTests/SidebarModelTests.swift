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
        app.requestSearch(inDrawer: true)
        XCTAssertTrue(app.showsSidebar)
        XCTAssertEqual(app.searchRequest, before + 1)
    }

    /// ⌘K (#675): a sidebar built after the request still focuses its
    /// search, once; on iPad the split view shows the column, and the
    /// drawer's flag stays down, so a narrower window opens nothing.
    func testSearchWaitsForTheSidebarAndLeavesTheDrawerOnIPad() throws {
        let app = try app()
        app.requestSearch(inDrawer: false)
        XCTAssertFalse(app.showsSidebar, "the split view ignores the drawer's flag")
        XCTAssertTrue(app.takeSearchRequest(), "a sidebar built now focuses its search")
        XCTAssertFalse(app.takeSearchRequest(), "only once")
    }

    /// The Documents row lists every document, whatever case was tapped
    /// before (#676).
    func testTheDocumentsRowClearsACasesFilter() throws {
        let app = try app()
        app.openCase(Fixture.harbour)
        XCTAssertEqual(app.section, .documents)
        XCTAssertEqual(app.home.caseFilter, Fixture.harbour)
        app.open(.documents)
        XCTAssertNil(app.home.caseFilter)
        XCTAssertEqual(app.home.shownDocuments.count, app.home.documents.count)
    }

    /// Move to case › New case… names the case as every new case is named
    /// (#677): trimmed, a blank name refused, a taken one numbered.
    func testMoveToANewCaseTrimsAndNumbersTheName() throws {
        let app = try app()
        let before = app.home.cases.count
        app.move(Fixture.letter, to: .newCase(name: "   "))
        XCTAssertEqual(app.home.cases.count, before, "a blank name makes no case")
        XCTAssertNil(app.toast)
        app.move(Fixture.letter, to: .newCase(name: "  Harbour Street lease "))
        let moved = try XCTUnwrap(app.home.documents.first { $0.id == Fixture.letter }?.caseID)
        XCTAssertEqual(app.home.cases.first { $0.id == moved }?.name, "Harbour Street lease 2")
        XCTAssertEqual(app.home.cases.map(\.name).filter { $0 == "Harbour Street lease" }.count, 1)
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
