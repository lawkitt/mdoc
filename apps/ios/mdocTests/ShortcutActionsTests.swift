import XCTest
@testable import mdoc

/// Pseudonymize text and Restore text as Shortcuts actions (#1164,
/// Milestone 2 item 19) on the real core over the seeded Vault: what each
/// action returns, and that nothing comes back while the preview is off or
/// a document is not Ready.
@MainActor
final class ShortcutActionsTests: XCTestCase {
    private let text = "Maya Ellison asked us to write to ann@example.com."

    /// The seeded library, with the alias the shared quotation gave Maya
    /// Ellison.
    private func seeded() throws -> (Seeded, String) {
        let seeded = try Fixture.seeded()
        let quotation = try seeded.session(seeded.quotation).snapshot()
        let alias = try XCTUnwrap(quotation.candidates.first { $0.original == "Maya Ellison" }?.alias)
        return (seeded, alias)
    }

    private func previewOn() -> Preferences {
        let preferences = Fixture.preferences()
        preferences.aiConnectorPreview = true
        return preferences
    }

    private func app(_ seeded: Seeded, preview: Bool = true) -> AppModel {
        let app = Fixture.app(seeded)
        app.preferences.aiConnectorPreview = preview
        return app
    }

    /// Every file under the Vault folder, to see that nothing is written.
    private func vaultFiles(_ seeded: Seeded) -> Set<String> {
        let items = FileManager.default.enumerator(at: seeded.storage.vault, includingPropertiesForKeys: nil)
        return Set((items?.allObjects as? [URL] ?? []).map(\.path))
    }

    // MARK: Restore text

    /// Done-when: an alias the Project knows comes back as its original,
    /// with the place shared last by default and with the Project named,
    /// and nothing is saved.
    func testRestoreTextReturnsTheOriginalsOfKnownAliases() throws {
        let (seeded, alias) = try seeded()
        XCTAssertEqual(seeded.sharedLast, .project(seeded.ellison), "the quotation was shared last")
        let before = vaultFiles(seeded)

        let restored = try ShortcutActions.restore(
            "\(alias) agrees.", project: nil, store: try seeded.replyStore(), sharedLast: seeded.sharedLast,
            preferences: previewOn())
        XCTAssertEqual(restored, "Maya Ellison agrees.")
        let named = try ShortcutActions.restore(
            "\(alias) agrees.", project: seeded.ellison, store: try seeded.replyStore(), sharedLast: nil,
            preferences: previewOn())
        XCTAssertEqual(named, "Maya Ellison agrees.")
        XCTAssertEqual(vaultFiles(seeded), before, "Restore text saves nothing")
    }

    /// Another Project's names leave the alias as it came.
    func testRestoreTextWithAnotherProjectKeepsTheAlias() throws {
        let (seeded, alias) = try seeded()
        let restored = try ShortcutActions.restore(
            "\(alias) agrees.", project: seeded.harbour, store: try seeded.replyStore(),
            sharedLast: seeded.sharedLast, preferences: previewOn())
        XCTAssertEqual(restored, "\(alias) agrees.")
    }

    func testRestoreTextRefusesAProjectNoLongerThere() throws {
        let (seeded, alias) = try seeded()
        XCTAssertThrowsError(try ShortcutActions.restore(
            "\(alias) agrees.", project: ProjectID(rawValue: String(repeating: "0", count: 32)),
            store: try seeded.replyStore(), sharedLast: nil, preferences: previewOn())
        ) { XCTAssertEqual($0 as? ShortcutFailure, .projectGone) }
    }

    func testRestoreTextRefusesWhileThePreviewIsOff() throws {
        let (seeded, alias) = try seeded()
        XCTAssertThrowsError(try ShortcutActions.restore(
            "\(alias) agrees.", project: nil, store: try seeded.replyStore(), sharedLast: seeded.sharedLast,
            preferences: Fixture.preferences())
        ) { error in
            XCTAssertEqual(error as? ShortcutFailure, .previewOff)
            XCTAssertEqual(error.localizedDescription, "Turn on AI connector (preview) in Settings")
        }
    }

