import UIKit
import XCTest
@testable import mdoc

/// Restore with mdoc on the Vault (#1091 Decisions 20 and 37, Vault &
/// Projects item 10): the extension's handle and the app's on one folder,
/// as the two processes open it.
@MainActor
final class RestoreExtensionTests: XCTestCase {
    /// The seeded library, with the alias the shared quotation gave Maya
    /// Ellison.
    private func seeded() throws -> (Seeded, String) {
        let seeded = try Fixture.seeded()
        let quotation = try seeded.session(seeded.quotation).snapshot()
        let alias = try XCTUnwrap(quotation.candidates.first { $0.original == "Maya Ellison" }?.alias)
        return (seeded, alias)
    }

    /// The extension's sheet model over its own handle on the seeded Vault.
    private func extensionModel(_ seeded: Seeded, reply: String) throws -> RestoreViewModel {
        RestoreViewModel(store: try seeded.replyStore(), reply: reply, sharedLast: seeded.sharedLast)
    }

    /// An Unfiled document with its email replaced, shared last: the place
    /// the extension restores with by default.
    private func sharedUnfiled(_ seeded: Seeded) throws -> (session: any mdoc.DocumentSession, vaultID: String, alias: String) {
        let memo = try seeded.library.importDocument(.text("Write to ann@example.com."), title: "Memo", into: nil)
        _ = try memo.recognize(observer: TestObserver())
        _ = try memo.scan(observer: TestObserver())
        let alias = try XCTUnwrap(try memo.replaceAllPending().candidates.first?.alias)
        _ = try memo.copyForAI()
        return (memo, try XCTUnwrap((memo as? RealSession)?.vaultID), alias)
    }

    private var manifestName: String { "manifest.enc" }

    /// Done-when (#1091 Testing Decisions, two handles on one folder): a
    /// reply the extension saves for the Project shared last writes only a
    /// reply file, never the manifest. The app sees it once it takes in
    /// the change, lists it on the Project's page and in its documents'
    /// Reply panes, and shows it restored.
    func testAReplySavedInTheExtensionShowsInTheAppRestored() throws {
        let (seeded, alias) = try seeded()
        let app = Fixture.app(seeded)
        let letter = try seeded.model(seeded.letter)
        XCTAssertNil(letter.snapshot.reply)
        let manifest = seeded.storage.vault.appendingPathComponent(manifestName)
        let before = try Data(contentsOf: manifest)

        let sheet = try extensionModel(seeded, reply: "\(alias) agrees.")
        XCTAssertEqual(sheet.selected, .project(seeded.ellison), "the Project shared last")
        XCTAssertEqual(sheet.result?.text, "Maya Ellison agrees.")
        sheet.save()
        let saved = try XCTUnwrap(sheet.saved)
        XCTAssertNil(sheet.errorMessage)
        XCTAssertEqual(saved.title, "Reply to \(Letters.ellisonProject)")
        XCTAssertEqual(try Data(contentsOf: manifest), before, "the extension never writes the manifest")

        letter.refresh()
        XCTAssertTrue(letter.snapshot.replies.isEmpty, "nothing until the app takes it in")
        app.takeInChanges()
        letter.refresh()
        XCTAssertEqual(app.library.replies(in: seeded.ellison).map(\.id), [saved.id])
        XCTAssertEqual(letter.snapshot.replies.map(\.id), [saved.id], "the Project's: every document in it lists it")
        XCTAssertEqual(letter.snapshot.reply?.id, saved.id)
        XCTAssertEqual(letter.snapshot.reply?.aliasText, "\(alias) agrees.")
        XCTAssertEqual(letter.snapshot.reply?.restored.text, "Maya Ellison agrees.")
        XCTAssertEqual(try app.library.restore(saved.id).get().text, "Maya Ellison agrees.")
        XCTAssertTrue(app.library.replies(in: seeded.harbour).isEmpty)

        // A document opened later, through a fresh handle, finds it too.
        let reopened = try self.reopened(seeded, seeded.quotation)
        XCTAssertEqual(reopened.snapshot().reply?.restored.text, "Maya Ellison agrees.")
    }

