import UIKit
import XCTest
@testable import mdoc

/// Edit mode in a document with a large table (ADR 0114, slice A), on the
/// real core: the pencil leaves the grids' cells alone inside SwiftUI's
/// update, ends a cell's selection on the way in, touches the cells only
/// when the mode changes, and keeps the line at the top of the view.
@MainActor
final class LargeTableEditModeTests: XCTestCase {
    /// A two-column contract table long enough to scroll through, between
    /// paragraphs, as the rc.9 contract (invented text).
    static func text(rows: Int = 40, paragraphs: Int = 30) -> String {
        var lines = ["# Contract", "", "| Clause | Terms |", "|---|---|"]
        for row in 1...rows {
            lines.append(
                "| \(row).1. The seller delivers the goods named in annex \(row) within ten days. "
                    + "| \(row).2. The buyer pays for them within thirty days of delivery, by bank transfer. |")
        }
        lines.append("")
        for paragraph in 1...paragraphs {
            lines.append("Paragraph \(paragraph). Either party may end this contract by written notice.")
            lines.append("")
        }
        return lines.joined(separator: "\n")
    }

    typealias Harness = EditToolbarTests.Harness

    /// The document shown in review, Show Markdown off.
    private func harness() throws -> Harness {
        let (core, storage) = try Fixture.core()
        let session = try core.library.importDocument(.text(Self.text()), title: "Contract", into: .newCase(name: "Contract"))
        _ = try session.recognize(observer: TestObserver())
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: storage)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let view = CandidateTextView()
        view.frame = window.bounds
        window.addSubview(view)
        window.isHidden = false
        let coordinator = MarkdownTextView.Coordinator(parent: LivePreviewTests.textView(model, rendered: true))
        coordinator.attach(to: view)
        view.delegate = coordinator
        let harness = Harness(
            session: session, model: model, window: window, view: view, coordinator: coordinator, rendered: true)
        update(harness, inViewUpdate: false)
        XCTAssertFalse(view.isEditMode)
        return harness
    }

    /// What SwiftUI does when the model changes.
    private func update(_ harness: Harness, inViewUpdate: Bool = true) {
        harness.coordinator.update(
            harness.view, with: LivePreviewTests.textView(harness.model, rendered: true), dynamicTypeSize: .large,
            colorScheme: .light, inViewUpdate: inViewUpdate)
        harness.view.layoutIfNeeded()
    }

    /// Lets what waits for the end of SwiftUI's update run.
    private func settle() {
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
    }

    private func grid(_ harness: Harness) throws -> MarkdownTableView {
        try XCTUnwrap(harness.view.tableViews.first)
    }

    /// The rc.9 hang: a word in a cell selected (a long press, for Hide
    /// name), then the pencil. Inside SwiftUI's update the cells stopped
    /// taking touches, the selected cell resigned the first responder, and
    /// SwiftUI's hosting view, asked to take over, read the view graph it
    /// was updating: on iOS 27 that never returned and iOS stopped mdoc.
    /// Now the update leaves the cells as they are; after it the cell's
    /// selection ends and the grid switches.
    func testThePencilLeavesASelectedCellAloneInsideTheUpdate() throws {
        let harness = try harness()
        let cell = try grid(harness).cells[3][0]
        XCTAssertTrue(cell.isUserInteractionEnabled)
        XCTAssertTrue(cell.becomeFirstResponder())
        cell.selectedRange = NSRange(location: 0, length: 4)
        harness.model.enterEditMode()
        update(harness)
        update(harness)
        XCTAssertTrue(cell.isUserInteractionEnabled, "a cell switched inside SwiftUI's update")
        XCTAssertTrue(cell.isFirstResponder, "the first responder moved inside SwiftUI's update")
        settle()
        XCTAssertTrue(harness.view.isEditMode)
        XCTAssertFalse(cell.isUserInteractionEnabled)
        XCTAssertFalse(cell.isFirstResponder)
        XCTAssertEqual(cell.selectedRange.length, 0, "the cell's selection ended")
        XCTAssertNotNil(harness.view.onTableTap)
        // Done gives the cells their touches back, after the update too.
        harness.model.doneEditing()
        update(harness)
        settle()
        XCTAssertFalse(harness.view.isEditMode)
        XCTAssertTrue(cell.isUserInteractionEnabled)
        XCTAssertNil(harness.view.onTableTap)
    }

    /// The pencil at row 17 of the rc.9 contract's table showed the end of
    /// the document. The text view was the first responder (a tap on the
    /// text, or a cell's selection ending hands it the focus), its caret
    /// where UIKit leaves it in review, at the text's end, and a first
    /// responder made editable scrolls to its caret. The line at the top of
    /// the view stays there (ADR 0110).
    func testThePencilInATableKeepsThePlace() throws {
        let harness = try harness()
        let view = harness.view
        let table = try grid(harness)
        let row = table.cells[17][0]
        view.setContentOffset(
            CGPoint(x: 0, y: table.frame.minY + row.frame.minY - view.adjustedContentInset.top), animated: false)
        view.layoutIfNeeded()
        let before = view.contentOffset.y
        XCTAssertGreaterThan(before, 1000)
        XCTAssertTrue(view.becomeFirstResponder())
        view.selectedRange = NSRange(location: view.textStorage.length, length: 0)
        harness.model.enterEditMode()
        update(harness)
        settle()
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        XCTAssertTrue(view.isEditMode)
        XCTAssertEqual(view.contentOffset.y, before, accuracy: 2, "the pencil moved the text")
        XCTAssertFalse(view.isFirstResponder, "no selection: the keyboard waits for a tap")
    }

    /// The same from a word selected in a cell, as on the phone.
    func testThePencilAfterASelectionInACellKeepsThePlace() throws {
        let harness = try harness()
        let view = harness.view
        let table = try grid(harness)
        let cell = table.cells[17][0]
        view.setContentOffset(
            CGPoint(x: 0, y: table.frame.minY + cell.frame.minY - view.adjustedContentInset.top), animated: false)
        view.layoutIfNeeded()
        let before = view.contentOffset.y
        view.selectedRange = NSRange(location: view.textStorage.length, length: 0)
        XCTAssertTrue(cell.becomeFirstResponder())
        cell.selectedRange = NSRange(location: 0, length: 4)
        harness.model.enterEditMode()
        update(harness)
        settle()
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        XCTAssertTrue(view.isEditMode)
        XCTAssertEqual(view.contentOffset.y, before, accuracy: 2, "the pencil moved the text")
    }

    /// Every update ran through every cell of every table (280 in the
    /// rc.9 contract's table), in review and in edit mode: an update that
    /// does not change the mode leaves the cells alone.
    func testAnUpdateThatKeepsTheModeLeavesTheCellsAlone() throws {
        let harness = try harness()
        let cell = try grid(harness).cells[5][1]
        // A mark no update makes: any write of the cells shows.
        cell.isUserInteractionEnabled = false
        update(harness)
        settle()
        XCTAssertFalse(cell.isUserInteractionEnabled, "a review update switched the cells")
        cell.isUserInteractionEnabled = true

        harness.model.enterEditMode()
        update(harness)
        settle()
        XCTAssertFalse(cell.isUserInteractionEnabled)
        cell.isUserInteractionEnabled = true
        update(harness)
        settle()
        XCTAssertTrue(cell.isUserInteractionEnabled, "an edit-mode update switched the cells")
    }
}