    // MARK: Pseudonymize text

    /// The text becomes a new document in the Project, opened and read,
    /// and the action returns nothing while it is not reviewed.
    func testPseudonymizeTextOpensANewDocumentForReviewAndReturnsNothingYet() async throws {
        let (seeded, _) = try seeded()
        let app = app(seeded)
        let before = Set(app.library.documents.map(\.id))

        await assertNeedsReview { try await app.pseudonymize(fromShortcut: self.text, into: seeded.ellison) }
        let opened = try XCTUnwrap(app.navigator.openDocument)
        XCTAssertFalse(before.contains(opened), "a new document")
        let model = try XCTUnwrap(app.openModel)
        XCTAssertEqual(model.snapshot.projectID, seeded.ellison)
        XCTAssertNotNil(model.snapshot.phase, "read as a paste is")
        XCTAssertTrue(model.snapshot.markdown.contains("Maya Ellison"), "nothing replaced without review")
    }

    /// Done-when: once the document is Ready, what the action returns has
    /// no original, and the Project becomes the place Restore uses.
    func testPseudonymizedTextHasNoOriginalOnceReviewed() async throws {
        let (seeded, alias) = try seeded()
        let app = app(seeded)
        await assertNeedsReview { try await app.pseudonymize(fromShortcut: self.text, into: seeded.ellison) }
        let model = try XCTUnwrap(app.openModel)
        let session = try seeded.session(model.id)
        _ = try session.scan(observer: TestObserver())
        _ = try session.replaceAllPending()
        model.refresh()

        let pseudonymized = try model.textForShortcut()
        XCTAssertFalse(pseudonymized.contains("Maya Ellison"))
        XCTAssertFalse(pseudonymized.contains("ann@example.com"))
        XCTAssertTrue(pseudonymized.contains(alias), "the Project's alias for her")
        XCTAssertEqual(seeded.sharedLast, .project(seeded.ellison))
    }

    func testPseudonymizeTextWithNoProjectMakesAnUnfiledDocument() async throws {
        let (seeded, _) = try seeded()
        let app = app(seeded)
        await assertNeedsReview { try await app.pseudonymize(fromShortcut: self.text, into: nil) }
        let opened = try XCTUnwrap(app.navigator.openDocument)
        XCTAssertNotNil(app.library.document(opened))
        XCTAssertNil(app.library.document(opened)?.projectID)
    }

    func testPseudonymizeTextRefusesWhileThePreviewIsOffAndAddsNothing() async throws {
        let (seeded, _) = try seeded()
        let app = app(seeded, preview: false)
        let before = app.library.documents.map(\.id)
        do {
            _ = try await app.pseudonymize(fromShortcut: text, into: seeded.ellison)
            XCTFail("refused")
        } catch {
            XCTAssertEqual(error as? ShortcutFailure, .previewOff)
        }
        XCTAssertEqual(app.library.documents.map(\.id), before)
        XCTAssertNil(app.navigator.openDocument)
    }

    func testPseudonymizeTextRefusesBlankTextAndAGoneProject() async throws {
        let (seeded, _) = try seeded()
        let app = app(seeded)
        let before = app.library.documents.map(\.id)
        for (input, project, failure) in [
            (" \n", seeded.ellison, ShortcutFailure.noText),
            (text, ProjectID(rawValue: String(repeating: "0", count: 32)), .projectGone),
        ] {
            do {
                _ = try await app.pseudonymize(fromShortcut: input, into: project)
                XCTFail("refused")
            } catch {
                XCTAssertEqual(error as? ShortcutFailure, failure)
            }
        }
        XCTAssertEqual(app.library.documents.map(\.id), before)
    }

    private func assertNeedsReview(
        _ call: () async throws -> String, file: StaticString = #filePath, line: UInt = #line
    ) async {
        do {
            let text = try await call()
            XCTFail("returned \(text.count) characters before review", file: file, line: line)
        } catch {
            XCTAssertEqual(error as? ShortcutFailure, .needsReview, "\(error)", file: file, line: line)
        }
    }
}
