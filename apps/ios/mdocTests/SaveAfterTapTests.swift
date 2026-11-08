import Foundation
import XCTest
@testable import mdoc

/// The save follows the tap (ADR 0104, Calls stay synchronous while they
/// are cheap): a review tap returns at once and its change is saved off the
/// main thread once the taps pause, several taps in one save, or at once at
/// a flush point: mdoc leaving the foreground, Copy for AI and Share, and
/// leaving the document for another screen.
@MainActor
final class SaveAfterTapTests: XCTestCase {
    // Set and cleared around each test, on the main thread.
    nonisolated(unsafe) private var seeded: Seeded!

    override func setUpWithError() throws {
        seeded = try Fixture.seeded()
    }

    override func tearDown() {
        ProtectedData.isAvailable = true
        if let seeded {
            try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: seeded.storage.url(.documents).path)
        }
    }

    private var library: RealLibrary { seeded.library as! RealLibrary }

    /// The letter's file as saved now.
    private func onDisk() throws -> String {
        let path = try XCTUnwrap((try seeded.session(seeded.letter) as? RealSession)?.path)
        return try String(contentsOfFile: path, encoding: .utf8)
    }

    /// Replace on each of the letter's first `count` proposals, as taps.
    @discardableResult
    private func tapReplace(_ count: Int = 1) throws -> DocumentSnapshot {
        let session = try seeded.session(seeded.letter)
        var snapshot = session.snapshot()
        for _ in 0..<count {
            let next = try XCTUnwrap(snapshot.candidates.first { $0.state == .proposed })
            snapshot = try session.decide(next.id, .replace, scope: .all)
        }
        return snapshot
    }

    private func makeDocumentsUnwritable() throws {
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: seeded.storage.url(.documents).path)
    }

    private func makeDocumentsWritable() throws {
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: seeded.storage.url(.documents).path)
    }

    // MARK: Scheduling

    /// Calls closer together than the pause make one save, after the last;
    /// a cancel leaves none.
    func testTheSchedulerSavesOnceTheCallsPause() {
        let scheduler = SaveScheduler(queue: DispatchQueue(label: "test"))
        let counter = Counter()
        let saved = expectation(description: "saved once")
        for _ in 0..<3 {
            scheduler.schedule(pause: .milliseconds(100)) {
                counter.add()
                saved.fulfill()
            }
        }
        wait(for: [saved], timeout: 5)
        let cancelled = expectation(description: "never saved")
        cancelled.isInverted = true
        scheduler.schedule(pause: .milliseconds(50)) { cancelled.fulfill() }
        scheduler.cancel()
        wait(for: [cancelled], timeout: 0.3)
        XCTAssertEqual(counter.value, 1)
    }

    /// Taps change the document at once and its file once they pause: three
    /// taps, one save, told to the app.
    func testTapsSaveOnceTheyPause() throws {
        library.savePause = .milliseconds(200)
        let before = try onDisk()
        let events = Events()
        let saved = expectation(description: "saved")
        library.observeSaves { event in
            events.add(event)
            saved.fulfill()
        }
        let tapped = try tapReplace(3)
        XCTAssertNotEqual(tapped.markdown, before)
        XCTAssertEqual(try onDisk(), before, "not saved during the taps")
        wait(for: [saved], timeout: 10)
        XCTAssertEqual(try onDisk(), tapped.markdown)
        let more = expectation(description: "no second save")
        more.isInverted = true
        library.observeSaves { _ in more.fulfill() }
        wait(for: [more], timeout: 0.5)
        XCTAssertEqual(events.saved, [seeded.letter])
    }

    // MARK: Flush points

    /// Copy for AI and Share save first: the Restore extension reads the
    /// case from disk.
    func testCopyForAISavesFirst() throws {
        library.savePause = .seconds(60)
        let session = try seeded.session(seeded.letter)
        _ = try session.replaceAllPending()
        _ = try session.copyForAI()
        XCTAssertEqual(try onDisk(), session.snapshot().markdown)
    }

    /// Leaving the document for any other screen saves it.
    func testLeavingADocumentSavesIt() throws {
        library.savePause = .seconds(60)
        let app = Fixture.app(seeded)
        app.open(seeded.letter)
        let tapped = try tapReplace()
        XCTAssertNotEqual(try onDisk(), tapped.markdown)
        app.open(.documents)
        XCTAssertEqual(try onDisk(), tapped.markdown)
    }

    /// mdoc leaving the foreground saves every document.
    func testLeavingTheForegroundSaves() throws {
        library.savePause = .seconds(60)
        let app = Fixture.app(seeded)
        app.open(seeded.letter)
        let tapped = try tapReplace()
        app.saveChanges()
        XCTAssertEqual(try onDisk(), tapped.markdown)
        XCTAssertNil(app.navigator.presentation)
    }

    // MARK: Failures

    /// A save after the taps that fails shows on the document as any save
    /// failure does, and the change stays to save.
    func testAFailedSaveShowsOnTheDocument() throws {
        library.savePause = .milliseconds(50)
        let app = Fixture.app(seeded)
        app.open(seeded.letter)
        let model = try XCTUnwrap(app.openModel)
        try makeDocumentsUnwritable()
        let tapped = try tapReplace()
        let shown = XCTNSPredicateExpectation(
            predicate: NSPredicate { _, _ in MainActor.assumeIsolated { model.errorMessage != nil } }, object: nil)
        wait(for: [shown], timeout: 10)
        let message = try XCTUnwrap(model.errorMessage)
        XCTAssertTrue(message.hasPrefix("mdoc could not save this change"), message)
        try makeDocumentsWritable()
        app.saveChanges()
        XCTAssertEqual(try onDisk(), tapped.markdown)
    }

    /// The phone locked before the save: the library's files are
    /// unwritable, so it saves once the phone unlocks, with no error.
    func testATapLeftWhenThePhoneLocksSavesOnUnlock() throws {
        library.savePause = .seconds(60)
        let session = try seeded.session(seeded.letter)
        let tapped = try tapReplace()
        try makeDocumentsUnwritable()
        ProtectedData.isAvailable = false
        XCTAssertNoThrow(try session.saveChanges())
        XCTAssertEqual(try library.saveDeferred(), [], "still locked")
        ProtectedData.isAvailable = true
        try makeDocumentsWritable()
        XCTAssertEqual(try library.saveDeferred(), [seeded.letter])
        XCTAssertEqual(try onDisk(), tapped.markdown)
    }

    /// An import read while the phone was locked waits for its first save,
    /// which joins the case chosen for it. A save of the taps once mdoc is
    /// back (the scene turns inactive before active) runs that save first,
    /// never one that starts a case of its own.
    func testAFlushRunsTheImportsHeldBackSaveFirst() throws {
        library.savePause = .seconds(60)
        let session = try library.importDocument(.text("Write to ann@example.com."), title: "Note", into: .existing(seeded.harbour))
        try makeDocumentsUnwritable()
        ProtectedData.isAvailable = false
        _ = try session.recognize(observer: TestObserver())
        try makeDocumentsWritable()
        try session.saveChanges()
        XCTAssertNotNil((session as? RealSession)?.path, "saved")
        XCTAssertEqual(session.snapshot().caseID, seeded.harbour)
        ProtectedData.isAvailable = true
        XCTAssertEqual(try library.saveDeferred(), [], "saved once")
        XCTAssertEqual(library.snapshot().cases.count, 2, "no case of its own")
    }

    /// A deleted document's scheduled save never runs: it neither writes
    /// the file again nor reports a save that failed.
    func testADeletedDocumentIsNotSavedAgain() throws {
        library.savePause = .milliseconds(100)
        // Held, as the document's model holds it while its delete's toast shows.
        let session = try XCTUnwrap(try seeded.session(seeded.letter) as? RealSession)
        let path = try XCTUnwrap(session.path)
        try tapReplace()
        let saved = expectation(description: "no save")
        saved.isInverted = true
        library.observeSaves { _ in saved.fulfill() }
        try library.deleteDocument(seeded.letter)
        wait(for: [saved], timeout: 0.5)
        XCTAssertFalse(FileManager.default.fileExists(atPath: path))
        withExtendedLifetime(session) {}
    }
}

private final class Counter: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0
    var value: Int { lock.withLock { count } }
    func add() { lock.withLock { count += 1 } }
}

private final class Events: @unchecked Sendable {
    private let lock = NSLock()
    private var events: [SaveEvent] = []
    func add(_ event: SaveEvent) { lock.withLock { events.append(event) } }
    var saved: [DocumentID] {
        lock.withLock {
            events.compactMap { if case .saved(let id, _) = $0 { id } else { nil } }
        }
    }
}
