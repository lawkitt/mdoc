import Foundation
import enum MdocFFI.CoreFailure
import XCTest
@testable import mdoc

/// ROADMAP iOS item 15 (ADR 0104): the core owns the rules and the state,
/// and the app reads them. Each test fails with the Swift copy it replaced.
final class CoreWiringTests: XCTestCase {
    private var storage: MdocStorage!

    override func setUpWithError() throws {
        storage = try Fixture.temporaryStorage()
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: storage.root)
    }

    private func core() throws -> Core {
        try RealCore.make(storage: storage, keys: FixedCaseKeyStore(), physicalMemory: Fixture.belowFloor)
    }

    private static let letter = """
        # Letter

        Please reply to maya.ellison@example.com or call 01632 960 418.
        Ms Ellison's account: GB82 WEST 1234 5698 7654 32.
        """

    private static let twoAddresses = "Write to ann@example.com or to bob@example.org."

    /// Read, not scanned yet.
    private func imported(_ core: Core, text: String = letter) throws -> any DocumentSession {
        let session = try core.library.importDocument(.text(text), title: "Letter", into: .newCase(name: "Ellison"))
        _ = try session.recognize(observer: TestObserver())
        return session
    }

    // MARK: #680, cancel by job

    /// A cancel that comes before the core starts the job (the user's tap,
    /// or the background time running out, just as it starts) stops it.
    func testACancelJustBeforeAJobStartsStopsIt() throws {
        let session = try imported(core())
        let observer = TestObserver()
        observer.cancel()
        XCTAssertThrowsError(try session.scan(observer: observer)) { error in
            XCTAssertEqual(error as? CoreError, .cancelled)
        }
        XCTAssertEqual(session.snapshot().phase, .unscanned)
        // The cancel was that job's: the next one runs.
        XCTAssertEqual(try session.scan(observer: TestObserver()).phase, .reviewing)
    }

    /// A cancel of a job that ended (its background time running out after
    /// it finished) never stops the job that runs next.
    func testACancelAfterAJobEndedNeverStopsTheNext() throws {
        let session = try imported(core())
        let first = TestObserver()
        _ = try session.scan(observer: first)
        let second = TestObserver()
        // While the second scan runs, the first job's cancel comes.
        second.onProgress = { _ in first.cancel() }
        let scanned = try session.scan(observer: second)
        XCTAssertTrue(first.isCancelled())
        XCTAssertEqual(scanned.phase, .reviewing)
    }

    // MARK: #681, no state beside the core

    /// The session holds no copy of what the core holds, so there is no
    /// cache to go stale and nothing a job writes while the main thread
    /// reads it.
    func testTheSessionKeepsNoCopyOfTheCoresState() throws {
        let core = try core()
        let session = try XCTUnwrap(try imported(core) as? RealSession)
        let stored = Set(Mirror(reflecting: session).children.compactMap(\.label))
        XCTAssertTrue(stored.isDisjoint(with: ["path", "caseID", "title", "review", "scanned"]), "\(stored)")
        // What it says is the core's.
        let info = try XCTUnwrap(session.info)
        XCTAssertEqual(info.title, "Letter")
        XCTAssertEqual(info.caseId, session.snapshot().caseID?.rawValue)
        XCTAssertNotNil(info.path)
        try core.library.renameDocument(session.id, to: "Reply")
        XCTAssertEqual(session.info?.title, "Reply")
        XCTAssertEqual(session.snapshot().title, "Reply")
    }

    /// The library reads a session's file while a job runs on it.
    func testTheLibraryReadsASessionWhileItsJobRuns() throws {
        let core = try core()
        let session = try imported(core)
        let done = expectation(description: "scan")
        DispatchQueue.global().async {
            _ = try? session.scan(observer: TestObserver())
            done.fulfill()
        }
        for _ in 0..<50 {
            XCTAssertEqual(core.library.snapshot().documents.count, 1)
            XCTAssertNotNil((session as? RealSession)?.path)
        }
        wait(for: [done], timeout: 30)
    }

    // MARK: One call per tap, the core's snapshot fields

    /// A mention keeps its id through a decision, so the bar's candidate
    /// stays the same one (`Mention.id`).
    func testAMentionKeepsItsIDThroughADecision() throws {
        let session = try imported(core())
        let scanned = try session.scan(observer: TestObserver())
        let email = try XCTUnwrap(scanned.candidates.first { $0.category == .email })
        let replaced = try session.decide(email.id, .replace, scope: .this)
        XCTAssertEqual(replaced.candidate(email.id)?.state, .replaced)
        XCTAssertEqual(try session.undo().candidate(email.id)?.state, .proposed)
    }

    /// The text's revision changes with the text only: a Keep leaves it,
    /// a Replace changes it. The styling is parsed once per text.
    func testTheTextRevisionFollowsTheText() throws {
        let session = try imported(core(), text: Self.twoAddresses)
        let scanned = try session.scan(observer: TestObserver())
        let revision = try XCTUnwrap(scanned.textRevision)
        let candidates = scanned.candidates.filter { $0.state == .proposed }
        XCTAssertGreaterThan(candidates.count, 1)
        let kept = try session.decide(candidates[0].id, .keep, scope: .this)
        XCTAssertEqual(kept.textRevision, revision)
        XCTAssertEqual(kept.styling, scanned.styling)
        let replaced = try session.decide(candidates[1].id, .replace, scope: .this)
        XCTAssertNotEqual(replaced.textRevision, revision)
        XCTAssertEqual(replaced.styling, .init(markdown: replaced.markdown))
    }

    /// Rename to an alias another name has: refused by the core, with that
    /// name, in the same call.
    func testAnAliasTakenComesBackWithItsIdentity() throws {
        let session = try imported(core(), text: Self.twoAddresses)
        let scanned = try session.scan(observer: TestObserver())
        let email = try XCTUnwrap(scanned.candidates.first { $0.category == .email })
        let other = try XCTUnwrap(scanned.candidates.first { $0.identityID != email.identityID })
        XCTAssertThrowsError(try session.rename(other.id, scope: .all, to: email.alias)) { error in
            XCTAssertEqual(error as? CoreError, .aliasTaken(alias: email.alias, by: email.identityID))
        }
        // With spaces around it, the same alias.
        XCTAssertThrowsError(try session.rename(other.id, scope: .all, to: " \(email.alias) ")) { error in
            XCTAssertEqual(error as? CoreError, .aliasTaken(alias: email.alias, by: email.identityID))
        }
    }

    /// A name kept everywhere lists under Kept with its mentions, as the
    /// core's `kept_identities` gives it.
    func testANameKeptEverywhereListsAsTheCoreGivesIt() throws {
        let session = try imported(core(), text: "Write to ann@example.com. Again: ann@example.com.")
        let scanned = try session.scan(observer: TestObserver())
        let email = try XCTUnwrap(scanned.candidates.first)
        let kept = try session.decide(email.id, .keep, scope: .all)
        let row = try XCTUnwrap(kept.identities.first { $0.id == email.identityID })
        XCTAssertEqual(row.state, .kept)
        XCTAssertEqual(row.mentionCount, 2)
        XCTAssertEqual(row.variants, ["ann@example.com"])
        XCTAssertEqual(kept.phase, .ready)
    }

    // MARK: Try a sample

    /// The sample's names are found as the platforms' search finds them:
    /// a name typed with é as one character matches the text's e and
    /// combining accent (`occurrences`). Swift's byte ranges took the
    /// wording's length and cut the name short.
    func testTheSampleFindsANameSpelledEitherWay() throws {
        let decomposed = "Jose\u{301} Martin signed. Jose\u{301} Martin left."
        let session = try imported(core(), text: decomposed)
        let proposed = try session.propose([SampleDocument.Name(wording: "Jos\u{E9} Martin", category: .person)])
        XCTAssertEqual(proposed.candidates.count, 2)
        XCTAssertEqual(proposed.candidates.map(\.original), ["Jose\u{301} Martin", "Jose\u{301} Martin"])
        XCTAssertEqual(proposed.candidates.map(\.original.utf8.count), [13, 13], "the whole name, accent and all")
        XCTAssertEqual(proposed.phase, .reviewing)
    }

    // MARK: Typed errors

    /// A review change that cannot be saved says so, not "could not open
    /// this file". The tap itself returns; its save fails.
    func testASaveFailureReadsAsOne() throws {
        let session = try imported(core())
        let scanned = try session.scan(observer: TestObserver())
        let email = try XCTUnwrap(scanned.candidates.first { $0.category == .email })
        let documents = storage.url(.documents)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: documents.path)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: documents.path) }
        XCTAssertNoThrow(try session.decide(email.id, .replace, scope: .all))
        XCTAssertThrowsError(try session.saveChanges()) { error in
            guard case .saveFailed = error as? CoreError else { return XCTFail("\(error)") }
            let message = error.localizedDescription
            XCTAssertTrue(message.hasPrefix("mdoc could not save this change"), message)
        }
    }

    /// Each of the core's errors maps to one of the app's, and no two read
    /// alike.
    func testEachCoreErrorHasItsOwnMessage() {
        let failures: [(CoreFailure, CoreError)] = [
            (.importFailed("x"), .importFailed("x")),
            (.needsOcr(pages: [1], pageCount: 1), .modelMissing),
            (.ocr("x"), .recognitionFailed("x")),
            (.detect("x"), .findNamesFailed("x")),
            (.cancelled, .cancelled),
            (.review("x"), .reviewChanged("x")),
            (.caseFailed("x"), .caseRefused("x")),
            (.setup("x"), .setupFailed("x")),
            (.invalid("x"), .invalid("x")),
            (.io("x"), .fileFailed("x")),
            (.pendingProposals(count: 2), .pendingReview(2)),
            (.aliasTaken(alias: "PERSON_1", identity: 7), .aliasTaken(alias: "PERSON_1", by: IdentityID(rawValue: "7"))),
            (.nameNotFound, .nameNotFound),
            (.refused("x"), .refused("x")),
            (.storage("x"), .saveFailed("x")),
        ]
        for (failure, expected) in failures {
            XCTAssertEqual(CoreError(failure), expected)
        }
        let prefixed: [CoreError] = [
            .importFailed("x"), .recognitionFailed("x"), .findNamesFailed("x"), .setupFailed("x"), .fileFailed("x"),
            .saveFailed("x"),
        ]
        let messages = prefixed.compactMap(\.errorDescription)
        XCTAssertEqual(Set(messages).count, prefixed.count, "\(messages)")
        XCTAssertEqual(messages.filter { $0.contains("could not open this file") }.count, 1)
    }

    // MARK: Document summaries

    /// The list comes from the core's summaries: no document is opened to
    /// draw it, in the app or the Restore extension.
    @MainActor
    func testTheListOpensNoDocument() throws {
        let seeded = try Fixture.seeded()
        let library = try RealCore.library(storage: seeded.storage, keys: FixedCaseKeyStore())
        let real = try XCTUnwrap(library as? RealLibrary)
        // The tenancy, an import not read yet, comes back from `pending.json`.
        XCTAssertEqual(real.openSessionCount, 1)
        let snapshot = library.snapshot()
        XCTAssertEqual(Set(snapshot.documents.map(\.title)), [Letters.letterTitle, Letters.quotationTitle, Letters.tenancyTitle])
        let letter = try XCTUnwrap(snapshot.documents.first { $0.title == Letters.letterTitle })
        XCTAssertEqual(letter.phase, .reviewing)
        XCTAssertGreaterThan(letter.pendingCount, 0)
        XCTAssertEqual(snapshot.documents.first { $0.title == Letters.quotationTitle }?.phase, .ready)
        _ = RestoreViewModel(library: library, reply: "Reply")
        XCTAssertEqual(real.openSessionCount, 1, "listing opened a document")
        // Opening one from the list gives the same id the list showed.
        XCTAssertEqual(try library.session(for: letter.id).snapshot().title, Letters.letterTitle)
    }

    /// Delete from the list, for a document not opened since mdoc started:
    /// the list opened none, so there is no session to delete through.
    func testDeletingADocumentNotOpenedThisRun() throws {
        let path = try XCTUnwrap((try imported(core()) as? RealSession)?.path)
        let library = try core().library
        let id = try XCTUnwrap(library.snapshot().documents.first?.id)
        try library.deleteDocument(id)
        XCTAssertTrue(library.snapshot().documents.isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: path))
    }

    /// A document open this run lists as it shows: a Keep of a name its
    /// case knows is not saved (reopened, the case proposes it again), so
    /// the saved summary still counts it.
    func testAnOpenDocumentListsAsItShows() throws {
        let core = try core()
        let first = try imported(core, text: "Write to ann@example.com.")
        let email = try XCTUnwrap(try first.scan(observer: TestObserver()).candidates.first)
        let caseID = try XCTUnwrap(try first.decide(email.id, .replace, scope: .all).caseID)
        let second = try core.library.importDocument(.text("Ann: ann@example.com."), title: "Note", into: .existing(caseID))
        let read = try second.recognize(observer: TestObserver())
        let proposal = try XCTUnwrap(read.candidates.first)
        XCTAssertEqual(read.pendingCount, 1)
        XCTAssertEqual(try second.decide(proposal.id, .keep, scope: .all).pendingCount, 0)
        let row = try XCTUnwrap(core.library.snapshot().documents.first { $0.id == second.id })
        XCTAssertEqual(row.pendingCount, 0)
        XCTAssertEqual(row.candidateCount, 1)
    }

    /// A library from before the core kept "scanned": what the app recorded
    /// (#685) goes to the core once, then the app's record goes.
    func testTheAppsScannedRecordGoesToTheCoreOnce() throws {
        let session = try imported(core(), text: "# Note\n\nNothing to hide here.")
        XCTAssertEqual(try session.scan(observer: TestObserver()).phase, .ready)
        let path = try XCTUnwrap((session as? RealSession)?.path)
        // As #685 left it: the core has no record, the app has one.
        try FileManager.default.removeItem(at: storage.root.appendingPathComponent("documents.json"))
        let index = storage.root.appendingPathComponent("originals.json")
        let key = OriginalsIndex(storage: storage).key(path)
        try JSONSerialization.data(withJSONObject: [key: ["kind": "none", "files": [], "folders": [], "scanned": true]])
            .write(to: index)
        let reopened = try core()
        XCTAssertEqual(reopened.library.snapshot().documents.first?.phase, .ready)
        let record = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: index)) as? [String: Any])
        XCTAssertNil(record[key], "the app's record went")
        XCTAssertEqual(try core().library.snapshot().documents.first?.phase, .ready)
    }

    // MARK: Q12, one review tap

    /// One review tap, the core call and the app's snapshot of its result:
    /// Replace on a name, then Undo, on a longer document. Reported with
    /// the os_signpost interval "Review tap"; prints its median. Nightly
    /// only (`TEST_RUNNER_MDOC_TIMING_TESTS=1`, ADR 0112): it only measures
    /// speed.
    @MainActor
    func testAReviewTapsTime() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_TIMING_TESTS"] == "1", "timing tests run nightly")
        let sample = SampleDocument.english
        let text = Array(repeating: sample.text, count: 12).joined(separator: "\n\n")
        let session = try core().library.importDocument(.text(text), title: sample.title, into: .newCase(name: "Timing"))
        _ = try session.recognize(observer: TestObserver())
        let proposed = try session.propose(sample.names)
        let first = try XCTUnwrap(proposed.candidates.first)
        var times: [Double] = []
        for _ in 0..<20 {
            let start = DispatchTime.now()
            _ = try session.decide(first.id, .replace, scope: .this)
            times.append(Double(DispatchTime.now().uptimeNanoseconds - start.uptimeNanoseconds) / 1e6)
            _ = try session.undo()
        }
        times.sort()
        print("REVIEW-TAP mentions=\(proposed.candidates.count) bytes=\(text.utf8.count) median=\(times[times.count / 2]) ms min=\(times[0]) ms")
        measure(metrics: [XCTClockMetric()]) {
            _ = try? session.decide(first.id, .replace, scope: .this)
            _ = try? session.undo()
        }
    }
}
