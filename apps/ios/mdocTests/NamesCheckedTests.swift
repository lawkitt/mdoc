import enum MdocFFI.Readiness
import UIKit
import XCTest
@testable import mdoc

/// "I checked the names myself" (Vault & Projects item 48): below the
/// floor, every AI exit of a document never scanned asks first, goes on
/// once the user says so, hands out nothing on Cancel, and asks again only
/// after its text changes. Above the floor nothing asks.
@MainActor
final class NamesCheckedTests: XCTestCase {
    private static let text = "Lunch notes\n\nThe park is lovely today."

    /// A pasted note with no names in it, read and saved, on the seeded
    /// library at `memory`.
    private func pasted(memory: UInt64 = Fixture.belowFloor) throws -> (DocumentViewModel, Seeded) {
        let seeded = try Fixture.seeded(memory: memory)
        let session = try seeded.library.importDocument(.text(Self.text), title: "Lunch", into: nil)
        _ = try session.recognize(observer: TestObserver())
        let model = try seeded.model(session.id, findsNames: memory != Fixture.belowFloor)
        return (model, seeded)
    }

    /// One AI exit of the document's own: its tap, and what it handed out.
    private struct Exit {
        let name: String
        let tap: @MainActor (DocumentViewModel, @escaping @MainActor (String) -> Void) -> Void
        let handed: @MainActor (DocumentViewModel, String?) -> Bool
    }

    private static let exits: [Exit] = [
        Exit(name: "Copy for AI", tap: { $0.copyForAI(to: $1) }, handed: { _, copied in copied != nil }),
        Exit(name: "Share", tap: { model, _ in model.share(asFile: false) }, handed: { model, _ in model.share != nil }),
        Exit(name: "Share as file", tap: { model, _ in model.share(asFile: true) }, handed: { model, _ in model.share != nil }),
        Exit(name: "Ask AI", tap: { model, _ in model.askAI() }, handed: { model, _ in model.askAIShare != nil }),
    ]

    private func readiness(_ model: DocumentViewModel, _ seeded: Seeded) throws -> Readiness {
        let library = try XCTUnwrap(seeded.library as? RealLibrary)
        let id = try XCTUnwrap((model.session as? RealSession)?.vaultID)
        return try library.vault.readiness(id: id)
    }

    /// Each exit asks, with I checked the names myself and Cancel, and
    /// hands out nothing; confirming goes on with the same exit.
    func testEachExitAsksThenGoesOnOnceConfirmed() throws {
        for exit in Self.exits {
            let (model, seeded) = try pasted()
            XCTAssertNil(model.handOffRefusal, "\(exit.name): the bar offers Share")
            var copied: String?
            exit.tap(model) { copied = $0 }
            XCTAssertEqual(model.refusal, .namesNotChecked, exit.name)
            XCTAssertFalse(exit.handed(model, copied), "\(exit.name): handed out unchecked")
            XCTAssertEqual(try readiness(model, seeded), .notReviewed, exit.name)
            let refusal = try XCTUnwrap(model.refusal)
            XCTAssertEqual(model.step(for: refusal), .namesChecked)
            XCTAssertEqual(HandOffRefusal.Step.namesChecked.title, "I checked the names myself")
            XCTAssertEqual(refusal.dismissTitle, "Cancel")

            model.take(.namesChecked)
            XCTAssertNil(model.refusal, exit.name)
            XCTAssertTrue(exit.handed(model, copied), "\(exit.name): did not go on")
            XCTAssertEqual(try readiness(model, seeded), .ready, exit.name)
            if exit.name == "Copy for AI" { XCTAssertEqual(copied, Self.text) }
        }
    }

    /// Cancel hands out nothing, and the next tap asks again.
    func testCancelHandsOutNothing() throws {
        let (model, seeded) = try pasted()
        var copied: String?
        model.copyForAI { copied = $0 }
        XCTAssertEqual(model.refusal, .namesNotChecked)
        model.refusal = nil  // Cancel
        XCTAssertNil(copied)
        XCTAssertEqual(try readiness(model, seeded), .notReviewed)

        model.copyForAI { copied = $0 }
        XCTAssertEqual(model.refusal, .namesNotChecked, "asks again")
        XCTAssertNil(copied)
    }

    /// Once checked it goes at once; a typed edit asks again, Undo back to
    /// the checked text does not, and a Replace asks again.
    func testAnEditAsksAgain() throws {
        let (model, _) = try pasted()
        var copied: String?
        model.copyForAI { copied = $0 }
        model.take(.namesChecked)
        XCTAssertNotNil(copied)
        copied = nil
        model.copyForAI { copied = $0 }
        XCTAssertNil(model.refusal, "asked again of unchanged text")
        XCTAssertNotNil(copied)

        let end = model.snapshot.markdown.utf8.count
        _ = try model.session.type(SourceRange(start: end, end: end), " Indeed.")
        model.session.endTyping()
        model.applyReview(model.session.snapshot())
        copied = nil
        model.copyForAI { copied = $0 }
        XCTAssertEqual(model.refusal, .namesNotChecked, "the typed text goes unchecked")
        XCTAssertNil(copied)
        model.refusal = nil

        model.undoTapped()
        XCTAssertEqual(model.snapshot.markdown, Self.text)
        model.copyForAI { copied = $0 }
        XCTAssertNil(model.refusal, "Undo back to the checked text asks again")
        XCTAssertEqual(copied, Self.text)

        let park = try XCTUnwrap(Self.text.utf8Range(of: "park"))
        model.markSelection(park, as: .address)
        model.replaceAllPending()
        XCTAssertFalse(model.snapshot.markdown.contains("park"))
        copied = nil
        model.copyForAI { copied = $0 }
        XCTAssertEqual(model.refusal, .namesNotChecked, "the replaced text goes unchecked")
        XCTAssertNil(copied)
    }

