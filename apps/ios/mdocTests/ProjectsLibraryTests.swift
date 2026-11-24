import UIKit
import XCTest
@testable import mdoc

/// Projects, intake into Unfiled or a Project, the library's search and
/// filter, and a renamed document (#1076 C, #1091), on the real core:
/// driven through `AppModel`, `LibraryModel` and `DocumentViewModel` as
/// the screens drive them, and checked by what the lists, the Projects
/// page and the document show.
@MainActor
final class ProjectsLibraryTests: XCTestCase {
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

    private func projectName(_ id: ProjectID, in app: AppModel) -> String? {
        app.library.projects.first { $0.id == id }?.name
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

    // MARK: Rename a Project

    /// Rename… on a Project: the name is trimmed, and shows at once in the
    /// lists and on a document of the Project that is open.
    func testARenamedProjectShowsOnItsOpenDocument() throws {
        let app = try app()
        app.open(seeded.letter)
        let model = try XCTUnwrap(app.openModel)
        XCTAssertEqual(model.snapshot.projectName, Letters.ellisonProject)

        app.renameProject(seeded.ellison, to: "  Ellison v Brightwater Ltd \n")
        XCTAssertNil(error(app))
        XCTAssertEqual(projectName(seeded.ellison, in: app), "Ellison v Brightwater Ltd")
        XCTAssertEqual(model.snapshot.projectName, "Ellison v Brightwater Ltd", "the open document follows")
        let letter = try XCTUnwrap(app.library.document(seeded.letter))
        XCTAssertEqual(app.library.projectSummary(of: letter)?.name, "Ellison v Brightwater Ltd")
        XCTAssertEqual(letter.title, Letters.letterTitle, "its documents keep their titles")
    }

    /// A blank name changes nothing and asks nothing: the sheet's Rename
    /// with an empty field is no error.
    func testABlankProjectNameChangesNothing() throws {
        let app = try app()
        app.renameProject(seeded.ellison, to: "  \n ")
        XCTAssertNil(error(app))
        XCTAssertEqual(projectName(seeded.ellison, in: app), Letters.ellisonProject)
        XCTAssertNoThrow(try app.library.renameProject(seeded.harbour, to: ""))
        XCTAssertEqual(projectName(seeded.harbour, in: app), Letters.harbourProject)
    }

    /// A rename the core cannot save says why, and the Project keeps its name
    /// in the lists and on its open document.
    func testAProjectRenameThatFailsSaysWhy() throws {
        let app = try app()
        app.open(seeded.letter)
        let model = try XCTUnwrap(app.openModel)
        try Fixture.setVault(seeded.storage, writable: false)
        defer { try? Fixture.setVault(seeded.storage, writable: true) }

        app.renameProject(seeded.ellison, to: "Brightwater")
        XCTAssertNotNil(error(app))
        XCTAssertEqual(projectName(seeded.ellison, in: app), Letters.ellisonProject)
        XCTAssertEqual(model.snapshot.projectName, Letters.ellisonProject)
        XCTAssertThrowsError(try app.library.renameProject(seeded.ellison, to: "Brightwater"))
    }

    // MARK: Library search

    /// The sidebar's one search: titles and Project names, in any case and
    /// trimmed; a document matches through its Project's name. Nothing for an
    /// empty query.
    func testSearchFindsTitlesAndProjectNames() throws {
        let app = try app()
        let library = app.library
        for query in ["", "   "] {
            XCTAssertTrue(library.search(query).documents.isEmpty, "'\(query)'")
            XCTAssertTrue(library.search(query).projects.isEmpty, "'\(query)'")
        }

        let byTitle = library.search("LETTER")
        XCTAssertEqual(byTitle.documents.map(\.id), [seeded.letter])
        XCTAssertTrue(byTitle.projects.isEmpty)

        let byProject = library.search(" brightwater ")
        XCTAssertEqual(Set(byProject.documents.map(\.id)), [seeded.letter, seeded.quotation])
        XCTAssertEqual(byProject.projects.map(\.id), [seeded.ellison])

        XCTAssertEqual(library.search("harbour").documents.map(\.id), [seeded.tenancy])
        XCTAssertTrue(library.search("zzzz-no-such-name").documents.isEmpty)
        XCTAssertTrue(library.search("zzzz-no-such-name").projects.isEmpty)
    }

    /// Search follows the library: a deleted document is gone while its
    /// Undo is offered and back after it; a renamed Project is found by its
    /// new name only.
    func testSearchFollowsDeleteAndRename() throws {
        let app = try app()
        app.delete(seeded.letter)
        XCTAssertTrue(app.library.search("letter").documents.isEmpty)
        app.undoDelete()
        XCTAssertEqual(app.library.search("letter").documents.map(\.id), [seeded.letter])

        app.renameProject(seeded.harbour, to: "Flat 3 tenancy")
        XCTAssertTrue(app.library.search("Harbour").projects.isEmpty)
        let found = app.library.search("flat 3")
        XCTAssertEqual(found.projects.map(\.id), [seeded.harbour])
        XCTAssertEqual(found.documents.map(\.id), [seeded.tenancy])
    }

    /// All Documents lists what its search lets through, from every
    /// Project and Unfiled.
    func testAllDocumentsAppliesTheSearch() throws {
        let app = try app()
        app.open(.documents)
        app.library.query = "quotation"
        XCTAssertEqual(app.library.shownDocuments.map(\.id), [seeded.quotation])
        app.library.query = "tenancy"
        XCTAssertEqual(app.library.shownDocuments.map(\.id), [seeded.tenancy], "another Project's document")
    }

    // MARK: Intake

    /// A new document, a paste or an import lands in Unfiled: no import
    /// makes a Project (#1091 Decision 35).
    func testIntakeLandsInUnfiledAndMakesNoProject() throws {
        let app = try app()
        XCTAssertNil(app.intakeProject)
        let typed = try newDocument(app, typing: "Shopping list")
        app.addText("Ann Lee signed.", title: "Pasted", into: app.intakeProject)
        let pasted = try XCTUnwrap(app.openModel)
        app.showStart()
        XCTAssertEqual(app.library.projects.count, 2, "no Project of its own")
        for id in [typed.id, pasted.id] {
            let row = try XCTUnwrap(app.library.document(id))
            XCTAssertNil(row.projectID, row.title)
        }
    }

    /// Intake started from a Project's page lands in that Project; from All
    /// Documents, Unfiled or Home, in Unfiled.
    func testIntakeStartedFromAProjectsPageLandsInIt() throws {
        let app = try app()
        app.openProject(seeded.harbour)
        XCTAssertEqual(app.intakeProject, seeded.harbour)
        let typed = try newDocument(app, typing: "Inventory of the flat")
        XCTAssertEqual(typed.snapshot.projectID, seeded.harbour)
        app.openProject(seeded.harbour)
        XCTAssertTrue(app.library.documents(in: seeded.harbour).contains { $0.id == typed.id })

        app.open(.documents)
        XCTAssertNil(app.intakeProject)
        app.open(.unfiled)
        XCTAssertNil(app.intakeProject)
        app.openProject(seeded.ellison)
        app.showStart()
        XCTAssertNil(app.intakeProject, "Home is no Project's page")
    }

    // MARK: Project colours

    /// Each of the core's swatches (`Colour::hue` in `mdoc-vault`'s
    /// `records.rs`) shows as its own colour, and a hue past red wraps to it.
    func testEachCoreSwatchShowsAsItsOwnColour() {
        let core: [(UInt32, ProjectColour)] = [
            (0, .red), (30, .orange), (45, .amber), (120, .green), (175, .teal), (220, .blue), (270, .violet), (325, .pink),
        ]
        for (hue, colour) in core {
            XCTAssertEqual(ProjectColour(hue: hue), colour, "hue \(hue)")
        }
        XCTAssertEqual(Set(core.map(\.1)), Set(ProjectColour.allCases), "every swatch")
        XCTAssertEqual(ProjectColour(hue: 355), .red)
        XCTAssertEqual(ProjectColour(hue: 360), .red)
        XCTAssertEqual(ProjectColour(hue: 365), .red)
        XCTAssertEqual(ProjectColour(hue: 335), .pink)
    }

    /// A Project keeps its colour when it is renamed and after mdoc restarts:
    /// the colour marks the Project.
    func testAProjectKeepsItsColour() throws {
        let app = try app()
        let colours = Dictionary(uniqueKeysWithValues: app.library.projects.map { ($0.id, $0.colour) })
        XCTAssertEqual(colours.count, 2)
        app.renameProject(seeded.ellison, to: "Brightwater")
        app.renameProject(seeded.harbour, to: "Flat 3")
        XCTAssertEqual(Dictionary(uniqueKeysWithValues: app.library.projects.map { ($0.id, $0.colour) }), colours)

        let relaunched = LibraryModel(
            library: try RealCore.make(storage: seeded.storage, keys: FixedVaultKeyStore(), physicalMemory: Fixture.belowFloor).library)
        XCTAssertEqual(Dictionary(uniqueKeysWithValues: relaunched.projects.map { ($0.id, $0.colour) }), colours)
    }

    // MARK: The Projects page

    private func counts(_ app: AppModel) -> [String: String] {
        let page = ProjectsView(app: app)
        return Dictionary(uniqueKeysWithValues: app.library.projects.map { ($0.name, page.count($0)) })
    }

    /// Each Project's line counts its documents and those to review, and
    /// follows Delete, its Undo and Move to Project.
    func testTheProjectsPageCountsFollowTheLibrary() throws {
        let app = try app()
        XCTAssertEqual(counts(app), [
            Letters.ellisonProject: "2 documents · 1 to review",
            Letters.harbourProject: "1 document",
        ])

        app.delete(seeded.quotation)
        XCTAssertEqual(counts(app)[Letters.ellisonProject], "1 document · 1 to review", "deleted, while Undo is offered")
        app.undoDelete()
        XCTAssertEqual(counts(app)[Letters.ellisonProject], "2 documents · 1 to review")

        app.move(seeded.letter, to: .existing(seeded.harbour))
        XCTAssertEqual(counts(app), [
            Letters.ellisonProject: "1 document",
            Letters.harbourProject: "2 documents · 1 to review",
        ])
        app.move(seeded.quotation, to: .existing(seeded.harbour))
        XCTAssertEqual(counts(app)[Letters.ellisonProject], "0 documents", "a Project left empty stays")
    }

    // MARK: Renaming a document

    /// A text PDF, Unfiled, read: its received file goes with its first
    /// save, which keeps it in the Vault.
    private func importedPDF(_ core: Core, _ storage: MdocStorage) throws -> (DocumentID, URL) {
        let folder = storage.url(.intake).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let pdf = folder.appendingPathComponent("upload.pdf")
        try UIGraphicsPDFRenderer(bounds: CGRect(x: 0, y: 0, width: 300, height: 200)).writePDF(to: pdf) { context in
            context.beginPage()
            NSString(string: "Reply to maya.ellison@example.com").draw(at: CGPoint(x: 20, y: 20), withAttributes: nil)
        }
        let session = try core.library.importDocument(.file(pdf, .pdf), title: "Statement", into: nil)
        XCTAssertEqual(session.snapshot().original, .pdf(.file(folder.appendingPathComponent("Statement.pdf"))), "shown while it is read")
        _ = try session.recognize(observer: TestObserver())
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.path), "the intake files went at its first save")
        return (session.id, folder)
    }

    /// Rename keeps the document as it was: it lists as a PDF after mdoc
    /// restarts, under its new title, with its Original, and Delete removes
    /// it from the Vault.
    func testARenamedDocumentKeepsItsKind() throws {
        let (core, storage) = try Fixture.core()
        let (id, _) = try importedPDF(core, storage)
        let app = self.app(core, storage)
        app.open(id)
        let model = try XCTUnwrap(app.openModel)

        app.rename(id, to: "Bank statement")
        XCTAssertNil(error(app))
        XCTAssertEqual(model.snapshot.title, "Bank statement")
        app.showStart()

        // mdoc restarts: the library is read again from the Vault.
        let relaunched = self.app(
            try RealCore.make(storage: storage, keys: FixedVaultKeyStore(), physicalMemory: Fixture.belowFloor), storage)
        let row = try XCTUnwrap(relaunched.library.documents.first { $0.title == "Bank statement" })
        XCTAssertEqual(row.kind, .pdf, "it lists as a PDF")
        XCTAssertNil(row.projectID)
        relaunched.open(row.id)
        let reopened = try XCTUnwrap(relaunched.openModel)
        XCTAssertEqual(reopened.snapshot.title, "Bank statement")
        XCTAssertTrue(reopened.hasOriginal, "its original is viewed from the Vault")
        XCTAssertTrue(reopened.panes(in: .compact).contains(.pane(.original)), "the Original pane is offered")

        relaunched.delete(row.id)
        relaunched.commitPendingDelete()
        XCTAssertNil(error(relaunched))
        let library = try XCTUnwrap(relaunched.core.library as? RealLibrary)
        XCTAssertTrue(library.vault.documents().isEmpty, "Delete removes it from the Vault")
    }
}
