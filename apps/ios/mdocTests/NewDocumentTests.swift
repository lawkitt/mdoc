import XCTest
@testable import mdoc

/// Slice 6 of ADR 0109 on the real core: a new document opens in edit
/// mode, follows its first line until renamed and goes when left empty;
/// Find names again runs only outside edit mode and proposes typed names;
/// Try a sample again restores the sample, asking first when it was edited.
@MainActor
final class NewDocumentTests: XCTestCase {
    private var seeded: Seeded!

    private func app() throws -> AppModel {
        seeded = try Fixture.seeded()
        return Fixture.app(seeded)
    }

    /// Waits, without a fixed delay, until `condition` holds.
    private func wait(_ what: String, timeout: TimeInterval = 10, until condition: @escaping @MainActor () -> Bool) {
        let met = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in MainActor.assumeIsolated { condition() } }, object: nil)
        met.expectationDescription = what
        wait(for: [met], timeout: timeout)
    }

    /// New document from the + menu: open, in edit mode, in a case of its
    /// own.
    private func newDocument(_ app: AppModel) throws -> DocumentViewModel {
        app.newDocument()
        let model = try XCTUnwrap(app.openModel)
        wait("edit mode") { model.isEditing }
        return model
    }

    /// `newDocument` from an async test, which must not block the main
    /// actor the import runs on.
    private func newDocument(_ app: AppModel) async throws -> DocumentViewModel {
        app.newDocument()
        let model = try XCTUnwrap(app.openModel)
        let editing = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in MainActor.assumeIsolated { model.isEditing } }, object: nil)
        await fulfillment(of: [editing], timeout: 10)
        return model
    }

    private func type(_ text: String, in model: DocumentViewModel) {
        let end = model.coreText().utf8.count
        XCTAssertNotNil(model.typed(SourceRange(start: end, end: end), text))
    }

    // MARK: New document

    func testANewDocumentOpensInEditModeAndGoesWhenLeftEmpty() throws {
        let app = try app()
        let before = app.library.documents.count
        let cases = app.library.cases.count
        let model = try newDocument(app)
        XCTAssertEqual(app.navigator.openDocument, model.id)
        XCTAssertEqual(model.snapshot.origin, .new)
        XCTAssertEqual(model.snapshot.title, "New document")
        XCTAssertEqual(model.snapshot.markdown, "")
        XCTAssertEqual(app.library.documents.count, before + 1)
        XCTAssertEqual(app.library.cases.count, cases + 1, "a case of its own")

        // Typed and deleted again, then left: no empty row, no empty case.
        type("x", in: model)
        XCTAssertNotNil(model.typed(SourceRange(start: 0, end: 1), ""))
        app.showStart()
        XCTAssertEqual(app.library.documents.count, before)
        XCTAssertEqual(app.library.cases.count, cases)
        XCTAssertFalse(app.library.documents.contains { $0.id == model.id })
        XCTAssertNil(app.documents.model(for: model.id))
    }

    /// The title follows the first line at each typing pause, the lists
    /// with it, until the user renames the document; then it stays, and
    /// the document stays even emptied.
    func testTheTitleFollowsTheFirstLineUntilRenamed() async throws {
        let app = try app()
        let model = try await newDocument(app)
        type("# Lease agreement\n\nThe tenant pays.", in: model)
        await model.discoverNow()
        XCTAssertEqual(model.snapshot.title, "Lease agreement")
        XCTAssertEqual(app.library.document(model.id)?.title, "Lease agreement")
        XCTAssertEqual(app.library.caseSummary(of: try XCTUnwrap(app.library.document(model.id)))?.name, "Lease agreement")

        model.doneEditing()
        app.rename(model.id, to: "Flat 3")
        XCTAssertEqual(model.snapshot.title, "Flat 3")
        model.enterEditMode()
        XCTAssertNotNil(model.typed(SourceRange(start: 2, end: 7), "Rental"))
        await model.discoverNow()
        XCTAssertEqual(model.snapshot.title, "Flat 3", "renamed, it no longer follows")
        XCTAssertEqual(model.snapshot.origin, .ordinary)

        // Emptied and left, a renamed document stays.
        let length = model.coreText().utf8.count
        XCTAssertNotNil(model.typed(SourceRange(start: 0, end: length), ""))
        app.showStart()
        XCTAssertEqual(app.library.document(model.id)?.title, "Flat 3")
    }

    /// A new document with text stays when left, under its first line.
    func testANewDocumentWithTextStays() throws {
        let app = try app()
        let model = try newDocument(app)
        type("Shopping list", in: model)
        app.open(seeded.letter)
        let row = try XCTUnwrap(app.library.documents.first { $0.origin == .new })
        XCTAssertEqual(row.title, "Shopping list", "leaving follows the line too, through the save")
    }

    // MARK: Find names again

    /// Offered once names were looked for, at the detector tier; disabled
    /// in edit mode with its reason; never below the floor (ADR 0088).
    func testFindNamesAgainIsOfferedOnlyOutsideEditMode() async throws {
        seeded = try Fixture.seeded()
        let letter = try seeded.model(seeded.letter)
        XCTAssertTrue(letter.offersFindNamesAgain)
        XCTAssertNil(letter.findNamesAgainBlockedReason)
        letter.enterEditMode()
        XCTAssertEqual(letter.findNamesAgainBlockedReason, "Leave edit mode to find names")
        let before = letter.snapshot.candidates.count
        type(" Copy to new.person@example.com.", in: letter)
        await letter.findNamesAgain()
        XCTAssertNil(letter.runningJob)
        XCTAssertEqual(letter.notice, "Leave edit mode to find names")
        XCTAssertEqual(letter.snapshot.candidates.count, before, "no scan in edit mode")
        letter.doneEditing()
        XCTAssertNil(letter.findNamesAgainBlockedReason)

        XCTAssertFalse(try seeded.model(seeded.letter, findsNames: false).offersFindNamesAgain, "below the floor")
        XCTAssertFalse(try seeded.model(seeded.tenancy).offersFindNamesAgain, "not read yet")
    }

    /// Find names again proposes a name typed since the first scan and
    /// keeps the decisions made. (Below the floor the core's scan runs its
    /// rules, which find the typed email: the stand-in for the detector.)
    func testFindNamesAgainProposesTypedNames() async throws {
        seeded = try Fixture.seeded()
        let letter = try seeded.model(seeded.letter)
        let first = try XCTUnwrap(letter.snapshot.candidates.first { $0.state == .proposed })
        letter.decide(first.id, .replace, scope: .all)
        letter.enterEditMode()
        type("\n\nCopy to new.person@example.com.", in: letter)
        letter.doneEditing()
        await letter.findNamesAgain()
        let candidates = letter.snapshot.candidates
        XCTAssertTrue(candidates.contains { $0.original == "new.person@example.com" && $0.state == .proposed })
        XCTAssertTrue(candidates.contains { $0.original == first.original && $0.state == .replaced })
    }

    // MARK: The sample

    /// Try a sample again opens the sample; edited, it asks first, and
    /// Restore brings its text and proposals back as they came.
    func testTryASampleAgainRestoresTheSample() throws {
        let app = try app()
        let sample = SampleDocument.forLanguages()
        app.trySample()
        let model = try XCTUnwrap(app.openModel)
        wait("the sample's proposals") { model.snapshot.pendingCount > 0 }
        XCTAssertEqual(model.snapshot.origin, .sample)
        let proposed = model.snapshot.pendingCount

        // As it came: opened again, nothing asked, no second sample.
        app.showStart()
        app.trySample()
        XCTAssertEqual(app.navigator.openDocument, model.id)
        XCTAssertNil(app.navigator.presentation)
        XCTAssertEqual(app.library.documents.filter { $0.origin == .sample }.count, 1)

        // Edited: it asks, and Restore puts the text back.
        let opened = try XCTUnwrap(app.openModel)
        opened.enterEditMode()
        type("\n\nMy own note.", in: opened)
        opened.doneEditing()
        app.showStart()
        app.trySample()
        XCTAssertEqual(app.navigator.openDocument, model.id)
        XCTAssertEqual(app.navigator.presentation?.id, Presentation.restoreSample(model.id).id)
        let restoring = try XCTUnwrap(app.openModel)
        app.navigator.dismiss()
        app.restoreSample(model.id)
        XCTAssertEqual(restoring.snapshot.markdown, sample.text)
        XCTAssertEqual(restoring.snapshot.pendingCount, proposed)
        XCTAssertTrue(SampleDocument.isOriginal(restoring.snapshot.markdown))
        XCTAssertEqual(app.library.documents.filter { $0.origin == .sample }.count, 1)
    }
}
