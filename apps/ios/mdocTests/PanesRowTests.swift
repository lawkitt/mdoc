import XCTest
@testable import mdoc

/// The panes' row and the Reply pane's bar (ADR 0111), on the real core.
@MainActor
final class PanesRowTests: XCTestCase {
    private typealias Shown = DocumentViewModel.Shown

    /// The row shows only with more than one pane: an original or a reply
    /// on iPhone, a choice for the right column on iPad.
    func testTheRowShowsWithMoreThanOnePane() throws {
        let seeded = try Fixture.seeded()
        let letter = try seeded.model(seeded.letter)
        XCTAssertFalse(letter.hasOriginal)
        XCTAssertEqual(letter.panes(in: .compact), [.pane(.text)])
        XCTAssertFalse(letter.showsPanesRow(in: .compact), "one pane: the title alone")

        let scan = try seeded.model(seeded.tenancy)
        XCTAssertEqual(scan.panes(in: .compact), [.pane(.text), .pane(.original)])
        XCTAssertTrue(scan.showsPanesRow(in: .compact))

        letter.restore(reply: "Thanks.")
        XCTAssertEqual(letter.panes(in: .compact), [.pane(.text), .pane(.reply)])
        XCTAssertTrue(letter.showsPanesRow(in: .compact))
        XCTAssertEqual(letter.shown(in: .compact), .pane(.reply))
    }

    /// In edit mode the row stays, so the text can be checked against the
    /// Original; only Identities leave it (ADR 0110).
    func testTheRowStaysInEditMode() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.quotation)
        model.restore(reply: "Thanks.")
        model.show(.text)
        model.enterEditMode()
        XCTAssertTrue(model.isEditing)
        XCTAssertTrue(model.showsPanesRow(in: .compact))
        XCTAssertEqual(model.panes(in: .compact), [.pane(.text), .pane(.reply)])

        model.doneEditing()
        model.windowChanged(to: .narrow)
        XCTAssertEqual(model.panes(in: .narrow), [.pane(.original), .identities, .pane(.reply)])
        model.enterEditMode()
        XCTAssertEqual(model.panes(in: .narrow), [.pane(.original), .pane(.reply)])
    }

    /// iPad's right column: one choice in a wide window is no row; the row
    /// picks Identities there in a narrow one, and shows the original
    /// where the text would be.
    func testTheRowPicksTheRightColumnOnIPad() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.tenancy)
        model.windowChanged(to: .wide)
        XCTAssertEqual(model.panes(in: .wide), [.pane(.original)])
        XCTAssertFalse(model.showsPanesRow(in: .wide))
        XCTAssertEqual(model.shown(in: .wide), .pane(.original))

        model.windowChanged(to: .narrow)
        XCTAssertEqual(model.panes(in: .narrow), [.pane(.original), .identities])
        model.show(Shown.identities)
        XCTAssertEqual(model.shown(in: .narrow), .identities)
        XCTAssertTrue(model.identitiesShowing)
        model.show(Shown.pane(.original))
        XCTAssertEqual(model.shown(in: .narrow), .pane(.original))
        XCTAssertFalse(model.identitiesShowing)
    }

    /// On iPhone the Reply pane covers the text, so its bar shares and
    /// copies the reply in place of Review and Replace N, and the review
    /// bar stays away; back on the text they return.
    func testTheReplyPaneHidesReviewAndReplaceOnIPhone() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        guard case .replaceAll = model.primaryAction else {
            return XCTFail("expected Replace N, got \(model.primaryAction)")
        }
        XCTAssertFalse(model.barActsOnReply)
        model.restore(reply: "Thanks.")
        XCTAssertEqual(model.pane, .reply)
        XCTAssertTrue(model.replyCoversText)
        XCTAssertTrue(model.barActsOnReply, "Share and Copy of the reply, not Review and Replace N")

        model.show(.text)
        XCTAssertFalse(model.replyCoversText)
        XCTAssertFalse(model.barActsOnReply, "Review and Replace N again")

        // A review in progress gives way to the reply's bar the same way.
        model.beginReview()
        XCTAssertTrue(model.isReviewing)
        model.show(.reply)
        XCTAssertTrue(model.replyCoversText)
        XCTAssertTrue(model.barActsOnReply)

        var copied: String?
        model.copyReply { copied = $0 }
        XCTAssertEqual(copied, "Thanks.")
        XCTAssertNotNil(model.notice, "Copy says so")
        model.shareReply()
        XCTAssertNotNil(model.share)
    }

    /// A name tapped in Identities while the Reply pane covers the text
    /// opens its review on the Text pane: under the reply nothing would
    /// show (ADR 0103's full view, ADR 0111).
    func testANameTappedInIdentitiesFromReplyShowsTheText() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        model.restore(reply: "Thanks.")
        XCTAssertTrue(model.replyCoversText)
        model.showIdentities(true)
        let identity = try XCTUnwrap(model.snapshot.identities.first)
        model.showIdentities(false)
        model.openIdentity(identity.id)
        XCTAssertEqual(model.pane, .text)
        XCTAssertTrue(model.isReviewing)
        XCTAssertTrue(model.barExpanded, "the bar's full view")
        XCTAssertFalse(model.replyCoversText, "the review bar shows")
        XCTAssertFalse(model.barActsOnReply)
    }

    /// Review this identity from the Reply pane does the same; so does a
    /// tap on a name and the Review button, from the Original too.
    func testAFocusedReviewFromReplyShowsTheText() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        model.restore(reply: "Thanks.")
        let identity = try XCTUnwrap(model.snapshot.identities.first)
        model.beginReview(focus: .identity(identity.id))
        XCTAssertEqual(model.pane, .text)
        XCTAssertTrue(model.isReviewing)
        XCTAssertEqual(model.focus, .identity(identity.id))
        XCTAssertFalse(model.replyCoversText)

        model.endReview()
        model.show(.reply)
        let candidate = try XCTUnwrap(model.snapshot.candidates.first)
        model.open(candidate.id)
        XCTAssertEqual(model.pane, .text)
        XCTAssertEqual(model.reviewing, candidate.id)
    }

    /// Beside the text (iPad) the reply stays where it is when review
    /// starts: the bar shows under both.
    func testReviewBesideTheReplyKeepsThePaneOnIPad() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        model.windowChanged(to: .wide)
        model.restore(reply: "Thanks.")
        let identity = try XCTUnwrap(model.snapshot.identities.first)
        model.openIdentity(identity.id)
        XCTAssertEqual(model.pane, .reply)
        XCTAssertTrue(model.isReviewing)
        XCTAssertFalse(model.replyCoversText)
    }

    /// On iPad the text stands beside the reply: Review and Replace N stay
    /// until every name is decided, then the bar follows the pane
    /// (ADR 0087).
    func testTheBarKeepsReviewBesideTheReplyOnIPad() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        model.windowChanged(to: .wide)
        model.restore(reply: "Thanks.")
        XCTAssertEqual(model.pane, .reply)
        XCTAssertFalse(model.replyCoversText)
        XCTAssertFalse(model.barActsOnReply)
        model.replaceAllPending()
        XCTAssertEqual(model.primaryAction, .handOff)
        XCTAssertTrue(model.barActsOnReply)
        model.show(.original)
        XCTAssertFalse(model.barActsOnReply)
    }
}
