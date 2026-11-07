import XCTest
@testable import mdoc

/// Identities as a drawer (2026-11-06 grill, round 2): state buttons, bulk
/// decisions and Same as with one Undo, and opening a name in the bar's
/// full view, over the fake core.
@MainActor
final class IdentitiesModelTests: XCTestCase {
    private func model() throws -> DocumentViewModel {
        let core = Fixture.core()
        let session = try core.library.session(for: Fixture.letter)
        return DocumentViewModel(session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: MdocStorage(root: FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)))
    }

    func testTheStateButtonReplacesThenSwitchesToKept() throws {
        let model = try model()
        let identity = try XCTUnwrap(model.snapshot.identities.first { $0.state == .proposed })
        model.toggle(identity)
        XCTAssertEqual(model.snapshot.identities.first { $0.id == identity.id }?.state, .replaced)
        XCTAssertNotNil(model.toast?.action, "Undo in the toast")
        let replaced = try XCTUnwrap(model.snapshot.identities.first { $0.id == identity.id })
        model.toggle(replaced)
        XCTAssertEqual(model.snapshot.identities.first { $0.id == identity.id }?.state, .kept)
    }

    func testSelectReplaceIsUndoneByOneTap() throws {
        let model = try model()
        let ids = model.snapshot.identities.filter { $0.state == .proposed }.prefix(2).map(\.id)
        XCTAssertEqual(ids.count, 2)
        model.decide(identities: Array(ids), .replace)
        XCTAssertTrue(ids.allSatisfy { id in model.snapshot.identities.first { $0.id == id }?.state == .replaced })
        model.toast?.action?()
        XCTAssertTrue(ids.allSatisfy { id in model.snapshot.identities.first { $0.id == id }?.state == .proposed })
    }

    func testSameAsMergesIntoTheNameMentionedMost() throws {
        let model = try model()
        let people = model.snapshot.identities.filter { $0.category == .person }
        XCTAssertGreaterThanOrEqual(people.count, 2)
        let before = model.snapshot.identities.count
        let target = try XCTUnwrap(people.max { $0.mentionCount < $1.mentionCount })
        model.merge(people.prefix(2).map(\.id) + (people.prefix(2).contains(target) ? [] : [target.id]))
        XCTAssertLessThan(model.snapshot.identities.count, before)
        XCTAssertNotNil(model.snapshot.identities.first { $0.id == target.id }, "the target stays")
        model.toast?.action?()
        XCTAssertEqual(model.snapshot.identities.count, before)
    }

    func testOpeningANameShowsTheBarsFullView() throws {
        let model = try model()
        let identity = try XCTUnwrap(model.snapshot.identities.first { $0.state == .proposed })
        model.openIdentity(identity.id)
        XCTAssertEqual(model.reviewedCandidate?.identityID, identity.id)
        XCTAssertTrue(model.barExpanded)
    }

    func testCountsSayHowManyAreLeft() throws {
        let model = try model()
        let counts = model.identityCounts
        XCTAssertEqual(counts.total, model.snapshot.identities.count)
        XCTAssertEqual(counts.toReview, model.snapshot.identities.filter { $0.state == .proposed }.count)
    }
}
