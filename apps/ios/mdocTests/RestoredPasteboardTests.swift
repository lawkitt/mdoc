import UIKit
import XCTest
@testable import mdoc

final class RestoredPasteboardTests: XCTestCase {
    private var pasteboard: UIPasteboard!

    override func setUp() {
        super.setUp()
        pasteboard = UIPasteboard(name: UIPasteboard.Name("mdoc-tests-\(UUID().uuidString)"), create: true)
    }

    override func tearDown() {
        UIPasteboard.remove(withName: pasteboard.name)
        pasteboard = nil
        super.tearDown()
    }

    func testOptionsKeepTheCopyLocalForTenMinutes() throws {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let options = RestoredPasteboard.options(now: now)
        XCTAssertEqual(options[.localOnly] as? Bool, true)
        let expiry = try XCTUnwrap(options[.expirationDate] as? Date)
        XCTAssertEqual(expiry, now.addingTimeInterval(600))
    }

    func testCopyPutsTheRestoredTextOnThePasteboard() {
        let reply = "Иванов Иван owes Maya Ellison £4,200."
        RestoredPasteboard.copy(reply, to: pasteboard)
        XCTAssertEqual(pasteboard.string, reply)
    }

    func testCopyExpires() {
        // Written as if eleven minutes ago, so its ten minutes are over.
        RestoredPasteboard.copy("Maya Ellison", to: pasteboard, now: Date.now.addingTimeInterval(-11 * 60))
        XCTAssertNil(pasteboard.string)
    }
}
