import PDFKit
import UIKit
import XCTest
@testable import mdoc

@MainActor
final class DocumentViewModelTests: XCTestCase {
    /// The seeded letter, or another of `Fixture.seeded()`'s documents.
    private func model(
        _ pick: (Seeded) -> DocumentID = \.letter, memory: UInt64 = Fixture.belowFloor,
        findsNames: Bool = true, slow seconds: Double? = nil
    ) throws -> (DocumentViewModel, Seeded) {
        let seeded = try Fixture.seeded(memory: memory)
        return (try seeded.model(pick(seeded), findsNames: findsNames, slow: seconds), seeded)
    }

    /// A new document of its own case on a fresh core, not yet read.
    private func imported(
        _ source: (MdocStorage) throws -> ImportSource, title: String = "Note", memory: UInt64 = Fixture.belowFloor,
        findsNames: Bool = true
    ) throws -> DocumentViewModel {
        let (core, storage) = try Fixture.core(memory: memory)
        let session = try core.library.importDocument(try source(storage), title: title, into: nil)
        return DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage,
            findsNames: findsNames)
    }

    /// The system shows a continued task's title outside the app, so a
    /// document job never puts the document's title there.
    func testBackgroundTaskTitlesNeverNameTheDocument() async throws {
        let title = "Halvorsen v. Okonkwo"
        let (core, storage) = try Fixture.core(memory: Fixture.belowFloor)
        let session = try core.library.importDocument(
            .text(Letters.quotation), title: title, into: nil)
        let background = RecordingContinuation()
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: background), storage: storage)
        await model.recognize()
        await model.findNames()
        XCTAssertEqual(background.titles, ["Recognizing a document", "Finding names"])
        XCTAssertFalse(background.titles.contains { $0.contains(title) })
    }

    /// Below the floor the document offers no Find names: it says why, and
    /// the hand-off is there for what the user marks (ADR 0088).
    func testBelowTheFloorTheNoteStandsWhereFindNamesWouldBe() async throws {
        let below = try imported({ _ in .text(Letters.quotation) }, findsNames: false)
        XCTAssertFalse(below.showsBelowFloorNote)
        await below.recognize()
        XCTAssertEqual(below.snapshot.phase, .unscanned)
        XCTAssertEqual(below.primaryAction, .handOff)
        XCTAssertTrue(below.showsBelowFloorNote)

        let detector = try imported({ _ in .text(Letters.quotation) }, memory: Fixture.detectorTier)
        await detector.recognize()
        XCTAssertEqual(detector.primaryAction, .findNames)
        XCTAssertFalse(detector.showsBelowFloorNote)
    }

    /// An interrupted import of a file that needs no recognition reads as
    /// an import, not as missing text recognition (ADR 0088).
    func testAStoppedDocxImportOffersFinishImport() async throws {
        let model = try imported(
            { try .file(Fixture.copy("tests/fixtures/import/text.docx", into: $0), .docx) }, title: "Contract")
        XCTAssertTrue(model.importNotFinished)
        XCTAssertEqual(model.primaryAction, .finishImport)
        await model.recognize()
        XCTAssertFalse(model.importNotFinished)
        XCTAssertEqual(model.primaryAction, .findNames)

        let seeded = try Fixture.seeded()
        let scanModel = try seeded.model(seeded.tenancy)
        XCTAssertFalse(scanModel.importNotFinished)
        XCTAssertEqual(scanModel.primaryAction, .recognize(resume: false))
    }

    func testPrimaryActionFollowsTheDocument() async throws {
        // A PDF with a text layer reads without text recognition.
        let model = try imported({ try .file(Letters.textPDF([Letters.letter], named: "letter", in: $0.url(.intake)), .pdf) })
        XCTAssertEqual(model.primaryAction, .finishImport)
        await model.recognize()
        XCTAssertEqual(model.primaryAction, .findNames)
        await model.findNames()
        guard case .replaceAll(let count) = model.primaryAction else {
            return XCTFail("expected Replace N, got \(model.primaryAction)")
        }
        XCTAssertGreaterThan(count, 0)
        model.replaceAllPending()
        XCTAssertEqual(model.primaryAction, .handOff)
    }

    func testReviewBarDecisionUpdatesTheSnapshot() throws {
        let (model, _) = try model()
        let maya = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Maya Ellison" })
        model.open(maya.id)
        XCTAssertEqual(model.reviewing, maya.id)
        model.decide(maya.id, .replace, scope: .all)
        XCTAssertTrue(model.snapshot.markdown.contains(maya.alias))
        XCTAssertEqual(model.snapshot.identities.first { $0.id == maya.identityID }?.state, .replaced)
    }

    func testReviewNextWrapsToTheFirstProposal() throws {
        let (model, _) = try model()
        let pending = model.snapshot.candidates.filter { $0.state == .proposed }.sorted { $0.range.start < $1.range.start }
        model.reviewNext()
        XCTAssertEqual(model.reviewing, pending.first?.id)
        model.select(try XCTUnwrap(pending.last).id)
        model.reviewNext()
        XCTAssertEqual(model.reviewing, pending.first?.id)
    }

    func testIdentityDecisionCoversAllMentions() throws {
        let (model, _) = try model()
        let identity = try XCTUnwrap(model.snapshot.identities.first { $0.primaryWording == "Maya Ellison" })
        XCTAssertGreaterThan(identity.mentionCount, 1)
        model.decide(identity: identity.id, .keep)
        let mentions = model.snapshot.mentions(of: identity.id)
        XCTAssertEqual(mentions.count, identity.mentionCount)
        XCTAssertTrue(mentions.allSatisfy { $0.state == .kept })
    }

    func testShareNeedsAFinishedReview() throws {
        let (model, _) = try model()
        let maya = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Maya Ellison" })
        model.share(asFile: false)
        XCTAssertNil(model.share)
        XCTAssertNotNil(model.errorMessage)
        model.errorMessage = nil
        model.replaceAllPending()
        model.share(asFile: false)
        // The text travels with the document's title as the share preview.
        let item = try XCTUnwrap(model.share?.items.first as? TitledText)
        XCTAssertEqual(item.title, model.snapshot.title)
        XCTAssertTrue(item.text.contains(maya.alias))
        XCTAssertFalse(item.text.contains("Maya Ellison"))
        // Save to Files gets the same text as a .md file.
        let file = try XCTUnwrap(item.file)
        XCTAssertEqual(file.pathExtension, "md")
        XCTAssertEqual(try String(contentsOf: file, encoding: .utf8), item.text)
    }

    func testShareAsFileWritesAMarkdownFileAndKeepsIt() throws {
        let (model, _) = try model()
        model.replaceAllPending()
        model.share(asFile: true)
        let items = try XCTUnwrap(model.share)
        let url = try XCTUnwrap(items.items.first as? URL)
        XCTAssertEqual(url.lastPathComponent, "Letter before claim.md")
        XCTAssertTrue(try String(contentsOf: url, encoding: .utf8).hasPrefix("Note:"))
        // Kept after the sheet closes, since the receiving app may read it
        // later; the next share replaces it.
        model.share = nil
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
        model.share(asFile: false)
        let files = try FileManager.default.contentsOfDirectory(atPath: url.deletingLastPathComponent().path)
        XCTAssertEqual(files, [url.lastPathComponent])
    }

    func testCopyForAIConfirms() throws {
        let (model, _) = try model()
        model.replaceAllPending()
        var copied: String?
        model.copyForAI { copied = $0 }
        XCTAssertNotNil(copied)
        XCTAssertNotNil(model.notice)
    }

    func testPastedReplyOpensRestored() throws {
        let (model, _) = try model()
        model.replaceAllPending()
        let maya = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Maya Ellison" }?.alias)
        let joinery = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Brightwater Joinery Ltd" }?.alias)
        let reply = "\(maya) should hear from \(joinery) by Friday."
        model.showsReplySheet = true
        model.restore(reply: reply)
        XCTAssertFalse(model.showsReplySheet)
        XCTAssertEqual(model.pane, .reply)
        XCTAssertEqual(model.snapshot.reply?.restored.text, "Maya Ellison should hear from Brightwater Joinery Ltd by Friday.")
        XCTAssertEqual(model.snapshot.reply?.aliasText, reply)
        model.clearReply()
        XCTAssertNil(model.snapshot.reply)
        XCTAssertEqual(model.pane, .text)
    }

    /// At the detector tier, Find names needs the name detector, which the
    /// tests never download.
    func testMissingModelAsksForSetup() async throws {
        let (model, _) = try model(\.quotation, memory: Fixture.detectorTier)
        await model.findNames()
        XCTAssertTrue(model.needsSetup)
        XCTAssertEqual(model.setupNeeds, .detector)
    }

    /// On iPad the Documents page and the document are on screen together: the setup card
    /// shows once, in the document that asked for it.
    func testHomeLeavesTheSetupCardToTheDocumentThatAsked() async throws {
        let seeded = try Fixture.seeded(memory: Fixture.detectorTier)
        let app = Fixture.app(seeded)
        XCTAssertTrue(app.setup.showsCard)
        app.open(seeded.quotation)
        let document = try XCTUnwrap(app.openModel)
        XCTAssertFalse(app.openDocumentShowsSetup)
        await document.findNames()
        XCTAssertTrue(document.needsSetup)
        XCTAssertTrue(app.openDocumentShowsSetup)
        app.showStart()
        XCTAssertFalse(app.openDocumentShowsSetup)
    }

    /// Below the floor the core's structured rules scan without a model.
    func testSetupThenScanBelowTheFloor() async throws {
        let model = try imported({ _ in .text(Letters.letter) })
        await model.recognize()
        await model.findNames()
        XCTAssertFalse(model.needsSetup)
        XCTAssertNotEqual(model.snapshot.phase, .unscanned)
    }

    /// A scan needs text recognition, and the document asks for it alone
    /// (ADR 0090). `OCRResumeTests` downloads it and recognizes after setup.
    func testMissingOCRAsksForTextRecognition() async throws {
        let (model, _) = try model(\.tenancy, memory: Fixture.detectorTier)
        await model.recognize()
        XCTAssertTrue(model.needsSetup)
        XCTAssertEqual(model.waitingForSetup, .recognize)
        XCTAssertEqual(model.setupNeeds, .ocr)
        XCTAssertNil(model.errorMessage)
        // Still without it, the job asks again rather than failing.
        await model.continueAfterSetup()
        XCTAssertEqual(model.waitingForSetup, .recognize)
        XCTAssertNil(model.errorMessage)
    }

    /// The core's left-out line (here, a picture) shows once the text is
    /// read; a Word file's comments count on its Original instead.
    func testTheLeftOutLineShowsWithTheText() async throws {
        let model = try imported(
            { try .file(Fixture.copy("tests/fixtures/import/text.docx", into: $0), .docx) },
            title: "Lease")
        XCTAssertNil(model.snapshot.leftOut)
        await model.recognize()
        XCTAssertEqual(model.snapshot.phase, .unscanned)
        XCTAssertEqual(model.snapshot.leftOut, "1 picture not imported.")
        XCTAssertNil(model.snapshot.notice)
        let comments = try imported(
            { try .file(Fixture.copy("tests/fixtures/import/handmade-hidden-content.docx", into: $0), .docx) },
            title: "Notes")
        await comments.recognize()
        XCTAssertNil(comments.snapshot.leftOut)
    }

    /// A locked DOCX opened from Files (an OLE compound file, not a ZIP)
    /// reaches the password card through the importer's format check.
    func testALockedDocxFromFilesAsksForItsPassword() async throws {
        let model = try imported(
            { storage in
                let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
                try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                let url = folder.appendingPathComponent("word-locked.docx")
                try FileManager.default.copyItem(
                    at: Fixture.repository.appendingPathComponent("tests/fixtures/locked-office/word-locked.docx"), to: url)
                return try XCTUnwrap(try Importer(storage: storage).file(url)).source
            },
            title: "Locked")
        await model.recognize()
        XCTAssertEqual(model.passwordPrompt, .needed)
        XCTAssertNil(model.errorMessage)
        await model.unlock(password: "Petrov-2026")
        XCTAssertNil(model.passwordPrompt)
        XCTAssertEqual(model.snapshot.phase, .unscanned)
    }

    /// A locked DOCX asks for its password in place of an error, asks again
    /// after a wrong one, and imports with the right one. The password is
    /// in none of the app's files afterwards.
    func testALockedDocxAsksForItsPasswordUntilItUnlocks() async throws {
        var storage: MdocStorage?
        let model = try imported(
            {
                storage = $0
                return try .file(Fixture.copy("tests/fixtures/locked-office/comments-locked.docx", into: $0), .docx)
            },
            title: "Locked")
        await model.recognize()
        XCTAssertEqual(model.passwordPrompt, .needed)
        XCTAssertNil(model.errorMessage)
        XCTAssertNil(model.snapshot.phase)
        await model.unlock(password: "petrov-2026")
        XCTAssertEqual(model.passwordPrompt, .wrong)
        XCTAssertNil(model.errorMessage)
        await model.unlock(password: "Petrov-2026")
        XCTAssertNil(model.passwordPrompt)
        XCTAssertNil(model.errorMessage)
        XCTAssertEqual(model.snapshot.phase, .unscanned)
        XCTAssertFalse(model.snapshot.markdown.isEmpty)
        let unlocked = try XCTUnwrap(model.unlockedDocx)
        // This URL has no file. Rendering the open document must use its
        // import's unlock, and the PDFKit page must have text.
        let missing = try XCTUnwrap(storage).root.appendingPathComponent("missing.docx")
        let own = try XCTUnwrap(storage).root
        let before = Self.temporaryFiles(keeping: own)
        let result = await RenderedDocx.render(.file(missing), unlocked: unlocked)
        XCTAssertEqual(Self.temporaryFiles(keeping: own), before)
        XCTAssertNil(result.unlocked)
        guard case .ready(let original) = result.rendering else {
            return XCTFail("The unlocked document should render its Original")
        }
        let page = try XCTUnwrap(PDFDocument(data: original.pdf)?.page(at: 0))
        XCTAssertFalse(try XCTUnwrap(page.string).isEmpty)
        XCTAssertNotNil(original.comments.marker(of: "0"))
        let closedPane = model.docxUnlockBinding
        model.closeOriginal()
        closedPane.wrappedValue = unlocked
        XCTAssertNil(model.unlockedDocx)
        guard case .docx(let source) = model.snapshot.original,
              case .password(.needed) = await RenderedDocx.render(source).rendering else {
            return XCTFail("Reopening the Original should ask for the password again")
        }
        let reopened = await RenderedDocx.render(source, password: "Petrov-2026")
        guard case .ready = reopened.rendering else {
            return XCTFail("The Original should render after unlocking from its card")
        }
        XCTAssertNotNil(reopened.unlocked)
        XCTAssertEqual(Self.temporaryFiles(keeping: own), before)
        let root = try XCTUnwrap(storage).root
        let savedFiles = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)?
            .compactMap { $0 as? URL } ?? []
        for file in savedFiles where (try? file.resourceValues(forKeys: [.isRegularFileKey]))?.isRegularFile == true {
            let bytes = try Data(contentsOf: file)
            XCTAssertNil(bytes.range(of: Data("Petrov-2026".utf8)), file.lastPathComponent)
        }
    }

    /// A locked PDF asks for its password as a locked DOCX does, and the
    /// password that imports it unlocks its Original in memory too, until
    /// the document is left. Opened again, the Original's own card unlocks
    /// it, drawing its page with nothing written to the temporary folder.
    /// The password is in none of the app's files afterwards.
    func testALockedPdfAsksForItsPasswordUntilItUnlocks() async throws {
        let password = "Petrov-2026"
        var storage: MdocStorage?
        let model = try imported(
            {
                storage = $0
                return try .file(Fixture.copy("tests/fixtures/pdf-source/comments-aes256.pdf", into: $0), .pdf)
            },
            title: "Locked")
        await model.recognize()
        XCTAssertEqual(model.passwordPrompt, .needed)
        XCTAssertNil(model.errorMessage)
        await model.unlock(password: "petrov-2026")
        XCTAssertEqual(model.passwordPrompt, .wrong)
        XCTAssertNil(model.errorMessage)
        XCTAssertNil(model.unlockedOriginal)
        await model.unlock(password: password)
        XCTAssertNil(model.passwordPrompt)
        XCTAssertNil(model.errorMessage)
        XCTAssertEqual(model.snapshot.phase, .unscanned)
        XCTAssertFalse(model.snapshot.markdown.isEmpty)
        let original = try XCTUnwrap(model.unlockedOriginal)
        XCTAssertFalse(original.isLocked)
        XCTAssertGreaterThan(original.pageCount, 0)
        let closedPane = model.pdfUnlockAction
        model.closeOriginal()
        XCTAssertFalse(closedPane(password))
        XCTAssertNil(model.unlockedOriginal)
        let reopened = DocumentViewModel(
            session: model.session, jobs: LongJobRunner(background: NoBackgroundContinuation()),
            storage: try XCTUnwrap(storage))
        XCTAssertNil(reopened.unlockedOriginal)
        XCTAssertFalse(reopened.unlockOriginal(password: "petrov-2026"))
        XCTAssertNil(reopened.unlockedOriginal)
        let own = try XCTUnwrap(storage).root
        let before = Self.temporaryFiles(keeping: own)
        XCTAssertTrue(reopened.pdfUnlockAction(password))
        let page = try XCTUnwrap(reopened.unlockedOriginal?.page(at: 0))
        XCTAssertGreaterThan(page.thumbnail(of: CGSize(width: 200, height: 280), for: .mediaBox).size.width, 0)
        XCTAssertEqual(Self.temporaryFiles(keeping: own), before)
        let root = try XCTUnwrap(storage).root
        let files = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)?
            .compactMap { $0 as? URL } ?? []
        for file in files where (try? file.resourceValues(forKeys: [.isRegularFileKey]))?.isRegularFile == true {
            let bytes = try Data(contentsOf: file)
            XCTAssertNil(bytes.range(of: Data(password.utf8)), file.lastPathComponent)
        }
    }

    /// A locked scan its password opens still needs the OCR model: the
    /// setup card takes the password card's place, and the password is
    /// asked again once the model is in.
    func testALockedScanAsksForTheModelOnceItsPasswordOpensIt() async throws {
        let model = try imported(
            { try .file(Fixture.copy("tests/fixtures/pdf-source/partly-scanned-aes256.pdf", into: $0), .pdf) },
            title: "Locked scan")
        await model.recognize()
        XCTAssertEqual(model.passwordPrompt, .needed)
        await model.unlock(password: "Petrov-2026")
        XCTAssertNil(model.passwordPrompt)
        XCTAssertEqual(model.waitingForSetup, .recognize)
        XCTAssertNil(model.errorMessage)
    }

    /// Every path under the temporary folder except other tests' libraries.
    /// Test classes run at the same time and each writes its own library
    /// under `mdoc-tests/`, so comparing the whole folder fails at random.
    /// `own` (this test's library) stays in the set, so a file the render
    /// writes anywhere it shouldn't still shows up.
    private static func temporaryFiles(keeping own: URL) -> Set<String> {
        let temporary = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
        let libraries = temporary.appendingPathComponent("mdoc-tests", isDirectory: true).path + "/"
        let ownPath = own.resolvingSymlinksInPath().path
        let found = FileManager.default.enumerator(at: temporary, includingPropertiesForKeys: nil)?
            .compactMap { ($0 as? URL)?.resolvingSymlinksInPath().path } ?? []
        return Set(found.filter { !$0.hasPrefix(libraries) || $0.hasPrefix(ownPath) })
    }

    /// A PDF locked only against editing imports without the card and
    /// shows as it is; encryption no password opens says so, with no card.
    func testOnlyAPdfLockedAgainstOpeningAsksForItsPassword() async throws {
        let ownerOnly = try imported(
            { try .file(Fixture.copy("tests/fixtures/pdf-source/comments-owner-only.pdf", into: $0), .pdf) },
            title: "Owner only")
        await ownerOnly.recognize()
        XCTAssertNil(ownerOnly.passwordPrompt)
        XCTAssertNil(ownerOnly.errorMessage)
        XCTAssertEqual(ownerOnly.snapshot.phase, .unscanned)
        guard case .pdf(let bytes) = ownerOnly.snapshot.original else { return XCTFail("a PDF's Original") }
        XCTAssertEqual(PDFDocument(data: try bytes.data())?.isLocked, false)
        let unknown = try imported(
            {
                // The AES-256 twin with a security handler no reader knows,
                // the same length so every offset still holds.
                let url = try Fixture.copy("tests/fixtures/pdf-source/comments-aes256.pdf", into: $0)
                var bytes = try Data(contentsOf: url)
                let at = try XCTUnwrap(bytes.range(of: Data("/Standard".utf8)))
                bytes.replaceSubrange(at, with: Data("/Nonstand".utf8))
                try bytes.write(to: url)
                return .file(url, .pdf)
            },
            title: "Unknown")
        await unknown.recognize()
        XCTAssertNil(unknown.passwordPrompt)
        XCTAssertEqual(
            unknown.errorMessage,
            "mdoc could not open this file: This PDF uses encryption mdoc can’t open. Import an unlocked copy.")
    }

    /// A cancelled recognition ends the job and offers it again. Resume,
    /// after OCR kept pages, is `ResumeDeleteTests`' and `OCRResumeTests`'.
    func testACancelledRecognitionCanStartAgain() async throws {
        let (model, _) = try model(\.tenancy, slow: 30)
        let run = Task { await model.recognize() }
        while model.runningJob == nil { await Task.yield() }
        model.cancelJob()
        await run.value
        XCTAssertNil(model.runningJob)
        XCTAssertNil(model.errorMessage)
        XCTAssertEqual(model.primaryAction, .recognize(resume: false))
    }

    /// While Find names runs on the document, review waits and says so: a
    /// change to the text would make the core throw the scan away.
    func testReviewWaitsWhileFindNamesRuns() async throws {
        let (model, _) = try model(slow: 30)
        let candidate = try XCTUnwrap(model.snapshot.candidates.first { $0.state == .proposed })
        let pending = model.snapshot.pendingCount
        let run = Task { await model.findNames() }
        while model.runningJob == nil { await Task.yield() }
        model.open(candidate.id)
        XCTAssertNil(model.reviewing)
        XCTAssertEqual(model.notice, "Finding names. Review when it finishes.")
        XCTAssertEqual(model.noticeSymbol, "hourglass")
        model.decide(candidate.id, .replace, scope: .all)
        model.replaceAllPending()
        XCTAssertEqual(model.snapshot.pendingCount, pending)
        model.markSelection(candidate.range)
        XCTAssertEqual(model.snapshot.pendingCount, pending)
        XCTAssertNil(model.reviewing)
        model.cancelJob()
        await run.value
        model.open(candidate.id)
        XCTAssertEqual(model.reviewing, candidate.id)
    }

    func testSafeFileNameDropsPathSeparators() {
        XCTAssertEqual(DocumentViewModel.safeFileName("a/b:c.md"), "a-b-c.md")
    }
}