    /// The text changed while the question was up: the answer checks
    /// nothing, nothing leaves, and the exit asks again of the new text;
    /// answered, it hands out the new text.
    func testATextChangedWhileAskedAsksAgain() throws {
        let (model, seeded) = try pasted()
        var copied: String?
        model.copyForAI { copied = $0 }
        XCTAssertEqual(model.refusal, .namesNotChecked)

        let end = model.snapshot.markdown.utf8.count
        _ = try model.session.type(SourceRange(start: end, end: end), " Indeed.")
        model.session.endTyping()
        model.take(.namesChecked)
        XCTAssertNil(copied, "copied text nobody was asked of")
        XCTAssertEqual(model.refusal, .namesNotChecked, "no new question")
        XCTAssertEqual(try readiness(model, seeded), .notReviewed)

        model.take(.namesChecked)
        XCTAssertNil(model.refusal)
        XCTAssertEqual(copied, Self.text + " Indeed.")
    }

    /// Checked, saved, then opened again in a later run: unchanged, it does
    /// not ask.
    func testUnchangedTextReopenedDoesNotAsk() throws {
        let (model, seeded) = try pasted()
        model.copyForAI { _ in }
        model.take(.namesChecked)
        let reopened = try reopen(model, seeded)
        var copied: String?
        reopened.copyForAI { copied = $0 }
        XCTAssertNil(reopened.refusal)
        XCTAssertEqual(copied, Self.text)
    }

    /// A record saved not checked asks once, opened again too, and never
    /// again after. (A record an older build saved Ready by hand reads the
    /// same: `Vault::readiness` says NotReviewed, which the core's
    /// `a_document_saved_ready_by_hand_before_the_check_asks_once` pins;
    /// the app can't write one.)
    func testASavedRecordNotCheckedAsksOnce() throws {
        let (model, seeded) = try pasted()
        try model.session.saveChanges()
        let first = try reopen(model, seeded)
        var copied: String?
        first.copyForAI { copied = $0 }
        XCTAssertEqual(first.refusal, .namesNotChecked)
        first.take(.namesChecked)
        XCTAssertNotNil(copied)

        let second = try reopen(first, seeded)
        copied = nil
        second.copyForAI { copied = $0 }
        XCTAssertNil(second.refusal, "asked twice")
        XCTAssertNotNil(copied)
    }

    /// A row's Share asks from where it was tapped, and I checked the names
    /// myself goes on with the same Share there, without opening the
    /// document.
    func testARowsShareAsksAndGoesOn() throws {
        let (model, seeded) = try pasted()
        let id = model.session.id
        let app = Fixture.app(seeded)
        app.share(id)
        guard case .refusal(id, .namesNotChecked)? = app.navigator.presentation else {
            return XCTFail("\(String(describing: app.navigator.presentation))")
        }
        XCTAssertEqual(app.step(for: .namesNotChecked, of: id), .namesChecked)

        app.take(.namesChecked, on: id)
        guard case .share? = app.navigator.presentation else {
            return XCTFail("\(String(describing: app.navigator.presentation))")
        }
        XCTAssertNil(app.navigator.openDocument, "opened the document")
    }

    /// Above the floor nothing asks: a document never scanned waits for
    /// Find names, and one scanned goes at once.
    func testAboveTheFloorNeverAsks() throws {
        let (model, seeded) = try pasted(memory: Fixture.detectorTier)
        XCTAssertEqual(model.handOffRefusal, .notScanned)
        model.copyForAI { _ in XCTFail("copied unscanned") }
        XCTAssertNotNil(model.refusal)
        XCTAssertNotEqual(model.refusal, .namesNotChecked)

        let quotation = try seeded.model(seeded.quotation)
        var copied: String?
        quotation.copyForAI { copied = $0 }
        XCTAssertNil(quotation.refusal)
        XCTAssertNotNil(copied)
    }

    /// Below the floor a document whose names were proposed (the seeded
    /// quotation) counts as scanned: it goes without asking.
    func testAScannedDocumentBelowTheFloorDoesNotAsk() throws {
        let seeded = try Fixture.seeded()
        let quotation = try seeded.model(seeded.quotation, findsNames: false)
        var copied: String?
        quotation.copyForAI { copied = $0 }
        XCTAssertNil(quotation.refusal)
        XCTAssertNotNil(copied)
    }

    /// The document of `model`, saved, opened in a later run of the app.
    private func reopen(_ model: DocumentViewModel, _ seeded: Seeded) throws -> DocumentViewModel {
        try model.session.saveChanges()
        try seeded.library.saveChanges()
        let library = try RealLibrary(storage: seeded.storage, keys: FixedVaultKeyStore(), setup: nil)
        let vaultID = try XCTUnwrap((model.session as? RealSession)?.vaultID)
        let session = try library.session(for: DocumentID(rawValue: vaultID))
        return DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: seeded.storage,
            findsNames: false)
    }
}

private extension String {
    /// The UTF-8 range of the first `word`.
    func utf8Range(of word: String) -> SourceRange? {
        guard let found = range(of: word) else { return nil }
        let start = utf8.distance(from: utf8.startIndex, to: found.lowerBound)
        return SourceRange(start: start, end: start + word.utf8.count)
    }
}
