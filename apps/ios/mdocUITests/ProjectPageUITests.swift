import XCTest

/// The Vault sidebar and a Project's page (#1091 Decisions 9, 33, 34):
/// New Project opens its page, titled with its name, with Documents,
/// Replies and Names; its + adds into it; Delete Project asks what becomes
/// of the documents, and either answer does what it says. Unfiled deletes
/// documents selected together.
final class ProjectPageUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// A fresh library.
    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-mdoc-ui-testing"]
        app.launch()
        return app
    }

    @MainActor
    private func row(_ app: XCUIApplication, _ title: String) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'document-row' AND label BEGINSWITH %@", "\(title),"))
            .firstMatch
    }

    /// A new document from `menu` (the sidebar's New, or a page's +), with
    /// `text` typed as its first line, which becomes its title.
    @MainActor
    private func newDocument(_ app: XCUIApplication, from menu: XCUIElement, typing text: String) {
        XCTAssertTrue(menu.waitUntilHittable(timeout: MdocUITest.timeout))
        menu.tap()
        let item = app.collectionViews.buttons["New document"].firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: MdocUITest.timeout))
        item.tap()
        XCTAssertTrue(app.buttons["done-editing"].waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: MdocUITest.timeout))
        app.typeText(text)
        app.buttons["done-editing"].tap()
    }

    /// New Project from the sidebar: its page opens on Documents, titled
    /// with its name; Replies and Names show their empty states.
    @MainActor
    private func newProject(_ app: XCUIApplication, named name: String) {
        MdocUITest.revealSidebar(app)
        let new = app.descendants(matching: .any)["sidebar-new-project"].firstMatch
        XCTAssertTrue(new.waitUntilHittable(timeout: MdocUITest.timeout))
        new.tap()
        let field = app.alerts.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: MdocUITest.timeout))
        field.typeText(name)
        app.alerts.buttons["Create"].tap()
        let title = app.descendants(matching: .any)["project-title"].firstMatch
        XCTAssertTrue(title.waitForExistence(timeout: MdocUITest.timeout), "the Project's page")
        XCTAssertEqual(title.label, name)
        XCTAssertFalse(app.navigationBars["Documents"].exists, "never titled Documents")
    }

    /// A Project with one document, added from its page's +; the page
    /// shows again, listing it.
    @MainActor
    private func projectWithADocument(_ app: XCUIApplication) {
        newProject(app, named: "Riverside")
        newDocument(app, from: app.buttons["add-menu"].firstMatch, typing: "Inventory of the flat")
        MdocUITest.revealSidebar(app)
        let project = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'sidebar-project' AND label == 'Riverside'")).firstMatch
        XCTAssertTrue(project.waitUntilHittable(timeout: MdocUITest.timeout))
        project.tap()
        XCTAssertTrue(row(app, "Inventory of the flat").waitForExistence(timeout: MdocUITest.timeout), "added into the Project")
    }

    /// Delete Project… from the page's ⋯ menu, answered `answer`.
    @MainActor
    private func deleteProject(_ app: XCUIApplication, answering answer: String) {
        app.buttons["project-menu"].firstMatch.tap()
        let delete = MdocUITest.menuItem("Delete Project", app)
        XCTAssertTrue(delete.waitForExistence(timeout: MdocUITest.timeout))
        delete.tap()
        let alert = app.alerts.firstMatch
        XCTAssertTrue(alert.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(alert.staticTexts["Delete “Riverside”?"].exists)
        for button in ["Move its documents to Unfiled", "Delete them", "Cancel"] {
            XCTAssertTrue(alert.buttons[button].exists, button)
        }
        alert.buttons[answer].tap()
        // Its page gives way to All Projects, which has none now.
        XCTAssertTrue(app.staticTexts["No Projects yet"].firstMatch.waitForExistence(timeout: MdocUITest.timeout))
    }

    @MainActor
    private func openSection(_ app: XCUIApplication, _ identifier: String) {
        MdocUITest.revealSidebar(app)
        let section = app.descendants(matching: .any)[identifier].firstMatch
        XCTAssertTrue(section.waitUntilHittable(timeout: MdocUITest.timeout))
        section.tap()
    }

    /// The page's three segments, Documents first.
    @MainActor
    func testAProjectsPageHasDocumentsRepliesAndNames() throws {
        let app = launch()
        projectWithADocument(app)
        let segments = app.segmentedControls["project-segments"]
        XCTAssertTrue(segments.waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertEqual(segments.buttons.count, 3)
        XCTAssertTrue(segments.buttons["Documents"].isSelected)
        segments.buttons["Replies"].tap()
        XCTAssertTrue(app.staticTexts["No replies yet"].waitForExistence(timeout: MdocUITest.timeout))
        segments.buttons["Names"].tap()
        XCTAssertTrue(app.staticTexts["No names yet"].waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(app.staticTexts["Always replace"].exists)
        XCTAssertTrue(app.staticTexts["Never replace"].exists)
        segments.buttons["Documents"].tap()
        XCTAssertTrue(row(app, "Inventory of the flat").waitForExistence(timeout: MdocUITest.timeout))
    }

    /// "Move its documents to Unfiled": the document is in Unfiled after.
    @MainActor
    func testDeletingAProjectCanMoveItsDocumentsToUnfiled() throws {
        let app = launch()
        projectWithADocument(app)
        deleteProject(app, answering: "Move its documents to Unfiled")
        openSection(app, "sidebar-unfiled")
        XCTAssertTrue(row(app, "Inventory of the flat").waitForExistence(timeout: MdocUITest.timeout), "moved to Unfiled")
    }

    /// "Delete them": the document goes with the Project.
    @MainActor
    func testDeletingAProjectCanDeleteItsDocuments() throws {
        let app = launch()
        projectWithADocument(app)
        deleteProject(app, answering: "Delete them")
        openSection(app, "sidebar-documents")
        XCTAssertTrue(app.navigationBars["All Documents"].waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(row(app, "Inventory of the flat").waitForNonExistence(timeout: 5), "deleted with it")
    }

    /// Unfiled's Select: two of three documents deleted at once, under one
    /// toast.
    @MainActor
    func testUnfiledDeletesTheDocumentsSelected() throws {
        let app = launch()
        for title in ["First note", "Second note", "Third note"] {
            MdocUITest.revealSidebar(app)
            newDocument(app, from: app.buttons["new-menu"].firstMatch, typing: title)
        }
        openSection(app, "sidebar-unfiled")
        for title in ["First note", "Second note", "Third note"] {
            XCTAssertTrue(row(app, title).waitForExistence(timeout: MdocUITest.timeout), title)
        }
        app.buttons["unfiled-select"].tap()
        row(app, "First note").tap()
        row(app, "Third note").tap()
        let delete = app.buttons["unfiled-delete"].firstMatch
        XCTAssertTrue(delete.waitUntilHittable(timeout: MdocUITest.timeout))
        XCTAssertEqual(delete.label, "Delete 2")
        delete.tap()
        XCTAssertTrue(app.staticTexts["Deleted 2 documents"].waitForExistence(timeout: MdocUITest.timeout))
        XCTAssertTrue(row(app, "First note").waitForNonExistence(timeout: 5))
        XCTAssertFalse(row(app, "Third note").exists)
        XCTAssertTrue(row(app, "Second note").exists, "the one not selected stays")
    }
}
