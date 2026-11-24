import MdocFFI
import XCTest
@testable import mdoc

/// A Project's page, Delete Project's two answers, Unfiled's multi-select
/// delete and a repeated import (#1091 Decisions 5, 7, 9, 29–34), on the
/// real core: driven through `AppModel` and `LibraryModel` as the screens
/// drive them, and checked by what the lists, the page and the Vault show.
@MainActor
final class ProjectPageTests: XCTestCase {
    private var seeded: Seeded!

    private func app() throws -> AppModel {
        seeded = try Fixture.seeded()
        return Fixture.app(seeded)
    }

    private var vault: MdocFFI.Vault {
        get throws { try XCTUnwrap(seeded.core.library as? RealLibrary).vault }
    }

    /// The error the app presents, if it presents one.
    private func error(_ app: AppModel) -> String? {
        if case .error(let message) = app.navigator.presentation { message } else { nil }
    }

    /// An AI reply saved by the Project itself, as Restore with mdoc saves
    /// one.
    private func projectReply(_ project: ProjectID) throws -> String {
        try vault.saveReply(
            sharedFrom: .project(id: project.rawValue), title: "Reply to the Project", sourceApp: nil,
            markdown: "Noted.").id
    }

    // MARK: The page

    /// Documents, Replies and Names: the Project's own documents, its
    /// saved replies, and its names grouped by category, searchable by
    /// alias or wording. A new Project's lists are empty.
    func testAProjectsPageShowsItsDocumentsRepliesAndNames() throws {
        let app = try app()
        app.openProject(seeded.ellison)
        let library = app.library
        XCTAssertEqual(Set(library.documents(in: seeded.ellison).map(\.id)), [seeded.letter, seeded.quotation])
        let reply = try projectReply(seeded.ellison)
        library.refresh()
        XCTAssertEqual(library.replies(in: seeded.ellison).map(\.id.rawValue), [reply])
        XCTAssertTrue(library.replies(in: seeded.harbour).isEmpty)

        let details = library.details(of: seeded.ellison)
        XCTAssertFalse(details.names.isEmpty, "the quotation's replaced names")
        let groups = LibraryModel.names(details.names, matching: "")
        XCTAssertEqual(groups.map(\.0), Category.allCases.filter { category in details.names.contains { $0.category == category } })
        XCTAssertEqual(groups.flatMap(\.1).count, details.names.count)
        let name = try XCTUnwrap(details.names.first { !$0.originals.isEmpty })
        let byWording = LibraryModel.names(details.names, matching: String(name.originals[0].prefix(5)).uppercased())
        XCTAssertTrue(byWording.flatMap(\.1).contains(name), "found by a wording, in any case")
        XCTAssertTrue(LibraryModel.names(details.names, matching: name.alias).flatMap(\.1).contains(name))
        XCTAssertTrue(LibraryModel.names(details.names, matching: "zzzz-no-such-name").isEmpty)
        XCTAssertTrue(details.always.isEmpty && details.never.isEmpty, "the phones add nothing to them")

        XCTAssertEqual(library.details(of: seeded.harbour).names, [], "nothing replaced there yet")
    }

    /// New Project: its page opens, and what + adds there lands in it. A
    /// blank name makes nothing; a taken one is numbered.
    func testANewProjectOpensItsPage() throws {
        let app = try app()
        app.newProject(named: "   ")
        XCTAssertEqual(app.library.projects.count, 2)
        app.newProject(named: " Riverside ")
        let riverside = try XCTUnwrap(app.navigator.project)
        XCTAssertEqual(app.library.project(riverside)?.name, "Riverside")
        XCTAssertEqual(app.intakeProject, riverside)
        app.addText("Ann Lee signed the lease.", title: "Lease", into: app.intakeProject)
        let lease = try XCTUnwrap(app.openModel)
        XCTAssertEqual(lease.snapshot.projectID, riverside)
        app.openProject(riverside)
        XCTAssertEqual(app.library.documents(in: riverside).map(\.id), [lease.id])

        app.newProject(named: "Riverside")
        XCTAssertEqual(app.navigator.project.flatMap { app.library.project($0)?.name }, "Riverside 2")
    }

    // MARK: Delete Project (#1091 Decision 9)

