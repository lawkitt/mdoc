import XCTest

/// The rendered view's survey (ADR 0096): each synthetic document in
/// `tests/fixtures/mobile-markdown/rendering`, opened in the UI-testing
/// library, kept as attachments `NN-name-top` and `NN-name-more`, and the
/// time it took to show. Tables must come up as grids of cells. Skipped
/// unless the run sets `MDOC_RENDERING=1` (`TEST_RUNNER_MDOC_RENDERING=1`
/// through xcodebuild). On a phone the documents are read from the data
/// container, as `scripts/ios-device-test.sh` copies fixtures there.
final class RenderedMarkdownUITests: XCTestCase {
    static let documents = [
        "bilingual-contract", "supply-contract-ru", "loan-agreement", "itinerary", "prompt-guide",
        "registration-ru",
    ]

    override func setUpWithError() throws {
        continueAfterFailure = false
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_RENDERING"] == "1", "rendering survey not requested")
    }

    @MainActor
    func testEveryDocumentRenders() throws {
        for (index, name) in Self.documents.enumerated() {
            let app = XCUIApplication()
            app.launchArguments = ["-mdoc-ui-testing"]
            app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo
                .appendingPathComponent("tests/fixtures/mobile-markdown/rendering/\(name).md").path
            app.launch()
            let row = app.descendants(matching: .any).matching(identifier: "document-row").firstMatch
            XCTAssertTrue(row.waitForExistence(timeout: 60), name)
            let started = Date()
            row.tap()
            let text = app.textViews["document-text"]
            XCTAssertTrue(text.waitForExistence(timeout: 20), name)
            let seconds = Date().timeIntervalSince(started)
            let prefix = String(format: "%02d-%@", index + 1, name)
            shot(app, "\(prefix)-top")
            if ["bilingual-contract", "itinerary", "prompt-guide", "registration-ru"].contains(name) {
                // A table's cells are text views of their own.
                XCTAssertGreaterThan(app.textViews.count, 4, "\(name): no table grid")
            }
            text.swipeUp()
            shot(app, "\(prefix)-more")
            let timing = XCTAttachment(string: String(format: "%@ opened in %.2f s", name, seconds))
            timing.name = "\(prefix)-timing"
            timing.lifetime = .keepAlways
            add(timing)
            app.terminate()
        }
    }

    @MainActor
    private func shot(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
