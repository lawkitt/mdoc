import XCTest
@testable import mdoc

/// The fake must keep the contract the views rely on, so the real core can
/// replace it.
final class FakeCoreTests: XCTestCase {
    func testSeedHasTwoCasesAndALetterToReview() throws {
        let library = Fixture.core().library
        let snapshot = library.snapshot()
        XCTAssertEqual(Set(snapshot.cases.map(\.id)), [Fixture.ellison, Fixture.harbour])
        let letter = try library.session(for: Fixture.letter).snapshot()
        XCTAssertEqual(letter.phase, .reviewing)
        XCTAssertGreaterThan(letter.pendingCount, 5)
        XCTAssertEqual(snapshot.currentCase, Fixture.ellison)
    }

    func testCandidateRangesPointAtTheirWording() throws {
        let letter = try Fixture.core().library.session(for: Fixture.letter).snapshot()
        let bytes = Array(letter.markdown.utf8)
        for candidate in letter.candidates {
            let shown = String(decoding: bytes[candidate.range.start..<candidate.range.end], as: UTF8.self)
            XCTAssertEqual(shown, candidate.original)
        }
    }

    func testReplaceAllOccurrencesPutsTheAliasInTheText() throws {
        let session = try Fixture.core().library.session(for: Fixture.letter)
        let maya = try XCTUnwrap(session.snapshot().candidates.first { $0.original == "Maya Ellison" })
        let after = try session.decide(maya.id, .replace, scope: .allOccurrences)
        XCTAssertFalse(after.markdown.contains("Maya Ellison"))
        XCTAssertTrue(after.markdown.contains("PERSON_1"))
        XCTAssertTrue(after.mentions(of: maya.identityID).allSatisfy { $0.state == .replaced })
    }

    func testJustThisOneLeavesOtherMentions() throws {
        let session = try Fixture.core().library.session(for: Fixture.letter)
        let mentions = session.snapshot().candidates.filter { $0.original == "Maya Ellison" }
        XCTAssertGreaterThan(mentions.count, 1)
        let after = try session.decide(mentions[0].id, .keep, scope: .justThisOne)
        XCTAssertEqual(after.candidate(mentions[0].id)?.state, .kept)
        XCTAssertEqual(after.candidate(mentions[1].id)?.state, .proposed)
    }

    func testCopyForAIWaitsForReviewThenAddsTheNote() throws {
        let library = Fixture.core().library
        let session = try library.session(for: Fixture.letter)
        XCTAssertThrowsError(try session.copyForAI()) { error in
            XCTAssertEqual(error as? CoreError, .pendingReview(session.snapshot().pendingCount))
        }
        _ = try session.replaceAllPending()
        let handOff = try session.copyForAI()
        XCTAssertTrue(handOff.text.hasPrefix("Note: names and other identifying details"))
        XCTAssertFalse(handOff.text.contains("Maya Ellison"))
        XCTAssertEqual(handOff.fileName, "Letter before claim.md")
        XCTAssertEqual(library.snapshot().currentCase, Fixture.ellison)
    }

    func testRestorePutsBackKnownAliasesAndFlagsUnknownOnes() throws {
        let library = Fixture.core().library
        let result = try library.restore(reply: "PERSON_1 and person-2 owe ORG_9 a reply.", in: Fixture.ellison)
        XCTAssertEqual(result.text, "Maya Ellison and Daniel Okafor owe ORG_9 a reply.")
        XCTAssertEqual(result.restoredCount, 2)
        XCTAssertEqual(result.unresolvedCount, 1)
    }

    func testBelowTheFloorTheRulesRunWithoutAModel() throws {
        let core = Fixture.core(modelsInstalled: false, memory: 4 * Fixture.gigabyte)
        let session = try core.library.session(for: Fixture.quotation)
        let letter = try core.library.session(for: Fixture.letter)
        let scanned = try letter.scan(observer: TestObserver())
        XCTAssertFalse(scanned.candidates.isEmpty)
        XCTAssertTrue(scanned.candidates.allSatisfy { [.email, .phone, .bank].contains($0.category) })
        XCTAssertNoThrow(try session.scan(observer: TestObserver()))
    }

    func testRestoreCountsDistinctValues() throws {
        let result = try Fixture.core().library.restore(reply: "PERSON_1, PERSON_1, ORG_9, ORG_9.", in: Fixture.ellison)
        XCTAssertEqual(result.restoredCount, 1)
        XCTAssertEqual(result.unresolvedCount, 1)
    }

    func testRestoreNeverUsesAnotherCase() throws {
        let library = Fixture.core().library
        let result = try library.restore(reply: "PERSON_1 signed.", in: Fixture.harbour)
        XCTAssertNotEqual(result.text, "Maya Ellison signed.")
    }

    func testRecognizeKeepsFinishedPagesWhenCancelled() throws {
        let session = try Fixture.core(timing: FakeCore.Timing(page: 0.2, scan: 0, install: 0))
            .library.session(for: Fixture.tenancy)
        let observer = TestObserver()
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.3) { observer.cancel() }
        XCTAssertThrowsError(try session.recognize(observer: observer))
        let stopped = session.snapshot()
        XCTAssertEqual(stopped.phase, .recognizing)
        XCTAssertGreaterThan(stopped.pagesDone, 1)
        let finished = try session.recognize(observer: TestObserver())
        XCTAssertEqual(finished.pagesDone, finished.pageCount)
        XCTAssertEqual(finished.phase, .unscanned)
    }

    func testScanNeedsTheDetector() throws {
        let session = try Fixture.core(modelsInstalled: false).library.session(for: Fixture.letter)
        XCTAssertThrowsError(try session.scan(observer: TestObserver())) { error in
            XCTAssertEqual(error as? CoreError, .modelMissing)
        }
    }

    func testManualReplacementCoversEveryRepeat() throws {
        let session = try Fixture.core().library.session(for: Fixture.letter)
        let text = session.snapshot().markdown
        let wording = "staircase"
        let range = try XCTUnwrap(text.range(of: wording))
        let start = text.utf8.distance(from: text.utf8.startIndex, to: range.lowerBound)
        let after = try session.addReplacement(SourceRange(start: start, end: start + wording.utf8.count), category: .other)
        XCTAssertFalse(after.markdown.contains(wording))
        XCTAssertTrue(after.markdown.contains("REDACTED_1"))
    }

    func testImportIntoANewCaseStartsRecognizing() throws {
        let library = Fixture.core().library
        let session = try library.importDocument(.scan([URL(fileURLWithPath: "/tmp/a.jpg")]), title: "Scan", into: .newCase(name: "Scan"))
        XCTAssertEqual(session.snapshot().phase, .recognizing)
        XCTAssertEqual(library.snapshot().cases.count, 3)
    }

    func testSetupOffersOnlyWhatFitsTheMemory() {
        let small = Fixture.core(modelsInstalled: false, memory: 4 * Fixture.gigabyte).setup.snapshot()
        XCTAssertEqual(small.tier, .rulesOnly)
        XCTAssertFalse(small.offers.contains { $0.purpose == .detector })
        let large = Fixture.core(modelsInstalled: false, memory: 8 * Fixture.gigabyte).setup.snapshot()
        XCTAssertEqual(large.tier, .detector)
        XCTAssertTrue(large.offers.contains { $0.purpose == .detector })
    }
}
