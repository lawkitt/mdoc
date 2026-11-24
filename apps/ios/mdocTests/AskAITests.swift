import UIKit
import XCTest
@testable import mdoc

/// Ask AI and the "Paste reply?" banner (#1164 decision 11, Milestone 2
/// item 18): what Ask AI hands the share sheet, when it refuses, and when
/// the banner shows and what it restores.
@MainActor
final class AskAITests: XCTestCase {
    /// The letter with every proposal replaced: Ready.
    private func readyLetter() throws -> (DocumentViewModel, Seeded) {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        model.replaceAllPending()
        return (model, seeded)
    }

    /// Ask AI on a Ready document whose title holds a client's name: the
    /// sheet gets the Vault's text with aliases, titled by the file's name
    /// with names replaced; neither carries an original.
    func testAskAISharesTheTextAndTitleWithNamesReplaced() throws {
        let (core, storage) = try Fixture.core()
        let ellison = try Fixture.createProject(Letters.ellisonProject, in: core)
        let session = try core.library.importDocument(.text(Letters.letter), title: "Maya Ellison letter", into: ellison)
        _ = try session.recognize(observer: TestObserver())
        _ = try session.propose(Letters.names)
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        model.replaceAllPending()
        let maya = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Maya Ellison" })

        model.askAI()

        XCTAssertNil(model.errorMessage)
        XCTAssertNil(model.share, "Share's own sheet stays closed")
        let item = try XCTUnwrap(model.askAIShare?.items.first as? TitledText)
        XCTAssertEqual(model.askAIShare?.items.count, 1)
        XCTAssertTrue(item.text.contains(maya.alias))
        XCTAssertEqual(item.title, "\(maya.alias) letter")
        for original in Letters.names.map(\.wording) {
            XCTAssertFalse(item.text.contains(original), original)
            XCTAssertFalse(item.title.contains(original), original)
        }
        XCTAssertNil(item.file, "no file: Save to Files is left out")
        // What Copy for AI copies, byte for byte.
        var copied: String?
        model.copyForAI { copied = $0 }
        XCTAssertEqual(item.text, copied)
        XCTAssertEqual(model.replyOffer, .sharing)
    }

    /// Until the document is Ready, Ask AI refuses with why, opens no
    /// sheet and waits for no answer.
    func testAskAIIsRefusedUntilTheDocumentIsReady() throws {
        let seeded = try Fixture.seeded()
        let model = try seeded.model(seeded.letter)
        XCTAssertNotNil(model.handOffBlockedReason)

        model.askAI()

        XCTAssertNil(model.askAIShare)
        XCTAssertNotNil(model.errorMessage)
        XCTAssertNil(model.replyOffer)
        model.appLeft()
        model.appReturned()
        XCTAssertFalse(model.showsReplyBanner)
    }

    /// Ask AI's sheet leaves out the system's entries that are no AI app.
    func testAskAILeavesOutEntriesThatAreNoAIApp() {
        let excluded = DocumentViewModel.askAIExcludedActivities
        for type in [UIActivity.ActivityType.mail, .message, .airDrop, TitledText.saveToFiles] {
            XCTAssertTrue(excluded.contains(type), type.rawValue)
        }
        XCTAssertFalse(excluded.contains(.copyToPasteboard))
    }

    /// The text went to an app, the user went to it and came back: the
    /// banner shows on return, not before.
    func testTheBannerShowsOnReturnFromTheAIApp() throws {
        let (model, _) = try readyLetter()
        model.askAI()
        model.askAIFinished(completed: true)
        XCTAssertNil(model.askAIShare)
        XCTAssertEqual(model.replyOffer, .awaitingReturn)
        XCTAssertFalse(model.showsReplyBanner, "not before mdoc is left")
        model.appReturned()
        XCTAssertFalse(model.showsReplyBanner, "back without having left")

        model.appLeft()
        XCTAssertFalse(model.showsReplyBanner)
        model.appReturned()
        XCTAssertTrue(model.showsReplyBanner)
    }

    /// An app that opens in front of mdoc may end the sheet only once mdoc
    /// is back: the banner shows at once then.
    func testTheBannerShowsWhenTheSheetEndsAfterTheReturn() throws {
        let (model, _) = try readyLetter()
        model.askAI()
        model.appLeft()
        model.appReturned()
        XCTAssertFalse(model.showsReplyBanner, "the sheet is still up")
        model.askAIFinished(completed: true)
        XCTAssertTrue(model.showsReplyBanner)
    }

    /// Cancelled, or Share rather than Ask AI: no banner.
    func testNoBannerWithoutAnAskAIThatWentThrough() throws {
        let (model, _) = try readyLetter()
        model.askAI()
        model.askAIFinished(completed: false)
        XCTAssertNil(model.replyOffer)
        model.appLeft()
        model.appReturned()
        XCTAssertFalse(model.showsReplyBanner)

        model.share(asFile: false)
        model.appLeft()
        model.appReturned()
        XCTAssertFalse(model.showsReplyBanner)
    }

    /// The banner's paste restores the answer with the real names, opens
    /// it on the Reply pane and goes; × puts the banner away and leaves
    /// nothing restored.
    func testTheBannerRestoresAPastedReply() throws {
        let (model, _) = try readyLetter()
        let maya = try XCTUnwrap(model.snapshot.candidates.first { $0.original == "Maya Ellison" }?.alias)
        model.askAI()
        model.askAIFinished(completed: true)
        model.appLeft()
        model.appReturned()
        XCTAssertTrue(model.showsReplyBanner)

        model.restore(reply: "   ")
        XCTAssertTrue(model.showsReplyBanner, "nothing pasted, nothing restored")
        model.restore(reply: "\(maya) should reply by Friday.")

        XCTAssertEqual(model.snapshot.reply?.restored.text, "Maya Ellison should reply by Friday.")
        XCTAssertEqual(model.pane, .reply)
        XCTAssertFalse(model.showsReplyBanner)

        model.askAI()
        model.askAIFinished(completed: true)
        model.appLeft()
        model.appReturned()
        model.dismissReplyBanner()
        XCTAssertFalse(model.showsReplyBanner)
        XCTAssertEqual(model.snapshot.replies.count, 1)
    }

    /// Settings › AI connector (preview) is off until turned on, and kept.
    func testThePreviewIsOffByDefault() {
        let preferences = Fixture.preferences()
        XCTAssertFalse(preferences.aiConnectorPreview)
        preferences.aiConnectorPreview = true
        XCTAssertTrue(preferences.aiConnectorPreview)
        preferences.reset()
        XCTAssertFalse(preferences.aiConnectorPreview)
    }
}
