import UIKit
import XCTest

/// The UI survey (ADR 0087): every screen the app shows, light and dark,
/// kept as test attachments named `NN-screen-light` and `NN-screen-dark`.
/// Skipped unless the run sets `MDOC_SURVEY=1` (`TEST_RUNNER_MDOC_SURVEY=1`
/// through xcodebuild); `scripts/ios-survey.sh` runs it and exports the
/// shots. Runs on iPhone and iPad; a few phone-only steps skip on iPad.
final class SurveyUITests: XCTestCase {
    private var isPad: Bool { UIDevice.current.userInterfaceIdiom == .pad }

    override func setUpWithError() throws {
        continueAfterFailure = false
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_SURVEY"] == "1", "survey not requested")
    }

    override func tearDown() {
        XCUIDevice.shared.appearance = .light
        XCUIDevice.shared.orientation = .portrait
        super.tearDown()
    }

    /// A first run at the detector tier: setup, paste, import, Settings.
    @MainActor
    func test1FirstRun() throws {
        let app = launch(seed: nil, detectorTier: true)
        XCTAssertTrue(app.buttons["setup-download"].waitForExistence(timeout: 30))
        shot("01-home-empty-setup")

        MdocUITest.revealSidebar(app)
        shot("37-sidebar-empty")
        app.buttons["new-menu"].firstMatch.tap()
        // The menu's item, not Start's Paste tile of the same name.
        let pasteItem = app.collectionViews.buttons["Paste text"].firstMatch
        XCTAssertTrue(pasteItem.waitForExistence(timeout: 5))
        shot("02-add-menu")
        pasteItem.tap()
        let editor = app.textViews.firstMatch
        XCTAssertTrue(editor.waitForExistence(timeout: 5))
        shot("03-paste-sheet-empty")
        editor.tap()
        editor.typeText(Self.pastedText)
        shot("04-paste-sheet-filled")
        nextOrAdd(app)

        if app.buttons["import-add"].waitForExistence(timeout: 5) {
            shot("05-import-sheet")
            app.buttons["import-add"].tap()
        }
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 30))
        shot("06-document-before-scan")
        app.buttons["primary-action"].tap()
        XCTAssertTrue(app.buttons["setup-download"].waitForExistence(timeout: 10))
        shot("07-document-needs-setup")

        goHome(app)
        XCTAssertTrue(app.descendants(matching: .any)["document-row"].firstMatch.waitForExistence(timeout: 10))
        shot("08-home-one-document")
        // Delete from the row's menu: the toast offers Undo (Q17), which
        // brings the document back, open.
        app.descendants(matching: .any)["document-row"].firstMatch.press(forDuration: 1.0)
        let delete = app.buttons["Delete"].firstMatch
        XCTAssertTrue(delete.waitForExistence(timeout: 5))
        delete.tap()
        let undo = app.buttons["toast-action"].firstMatch
        XCTAssertTrue(undo.waitForExistence(timeout: 5))
        shot("40-delete-undo")
        undo.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))
        goHome(app)
        app.buttons["settings-button"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings-theme"].firstMatch.waitForExistence(timeout: 5))
        shot("09-settings")
        // About is the last row, below the fold on a phone.
        let about = app.buttons["settings-about"]
        for _ in 0..<4 where !about.isHittable { app.swipeUp() }
        shot("09-settings-end")
        about.tap()
        XCTAssertTrue(app.staticTexts["Version"].waitForExistence(timeout: 5))
        shot("10-about")
    }

    /// The review of the demo agreement (the seed's rules scan), through
    /// share, restore and the reply.
    @MainActor
    func test2Review() throws {
        let app = launch(seed: MdocUITest.defaultSeed)
        // Start, with the document to go back to (iPhone; iPad opens beside it).
        if app.buttons["continue-card"].waitForExistence(timeout: 60) {
            shot("38-start-continue")
        }
        let row = MdocUITest.documentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        shot("11-home-to-review")
        app.buttons["sidebar-cases"].firstMatch.tap()
        XCTAssertTrue(app.descendants(matching: .any)["case-row"].firstMatch.waitForExistence(timeout: 5))
        shot("39-cases")
        MdocUITest.revealSidebar(app)
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        shot("12-review")
        let email = text.links[MdocUITest.email].firstMatch
        XCTAssertTrue(email.waitForExistence(timeout: 30))
        scroll(text, toReveal: email)
        shot("13-review-candidate")

        email.tap()
        let bar = app.descendants(matching: .any)["review-bar"].firstMatch
        XCTAssertTrue(bar.waitForExistence(timeout: 5))
        shot("14-review-bar")
        // Swipe up on the grabber: the identity's mentions (ADR 0098).
        app.descendants(matching: .any)["review-grabber"].firstMatch.swipeUp()
        XCTAssertTrue(app.descendants(matching: .any)["review-details"].firstMatch.waitForExistence(timeout: 5))
        shot("15-review-bar-expanded")
        app.descendants(matching: .any)["review-grabber"].firstMatch.swipeDown()
        app.buttons["review-replace"].tap()
        XCTAssertTrue(app.links["EMAIL_1"].firstMatch.waitForExistence(timeout: 10))
        // The only proposal: All reviewed, with Undo in the toast.
        shot("15-review-bar-after-replace")
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }

        // On iPad the list already sits beside the text.
        if !app.staticTexts["Identities"].waitForExistence(timeout: 2) {
            app.buttons["identities-button"].tap()
        }
        XCTAssertTrue(app.staticTexts["Identities"].waitForExistence(timeout: 5))
        shot("17-identities")
        // Select: a tick on the left and a ≡ handle on the right of each row.
        app.buttons["identities-select"].firstMatch.tap()
        XCTAssertTrue(app.descendants(matching: .any)["identity-drag-handle"].firstMatch.waitForExistence(timeout: 5))
        shot("41-identities-select")
        app.buttons["identities-select"].firstMatch.tap()
        if !isPad {
            closeSheet(app)
        }

        // Replace… for a selected word: the title's first word, from the top.
        for _ in 0..<4 {
            text.swipeDown(velocity: .fast)
        }
        let word = text.coordinate(withNormalizedOffset: CGVector(dx: 0.2, dy: 0.04))
        word.press(forDuration: 1.0)
        let replace = app.menuItems["Hide name"].firstMatch
        XCTAssertTrue(replace.waitForExistence(timeout: 5), "no Hide name in the edit menu")
        shot("18-selection-edit-menu")
        // Switching to dark and back closes the edit menu: open it again.
        if !replace.waitForExistence(timeout: 1) {
            word.press(forDuration: 1.0)
            XCTAssertTrue(replace.waitForExistence(timeout: 5))
        }
        replace.tap()
        // The mark is a proposal, and the bar opens on it.
        XCTAssertTrue(app.descendants(matching: .any)["review-bar"].firstMatch.waitForExistence(timeout: 5))
        shot("19-marked-proposal")
        if app.buttons["review-end"].exists { app.buttons["review-end"].tap() }

        let primary = app.buttons["primary-action"]
        if primary.label.hasPrefix("Replace") {
            primary.tap()
        }
        XCTAssertTrue(app.buttons["paste-reply"].waitForExistence(timeout: 10))
        shot("16-review-replaced")
        share(app)
        shot("21-share-sheet")
        restoreInExtension(app)

        app.buttons["paste-reply"].tap()
        let reply = app.textViews["reply-editor"]
        XCTAssertTrue(reply.waitForExistence(timeout: 5))
        shot("23-reply-sheet-empty")
        reply.tap()
        reply.typeText(Self.replyText)
        app.buttons["reply-restore"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["restore-summary"].waitForExistence(timeout: 10))
        shot("24-reply-pane")
    }

    /// A PDF: its text and the original page.
    @MainActor
    func test3Original() throws {
        let app = launch(seed: "tests/fixtures/import/text.pdf")
        let row = MdocUITest.anyDocumentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))
        if isPad {
            // A portrait iPad shows the original beside the text, Identities
            // over it from the picker.
            shot("26-pdf-text-original")
            let segment = app.segmentedControls.buttons["Identities"].firstMatch
            if segment.exists {
                segment.tap()
                segment.wait(until: NSPredicate(format: "isSelected == true"), timeout: 5)
                shot("26-pdf-text-identities")
            }
            // Landscape keeps Home, the text, the original and Identities
            // side by side (ADR 0085).
            XCUIDevice.shared.orientation = .landscapeLeft
            defer { XCUIDevice.shared.orientation = .portrait }
            let window = app.windows.firstMatch
            MdocUITest.wait(timeout: 10) { window.frame.width > window.frame.height }
            shot("26-pdf-landscape")
            return
        }
        shot("26-pdf-text")
        let original = app.buttons["Original"].firstMatch
        original.tap()
        original.wait(until: NSPredicate(format: "isSelected == true"), timeout: 5)
        shot("27-pdf-original")
    }

    /// The Markdown-rich fixture, and the review at a large text size.
    @MainActor
    func test4Markdown() throws {
        let app = launch(seed: MdocUITest.markdownFixture)
        let row = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        shot("28-markdown-review")
        text.swipeUp(velocity: .fast)
        // shot waits for the scroll to come to rest.
        shot("29-markdown-review-end")

        let large = launch(seed: MdocUITest.markdownFixture, contentSize: "UICTContentSizeCategoryAccessibilityXL")
        let largeRow = MdocUITest.documentRow(large, title: MdocUITest.markdownTitle)
        // The setup card can fill the screen at this size; the list makes
        // rows only as they scroll in.
        var swipes = 0
        while !largeRow.waitForExistence(timeout: 10), swipes < 6 {
            large.swipeUp()
            swipes += 1
        }
        XCTAssertTrue(largeRow.exists)
        largeRow.tap()
        let largeText = large.textViews["document-text"]
        XCTAssertTrue(largeText.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        scroll(largeText, toReveal: largeText.links[MdocUITest.markdownEmail].firstMatch)
        shot("30-review-ax-text")
    }

    /// Edit mode with Show Markdown on (ADR 0109): the keyboard up, the
    /// review bar away, names dimmed and aliases tinted.
    @MainActor
    func test7EditMode() throws {
        let app = launch(seed: MdocUITest.markdownFixture)
        let row = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        app.buttons["document-menu"].tap()
        let showMarkdown = app.descendants(matching: .any)["show-markdown"].firstMatch
        XCTAssertTrue(showMarkdown.waitForExistence(timeout: 5))
        showMarkdown.tap()
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        edit.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 5))
        MdocUITest.tapIntoText(app)
        shot("42-markdown-edit-mode")
    }

    /// Live-preview editing (ADR 0109, slice 3), Show Markdown off: the
    /// caret on a heading shows its `#`, and inside a bold word its `**`,
    /// dimmed.
    @MainActor
    func test8LivePreviewEditing() throws {
        let app = launch(seed: MdocUITest.markdownFixture)
        let row = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        edit.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 5))
        MdocUITest.tapIntoText(app)
        app.typeKey(.upArrow, modifierFlags: .command)
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value BEGINSWITH %@", "# "), timeout: 5))
        shot("43-live-preview-heading")
        // A bold word typed at the end, the caret moved back into it.
        app.typeKey(.downArrow, modifierFlags: .command)
        app.typeText("\nPay **monthly**")
        for _ in 0..<4 { app.typeKey(.leftArrow, modifierFlags: []) }
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "**monthly**"), timeout: 5))
        shot("44-live-preview-bold")
    }

    /// Edit mode's toolbar and formatting keyboard (ADR 0109, slice 4), live
    /// preview: the capsule over the keyboard with Bold on in a bold word,
    /// and Aa's formatting keyboard with the line's block selected.
    @MainActor
    func test9EditToolbar() throws {
        let app = launch(seed: MdocUITest.markdownFixture)
        let row = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        edit.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 5))
        MdocUITest.tapIntoText(app)
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10))
        app.typeKey(.downArrow, modifierFlags: .command)
        app.typeText("\nPay **monthly**")
        for _ in 0..<4 { app.typeKey(.leftArrow, modifierFlags: []) }
        XCTAssertTrue(app.buttons["toolbar-bold"].wait(until: NSPredicate(format: "isSelected == true"), timeout: 5))
        shot("45-edit-toolbar")
        app.buttons["toolbar-formatting"].tap()
        XCTAssertTrue(app.buttons["turn-text"].wait(until: NSPredicate(format: "isSelected == true"), timeout: 5))
        shot("46-formatting-keyboard")
        // The caret in the link: its chip offers Open and Edit.
        app.buttons["formatting-keyboard-return"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
        app.typeKey(.downArrow, modifierFlags: .command)
        app.typeText(" to [the agent](https://example.com)")
        app.typeKey(.leftArrow, modifierFlags: [])
        XCTAssertTrue(app.buttons["link-edit"].waitForExistence(timeout: 5))
        shot("47-edit-link-chip")
    }

    /// The caret and the toolbar (ADR 0110): after Return the caret on the
    /// empty line is a body line tall; lines typed down to the toolbar keep
    /// the caret's line above it.
    @MainActor
    func test9EditCaret() throws {
        let app = launch(seed: MdocUITest.markdownFixture)
        let row = MdocUITest.documentRow(app, title: MdocUITest.markdownTitle)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        let text = app.textViews["document-text"]
        XCTAssertTrue(text.links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        let edit = app.buttons["edit-button"]
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        edit.tap()
        // The keyboard may wait for a tap in the text (ADR 0110).
        if !app.buttons["toolbar-bold"].waitForExistence(timeout: 3) { text.tap() }
        XCTAssertTrue(app.buttons["toolbar-bold"].waitForExistence(timeout: 10))
        app.typeKey(.downArrow, modifierFlags: .command)
        app.typeText("\nPay monthly\n")
        XCTAssertTrue(text.wait(until: NSPredicate(format: "NOT (value ENDSWITH %@)", "monthly"), timeout: 5))
        _ = MdocUITest.caretRect(in: text)
        shot("52-edit-caret-empty-line")
        app.typeText((1...8).map { "Clause \($0) of the lease" }.joined(separator: "\n"))
        XCTAssertTrue(text.wait(until: NSPredicate(format: "value ENDSWITH %@", "Clause 8 of the lease"), timeout: 5))
        shot("53-edit-caret-last-line")
    }

    /// New document (ADR 0109, slice 6): the sidebar's + menu with New
    /// document first, then a new document in edit mode, named after its
    /// first line once typing pauses.
    @MainActor
    func test9NewDocument() throws {
        let app = launch(seed: nil)
        MdocUITest.revealSidebar(app)
        app.buttons["new-menu"].firstMatch.tap()
        let item = app.collectionViews.buttons["New document"].firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 5))
        shot("50-new-menu")
        item.tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 10))
        app.typeText("# Lease agreement\n\nThe tenant pays the rent on the first day of each month.")
        XCTAssertTrue(app.buttons["title-menu"].wait(until: NSPredicate(format: "label == 'Lease agreement'"), timeout: 10))
        shot("51-new-document-named")
    }

    /// Table editing (ADR 0109, slice 5): the caret in a drawn cell with
    /// text typed there, then Aa's table controls.
    @MainActor
    func test10TableEditing() throws {
        let (app, _) = TableEditingUITests.editCell("Депозит")
        app.typeText(" paid")
        XCTAssertTrue(TableEditingUITests.cell(app, "Депозит paid").waitForExistence(timeout: 5))
        shot("48-table-cell")
        app.buttons["toolbar-formatting"].tap()
        XCTAssertTrue(app.buttons["table-align-left"].wait(until: NSPredicate(format: "isSelected == true"), timeout: 5))
        shot("49-table-controls")
    }

    /// Below the detector floor (ADR 0088): no setup card on Home; where
    /// Find names would be, the document says it cannot find names; Settings
    /// says the same.
    /// Edit mode's bar (ADR 0110): review with the pencil and Identities,
    /// then edit mode with the checkmark in its own capsule and no
    /// Identities, under a title long enough to be cut at its end.
    @MainActor
    func test11EditModes() throws {
        let title = "Settlement agreement between Harbour Street Lettings and the tenant"
        let seed = FileManager.default.temporaryDirectory.appendingPathComponent("\(title).md")
        try? FileManager.default.removeItem(at: seed)
        try FileManager.default.copyItem(at: MdocUITest.repo.appendingPathComponent(MdocUITest.markdownFixture), to: seed)
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launchEnvironment["MDOC_SEED_FILE"] = seed.path
        app.launchEnvironment["MDOC_SLOW_TOASTS"] = "1"
        app.launch()
        let row = MdocUITest.documentRow(app, title: title)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].links[MdocUITest.markdownEmail].firstMatch.waitForExistence(timeout: 30))
        shot("54-review-long-title")
        app.buttons["edit-button"].tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: 5))
        shot("55-edit-mode-bar")
        MdocUITest.tapIntoText(app)
        shot("56-edit-mode-keyboard")
    }

    @MainActor
    func test5BelowFloor() throws {
        let app = launch(seed: nil)
        XCTAssertTrue(app.buttons["empty-paste"].firstMatch.waitForExistence(timeout: 30))
        XCTAssertFalse(app.buttons["setup-download"].exists)
        shot("31-home-empty-below-floor")

        app.buttons["empty-paste"].firstMatch.tap()
        let editor = app.textViews.firstMatch
        XCTAssertTrue(editor.waitForExistence(timeout: 5))
        editor.tap()
        editor.typeText(Self.pastedText)
        nextOrAdd(app)
        if app.buttons["import-add"].waitForExistence(timeout: 5) {
            app.buttons["import-add"].tap()
        }
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 30))
        XCTAssertTrue(app.descendants(matching: .any)["below-floor-note"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["Find names"].exists)
        XCTAssertTrue(app.buttons["paste-reply"].exists)
        shot("32-document-below-floor")

        goHome(app)
        app.buttons["settings-button"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["below-floor-note"].firstMatch.waitForExistence(timeout: 5))
        shot("33-settings-below-floor")
    }

    /// Home and Settings at a large accessibility text size, and the
    /// review in the Serif document font (ADR 0099).
    @MainActor
    func test6LargeTextAndSerif() throws {
        let app = launch(seed: MdocUITest.defaultSeed, contentSize: "UICTContentSizeCategoryAccessibilityXL")
        let row = MdocUITest.documentRow(app)
        XCTAssertTrue(row.waitForExistence(timeout: 60))
        shot("34-home-ax-text")
        app.buttons["settings-button"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings-theme"].firstMatch.waitForExistence(timeout: 5))
        shot("35-settings-ax-text")
        let font = app.descendants(matching: .any)["settings-document-font"].firstMatch
        for _ in 0..<3 where !font.isHittable { app.swipeUp() }
        font.tap()
        let serif = app.buttons["Serif"].firstMatch
        XCTAssertTrue(serif.waitForExistence(timeout: 5))
        serif.tap()
        app.buttons["Done"].firstMatch.tap()
        MdocUITest.revealSidebar(app)
        row.tap()
        XCTAssertTrue(app.textViews["document-text"].waitForExistence(timeout: 10))
        shot("36-review-serif-ax-text")
    }

    // MARK: Steps

    private static let pastedText = """
        # Settlement agreement

        Ms Maya Bennett of Flat 3, 41 Larch Road, Bristol agrees the terms below \
        with Harrow & Finch Ltd. Write to maya.bennett@example.com.
        """
    private static let replyText = "Thanks. Write to EMAIL_1 today about the payment. EMAIL_9 is new.\n\n## Next steps\n\n- Confirm the **£18,500** figure\n- Send the draft to EMAIL_1"

    @MainActor
    private func launch(seed: String?, detectorTier: Bool = false, contentSize: String? = nil) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        if let contentSize {
            app.launchArguments += ["-UIPreferredContentSizeCategoryName", contentSize]
        }
        if let seed {
            app.launchEnvironment["MDOC_SEED_FILE"] = MdocUITest.repo.appendingPathComponent(seed).path
        }
        if detectorTier {
            app.launchEnvironment["MDOC_UI_DETECTOR_TIER"] = "1"
        }
        app.launchEnvironment["MDOC_SLOW_TOASTS"] = "1"
        app.launch()
        return app
    }

    /// Paste's sheet ends in Next (then the import sheet) or Add.
    @MainActor
    private func nextOrAdd(_ app: XCUIApplication) {
        let next = app.buttons["Next"].firstMatch
        if next.exists {
            next.tap()
        } else {
            app.buttons["import-add"].firstMatch.tap()
        }
    }

    /// The sidebar: on iPhone ☰ slides it out; on iPad it stands beside
    /// the screen, or shows from ☰ in portrait once a document opens.
    @MainActor
    private func goHome(_ app: XCUIApplication) {
        MdocUITest.revealSidebar(app)
        app.buttons["settings-button"].firstMatch.waitUntilHittable(timeout: 5)
    }

    @MainActor
    private func closeSheet(_ app: XCUIApplication) {
        // Identities is a drawer on iPhone: its own close button.
        // The drawer's panel goes once it has slid shut.
        let drawer = app.buttons["identities-done"].firstMatch
        if drawer.exists {
            drawer.tap()
            XCTAssertTrue(drawer.waitForNonExistence(timeout: 5), "Identities stayed open")
            return
        }
        let done = app.buttons["Done"].firstMatch
        if done.waitForExistence(timeout: 1) {
            done.tap()
        } else {
            app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)).tap()
        }
        XCTAssertTrue(app.staticTexts["Identities"].waitForNonExistence(timeout: 5), "Identities stayed open")
    }

    /// Share: one tap, or the older Share menu's "Share text".
    @MainActor
    private func share(_ app: XCUIApplication) {
        app.buttons["primary-action"].tap()
        let menuItem = app.buttons["Share text"]
        if menuItem.waitForExistence(timeout: 2) {
            shot("20-share-menu")
            menuItem.tap()
        }
        // shot waits for the sheet to finish sliding up.
        XCTAssertTrue(app.otherElements["ActivityListView"].firstMatch.waitForExistence(timeout: 10))
    }

    @MainActor
    private func restoreInExtension(_ app: XCUIApplication) {
        let sheet = app.otherElements["ActivityListView"].firstMatch
        let restore = sheet.descendants(matching: .any)
            .matching(NSPredicate(format: "label IN {'Restore with mdoc', 'mdoc'}"))
            .firstMatch
        XCTAssertTrue(restore.waitForExistence(timeout: 5), "Restore with mdoc is not in the share sheet")
        let sharing = XCUIScreen.main.screenshot()
        restore.tap()
        // The extension runs in its own process; its sheet shows in the
        // app's tree once it is up, by its title. Either way, the screen
        // must have left the share sheet before shot waits for it to settle.
        MdocUITest.firstToAppear(
            [app.navigationBars["Restore with mdoc"], app.staticTexts["Restore with mdoc"]], timeout: 10)
        MdocUITest.settledScreen(unlike: sharing, timeout: 10)
        shot("22-restore-extension")
        let done = app.buttons["Done"].firstMatch
        if done.waitForExistence(timeout: 3) {
            done.tap()
        }
        _ = app.buttons["paste-reply"].waitForExistence(timeout: 10)
    }

    @MainActor
    private func scroll(_ text: XCUIElement, toReveal element: XCUIElement) {
        var tries = 0
        while tries < 8, !(element.isHittable && element.frame.maxY < text.frame.maxY - 80) {
            text.swipeUp(velocity: .slow)
            tries += 1
        }
    }

    /// The screen in light, then in dark, each once it has settled (dark
    /// once it differs from light), then back to light. Setting the
    /// appearance waits for the app to go idle, so the next step sees light.
    @MainActor
    private func shot(_ name: String) {
        XCUIDevice.shared.appearance = .light
        let light = MdocUITest.settledScreen()
        keep(light, "\(name)-light")
        XCUIDevice.shared.appearance = .dark
        // A blinking caret differs from light at once: dark gets a moment.
        RunLoop.current.run(until: Date().addingTimeInterval(1))
        keep(MdocUITest.settledScreen(unlike: light), "\(name)-dark")
        XCUIDevice.shared.appearance = .light
    }

    @MainActor
    private func keep(_ screenshot: XCUIScreenshot, _ name: String) {
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
