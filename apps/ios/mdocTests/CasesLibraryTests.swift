import UIKit
import XCTest
@testable import mdoc

/// Cases, the library's search and filter, and a renamed document's
/// Original (#1076 C), on the real core: driven through `AppModel`,
/// `LibraryModel` and `DocumentViewModel` as the screens drive them, and
/// checked by what the lists, the Cases page and the document show.
@MainActor
final class CasesLibraryTests: XCTestCase {
    private var seeded: Seeded!

    private func app() throws -> AppModel {
        seeded = try Fixture.seeded()
        return Fixture.app(seeded)
    }

    /// The app's model over `core`, as a launch makes it.
    private func app(_ core: Core, _ storage: MdocStorage) -> AppModel {
        AppModel(
            core: core, storage: storage,
            background: NoBackgroundContinuation(), downloader: FakeModelDownloader(seconds: 0),
            network: FakeNetwork(.wifi), preferences: Fixture.preferences())
    }

    /// The error the app presents, if it presents one.
    private func error(_ app: AppModel) -> String? {
        if case .error(let message) = app.navigator.presentation { message } else { nil }
    }

    private func caseName(_ id: CaseID, in app: AppModel) -> String? {
        app.library.cases.first { $0.id == id }?.name
    }

    /// New document from the + menu, with `text` typed in it.
    private func newDocument(_ app: AppModel, typing text: String = "") throws -> DocumentViewModel {
        app.newDocument()
        let model = try XCTUnwrap(app.openModel)
        XCTAssertTrue(model.isEditing)
        if !text.isEmpty {
            XCTAssertNotNil(model.typed(SourceRange(start: 0, end: 0), text))
        }
        return model
    }

    // MARK: Rename a case

    /// Rename… on a case: the name is trimmed, and shows at once in the
    /// lists and on a document of the case that is open.
    func testARenamedCaseShowsOnItsOpenDocument() throws {
        let app = try app()
        app.open(seeded.letter)
        let model = try XCTUnwrap(app.openModel)
        XCTAssertEqual(model.snapshot.caseName, Letters.ellisonCase)

        app.renameCase(seeded.ellison, to: "  Ellison v Brightwater Ltd \n")
        XCTAssertNil(error(app))
        XCTAssertEqual(caseName(seeded.ellison, in: app), "Ellison v Brightwater Ltd")
        XCTAssertEqual(model.snapshot.caseName, "Ellison v Brightwater Ltd", "the open document follows")
        let letter = try XCTUnwrap(app.library.document(seeded.letter))
        XCTAssertEqual(app.library.caseSummary(of: letter)?.name, "Ellison v Brightwater Ltd")
        XCTAssertEqual(letter.title, Letters.letterTitle, "its documents keep their titles")
    }

    /// A blank name changes nothing and asks nothing: the sheet's Rename
    /// with an empty field is no error.
    func testABlankCaseNameChangesNothing() throws {
        let app = try app()
        app.renameCase(seeded.ellison, to: "  \n ")
        XCTAssertNil(error(app))
        XCTAssertEqual(caseName(seeded.ellison, in: app), Letters.ellisonCase)
        XCTAssertNoThrow(try app.library.renameCase(seeded.harbour, to: ""))
        XCTAssertEqual(caseName(seeded.harbour, in: app), Letters.harbourCase)
    }