@MainActor
final class LibraryModelTests: XCTestCase {
    func testDocumentsAreNewestFirstWithTheirProject() throws {
        let seeded = try Fixture.seeded()
        let home = LibraryModel(library: seeded.library)
        let documents = home.documents
        XCTAssertEqual(documents.map(\.id), [seeded.tenancy, seeded.letter, seeded.quotation])
        let tenancy = try XCTUnwrap(documents.first { $0.id == seeded.tenancy })
        XCTAssertEqual(home.projectSummary(of: tenancy)?.id, seeded.harbour)
        let letter = try XCTUnwrap(documents.first { $0.id == seeded.letter })
        XCTAssertEqual(home.projectSummary(of: letter)?.id, seeded.ellison)
    }

    func testImportIntoAnExistingProject() throws {
        let seeded = try Fixture.seeded()
        let home = LibraryModel(library: seeded.library)
        let pending = PendingImport(source: .text("Maya Ellison phoned."), suggestedTitle: "Call note")
        let session = try home.importDocument(pending, title: " ", into: seeded.harbour)
        XCTAssertEqual(session.snapshot().title, "Call note")
        XCTAssertEqual(session.snapshot().projectID, seeded.harbour)
        XCTAssertEqual(home.documents.filter { $0.projectID == seeded.harbour }.count, 2)
    }

