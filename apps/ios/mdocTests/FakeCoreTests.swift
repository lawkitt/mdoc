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
        XCTAssertEqual(small.tier, .belowFloor)
        XCTAssertFalse(small.offers.contains { $0.purpose == .detector })
        let large = Fixture.core(modelsInstalled: false, memory: 8 * Fixture.gigabyte).setup.snapshot()
        XCTAssertEqual(large.tier, .detector)
        XCTAssertTrue(large.offers.contains { $0.purpose == .detector })
    }

    // MARK: Review bar (ADR 0098)

    private func letter() throws -> any DocumentSession {
        try Fixture.core().library.session(for: Fixture.letter)
    }

    private func mention(_ session: any DocumentSession, _ original: String) throws -> Candidate {
        try XCTUnwrap(session.snapshot().candidates.first { $0.original == original })
    }

    func testUndoAndRedoTravelOneDecisionAtATime() throws {
        let session = try letter()
        let start = session.snapshot()
        XCTAssertFalse(start.canUndo)
        let maya = try mention(session, "Maya Ellison")
        let replaced = try session.decide(maya.id, .replace, scope: .allOccurrences)
        XCTAssertTrue(replaced.canUndo)
        XCTAssertEqual(replaced.undoStep?.label, "Replace · Maya Ellison")
        let undone = try session.undo()
        XCTAssertEqual(undone.markdown, start.markdown)
        XCTAssertTrue(undone.canRedo)
        XCTAssertEqual(undone.redoStep?.kind, .replace)
        XCTAssertEqual(undone.redoStep?.candidateID, maya.id)
        let redone = try session.redo()
        XCTAssertEqual(redone.markdown, replaced.markdown)
        XCTAssertFalse(redone.canRedo)
        // A new step clears Redo.
        _ = try session.undo()
        _ = try session.decide(maya.id, .keep, scope: .justThisOne)
        XCTAssertFalse(session.snapshot().canRedo)
    }

    /// What the real core leaves alone, the fake does too: the reply across
    /// Undo, a custom alias across a category change, a kept mention under
    /// New alias, an owner set again; the last two keep Redo.
    func testTheFakeKeepsWhatTheCoreKeeps() throws {
        let session = try letter()
        let maya = try mention(session, "Maya Ellison")
        _ = try session.decide(maya.id, .replace, scope: .allOccurrences)
        _ = try session.restoreReply("Thanks, PERSON_1.")
        XCTAssertNotNil(try session.undo().reply, "Undo leaves the reply")
        _ = try session.redo()

        _ = try session.rename(maya.id, scope: .allOccurrences, to: "CLIENT")
        let choices = try session.categoryChoices(for: maya.id, scope: .allOccurrences)
        XCTAssertNil(choices.first { $0.category == .organization }?.alias, "a custom alias stays")
        let recategorized = try session.setCategory(maya.id, scope: .allOccurrences, to: .organization)
        XCTAssertEqual(recategorized.candidate(maya.id)?.alias, "CLIENT")

        let daniel = try mention(session, "Mr Okafor")
        _ = try session.decide(daniel.id, .keep, scope: .justThisOne)
        _ = try session.undo()
        _ = try session.redo()
        XCTAssertEqual(try session.newAlias(daniel.id).candidate(daniel.id)?.identityID, daniel.identityID)
        let email = try mention(session, "maya.ellison@example.com")
        _ = try session.undo()
        XCTAssertTrue(try session.setOwner(email.identityID, to: nil).canRedo)
    }

    func testRenameRefusesAnAliasAnotherNameHas() throws {
        let session = try letter()
        let maya = try mention(session, "Maya Ellison")
        let daniel = try mention(session, "Mr Okafor")
        XCTAssertThrowsError(try session.rename(maya.id, scope: .allOccurrences, to: daniel.alias)) { error in
            XCTAssertEqual(error as? CoreError, .aliasTaken(alias: daniel.alias, by: daniel.identityID))
        }
        let renamed = try session.rename(maya.id, scope: .allOccurrences, to: "CLIENT")
        XCTAssertTrue(renamed.mentions(of: maya.identityID).allSatisfy { $0.alias == "CLIENT" })
        XCTAssertEqual(renamed.undoStep?.label, "Rename · CLIENT")
        XCTAssertFalse(try session.undo().candidates.contains { $0.alias == "CLIENT" })
    }

    func testSameAsLinksAndNewAliasSplitsWithAHint() throws {
        let session = try letter()
        let ms = try mention(session, "Ms Ellison")
        let split = try session.newAlias(ms.id)
        let alone = try XCTUnwrap(split.candidate(ms.id))
        XCTAssertNotEqual(alone.identityID, ms.identityID)
        XCTAssertEqual(split.undoStep?.kind, .newAlias)
        let hint = try XCTUnwrap(split.mergeHints.first { $0.from == alone.identityID })
        XCTAssertEqual(hint.into, ms.identityID)
        XCTAssertEqual(hint.wording, "Maya Ellison")
        let linked = try session.sameAs(ms.id, scope: .allOccurrences, target: hint.into)
        XCTAssertEqual(linked.candidate(ms.id)?.identityID, ms.identityID)
        XCTAssertEqual(linked.undoStep?.label, "Same as · Maya Ellison")
        XCTAssertTrue(linked.mergeHints.isEmpty)
        // Undone, the hint is back; × hides it without a step.
        let undone = try session.undo()
        XCTAssertEqual(undone.mergeHints, [hint])
        let dismissed = try session.dismissHint(hint)
        XCTAssertTrue(dismissed.mergeHints.isEmpty)
        XCTAssertEqual(dismissed.redoStep?.kind, .sameAs)
    }

    func testCategoryChoicesOwnerAndAMarkedProposal() throws {
        let session = try letter()
        let email = try mention(session, "maya.ellison@example.com")
        let maya = try mention(session, "Maya Ellison")
        let choices = try session.categoryChoices(for: maya.id, scope: .allOccurrences)
        XCTAssertNil(choices.first { $0.category == .person }?.alias)
        XCTAssertTrue(choices.first { $0.category == .person }?.isCurrent ?? false)
        let organization = try XCTUnwrap(choices.first { $0.category == .organization }?.alias)
        XCTAssertTrue(organization.hasPrefix("ORG_"))
        XCTAssertFalse(session.snapshot().candidates.contains { $0.alias == organization })
        let owned = try session.setOwner(email.identityID, to: maya.identityID)
        XCTAssertEqual(owned.identities.first { $0.id == email.identityID }?.owner, maya.identityID)
        XCTAssertThrowsError(try session.setOwner(maya.identityID, to: email.identityID))
        let recategorized = try session.setCategory(maya.id, scope: .justThisOne, to: .organization)
        XCTAssertEqual(recategorized.candidate(maya.id)?.category, .organization)
        XCTAssertEqual(recategorized.undoStep?.label, "Category · Organization")

        let text = session.snapshot().markdown
        let range = try XCTUnwrap(text.range(of: "staircase"))
        let start = text.utf8.distance(from: text.utf8.startIndex, to: range.lowerBound)
        let marked = try session.markSelection(SourceRange(start: start, end: start + "staircase".utf8.count))
        XCTAssertTrue(marked.markdown.contains("staircase"), "a mark proposes, it does not replace")
        XCTAssertTrue(marked.candidates.contains { $0.original == "staircase" && $0.state == .proposed })
        XCTAssertEqual(marked.undoStep?.label, "Mark · staircase")
        XCTAssertFalse(try session.undo().candidates.contains { $0.original == "staircase" })
    }
}
