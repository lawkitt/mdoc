import UIKit
import XCTest
@testable import mdoc

/// Viewing a Vault original (#1091 Decision 24): what a close leaves, and
/// the pane when the original goes.
final class VaultOriginalTests: XCTestCase {
    /// A close while the original decrypts (the user leaves the document
    /// as the pane opens) keeps nothing: the decrypt's result is thrown
    /// away and its copy removed, and the close never waits for it
    /// (review 1).
    func testACloseWhileDecryptingLeavesNothing() throws {
        let started = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        let removed = Counter()
        let stored = StoredOriginal(
            decrypt: {
                started.signal()
                release.wait()
                return .memory(OriginalData(Data("%PDF".utf8)))
            },
            remove: { removed.add() })
        let finished = expectation(description: "load ends")
        let outcome = Outcome()
        DispatchQueue.global().async {
            do {
                _ = try stored.load()
                outcome.set("loaded")
            } catch {
                outcome.set("\(error)")
            }
            finished.fulfill()
        }
        started.wait()
        stored.close()
        XCTAssertEqual(removed.value, 1, "the close returned while the decrypt ran")
        release.signal()
        wait(for: [finished], timeout: 5)
        XCTAssertEqual(outcome.value, "\(CoreError.cancelled)")
        XCTAssertEqual(removed.value, 2, "what the decrypt wrote goes too")

        // The next view decrypts again, and keeps it until the next close.
        release.signal()
        guard case .memory = try stored.load() else { return XCTFail("decrypted") }
        stored.close()
        XCTAssertEqual(removed.value, 3)
    }

    /// The Original pane open while the original goes (pages picked
    /// together keep none once saved) gives the text back (review 2).
    @MainActor
    func testThePaneGoesBackToTextWhenTheOriginalGoes() throws {
        let (core, storage) = try Fixture.core()
        let pdf = storage.url(.intake).appendingPathComponent("statement.pdf")
        try FileManager.default.createDirectory(at: pdf.deletingLastPathComponent(), withIntermediateDirectories: true)
        try UIGraphicsPDFRenderer(bounds: CGRect(x: 0, y: 0, width: 300, height: 200)).writePDF(to: pdf) { context in
            context.beginPage()
        }
        let session = try core.library.importDocument(.file(pdf, .pdf), title: "Statement", into: nil)
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        XCTAssertTrue(model.hasOriginal)
        model.show(.original)
        XCTAssertEqual(model.pane, .original)
        var gone = model.snapshot
        gone.original = .none
        model.applyReview(gone)
        XCTAssertEqual(model.pane, .text)
    }
}

private final class Counter: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0
    var value: Int { lock.withLock { count } }
    func add() { lock.withLock { count += 1 } }
}

private final class Outcome: @unchecked Sendable {
    private let lock = NSLock()
    private var text = ""
    var value: String { lock.withLock { text } }
    func set(_ value: String) { lock.withLock { text = value } }
}