    /// An import lands in Unfiled by default: no Project is made for it
    /// (#1091 Decision 35).
    func testImportLandsInUnfiledByDefault() throws {
        let home = LibraryModel(library: try Fixture.seeded().library)
        let pending = PendingImport(source: .text("Text"), suggestedTitle: "Note")
        _ = try home.importDocument(pending, title: "Note", into: nil)
        XCTAssertEqual(home.projects.count, 2)
        let note = try XCTUnwrap(home.documents.first { $0.title == "Note" })
        XCTAssertNil(home.projectSummary(of: note))
    }

    /// A scan that found nothing says so in the lists (ADR 0087).
    func testStatusSaysWhenNothingWasFound() {
        var document = LibraryDocument(
            id: DocumentID(rawValue: "letter"), title: "Letter", projectID: nil, kind: .text, phase: .ready,
            pendingCount: 0, updated: .now, candidateCount: 0)
        XCTAssertEqual(LibraryModel.status(of: document), "Nothing found")
        document.candidateCount = 3
        XCTAssertEqual(LibraryModel.status(of: document), "Ready to share")
    }

    /// An interrupted import reads as one; scans and photos keep the
    /// recognition wording (ADR 0088).
    func testStatusSaysWhenAnImportIsNotFinished() {
        var document = LibraryDocument(
            id: DocumentID(rawValue: "quotation"), title: "Quotation", projectID: nil, kind: .docx, phase: nil,
            pendingCount: 0, updated: .now)
        XCTAssertEqual(LibraryModel.status(of: document), "Import not finished")
        document.kind = .pdf
        XCTAssertEqual(LibraryModel.status(of: document), "Import not finished")
        document.kind = .scan
        XCTAssertEqual(LibraryModel.status(of: document), "Text not recognized yet")
        document.kind = .image
        XCTAssertEqual(LibraryModel.status(of: document), "Text not recognized yet")
    }

    func testBelowTheFloorNothingWaitsForFindNames() {
        let document = LibraryDocument(
            id: DocumentID(rawValue: "letter"), title: "Letter", projectID: nil, kind: .text, phase: .unscanned,
            pendingCount: 0, updated: .now)
        XCTAssertEqual(LibraryModel.status(of: document), "Ready to find names")
        XCTAssertEqual(LibraryModel.status(of: document, findsNames: false), "Ready to mark names")
    }

    func testDeleteRemovesTheDocument() throws {
        let seeded = try Fixture.seeded()
        let home = LibraryModel(library: seeded.library)
        try home.delete([seeded.tenancy])
        XCTAssertFalse(home.documents.contains { $0.projectID == seeded.harbour })
    }
}

/// Records the title of every continued task a job asks for.
private final class RecordingContinuation: BackgroundContinuation, @unchecked Sendable {
    private(set) var titles: [String] = []

    @MainActor
    func begin(title: String, expire: @escaping @Sendable () -> Void) -> any BackgroundLease {
        titles.append(title)
        return NoBackgroundContinuation().begin(title: title, expire: expire)
    }
}
