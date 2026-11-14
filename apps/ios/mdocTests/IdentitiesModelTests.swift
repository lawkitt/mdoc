import XCTest
@testable import mdoc

/// Identities as a drawer (2026-11-06 grill, round 2): state buttons, bulk
/// decisions and Same as with one Undo, and opening a name in the bar's
/// full view, over the real core and the seeded letter.
@MainActor
final class IdentitiesModelTests: XCTestCase {
    private func model() throws -> DocumentViewModel {
        let seeded = try Fixture.seeded()
        return try seeded.model(seeded.letter)
    }

    /// Identities wanted (a wide iPad's inspector) is not the drawer: a
    /// window that narrows never opens it over the text (ADR 0104, #674).
    /// Only opening Identities in a compact window does.
    func testWantingIdentitiesNeverOpensTheDrawer() throws {
        let model = try model()
        model.windowChanged(to: .wide)
        model.showIdentities(true)
        XCTAssertTrue(model.wantsIdentities)
        XCTAssertFalse(model.identitiesDrawerOpen, "the inspector's wish is not the drawer")
        model.windowChanged(to: .compact)
        XCTAssertFalse(model.identitiesDrawerOpen, "a window that narrows opens nothing")
        XCTAssertFalse(model.identitiesShowing)
        model.showIdentities(true)
        XCTAssertTrue(model.identitiesDrawerOpen)
        model.showIdentities(false)
        XCTAssertFalse(model.identitiesDrawerOpen)
        XCTAssertFalse(model.wantsIdentities, "closing the drawer is not wanting them")
    }

