import SwiftUI
import XCTest
@testable import mdoc

/// Home's rows and Settings' rows (ADR 0099). Lists themselves don't draw
/// offscreen, so the rows are drawn in a card as the list groups them.
@MainActor
final class ScreenSnapshotTests: XCTestCase {
    private let today = Calendar.current.date(bySettingHour: 17, minute: 24, second: 0, of: .now)!
    private let earlier = Calendar.current.date(from: DateComponents(year: 2026, month: 10, day: 28, hour: 12))!

    private func document(_ title: String, _ phase: DocumentPhase, kind: SourceKind, pending: Int = 0, candidates: Int = 0, updated: Date, caseID: String? = nil) -> DocumentSummary {
        DocumentSummary(
            id: DocumentID(rawValue: title), title: title, caseID: caseID.map(CaseID.init), kind: kind, phase: phase,
            pendingCount: pending, updated: updated, candidateCount: candidates)
    }

    func testHomeRows() {
        let agreement = CaseSummary(id: CaseID(rawValue: "a"), name: "Riverside v Bennett", colour: .teal)
        let rows = VStack(alignment: .leading, spacing: Space.s) {
            DocumentRow(
                document: document("Services agreement", .reviewing, kind: .pdf, pending: 11, candidates: 14, updated: today, caseID: "a"),
                caseSummary: agreement, findsNames: true)
            Divider()
            DocumentRow(
                document: document("Riverside v Bennett", .ready, kind: .scan, candidates: 6, updated: earlier, caseID: "a"),
                caseSummary: agreement, findsNames: true)
            Divider()
            DocumentRow(
                document: document("Note from a client", .unscanned, kind: .text, updated: earlier),
                caseSummary: nil, findsNames: true)
        }
        .card()
        .padding()
        .background(Palette.background)
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(rows, named: "home-rows", appearance: appearance)
        }
        Snapshot.assert(rows, named: "home-rows-ax3", size: .accessibility3)
    }

    func testSettingsRows() {
        let detector = ModelOffer(id: "gliner2-fp16", name: "GLiNER2 PII FP16", purpose: .detector, byteSize: 631_500_000, state: .installed)
        let ocr = ModelOffer(id: "ocr", name: "PP-OCRv5", purpose: .ocr, byteSize: 18_000_000, state: .notInstalled)
        let rows = VStack(alignment: .leading, spacing: Space.s) {
            SettingsLabel("Theme", systemImage: "circle.lefthalf.filled", color: .indigo)
            Divider()
            SettingsLabel("Haptics", systemImage: "hand.tap", color: .pink)
            Divider()
            ModelSummaryRow(offer: detector, state: .installed)
            Divider()
            ModelSummaryRow(offer: ocr, state: .downloading(fraction: 0.4))
            Divider()
            SettingsLabel("Documents stay on this iPhone", systemImage: "lock.fill", color: .green)
        }
        .card()
        .padding()
        .background(Palette.background)
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(rows, named: "settings-rows", appearance: appearance)
        }
        Snapshot.assert(rows, named: "settings-rows-ax3", size: .accessibility3)
    }

    /// Search stays while a query is typed, so it can be cleared even when
    /// the list shrinks below the threshold.
    func testSearchStaysWhileAQueryIsTyped() throws {
        let home = HomeViewModel(library: Fixture.core().library)
        XCTAssertLessThan(home.documents.count, HomeViewModel.searchThreshold)
        XCTAssertFalse(home.offersSearch)
        home.query = "x"
        XCTAssertTrue(home.offersSearch)
        home.query = ""
        XCTAssertFalse(home.offersSearch)
    }

    func testThemeMapsToTheSystemStyles() {
        XCTAssertNil(AppTheme.automatic.colorScheme)
        XCTAssertEqual(AppTheme.light.colorScheme, .light)
        XCTAssertEqual(AppTheme.dark.colorScheme, .dark)
        XCTAssertEqual(AppTheme.dark.interfaceStyle, .dark)
        XCTAssertEqual(AppTheme(rawValue: "dark"), .dark)
    }

    func testSerifSwitchesTheDocumentFont() {
        let store = MarkdownDisplay.store
        let before = store.bool(forKey: MarkdownDisplay.serifKey)
        defer { store.set(before, forKey: MarkdownDisplay.serifKey) }
        let traits = UITraitCollection(preferredContentSizeCategory: .large)
        store.set(false, forKey: MarkdownDisplay.serifKey)
        let system = MarkdownStyler.font(.body, traits: traits)
        store.set(true, forKey: MarkdownDisplay.serifKey)
        let serif = MarkdownStyler.font(.body, traits: traits)
        XCTAssertEqual(system.pointSize, serif.pointSize)
        XCTAssertNotEqual(system.fontName, serif.fontName)
    }

    func testMemoryReadsAsSold() {
        XCTAssertEqual(ModelDetailView.memoryText(12_294_000_000), "12 GB")
        XCTAssertEqual(ModelDetailView.memoryText(7_900_000_000), "8 GB")
    }

    func testHomeSearchAndCaseFilter() {
        let home = HomeViewModel(library: Fixture.core().library)
        XCTAssertEqual(home.shownDocuments.count, home.documents.count)
        let first = try! XCTUnwrap(home.documents.first)
        home.query = String(first.title.prefix(4))
        XCTAssertTrue(home.shownDocuments.contains(first))
        home.query = "zzzz-no-such-document"
        XCTAssertTrue(home.shownDocuments.isEmpty)
        home.query = ""
        if let caseID = first.caseID {
            home.caseFilter = caseID
            XCTAssertTrue(home.shownDocuments.allSatisfy { $0.caseID == caseID })
        }
    }
}