    /// A reply to an Unfiled document shared last is that document's: the
    /// extension restores with its names, and its Reply pane lists it.
    func testAReplyToAnUnfiledDocumentIsItsOwn() throws {
        let seeded = try Fixture.seeded()
        let app = Fixture.app(seeded)
        let (memo, vaultID, alias) = try sharedUnfiled(seeded)
        XCTAssertEqual(seeded.sharedLast, .document(DocumentID(rawValue: vaultID)))

        let sheet = try extensionModel(seeded, reply: "Ask \(alias).")
        XCTAssertEqual(sheet.places.unfiled.map(\.title), ["Memo"])
        XCTAssertEqual(sheet.selected, .document(DocumentID(rawValue: vaultID)))
        XCTAssertEqual(sheet.selectedName, "Memo")
        XCTAssertEqual(sheet.result?.text, "Ask ann@example.com.")
        sheet.save()
        let saved = try XCTUnwrap(sheet.saved)

        app.takeInChanges()
        let snapshot = memo.snapshot()
        XCTAssertEqual(snapshot.replies.map(\.id), [saved.id])
        XCTAssertEqual(snapshot.reply?.restored.text, "Ask ann@example.com.")
        XCTAssertTrue(app.library.replies(in: seeded.ellison).isEmpty, "not a Project's")
    }

    /// The picker lists Projects, pinned then most recently updated, then
    /// Unfiled documents; a place shared last that is gone falls back to
    /// the first Project.
    func testThePickerListsProjectsThenUnfiled() throws {
        let seeded = try Fixture.seeded()
        let (_, vaultID, _) = try sharedUnfiled(seeded)
        let store = try seeded.replyStore()
        let places = store.places()
        let library = try XCTUnwrap(seeded.library as? RealLibrary)
        XCTAssertEqual(places.projects.map(\.id.rawValue), library.vault.projects().map(\.id))
        XCTAssertEqual(Set(places.projects.map(\.name)), [Letters.ellisonProject, Letters.harbourProject])
        XCTAssertEqual(places.all.last, .document(DocumentID(rawValue: vaultID)))

        let gone = RestorePlace.document(DocumentID(rawValue: String(repeating: "0", count: 32)))
        XCTAssertEqual(RestoreViewModel.defaultPlace(sharedLast: gone, in: places), places.all.first)
        XCTAssertEqual(RestoreViewModel.defaultPlace(sharedLast: nil, in: places), .project(places.projects[0].id))
        let unfiledOnly = RestorePlaces(projects: [], unfiled: places.unfiled)
        XCTAssertEqual(RestoreViewModel.defaultPlace(sharedLast: nil, in: unfiledOnly), .document(DocumentID(rawValue: vaultID)))
        XCTAssertNil(RestoreViewModel.defaultPlace(sharedLast: nil, in: .empty))
    }

    func testPickingAnotherPlaceRestoresWithItsNamesOnly() throws {
        let (seeded, alias) = try seeded()
        let sheet = try extensionModel(seeded, reply: "\(alias) agrees.")
        sheet.select(.project(seeded.harbour))
        XCTAssertEqual(sheet.selected, .project(seeded.harbour))
        XCTAssertEqual(sheet.selectedName, Letters.harbourProject)
        XCTAssertEqual(sheet.result?.text, "\(alias) agrees.")
        XCTAssertEqual(sheet.result?.unresolvedCount, 1)
    }

