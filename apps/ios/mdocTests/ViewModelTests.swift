import XCTest
@testable import mdoc

@MainActor
final class DocumentViewModelTests: XCTestCase {
    private func model(
        _ id: DocumentID = Fixture.letter,
        core: Core = Fixture.core(),
        jobs: LongJobRunner? = nil
    ) throws -> DocumentViewModel {
        DocumentViewModel(
            session: try core.library.session(for: id),
            jobs: jobs ?? LongJobRunner(background: NoBackgroundContinuation()),
            storage: try Fixture.temporaryStorage())
    }

    func testPrimaryActionFollowsTheDocument() async throws {
        let model = try model(Fixture.tenancy)
        XCTAssertEqual(model.primaryAction, .recognize(resume: true))
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

    /// Next proposal counts what the chosen scope leaves undecided (#285).
    func testNextProposalFollowsTheScope() throws {
        let model = try model()
        let candidates = model.snapshot.candidates
        let repeated = try XCTUnwrap(candidates.first { candidate in
            candidates.filter { $0.identityID == candidate.identityID }.count > 1
        }, "the fixture needs a name mentioned twice")
        let others = candidates.filter { $0.state == .proposed && $0.identityID != repeated.identityID }
        XCTAssertTrue(model.hasProposal(after: repeated.id, scope: .justThisOne))
        XCTAssertEqual(model.hasProposal(after: repeated.id, scope: .allOccurrences), !others.isEmpty)
        // Deciding every other identity leaves only the repeat's mentions.
        for other in others {
            model.decide(other.id, .keep, scope: .allOccurrences)
        }
        XCTAssertFalse(model.hasProposal(after: repeated.id, scope: .allOccurrences))
        XCTAssertTrue(model.hasProposal(after: repeated.id, scope: .justThisOne))
    }

    func testCandidateSheetDecisionUpdatesTheSnapshot() throws {
        let model = try model()
        let maya = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Maya Ellison" })
        model.open(maya.id)
        XCTAssertEqual(model.reviewing, maya.id)
        model.decide(maya.id, .replace, scope: .allOccurrences)
        XCTAssertTrue(model.snapshot.markdown.contains(maya.alias))
        XCTAssertEqual(model.snapshot.identities.first { $0.id == maya.identityID }?.state, .replaced)
    }

    func testReviewNextWrapsToTheFirstProposal() throws {
        let model = try model()
        let pending = model.snapshot.candidates.filter { $0.state == .proposed }
        model.reviewNext()
        XCTAssertEqual(model.reviewing, pending.first?.id)
        model.reviewing = pending.last?.id
        model.reviewNext()
        XCTAssertEqual(model.reviewing, pending.first?.id)
    }

    func testIdentityDecisionCoversAllMentions() throws {
        let model = try model()
        let identity = try XCTUnwrap(model.snapshot.identities.first { $0.primaryWording == "Daniel Okafor" })
        model.decide(identity: identity.id, .keep)
        XCTAssertTrue(model.snapshot.mentions(of: identity.id).allSatisfy { $0.state == .kept })
    }

    func testShareNeedsAFinishedReview() throws {
        let model = try model()
        model.share(asFile: false)
        XCTAssertNil(model.share)
        XCTAssertNotNil(model.errorMessage)
        model.errorMessage = nil
        model.replaceAllPending()
        model.share(asFile: false)
        let text = try XCTUnwrap(model.share?.items.first as? String)
        XCTAssertTrue(text.contains("PERSON_1"))
        XCTAssertFalse(text.contains("Maya Ellison"))
    }

    func testShareAsFileWritesAMarkdownFileAndRemovesIt() throws {
        let model = try model()
        model.replaceAllPending()
        model.share(asFile: true)
        let items = try XCTUnwrap(model.share)
        let url = try XCTUnwrap(items.items.first as? URL)
        XCTAssertEqual(url.lastPathComponent, "Letter before claim.md")
        XCTAssertTrue(try String(contentsOf: url, encoding: .utf8).hasPrefix("Note:"))
        model.shareFinished(items)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path))
    }

    func testCopyForAIConfirms() throws {
        let model = try model()
        model.replaceAllPending()
        var copied: String?
        model.copyForAI { copied = $0 }
        XCTAssertNotNil(copied)
        XCTAssertNotNil(model.notice)
    }

    func testPastedReplyOpensRestored() throws {
        let model = try model()
        model.replaceAllPending()
        model.showsReplySheet = true
        model.restore(reply: "PERSON_1 should hear from ORG_1 by Friday.")
        XCTAssertFalse(model.showsReplySheet)
        XCTAssertEqual(model.pane, .reply)
        XCTAssertEqual(model.snapshot.reply?.restored.text, "Maya Ellison should hear from Brightwater Joinery Ltd by Friday.")
        XCTAssertEqual(model.snapshot.reply?.aliasText, "PERSON_1 should hear from ORG_1 by Friday.")
        model.clearReply()
        XCTAssertNil(model.snapshot.reply)
        XCTAssertEqual(model.pane, .text)
    }

    func testMissingModelAsksForSetup() async throws {
        let core = Fixture.core(modelsInstalled: false)
        let model = try model(Fixture.quotation, core: core)
        // The quotation is scanned already; scanning again needs the model.
        await model.findNames()
        XCTAssertTrue(model.needsSetup)
    }

    func testSetupThenScanBelowTheFloor() async throws {
        let core = Fixture.core(modelsInstalled: false, memory: 4 * Fixture.gigabyte)
        let model = try model(Fixture.tenancy, core: core)
        await model.recognize()
        await model.findNames()
        XCTAssertFalse(model.needsSetup)
        XCTAssertNotEqual(model.snapshot.phase, .unscanned)
    }

    func testMissingOCRSetsUpThenRecognizes() async throws {
        let core = Fixture.core(modelsInstalled: false)
        let model = try model(Fixture.tenancy, core: core)
        await model.recognize()
        XCTAssertTrue(model.needsSetup)
        XCTAssertEqual(model.waitingForSetup, .recognize)
        XCTAssertNil(model.errorMessage)
        for offer in core.setup.snapshot().offers {
            for request in try core.setup.downloadRequests(for: offer.id) {
                _ = try core.setup.install(request, from: URL(fileURLWithPath: "/dev/null"))
            }
        }
        await model.continueAfterSetup()
        XCTAssertFalse(model.needsSetup)
        XCTAssertEqual(model.snapshot.phase, .unscanned)
    }

    func testAnImportWarningShowsWithTheText() async throws {
        let core = Fixture.core()
        let session = try core.library.importDocument(
            .file(URL(fileURLWithPath: "/tmp/statement.pdf"), .pdf), title: "Statement", into: .newCase(name: "Statement"))
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: try Fixture.temporaryStorage())
        XCTAssertNil(model.snapshot.warning)
        await model.recognize()
        XCTAssertEqual(model.snapshot.warning, FakeFixtures.importWarning)
    }

    func testCancelledRecognitionCanResume() async throws {
        let jobs = LongJobRunner(background: NoBackgroundContinuation())
        let core = Fixture.core(timing: FakeCore.Timing(page: 5, scan: 0, install: 0))
        let model = try model(Fixture.tenancy, core: core, jobs: jobs)
        let run = Task { await model.recognize() }
        while model.runningJob == nil { await Task.yield() }
        model.cancelJob()
        await run.value
        XCTAssertNil(model.runningJob)
        XCTAssertEqual(model.primaryAction, .recognize(resume: true))
    }

    func testSafeFileNameDropsPathSeparators() {
        XCTAssertEqual(DocumentViewModel.safeFileName("a/b:c.md"), "a-b-c.md")
    }
}

