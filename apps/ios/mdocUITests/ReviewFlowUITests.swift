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
        email.tap()
        let done = app.buttons["candidate-done"]
        XCTAssertTrue(done.waitForExistence(timeout: 5))
        XCTAssertTrue(app.descendants(matching: .any)["candidate-alias"].exists)
        done.tap()
        // The text view rebuilds its elements after a change; ask the app.
        XCTAssertTrue(app.links["EMAIL_1"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertFalse(app.links[MdocUITest.email].exists)

        // Replace any remaining proposals, as reviewed.
        let primary = app.buttons["primary-action"]
        XCTAssertTrue(primary.waitForExistence(timeout: 5))
        if primary.label.hasPrefix("Replace") {
            primary.tap()
        }

        // Share: the share sheet opens with the pseudonymized text.
        XCTAssertTrue(primary.label.hasPrefix("Share"))
        primary.tap()
        app.buttons["Share text"].tap()
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

        let restored = app.staticTexts["restored-text"]
        XCTAssertTrue(restored.waitForExistence(timeout: 10))
        XCTAssertTrue(restored.label.contains("Write to \(MdocUITest.email) today."))
        let summary = app.staticTexts["restore-summary"]
        XCTAssertTrue(summary.label.contains("1 value restored"))
        XCTAssertTrue(summary.label.contains("1 unrecognised"))
    }
}

/// What the UI tests share: the launch with a fixture import, and lookups.
enum MdocUITest {
    static let email = "maya.bennett@example.com"
    static let title = "Settlement agreement"

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
    static func launch(seed: String = "docs/product-media/Settlement agreement.docx") -> XCUIApplication {
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

    @MainActor
    static func documentRow(_ app: XCUIApplication, title: String = title) -> XCUIElement {
        // Rows read "<title>, <status>"; the row is ready to open once it
        // has candidates to review.
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH %@ AND label CONTAINS 'to review'", title))
            .firstMatch
    }

    @MainActor
    static func closeShareSheet(_ app: XCUIApplication) {
        let close = app.buttons["Close"].firstMatch
        if close.waitForExistence(timeout: 2) {
            close.tap()
        } else {
            app.otherElements["ActivityListView"].firstMatch.swipeDown(velocity: .fast)
        }
        _ = app.buttons["paste-reply"].waitForExistence(timeout: 5)
    }
}
