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
        let session = try core.library.importDocument(try source(storage), title: title, into: .newCase(name: title))
        return DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage,
            findsNames: findsNames)
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
        let model = try imported({ try .file(Letters.textPDF([Letters.letter], named: "letter", in: $0.url(.originals)), .pdf) })
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
        model.decide(maya.id, .replace, scope: .allOccurrences)
        XCTAssertTrue(model.snapshot.markdown.contains(maya.alias))
        XCTAssertEqual(model.snapshot.identities.first { $0.id == maya.identityID }?.state, .replaced)
    }

    func testReviewNextWrapsToTheFirstProposal() throws {
        let (model, _) = try model()
        let pending = model.snapshot.candidates.filter { $0.state == .proposed }.sorted { $0.range.start < $1.range.start }
        model.reviewNext()
        XCTAssertEqual(model.reviewing, pending.first?.id)
        model.reviewing = pending.last?.id
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

    /// The core's import note (here, Word comments left out) shows once the
    /// text is read.
    func testAnImportWarningShowsWithTheText() async throws {
        let model = try imported(
            { try .file(Fixture.copy("tests/fixtures/import/handmade-hidden-content.docx", into: $0), .docx) },
            title: "Lease")
        XCTAssertNil(model.snapshot.warning)
        await model.recognize()
        XCTAssertEqual(model.snapshot.phase, .unscanned)
        XCTAssertEqual(model.snapshot.warning, "2 comments not imported.")
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
        model.decide(candidate.id, .replace, scope: .allOccurrences)
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
    func testDocumentsAreNewestFirstWithTheirCase() throws {
        let seeded = try Fixture.seeded()
        let home = LibraryModel(library: seeded.library)
        let documents = home.documents
        XCTAssertEqual(documents.map(\.id), [seeded.tenancy, seeded.letter, seeded.quotation])
        let tenancy = try XCTUnwrap(documents.first { $0.id == seeded.tenancy })
        XCTAssertEqual(home.caseSummary(of: tenancy)?.id, seeded.harbour)
        let letter = try XCTUnwrap(documents.first { $0.id == seeded.letter })
        XCTAssertEqual(home.caseSummary(of: letter)?.id, seeded.ellison)
    }

    func testImportIntoAnExistingCase() throws {
        let seeded = try Fixture.seeded()
        let home = LibraryModel(library: seeded.library)
        let pending = PendingImport(source: .text("Maya Ellison phoned."), suggestedTitle: "Call note")
        let session = try home.importDocument(pending, title: " ", into: .existing(seeded.harbour))
        XCTAssertEqual(session.snapshot().title, "Call note")
        XCTAssertEqual(session.snapshot().caseID, seeded.harbour)
        XCTAssertEqual(home.documents.filter { $0.caseID == seeded.harbour }.count, 2)
    }

    func testImportStartsANewCaseByDefault() throws {
        let home = LibraryModel(library: try Fixture.seeded().library)
        let pending = PendingImport(source: .text("Text"), suggestedTitle: "Note")
        _ = try home.importDocument(pending, title: "Note", into: .newCase(name: "Note"))
        XCTAssertEqual(home.cases.count, 3)
        let note = try XCTUnwrap(home.documents.first { $0.title == "Note" })
        XCTAssertEqual(home.caseSummary(of: note)?.name, "Note")
    }

    /// A new case never takes a name a case already has: the case pickers
    /// could not tell the two apart.
    func testANewCaseOfATakenNameIsNumbered() throws {
        let home = LibraryModel(library: try Fixture.seeded().library)
        let taken = try XCTUnwrap(home.cases.first?.name)
        let pending = PendingImport(source: .text("Text"), suggestedTitle: "Note")
        _ = try home.importDocument(pending, title: "Note", into: .newCase(name: taken))
        XCTAssertEqual(home.cases.filter { $0.name == taken }.count, 1)
        XCTAssertTrue(home.cases.contains { $0.name == "\(taken) 2" })
        // Spaces around a name count for nothing, as the core trims them.
        _ = try home.importDocument(pending, title: "Note", into: .newCase(name: " \(taken) "))
        XCTAssertEqual(home.cases.filter { $0.name == taken }.count, 1)
        XCTAssertTrue(home.cases.contains { $0.name == "\(taken) 3" })
        XCTAssertEqual(LibraryModel.unusedName("Lease", among: ["Lease", "Lease 2"]), "Lease 3")
        XCTAssertEqual(LibraryModel.unusedName("Lease", among: ["Other"]), "Lease")
    }

    /// A scan that found nothing says so in the lists (ADR 0087).
    func testStatusSaysWhenNothingWasFound() {
        var document = DocumentSummary(
            id: DocumentID(rawValue: "letter"), title: "Letter", caseID: nil, kind: .text, phase: .ready,
            pendingCount: 0, updated: .now, candidateCount: 0)
        XCTAssertEqual(LibraryModel.status(of: document), "Nothing found")
        document.candidateCount = 3
        XCTAssertEqual(LibraryModel.status(of: document), "Ready to share")
    }

    /// An interrupted import reads as one; scans and photos keep the
    /// recognition wording (ADR 0088).
    func testStatusSaysWhenAnImportIsNotFinished() {
        var document = DocumentSummary(
            id: DocumentID(rawValue: "quotation"), title: "Quotation", caseID: nil, kind: .docx, phase: .recognizing,
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
        let document = DocumentSummary(
            id: DocumentID(rawValue: "letter"), title: "Letter", caseID: nil, kind: .text, phase: .unscanned,
            pendingCount: 0, updated: .now)
        XCTAssertEqual(LibraryModel.status(of: document), "Ready to find names")
        XCTAssertEqual(LibraryModel.status(of: document, findsNames: false), "Ready to mark names")
    }

    func testDeleteRemovesTheDocument() throws {
        let seeded = try Fixture.seeded()
        let home = LibraryModel(library: seeded.library)
        try home.delete(seeded.tenancy)
        XCTAssertFalse(home.documents.contains { $0.caseID == seeded.harbour })
    }
}

@MainActor
final class RestoreViewModelTests: XCTestCase {
    /// The seeded library, and the alias the shared quotation gave Maya Ellison.
    private func seeded() throws -> (Seeded, String) {
        let seeded = try Fixture.seeded()
        let quotation = try seeded.session(seeded.quotation).snapshot()
        let alias = try XCTUnwrap(quotation.candidates.first { $0.original == "Maya Ellison" }?.alias)
        return (seeded, alias)
    }

    func testDefaultsToTheCaseSharedLast() throws {
        let (seeded, alias) = try seeded()
        let model = RestoreViewModel(library: seeded.library, reply: "\(alias) agrees.")
        XCTAssertEqual(model.selectedCase, seeded.ellison)
        XCTAssertEqual(model.result?.text, "Maya Ellison agrees.")
        // Save to document's default: the case's most recently changed one.
        XCTAssertEqual(model.documentsInCase.map(\.id), [seeded.letter, seeded.quotation])
        XCTAssertEqual(model.defaultSaveTarget?.id, seeded.letter)
    }

    func testPickingAnotherCaseRestoresWithItsNamesOnly() throws {
        let (seeded, alias) = try seeded()
        let model = RestoreViewModel(library: seeded.library, reply: "\(alias) agrees.")
        model.selectCase(seeded.harbour)
        XCTAssertEqual(model.selectedCase, seeded.harbour)
        XCTAssertEqual(model.result?.text, "\(alias) agrees.")
        XCTAssertEqual(model.result?.unresolvedCount, 1)
        XCTAssertEqual(model.documentsInCase.map(\.id), [seeded.tenancy])
    }

    func testSaveKeepsTheReplyInAliasForm() throws {
        let (seeded, alias) = try seeded()
        let model = RestoreViewModel(library: seeded.library, reply: "\(alias) agrees.")
        model.save(to: seeded.quotation)
        XCTAssertEqual(model.savedTo, seeded.quotation)
        let reply = try seeded.session(seeded.quotation).snapshot().reply
        XCTAssertEqual(reply?.aliasText, "\(alias) agrees.")
        XCTAssertEqual(reply?.restored.text, "Maya Ellison agrees.")
    }

    func testWithoutCasesItSaysSo() {
        let model = RestoreViewModel(library: EmptyLibrary(), reply: "PERSON_1")
        XCTAssertNil(model.selectedCase)
        XCTAssertNil(model.result)
        XCTAssertEqual(model.errorMessage, CoreError.noCase.errorDescription)
    }
}

/// A library with no cases at all.
private final class EmptyLibrary: Library {
    func snapshot() -> LibrarySnapshot { .empty }
    func session(for document: DocumentID) throws -> any DocumentSession { throw CoreError.notFound }
    func importDocument(_ source: ImportSource, title: String, into caseChoice: CaseChoice) throws -> any DocumentSession {
        throw CoreError.notFound
    }
    func deleteDocument(_ document: DocumentID) throws {}
    func restore(reply: String, in caseID: CaseID) throws -> RestoreResult { throw CoreError.noCase }
    func saveReply(_ reply: String, to document: DocumentID) throws {}
}