    /// "Move its documents to Unfiled": they land there with their text and
    /// names, the open one with them (an iPad's sidebar stands beside it);
    /// the Project and its own AI replies go.
    func testDeletingAProjectCanMoveItsDocumentsToUnfiled() throws {
        let app = try app()
        let reply = try projectReply(seeded.ellison)
        app.open(seeded.letter)
        let letter = try XCTUnwrap(app.openModel)
        let text = letter.snapshot.markdown
        app.askToDeleteProject(seeded.ellison)
        guard case .deleteProject(seeded.ellison) = app.navigator.presentation else {
            return XCTFail("no question: \(String(describing: app.navigator.presentation))")
        }

        app.deleteProject(seeded.ellison, documents: .moveToUnfiled)
        XCTAssertNil(error(app))
        XCTAssertNil(app.library.project(seeded.ellison))
        XCTAssertEqual(Set(app.library.unfiled.map(\.id)), [seeded.letter, seeded.quotation])
        XCTAssertEqual(app.navigator.openDocument, seeded.letter, "it stays open")
        XCTAssertEqual(app.toast?.text, "Deleted \(Letters.ellisonProject)")
        XCTAssertThrowsError(try vault.reply(id: reply), "the Project's own reply went with it")
        XCTAssertNil(letter.snapshot.projectID, "the open letter shows it in Unfiled")
        XCTAssertEqual(letter.snapshot.markdown, text)
        XCTAssertNotNil(app.library.project(seeded.harbour), "other Projects stay")
    }

    /// "Delete them": the documents go with the Project, the open one
    /// closing first; other Projects keep theirs.
    func testDeletingAProjectCanDeleteItsDocuments() throws {
        let app = try app()
        app.open(seeded.letter)
        app.deleteProject(seeded.ellison, documents: .deleteDocuments)
        XCTAssertNil(error(app))
        XCTAssertNil(app.library.project(seeded.ellison))
        XCTAssertNil(app.library.document(seeded.letter))
        XCTAssertNil(app.library.document(seeded.quotation))
        XCTAssertEqual(app.navigator.destination, .start, "the open document closed")
        XCTAssertNil(app.documents.model(for: seeded.letter))
        XCTAssertEqual(try vault.documents().count, 0, "the tenancy is not saved yet; nothing else is left")
        XCTAssertEqual(app.library.documents(in: seeded.harbour).map(\.id), [seeded.tenancy])
    }

    /// An empty Project asks plainly and goes; its page gives way to All
    /// Projects.
    func testAnEmptyProjectIsDeleted() throws {
        let app = try app()
        app.newProject(named: "Empty")
        let empty = try XCTUnwrap(app.navigator.project)
        app.askToDeleteProject(empty)
        XCTAssertEqual(app.navigator.presentation?.id, Presentation.deleteProject(empty).id)
        app.deleteProject(empty, documents: .deleteDocuments)
        XCTAssertNil(app.library.project(empty))
        XCTAssertEqual(app.navigator.destination, .section(.projects))
        XCTAssertEqual(app.library.documents.count, 3, "no document went")
    }

    /// An import not read yet that was to join the deleted Project lands in
    /// Unfiled once it is read; with "Delete them" it goes too.
    func testAnImportNotReadYetFollowsItsProject() throws {
        let app = try app()
        let moved = try seeded.library.importDocument(.text(Letters.quotation), title: "Second quotation", into: seeded.ellison)
        app.deleteProject(seeded.ellison, documents: .moveToUnfiled)
        _ = try moved.recognize(observer: TestObserver())
        app.library.refresh()
        let row = try XCTUnwrap(app.library.document(moved.id))
        XCTAssertNil(row.projectID)
        XCTAssertNil(error(app))

        let deleted = try seeded.library.importDocument(.text("Ann Lee"), title: "Note", into: seeded.harbour)
        app.library.refresh()
        app.deleteProject(seeded.harbour, documents: .deleteDocuments)
        XCTAssertNil(app.library.document(deleted.id))
        XCTAssertNil(app.library.document(seeded.tenancy))
    }

    // MARK: Unfiled's multi-select delete (#1091 Decision 5)

    /// Documents selected together leave the lists under one toast whose
    /// Undo brings them all back; once it ends, they are deleted, and the
    /// others stay.
    func testUnfiledDeletesSeveralUnderOneUndo() throws {
        let app = try app()
        let ids = try ["One", "Two", "Three"].map { title in
            let session = try seeded.library.importDocument(.text("\(title) Ann Lee"), title: title, into: nil)
            _ = try session.recognize(observer: TestObserver())
            return session.id
        }
        app.library.refresh()
        app.open(.unfiled)
        XCTAssertEqual(Set(app.library.unfiled.map(\.id)), Set(ids))

        app.delete([ids[0], ids[2]])
        XCTAssertEqual(app.library.unfiled.map(\.id), [ids[1]])
        let toast = try XCTUnwrap(app.toast)
        XCTAssertEqual(toast.text, "Deleted 2 documents")
        toast.action?()
        XCTAssertEqual(Set(app.library.unfiled.map(\.id)), Set(ids), "Undo brings both back")

        app.delete([ids[0], ids[2]])
        app.toastEnded(try XCTUnwrap(app.toast).id)
        XCTAssertNil(error(app))
        XCTAssertEqual(app.library.unfiled.map(\.id), [ids[1]])
        XCTAssertEqual(try vault.unfiled().count, 1, "deleted in the Vault")
        XCTAssertEqual(app.library.documents.count, 4, "the Projects' documents stay")
    }