@MainActor
final class HomeViewModelTests: XCTestCase {
    func testDocumentsAreNewestFirstWithTheirCase() {
        let home = HomeViewModel(library: Fixture.core().library)
        let documents = home.documents
        XCTAssertEqual(Set(documents.map(\.id)), [Fixture.letter, Fixture.quotation, Fixture.tenancy])
        XCTAssertEqual(documents.map(\.updated), documents.map(\.updated).sorted(by: >))
        let tenancy = try? XCTUnwrap(documents.first { $0.id == Fixture.tenancy })
        XCTAssertEqual(tenancy.flatMap(home.caseSummary(of:))?.id, Fixture.harbour)
        let letter = try? XCTUnwrap(documents.first { $0.id == Fixture.letter })
        XCTAssertEqual(letter.flatMap(home.caseSummary(of:))?.id, Fixture.ellison)
    }

    func testImportIntoAnExistingCase() throws {
        let home = HomeViewModel(library: Fixture.core().library)
        let pending = PendingImport(source: .text("Maya Ellison phoned."), suggestedTitle: "Call note")
        home.pendingImport = pending
        let session = try XCTUnwrap(home.importDocument(pending, title: " ", into: .existing(Fixture.harbour)))
        XCTAssertNil(home.pendingImport)
        XCTAssertEqual(session.snapshot().title, "Call note")
        XCTAssertEqual(session.snapshot().caseID, Fixture.harbour)
        XCTAssertEqual(home.documents.filter { $0.caseID == Fixture.harbour }.count, 2)
    }

    func testImportStartsANewCaseByDefault() throws {
        let home = HomeViewModel(library: Fixture.core().library)
        let pending = PendingImport(source: .text("Text"), suggestedTitle: "Note")
        _ = home.importDocument(pending, title: "Note", into: .newCase(name: "Note"))
        XCTAssertEqual(home.cases.count, 3)
        let note = try XCTUnwrap(home.documents.first { $0.title == "Note" })
        XCTAssertEqual(home.caseSummary(of: note)?.name, "Note")
    }

    func testDeleteRemovesTheDocument() {
        let home = HomeViewModel(library: Fixture.core().library)
        home.delete(Fixture.tenancy)
        XCTAssertFalse(home.documents.contains { $0.caseID == Fixture.harbour })
    }
}

@MainActor
final class RestoreViewModelTests: XCTestCase {
    func testDefaultsToTheCaseSharedLast() throws {
        let model = RestoreViewModel(library: Fixture.core().library, reply: "PERSON_1 agrees.")
        XCTAssertEqual(model.selectedCase, Fixture.ellison)
        XCTAssertEqual(model.result?.text, "Maya Ellison agrees.")
        XCTAssertEqual(model.defaultSaveTarget?.id, Fixture.quotation)
    }

    func testPickingAnotherCaseRestoresWithItsNamesOnly() throws {
        let model = RestoreViewModel(library: Fixture.core().library, reply: "PERSON_1 agrees.")
        model.selectCase(Fixture.harbour)
        XCTAssertEqual(model.selectedCase, Fixture.harbour)
        XCTAssertEqual(model.result?.text, "PERSON_1 agrees.")
        XCTAssertEqual(model.result?.unresolvedCount, 1)
        XCTAssertEqual(model.documentsInCase.map(\.id), [Fixture.tenancy])
    }

    func testSaveKeepsTheReplyInAliasForm() throws {
        let core = Fixture.core()
        let model = RestoreViewModel(library: core.library, reply: "PERSON_1 agrees.")
        model.save(to: Fixture.quotation)
        XCTAssertEqual(model.savedTo, Fixture.quotation)
        let reply = try core.library.session(for: Fixture.quotation).snapshot().reply
        XCTAssertEqual(reply?.aliasText, "PERSON_1 agrees.")
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