    /// Identities are hidden while editing (ADR 0110): edit mode closes
    /// the drawer and the iPad inspector, opening them waits for Done, and
    /// Done brings back the inspector that showed, never the drawer.
    func testIdentitiesHideWhileEditing() throws {
        let model = try model()
        model.windowChanged(to: .wide)
        model.showIdentities(true)
        model.enterEditMode()
        XCTAssertTrue(model.isEditing)
        XCTAssertFalse(model.identitiesShowing, "the inspector gives way")
        model.showIdentities(true)
        XCTAssertFalse(model.identitiesShowing, "nothing opens them while editing")
        model.doneEditing()
        XCTAssertTrue(model.identitiesShowing, "Done brings the inspector back")

        model.windowChanged(to: .compact)
        model.showIdentities(true)
        XCTAssertTrue(model.identitiesDrawerOpen)
        model.enterEditMode()
        XCTAssertFalse(model.identitiesDrawerOpen, "edit mode closes the drawer")
        model.doneEditing()
        XCTAssertFalse(model.identitiesDrawerOpen, "Done leaves the drawer shut over the text")
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

    /// A name dropped on another joins it (ADR 0106): the target keeps its
    /// alias, the toast says "PERSON_3 is now PERSON_1" with Undo, and one
    /// Undo takes it back. A row takes no drop of itself. (The restore
    /// wording is the core's fullest form, the target's on a tie: here the
    /// target's is the longer.)
    func testDroppingANameOnAnotherMakesThemOne() throws {
        let model = try model()
        let people = model.snapshot.identities.filter { $0.category == .person }
            .sorted { $0.primaryWording.count > $1.primaryWording.count }
        let (dragged, target) = (people[1], people[0])
        XCTAssertFalse(model.canMerge(target.id, into: target.id), "no drop on itself")
        XCTAssertFalse(model.canMerge(nil, into: target.id), "nothing dragged from this list")
        XCTAssertFalse(model.canMerge(IdentityID(rawValue: "999999"), into: target.id), "not a name here")
        XCTAssertTrue(model.canMerge(dragged.id, into: target.id))
        let before = model.snapshot.identities.count
        model.dragMerge(dragged.id, into: target.id)
        XCTAssertEqual(model.snapshot.identities.count, before - 1)
        XCTAssertNil(model.snapshot.identities.first { $0.id == dragged.id })
        let merged = try XCTUnwrap(model.snapshot.identities.first { $0.id == target.id })
        XCTAssertEqual(merged.alias, target.alias, "the target keeps its alias")
        XCTAssertEqual(merged.primaryWording, target.primaryWording, "and its restore wording")
        XCTAssertEqual(model.toast?.text, "\(dragged.alias) is now \(target.alias)")
        XCTAssertEqual(model.toast?.actionTitle, "Undo")
        model.toast?.action?()
        XCTAssertEqual(model.snapshot.identities.count, before, "one undo step")
        XCTAssertNotNil(model.snapshot.identities.first { $0.id == dragged.id })
    }

    /// Another category takes the target's, as desktop's drag (ADR 0025).
    func testADroppedNameTakesTheTargetsCategory() throws {
        let model = try model()
        let dragged = try XCTUnwrap(model.snapshot.identities.first { $0.category == .organization })
        let target = try XCTUnwrap(model.snapshot.identities.first { $0.category == .person })
        let mentions = model.snapshot.candidates.filter { $0.identityID == dragged.id }.map(\.range)
        XCTAssertFalse(mentions.isEmpty)
        model.dragMerge(dragged.id, into: target.id)
        for range in mentions {
            let now = try XCTUnwrap(model.snapshot.candidates.first { $0.range == range })
            XCTAssertEqual(now.identityID, target.id)
            XCTAssertEqual(now.category, .person)
        }
    }

    /// The Select demo (ADR 0114) is due with two names or more, until the
    /// first drag-merge or Same as; Reset tips brings it back, and it is
    /// off where TipKit hides the tips (UI tests).
    func testTheSelectDemoIsDueUntilTheFirstMerge() throws {
        XCTAssertEqual(
            String(localized: ReviewHint.dragMerge.text),
            "Tick names to act on them, or drag ≡ onto another name to make them one")
        Hints.shared.setAvailableForTesting(Set(ReviewHint.allCases))
        defer { Hints.shared.setAvailableForTesting([]) }
        let model = try model()
        XCTAssertTrue(model.offersSelectDemo, "the letter has several names")
        let people = model.snapshot.identities.filter { $0.category == .person }
        XCTAssertGreaterThanOrEqual(people.count, 4)
        model.dragMerge(people[1].id, into: people[0].id)
        XCTAssertFalse(model.offersSelectDemo, "learned by the first drag-merge")
        model.undo()
        XCTAssertFalse(model.offersSelectDemo, "gone for good")
        ReviewHint.resetAll()
        XCTAssertTrue(model.offersSelectDemo)
        // Select › Same as learns it too.
        model.merge([people[2].id, people[3].id])
        XCTAssertFalse(model.offersSelectDemo, "learned by Same as")
        Hints.shared.setAvailableForTesting([])
        XCTAssertFalse(model.offersSelectDemo, "off where TipKit hides the tips")
    }

    /// One name has nothing to drag onto: no demo. Two of any categories
    /// have.
    func testTheSelectDemoNeedsTwoNames() throws {
        Hints.shared.setAvailableForTesting(Set(ReviewHint.allCases))
        defer { Hints.shared.setAvailableForTesting([]) }
        let seeded = try Fixture.seeded()
        let one = try seeded.library.importDocument(
            .text("Write to maya.ellison@example.com."), title: "One name", into: .existing(seeded.ellison))
        _ = try one.recognize(observer: TestObserver())
        _ = try one.propose(Letters.names.filter { $0.category == .email })
        let alone = try seeded.model(one.id)
        XCTAssertEqual(alone.snapshot.identities.count, 1)
        XCTAssertFalse(alone.offersSelectDemo)
        let two = try seeded.library.importDocument(
            .text("Write to maya.ellison@example.com or call 01632 960 418."), title: "Two names", into: .existing(seeded.ellison))
        _ = try two.recognize(observer: TestObserver())
        _ = try two.propose(Letters.names.filter { $0.category == .email || $0.category == .phone })
        XCTAssertTrue(try seeded.model(two.id).offersSelectDemo, "names of two categories are enough")
    }

    /// While the demo shows, the review bar's hints wait: one at a time.
    func testTheDemoKeepsTheBarsHintsBack() {
        Hints.shared.setAvailableForTesting(Set(ReviewHint.allCases))
        defer { Hints.shared.setAvailableForTesting([]) }
        Hints.shared.show(.dragMerge)
        XCTAssertEqual(ReviewHint.teachable, [.dragMerge])
        Hints.shared.learn(.dragMerge, closed: true)
        XCTAssertEqual(ReviewHint.teachable, [.swipe, .longPress, .adjustWords], "dismissed: learned, and the others are back")
    }

    func testCountsSayHowManyAreLeft() throws {
        let model = try model()
        let counts = model.identityCounts
        XCTAssertEqual(counts.total, model.snapshot.identities.count)
        XCTAssertEqual(counts.toReview, model.snapshot.identities.filter { $0.state == .proposed }.count)
    }
}