    // MARK: Repeated imports (#1091 Decision 7)

    /// A file whose bytes are already in mdoc asks first: Open Existing
    /// opens that document and keeps no copy; Import Again goes on to the
    /// import sheet; Cancel imports nothing.
    func testAFileAlreadyInMdocAsksFirst() async throws {
        let app = try app()
        let source = Fixture.repository.appendingPathComponent("tests/fixtures/import/text.pdf")
        let first = try seeded.library.importDocument(
            .file(try Fixture.copy("tests/fixtures/import/text.pdf", into: seeded.storage), .pdf), title: "Lease", into: nil)
        _ = try first.recognize(observer: TestObserver())
        app.library.refresh()

        func receive() async throws -> RepeatedImport {
            let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString)-lease.pdf")
            try FileManager.default.copyItem(at: source, to: file)
            await app.receiveFile(file).value
            guard case .repeatedImport(let repeated) = app.navigator.presentation else {
                XCTFail("no question: \(String(describing: app.navigator.presentation))")
                throw CoreError.notFound
            }
            XCTAssertEqual(repeated.existing, first.id)
            XCTAssertEqual(repeated.name, file.lastPathComponent)
            return repeated
        }

        let opened = try await receive()
        let copy = try XCTUnwrap(opened.pending.sourceFile)
        app.openExisting(opened)
        XCTAssertEqual(app.navigator.openDocument, first.id)
        XCTAssertNil(app.navigator.presentation)
        XCTAssertFalse(FileManager.default.fileExists(atPath: copy.path), "the copy went")
        XCTAssertEqual(app.library.documents.count, 4)

        let again = try await receive()
        app.importAgain(again)
        guard case .importing(let pending) = app.navigator.presentation else {
            return XCTFail("no import sheet")
        }
        XCTAssertEqual(pending, again.pending)
        app.navigator.dismiss()

        let cancelled = try await receive()
        app.cancelImport(cancelled)
        XCTAssertNil(app.navigator.presentation)
        XCTAssertFalse(FileManager.default.fileExists(atPath: try XCTUnwrap(cancelled.pending.sourceFile).path))
    }

    /// Open in mdoc comes from another app, not from a Project's page: its
    /// import sheet starts in Unfiled even while a Project's page shows. A
    /// file picked from that page's + menu starts in its Project (#1091
    /// Decision 35).
    func testOpenInMdocStartsInUnfiled() async throws {
        let app = try app()
        app.openProject(seeded.ellison)
        func received(_ receive: (URL) -> Task<Void, Never>) async throws -> PendingImport? {
            let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).pdf")
            try FileManager.default.copyItem(at: Fixture.repository.appendingPathComponent("tests/fixtures/import/text.pdf"), to: file)
            await receive(file).value
            guard case .importing(let pending) = app.navigator.presentation else {
                XCTFail("no import sheet: \(String(describing: app.navigator.presentation))")
                return nil
            }
            app.navigator.dismiss()
            return pending
        }
        let opened = try await received(app.openInMdoc)
        XCTAssertNotNil(opened)
        XCTAssertNil(opened?.project, "Open in mdoc starts in Unfiled")
        let picked = try await received(app.receiveFile)
        XCTAssertEqual(picked?.project, seeded.ellison, "a file picked on the page starts in its Project")
    }

    /// A new file asks nothing; neither do a paste or camera pages, which
    /// keep no source of their own to compare.
    func testANewFileAsksNothing() async throws {
        let app = try app()
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID().uuidString).pdf")
        try FileManager.default.copyItem(at: Fixture.repository.appendingPathComponent("tests/fixtures/import/text.pdf"), to: file)
        await app.receiveFile(file).value
        guard case .importing(let pending) = app.navigator.presentation else {
            return XCTFail("no import sheet: \(String(describing: app.navigator.presentation))")
        }
        XCTAssertEqual(pending.fileName, file.lastPathComponent)
        XCTAssertNil(PendingImport(source: .text("Ann"), suggestedTitle: "Note").sourceFile)
        XCTAssertNil(PendingImport(source: .scan([file]), suggestedTitle: "Scan").sourceFile)
        XCTAssertNil(PendingImport(source: .photos([file, file]), suggestedTitle: "Photos").sourceFile)
        XCTAssertEqual(PendingImport(source: .photos([file]), suggestedTitle: "Photo").sourceFile, file)
    }
}
