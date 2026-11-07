import SwiftUI
import XCTest
@testable import mdoc

/// The design components (docs/mobile-ui.md) in light and dark, and at an
/// accessibility size where it changes their layout.
@MainActor
final class ComponentSnapshotTests: XCTestCase {
    func testChips() {
        let chips = VStack(alignment: .leading, spacing: Space.s) {
            HStack {
                Chip("Person", systemImage: "person", tint: .blue, opensMenu: true)
                Chip("PERSON_1", opensMenu: true, monospaced: true)
                Chip("All 6", opensMenu: true)
            }
            Chip("Same as", systemImage: "link", tint: .teal)
        }
        .padding()
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(chips, named: "chips", appearance: appearance)
        }
        Snapshot.assert(chips, named: "chips-ax3", size: .accessibility3)
    }

    func testStatusPills() {
        let pills = VStack(alignment: .leading, spacing: Space.xs) {
            StatusPill("11 to review", systemImage: "exclamationmark.circle", kind: .toReview)
            StatusPill("Ready to share", systemImage: "checkmark.circle", kind: .ready)
            StatusPill("Reply restored", systemImage: "arrow.uturn.backward.circle", kind: .restored)
            StatusPill("Recognizing text", systemImage: "text.viewfinder", kind: .neutral)
        }
        .padding()
        .frame(maxWidth: .infinity, alignment: .leading)
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(pills, named: "status-pills", appearance: appearance)
        }
    }

    func testIconTiles() {
        let tiles = HStack(spacing: Space.s) {
            IconTile(systemImage: "paintbrush", color: .blue)
            IconTile(systemImage: "lock", color: .green)
            IconTile(systemImage: "doc.richtext", color: .red, tinted: true, size: 40)
        }
        .padding()
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(tiles, named: "icon-tiles", appearance: appearance)
        }
    }

    func testCardAndToast() {
        let view = VStack(spacing: Space.m) {
            VStack(alignment: .leading, spacing: Space.xs) {
                Text(verbatim: "Riverside Holdings Ltd").font(Typography.title)
                Text(verbatim: "Organization · ORG_1").font(Typography.caption).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .card()
            Toast(message: ToastMessage(text: "12 replaced", actionTitle: "Undo", action: {}), opaque: true)
            Toast(message: ToastMessage(text: "All 15 reviewed"), opaque: true)
        }
        .padding()
        .background(Palette.background)
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(view, named: "card-toast", appearance: appearance)
        }
    }

    func testToastActionKeepsTheToastItShows() {
        var shown: ToastMessage? = ToastMessage(text: "12 replaced")
        let undo = ToastMessage(text: "12 replaced", actionTitle: "Undo") {
            shown = ToastMessage(text: "Undid Replace all", actionTitle: "Redo", action: {})
        }
        shown = undo
        Toast(message: undo) { if shown?.id == undo.id { shown = nil } }.perform()
        XCTAssertEqual(shown?.text, "Undid Replace all")
    }

    func testToastCountdownCountsTheSecondsLeft() {
        let start = Date(timeIntervalSinceReferenceDate: 1000)
        let countdown = ToastCountdown(start: start, duration: 6)
        XCTAssertEqual(countdown.seconds(at: start), 6)
        XCTAssertEqual(countdown.fraction(at: start), 1)
        XCTAssertEqual(countdown.seconds(at: start.addingTimeInterval(0.5)), 6)
        XCTAssertEqual(countdown.seconds(at: start.addingTimeInterval(5.5)), 1)
        XCTAssertEqual(countdown.fraction(at: start.addingTimeInterval(3)), 0.5)
        XCTAssertEqual(countdown.seconds(at: start.addingTimeInterval(9)), 0, "never below zero")
        XCTAssertEqual(countdown.fraction(at: start.addingTimeInterval(9)), 0)
        XCTAssertEqual(countdown.fraction(at: start.addingTimeInterval(-1)), 1, "never above full")
    }

    /// The ring at default size, and at the largest, where "Undo" stays whole.
    func testToastCountdownRing() {
        let undo = ToastMessage(text: "Same as Maya Bennett", actionTitle: "Undo", action: {})
        let view = VStack(spacing: Space.m) {
            Toast(message: undo, duration: 6, frozenAt: undo.created.addingTimeInterval(2), opaque: true)
            Toast(message: undo, duration: 6, frozenAt: undo.created.addingTimeInterval(2), opaque: true)
                .environment(\.dynamicTypeSize, .accessibility5)
        }
        .padding()
        .background(Palette.background)
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(view, named: "toast-countdown", appearance: appearance)
        }
    }

    /// A toast's view going (the Identities drawer closing) ends its wait,
    /// not the toast: the message stays for the screen still showing it.
    func testACancelledToastWaitLeavesTheToast() async {
        let wait = Task { await ToastPresenter.waitOut(ToastCountdown(start: .now, duration: 60)) }
        wait.cancel()
        let ranOut = await wait.value
        XCTAssertFalse(ranOut)
    }

    /// One shown again carries on the same clock: nothing left, it ends now.
    func testAToastWaitRunsOnItsOwnClock() async {
        let start = Date.now.addingTimeInterval(-10)
        let began = Date.now
        let ranOut = await ToastPresenter.waitOut(ToastCountdown(start: start, duration: 6))
        XCTAssertTrue(ranOut)
        XCTAssertLessThan(Date.now.timeIntervalSince(began), 1)
    }

    /// The ring shows the time the toast really stays.
    func testToastStaysLongerWithAnAction() {
        let undo = ToastMessage(text: "12 replaced", actionTitle: "Undo", action: {})
        XCTAssertEqual(ToastPresenter.duration(of: undo, seconds: 4), 6)
        XCTAssertEqual(ToastPresenter.duration(of: ToastMessage(text: "All 15 reviewed"), seconds: 4), 4)
    }

    func testSampleChoosesItsLanguage() {
        XCTAssertEqual(SampleDocument.forLanguages(["ru-RU", "en-GB"]), .russian)
        XCTAssertEqual(SampleDocument.forLanguages(["en-GB", "ru-RU"]), .english)
        XCTAssertEqual(SampleDocument.forLanguages([]), .english)
    }

    func testSampleNamesAppearInTheirText() {
        for sample in [SampleDocument.english, .russian] {
            for name in sample.names {
                XCTAssertTrue(sample.text.contains(name.wording), "\(name.wording) is not in \(sample.title)")
            }
        }
    }
}