    /// Copied says the restored text shown was copied: another place's
    /// restored text is not, so the button reads Copy again. Saved goes
    /// too: the reply is not saved with the new place yet.
    func testCopiedAndSavedGoWhenThePlaceChanges() throws {
        let (seeded, alias) = try seeded()
        let sheet = try extensionModel(seeded, reply: "\(alias) agrees.")
        let pasteboard = UIPasteboard.withUniqueName()
        defer { UIPasteboard.remove(withName: pasteboard.name) }
        sheet.copy(to: pasteboard)
        XCTAssertTrue(sheet.copied)
        XCTAssertEqual(pasteboard.string, "Maya Ellison agrees.")
        sheet.save()
        XCTAssertNotNil(sheet.saved)
        XCTAssertTrue(sheet.copied, "saving copies nothing new")
        sheet.select(.project(seeded.ellison))
        XCTAssertTrue(sheet.copied, "the same place, the same text")
        sheet.select(.project(seeded.harbour))
        XCTAssertFalse(sheet.copied)
        XCTAssertNil(sheet.saved)
        sheet.copy(to: pasteboard)
        XCTAssertEqual(pasteboard.string, "\(alias) agrees.")
    }

    /// A save that fails says why, and the restored text stays: here the
    /// Unfiled document was deleted in the app after the sheet opened.
    func testAFailedSaveSaysWhy() throws {
        let seeded = try Fixture.seeded()
        let (memo, _, alias) = try sharedUnfiled(seeded)
        let sheet = try extensionModel(seeded, reply: "Ask \(alias).")
        try seeded.library.deleteDocument(memo.id)
        sheet.save()
        XCTAssertNil(sheet.saved)
        XCTAssertNotNil(sheet.errorMessage)
        XCTAssertNotNil(sheet.result, "the restored text stays")
    }

    /// The extension never makes the Vault: before the app has, it says so
    /// and leaves no folder behind.
    func testAVaultNotMadeYetIsRefused() throws {
        let storage = try Fixture.temporaryStorage()
        try FileManager.default.removeItem(at: storage.vault)
        XCTAssertThrowsError(try RealCore.replyStore(storage: storage, keys: FixedVaultKeyStore()))
        XCTAssertFalse(FileManager.default.fileExists(atPath: storage.vault.path))
        guard case .failed(let message) = RestoreSheet.make(text: "PERSON_1", store: {
            (try RealCore.replyStore(storage: storage, keys: FixedVaultKeyStore()), nil)
        }) else {
            return XCTFail("refused")
        }
        XCTAssertEqual(message, "Open mdoc first: it has nothing to restore with yet.")
    }

    /// The sheet says which of its states it is in: nothing shared as
    /// text, the Vault failing with the error's own words, an empty Vault,
    /// or the reply restored.
    func testTheSheetSaysWhenRestoreFailed() throws {
        guard case .noText = RestoreSheet.make(text: nil, store: { throw CoreError.notFound }) else {
            return XCTFail("no text")
        }
        guard case .failed(let message) = RestoreSheet.make(text: "PERSON_1", store: { throw CoreError.notFound }) else {
            return XCTFail("the store failed: not nothing to restore")
        }
        XCTAssertEqual(message, CoreError.notFound.errorDescription)

        let (core, storage) = try Fixture.core()
        _ = core
        let empty = RestoreViewModel(
            store: try RealCore.replyStore(storage: storage, keys: FixedVaultKeyStore()), reply: "PERSON_1", sharedLast: nil)
        XCTAssertTrue(empty.places.isEmpty)
        XCTAssertNil(empty.selected)
        XCTAssertEqual(empty.errorMessage, CoreError.noProject.errorDescription)

        let (seeded, alias) = try seeded()
        guard case .ready(let model) = RestoreSheet.make(
            text: "\(alias) agrees.", store: { (try seeded.replyStore(), seeded.sharedLast) })
        else {
            return XCTFail("ready")
        }
        XCTAssertEqual(model.result?.text, "Maya Ellison agrees.")
    }

    /// Document `id` opened through a fresh app handle on the same Vault.
    private func reopened(_ seeded: Seeded, _ id: DocumentID) throws -> any mdoc.DocumentSession {
        try seeded.library.saveChanges()
        let library = try RealLibrary(storage: seeded.storage, keys: FixedVaultKeyStore(), setup: nil)
        let vaultID = try XCTUnwrap((try seeded.session(id) as? RealSession)?.vaultID)
        return try library.session(for: DocumentID(rawValue: vaultID))
    }
}
