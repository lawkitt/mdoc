import UIKit
import XCTest

/// Open → review → share → restore on the Rust core (ADR 0085's short
/// XCUITest). The app imports the demo DOCX at launch and scans it with the
/// structured rules, so no model is needed.
final class ReviewFlowUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testOpenReviewShareRestore() throws {
        let app = MdocUITest.launch()

        // Open: the imported document from Home.
        let document = MdocUITest.documentRow(app)
        XCTAssertTrue(document.waitForExistence(timeout: 60))
        document.tap()

        // Review: tap the email the rules found and replace every occurrence.
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        let email = text.links[MdocUITest.email].firstMatch
        XCTAssertTrue(email.waitForExistence(timeout: 30))
        MdocUITest.tapAboveBottomBar(email, in: text, app)
        // The review bar opens on it (ADR 0098); Replace takes every mention.
        let replace = app.buttons["review-replace"]
        XCTAssertTrue(replace.waitForExistence(timeout: 5))
        XCTAssertTrue(app.descendants(matching: .any)["review-alias"].firstMatch.exists)
        replace.tap()
        // The text view rebuilds its elements after a change; ask the app.
        XCTAssertTrue(app.links["EMAIL_1"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertFalse(app.links[MdocUITest.email].exists)

        // The only proposal: All reviewed, with Undo in the toast; undo puts
        // the bar back on it, and the next toast redoes.
        let toastUndo = app.buttons["toast-action"]
        XCTAssertTrue(toastUndo.waitForExistence(timeout: 5))
        toastUndo.tap()
        let redo = app.buttons["toast-action"].firstMatch
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "identifier == 'toast-action' AND label == 'Redo'")).firstMatch.waitForExistence(timeout: 5))
        XCTAssertTrue(app.links[MdocUITest.email].firstMatch.exists)
        XCTAssertTrue(app.buttons["review-undo"].exists)
        redo.tap()
        XCTAssertTrue(app.links["EMAIL_1"].firstMatch.waitForExistence(timeout: 10))
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }

        // Replace any remaining proposals, as reviewed.
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.waitForExistence(timeout: 5))
        if primary.label.hasPrefix("Replace") {
            primary.tap()
        }

        // Share: the share sheet opens with the pseudonymized text.
        XCTAssertTrue(primary.label.hasPrefix("Share"))
        primary.tap()
        let sheet = app.otherElements["ActivityListView"].firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 10))
        MdocUITest.closeShareSheet(app)

        // Restore: paste the AI's reply into the document.
        app.buttons["paste-reply"].tap()
        let editor = app.textViews["reply-editor"]
        XCTAssertTrue(editor.waitForExistence(timeout: 5))
        editor.tap()
        editor.typeText("Write to EMAIL_1 today. EMAIL_9 is new.")
        app.buttons["reply-restore"].tap()

        let restored = app.textViews["restored-text"]
        XCTAssertTrue(restored.waitForExistence(timeout: 10))
        XCTAssertTrue((restored.value as? String)?.contains("Write to \(MdocUITest.email) today.") ?? false)
        let summary = app.staticTexts["restore-summary"]
        XCTAssertTrue(summary.label.contains("1 value restored"))
        XCTAssertTrue(summary.label.contains("1 unrecognized"))
    }
}

/// What the UI tests share: the launch with a fixture import, and lookups.
enum MdocUITest {
    static let email = "maya.bennett@example.com"
    static let title = "Settlement agreement"
    /// The demo agreement the review tests import.
    static let defaultSeed = "docs/product-media/Settlement agreement.docx"

    /// The repository, from this file's path (the Simulator reads the Mac's files).
    static let repo = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()

    /// The Markdown-rich fixture (`tests/fixtures/mobile-markdown`): its
    /// title, and the email the rules find in it.
    static let markdownFixture = "tests/fixtures/mobile-markdown/review.md"
    static let markdownTitle = "review"
    static let markdownEmail = "anna.smirnova@example.com"