    /// A rename the core cannot save says why, and the case keeps its name
    /// in the lists and on its open document.
    func testACaseRenameThatFailsSaysWhy() throws {
        let app = try app()
        app.open(seeded.letter)
        let model = try XCTUnwrap(app.openModel)
        let cases = seeded.storage.url(.cases).path
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: cases)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: cases) }

        app.renameCase(seeded.ellison, to: "Brightwater")
        XCTAssertNotNil(error(app))
        XCTAssertEqual(caseName(seeded.ellison, in: app), Letters.ellisonCase)
        XCTAssertEqual(model.snapshot.caseName, Letters.ellisonCase)
        XCTAssertThrowsError(try app.library.renameCase(seeded.ellison, to: "Brightwater"))
    }

    // MARK: Library search

    /// The sidebar's one search: titles and case names, in any case and
    /// trimmed; a document matches through its case's name. Nothing for an
    /// empty query.
    func testSearchFindsTitlesAndCaseNames() throws {
        let app = try app()
        let library = app.library
        for query in ["", "   "] {
            XCTAssertTrue(library.search(query).documents.isEmpty, "'\(query)'")
            XCTAssertTrue(library.search(query).cases.isEmpty, "'\(query)'")
        }

        let byTitle = library.search("LETTER")
        XCTAssertEqual(byTitle.documents.map(\.id), [seeded.letter])
        XCTAssertTrue(byTitle.cases.isEmpty)

        let byCase = library.search(" brightwater ")
        XCTAssertEqual(Set(byCase.documents.map(\.id)), [seeded.letter, seeded.quotation])
        XCTAssertEqual(byCase.cases.map(\.id), [seeded.ellison])

        XCTAssertEqual(library.search("harbour").documents.map(\.id), [seeded.tenancy])
        XCTAssertTrue(library.search("zzzz-no-such-name").documents.isEmpty)
        XCTAssertTrue(library.search("zzzz-no-such-name").cases.isEmpty)
    }

    /// Search follows the library: a deleted document is gone while its
    /// Undo is offered and back after it; a renamed case is found by its
    /// new name only.
    func testSearchFollowsDeleteAndRename() throws {
        let app = try app()
        app.delete(seeded.letter)
        XCTAssertTrue(app.library.search("letter").documents.isEmpty)
        app.undoDelete()
        XCTAssertEqual(app.library.search("letter").documents.map(\.id), [seeded.letter])

        app.renameCase(seeded.harbour, to: "Flat 3 tenancy")
        XCTAssertTrue(app.library.search("Harbour").cases.isEmpty)
        let found = app.library.search("flat 3")
        XCTAssertEqual(found.cases.map(\.id), [seeded.harbour])
        XCTAssertEqual(found.documents.map(\.id), [seeded.tenancy])
    }

    /// The Documents page lists what its case filter and its search both
    /// let through; the Documents row lifts the filter, not the search.
    func testTheDocumentsPageAppliesTheFilterAndTheSearch() throws {
        let app = try app()
        app.openCase(seeded.ellison)
        app.library.query = "quotation"
        XCTAssertEqual(app.library.shownDocuments.map(\.id), [seeded.quotation])
        app.library.query = "tenancy"
        XCTAssertTrue(app.library.shownDocuments.isEmpty, "another case's document")
        app.open(.documents)
        XCTAssertEqual(app.library.shownDocuments.map(\.id), [seeded.tenancy])
    }

    // MARK: The case filter

    /// A case's page whose case goes (its one new document emptied and
    /// left) filters nothing: the Documents page lists every document.
    func testACaseFilterGoesWithItsCase() throws {
        let app = try app()
        let typed = try newDocument(app, typing: "Shopping list")
        let caseID = try XCTUnwrap(app.library.document(typed.id)?.caseID)
        app.openCase(caseID)
        XCTAssertEqual(app.library.caseFilter, caseID)
        XCTAssertEqual(app.library.shownDocuments.map(\.id), [typed.id])

        app.open(typed.id)
        let model = try XCTUnwrap(app.openModel)
        model.enterEditMode()
        XCTAssertNotNil(model.typed(SourceRange(start: 0, end: model.coreText().utf8.count), ""))
        app.showStart()
        XCTAssertNil(caseName(caseID, in: app), "the case went with its empty document")
        XCTAssertNil(app.library.caseFilter)
        XCTAssertEqual(app.library.shownDocuments.map(\.id), app.library.documents.map(\.id))
    }

    /// A case found in the sidebar's search while its empty new document is
    /// open: leaving the document takes the case with it, so its page
    /// filters nothing rather than showing a case that is gone.
    func testOpeningACaseThatGoesAsItOpensFiltersNothing() throws {
        let app = try app()
        let empty = try newDocument(app)
        let caseID = try XCTUnwrap(app.library.document(empty.id)?.caseID)
        XCTAssertEqual(app.library.search("New document").cases.map(\.id), [caseID])
        app.openCase(caseID)
        XCTAssertNil(caseName(caseID, in: app), "the case went with its empty document")
        XCTAssertEqual(app.navigator.section, .documents)
        XCTAssertNil(app.library.caseFilter)
        XCTAssertEqual(app.library.shownDocuments.map(\.id), app.library.documents.map(\.id))
    }

    // MARK: Case colours

    /// Each of the core's swatches (`CaseColor::hue` in `mdoc-store`'s
    /// `cases.rs`) shows as its own colour, and a hue past red wraps to it.
    func testEachCoreSwatchShowsAsItsOwnColour() {
        let core: [(UInt32, CaseColour)] = [
            (0, .red), (30, .orange), (45, .amber), (120, .green), (175, .teal), (220, .blue), (270, .violet), (325, .pink),
        ]
        for (hue, colour) in core {
            XCTAssertEqual(CaseColour(hue: hue), colour, "hue \(hue)")
        }
        XCTAssertEqual(Set(core.map(\.1)), Set(CaseColour.allCases), "every swatch")
        XCTAssertEqual(CaseColour(hue: 355), .red)
        XCTAssertEqual(CaseColour(hue: 360), .red)
        XCTAssertEqual(CaseColour(hue: 365), .red)
        XCTAssertEqual(CaseColour(hue: 335), .pink)
    }

    /// A case keeps its colour when it is renamed and after mdoc restarts:
    /// the colour marks the case.
    func testACaseKeepsItsColour() throws {
        let app = try app()
        let colours = Dictionary(uniqueKeysWithValues: app.library.cases.map { ($0.id, $0.colour) })
        XCTAssertEqual(colours.count, 2)
        app.renameCase(seeded.ellison, to: "Brightwater")
        app.renameCase(seeded.harbour, to: "Flat 3")
        XCTAssertEqual(Dictionary(uniqueKeysWithValues: app.library.cases.map { ($0.id, $0.colour) }), colours)

        let relaunched = LibraryModel(
            library: try RealCore.make(storage: seeded.storage, keys: FixedCaseKeyStore(), physicalMemory: Fixture.belowFloor).library)
        XCTAssertEqual(Dictionary(uniqueKeysWithValues: relaunched.cases.map { ($0.id, $0.colour) }), colours)
    }

    // MARK: The Cases page

    private func counts(_ app: AppModel) -> [String: String] {
        let page = CasesView(app: app)
        return Dictionary(uniqueKeysWithValues: app.library.cases.map { ($0.name, page.count($0)) })
    }

    /// Each case's line counts its documents and those to review, and
    /// follows Delete, its Undo and Move to case.
    func testTheCasesPageCountsFollowTheLibrary() throws {
        let app = try app()
        XCTAssertEqual(counts(app), [
            Letters.ellisonCase: "2 documents · 1 to review",
            Letters.harbourCase: "1 document",
        ])

        app.delete(seeded.quotation)
        XCTAssertEqual(counts(app)[Letters.ellisonCase], "1 document · 1 to review", "deleted, while Undo is offered")
        app.undoDelete()
        XCTAssertEqual(counts(app)[Letters.ellisonCase], "2 documents · 1 to review")

        app.move(seeded.letter, to: .existing(seeded.harbour))
        XCTAssertEqual(counts(app), [
            Letters.ellisonCase: "1 document",
            Letters.harbourCase: "2 documents · 1 to review",
        ])
        app.move(seeded.quotation, to: .existing(seeded.harbour))
        XCTAssertEqual(counts(app)[Letters.ellisonCase], "0 documents", "a case left empty stays")
    }

    // MARK: Renaming a document that has an Original

    /// A text PDF in a case of its own, read.
    private func importedPDF(_ core: Core, _ storage: MdocStorage) throws -> (DocumentID, URL) {
        let folder = storage.url(.originals).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let pdf = folder.appendingPathComponent("upload.pdf")
        try UIGraphicsPDFRenderer(bounds: CGRect(x: 0, y: 0, width: 300, height: 200)).writePDF(to: pdf) { context in
            context.beginPage()
            NSString(string: "Reply to maya.ellison@example.com").draw(at: CGPoint(x: 20, y: 20), withAttributes: nil)
        }
        let session = try core.library.importDocument(.file(pdf, .pdf), title: "Statement", into: .newCase(name: "Statement"))
        let original = folder.appendingPathComponent("Statement.pdf")
        XCTAssertEqual(try session.recognize(observer: TestObserver()).original, .pdf(original))
        return (session.id, original)
    }

    /// Rename keeps the document's Original: it still shows, after mdoc
    /// restarts too, and Delete still removes its file.
    func testARenamedDocumentKeepsItsOriginal() throws {
        let (core, storage) = try Fixture.core()
        let (id, original) = try importedPDF(core, storage)
        let app = self.app(core, storage)
        app.open(id)
        let model = try XCTUnwrap(app.openModel)
        XCTAssertTrue(model.hasOriginal)

        app.rename(id, to: "Bank statement")
        XCTAssertNil(error(app))
        XCTAssertEqual(model.snapshot.title, "Bank statement")
        XCTAssertTrue(model.hasOriginal)
        app.showStart()

        // mdoc restarts: the library is read again from its files.
        let relaunched = self.app(
            try RealCore.make(storage: storage, keys: FixedCaseKeyStore(), physicalMemory: Fixture.belowFloor), storage)
        let row = try XCTUnwrap(relaunched.library.documents.first { $0.title == "Bank statement" })
        XCTAssertEqual(row.kind, .pdf, "it lists as a PDF")
        relaunched.open(row.id)
        let reopened = try XCTUnwrap(relaunched.openModel)
        XCTAssertEqual(reopened.snapshot.original, .pdf(original))
        XCTAssertTrue(reopened.hasOriginal)
        XCTAssertTrue(reopened.panes(in: .compact).contains(.pane(.original)), "the Original pane is offered")

        relaunched.delete(row.id)
        relaunched.commitPendingDelete()
        XCTAssertNil(error(relaunched))
        XCTAssertFalse(FileManager.default.fileExists(atPath: original.path), "Delete removes the Original")
        XCTAssertFalse(FileManager.default.fileExists(atPath: original.deletingLastPathComponent().path))
    }
}