    /// Launches with a fresh library that imports `seed` from the repository.
    @MainActor
    static func launch(seed: String = defaultSeed) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = repo.appendingPathComponent(seed).path
        app.launch()
        return app
    }

    /// Rendered pages of a fictional tenancy, as the camera would give
    /// them, in a folder the app's seed reads as a scan.
    static func scanPages(named name: String, count: Int) throws -> URL {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(name, isDirectory: true)
        try? FileManager.default.removeItem(at: folder)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        for number in 1...count {
            let format = UIGraphicsImageRendererFormat()
            format.scale = 1
            let image = UIGraphicsImageRenderer(size: CGSize(width: 1240, height: 1754), format: format).image { context in
                UIColor.white.setFill()
                context.fill(CGRect(x: 0, y: 0, width: 1240, height: 1754))
                let text = """
                    Tenancy agreement, page \(number)

                    Landlord: Harbour Street Lettings
                    Tenant: Grace Whitlow
                    The rent is 1,150 pounds a month, paid on day \(number).
                    """
                NSString(string: text).draw(
                    in: CGRect(x: 120, y: 160, width: 1000, height: 1400),
                    withAttributes: [.font: UIFont.systemFont(ofSize: 44), .foregroundColor: UIColor.black])
            }
            guard let data = image.jpegData(compressionQuality: 0.9) else { continue }
            try data.write(to: folder.appendingPathComponent("page-\(number).jpg"))
        }
        return folder
    }

    /// Says Use cellular when mdoc asks before a download on cellular (ADR
    /// 0088), so the device tests run on Wi-Fi or a phone's own data. Without
    /// the question (on Wi-Fi) it does nothing.
    @MainActor
    static func answerCellularQuestion(_ app: XCUIApplication) {
        // The dialog's button, not the "Waiting for Wi-Fi" card's, which asks it.
        let use = app.buttons
            .matching(NSPredicate(format: "label == 'Use cellular' AND identifier != 'setup-use-cellular'"))
            .firstMatch
        if use.waitForExistence(timeout: 3) {
            use.tap()
        }
    }

    /// Waits up to `timeout` for `done` while a download runs, answering Use
    /// cellular whenever mdoc holds it for Wi-Fi: a phone can move between
    /// Wi-Fi and its own data during the run. Checks once a second.
    @MainActor
    static func waitThroughDownload(_ app: XCUIApplication, timeout: TimeInterval, until done: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        let held = app.buttons["setup-use-cellular"].firstMatch
        while deadline.timeIntervalSinceNow > 0 {
            if done() { return true }
            if held.exists {
                held.tap()
                answerCellularQuestion(app)
            }
            Thread.sleep(forTimeInterval: 1)
        }
        return done()
    }

    /// The scan's pages are being recognized, or already are: a fast phone
    /// recognizes six pages in a second or two, between two checks.
    @MainActor
    static func recognizingOrRecognized(_ app: XCUIApplication) -> Bool {
        let progress = app.descendants(matching: .any)["job-progress"].firstMatch
        if progress.exists, progress.label.contains("Recognizing") { return true }
        let text = app.textViews["document-text"].firstMatch
        return text.exists && ((text.value as? String)?.contains("Harbour Street") ?? false)
    }

    /// Taps `element` in the document text once it is clear of the bottom
    /// bar. XCUITest scrolls an element into the text view's frame, which
    /// runs under the bar, so a plain tap on a word near the bottom can land
    /// on Replace instead (seen on an iPhone 17 Pro and 12 Pro). The bar is a
    /// safe-area inset, so the text always scrolls clear of it.
    @MainActor
    static func tapAboveBottomBar(_ element: XCUIElement, in text: XCUIElement, _ app: XCUIApplication) {
        let bar = app.buttons["primary-action"].firstMatch
        for _ in 0..<6 {
            guard bar.exists, element.frame.maxY > bar.frame.minY - 16 else { break }
            // Drag the text up by the overlap and a margin, at most half the view.
            let distance = min(element.frame.maxY - bar.frame.minY + 120, text.frame.height / 2)
            let start = text.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6))
            start.press(forDuration: 0.05, thenDragTo: start.withOffset(CGVector(dx: 0, dy: -distance)))
        }
        element.tap()
    }

    @MainActor
    static func documentRow(_ app: XCUIApplication, title: String = title) -> XCUIElement {
        // Rows read "<title>, <status>"; the row is ready to open once it
        // has candidates to review.
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH %@ AND label CONTAINS 'to review'", title))
            .firstMatch
    }

    /// Shows Identities and waits for it: on iPad it is open beside the
    /// text already, and its button would close it. False if it never shows.
    @MainActor
    static func showIdentities(_ app: XCUIApplication) -> Bool {
        let heading = app.staticTexts["Identities"].firstMatch
        if !heading.exists {
            let button = app.buttons["identities-button"].firstMatch
            guard button.waitForExistence(timeout: 2) else { return false }
            button.tap()
        }
        return heading.waitForExistence(timeout: 5)
    }

    @MainActor
    static func closeShareSheet(_ app: XCUIApplication) {
        let sheet = app.otherElements["ActivityListView"].firstMatch
        let close = app.buttons["Close"].firstMatch
        if close.waitForExistence(timeout: 2) {
            close.tap()
        } else {
            sheet.swipeDown(velocity: .fast)
        }
        // A tap while the sheet still slides away goes nowhere (#542): wait
        // until it is gone and Paste reply takes taps.
        XCTAssertTrue(sheet.waitForNonExistence(timeout: 10), "the share sheet stayed open")
        let paste = app.buttons["paste-reply"]
        let hittable = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isHittable == true"), object: paste)
        XCTAssertEqual(XCTWaiter.wait(for: [hittable], timeout: 10), .completed, "Paste reply never took taps")
    }
}
