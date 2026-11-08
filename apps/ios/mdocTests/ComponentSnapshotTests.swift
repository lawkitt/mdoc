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

    /// The big buttons and what sits on the ink accent (ADR 0099, amended
    /// 2026-11-08): the primary is ink glass with white words in light mode
    /// and black in dark mode, the secondary clear glass with ink words, and
    /// Undo a clear glass circle; the Replaced mark's symbol and the selected
    /// chip's words read on ink; the progress ring is ink.
    func testAccentControls() {
        let view = VStack(alignment: .leading, spacing: Space.m) {
            HStack(spacing: Space.s) {
                Button {} label: {
                    Label(String("Undo"), systemImage: "arrow.uturn.backward")
                        .labelStyle(.iconOnly)
                }
                .secondaryAction(circle: true)
                Button {} label: {
                    Text(verbatim: "Keep").frame(maxWidth: .infinity)
                }
                .secondaryAction()
                Button {} label: {
                    Label(String("Replace 3"), systemImage: "eye.slash")
                        .fontWeight(.semibold)
                        .frame(maxWidth: .infinity)
                }
                .primaryAction()
            }
            // Disabled: grey words, not white on grey.
            HStack(spacing: Space.s) {
                Button {} label: {
                    Text(verbatim: "Replace").frame(maxWidth: .infinity)
                }
                .primaryAction()
                Button {} label: {
                    Text(verbatim: "Same as").frame(maxWidth: .infinity)
                }
                .secondaryAction()
            }
            .disabled(true)
            HStack(spacing: Space.s) {
                StateMark(state: .replaced)
                StateMark(state: .kept)
                StateMark(state: .proposed)
                ProgressRing(fraction: 0.6)
                Chip(Text(verbatim: "All"), filled: true)
                Chip(Text(verbatim: "Person 2"), systemImage: "person")
                Button(String("Select")) {}
                    .font(Typography.chip)
                    .textAction()
            }
        }
        .padding()
        .background(Palette.background)
        for appearance in Snapshot.Appearance.allCases {
            Snapshot.assert(view, named: "accent-controls", appearance: appearance)
        }
    }

    /// A button's size as laid out, without safe areas.
    private func size(_ view: some View, _ size: DynamicTypeSize = .large) -> CGSize {
        let host = UIHostingController(rootView: view.environment(\.dynamicTypeSize, size))
        host.safeAreaRegions = []
        return host.sizeThatFits(in: CGSize(width: 402, height: 874))
    }

    /// Every big button is 44 pt high at the default text size and never
    /// less (ADR 0099, amended 2026-11-08): the system's glass adds
    /// `BigButton.systemPadding` above and below the label, and a change to
    /// that shows here. Undo is a 44 pt circle.
    func testBigButtonsAre44PointsHigh() {
        let keep = Button {} label: { Text(verbatim: "Keep") }
        let replace = Button {} label: { Label(String("Replace all 3"), systemImage: "eye.slash") }
        let undo = Button {} label: { Label(String("Undo"), systemImage: "arrow.uturn.backward").labelStyle(.iconOnly) }
        XCTAssertEqual(size(keep.secondaryAction()).height, 44, accuracy: 0.5, "secondary")
        XCTAssertEqual(size(replace.primaryAction()).height, 44, accuracy: 0.5, "primary")
        XCTAssertEqual(size(undo.secondaryAction(circle: true)).height, 44, accuracy: 0.5, "Undo")
        XCTAssertEqual(size(undo.secondaryAction(circle: true)).width, 44, accuracy: 0.5, "Undo is round")
        for small in [DynamicTypeSize.xSmall, .small] {
            XCTAssertGreaterThanOrEqual(size(keep.secondaryAction(), small).height, 44 - 0.5, "\(small)")
            XCTAssertGreaterThanOrEqual(size(replace.primaryAction(), small).height, 44 - 0.5, "\(small)")
        }
        XCTAssertGreaterThan(size(keep.secondaryAction(), .accessibility2).height, 44, "grows with the text")
        let round = size(undo.secondaryAction(circle: true), .accessibility2)
        XCTAssertEqual(round.width, round.height, accuracy: 0.5, "Undo stays round at AX2")
        XCTAssertGreaterThan(round.height, 44, "and grows with the text")
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

    /// What a screen draws in the safe areas stays on screen: the drawer
    /// clipped to its frame, which cut the bottom toolbar's buttons in half.
    func testSideDrawerKeepsWhatTheScreenDrawsInTheSafeAreas() throws {
        let host = UIHostingController(rootView: SideDrawer(isOpen: .constant(false)) {
            Color.clear
        } content: {
            Color(red: 1, green: 0, blue: 0).ignoresSafeArea()
        })
        host.additionalSafeAreaInsets = UIEdgeInsets(top: 60, left: 0, bottom: 40, right: 0)
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 300, height: 600)
        window.rootViewController = host
        window.isHidden = false
        host.view.layoutIfNeeded()
        let format = UIGraphicsImageRendererFormat()
        format.preferredRange = .standard // 8 bits a channel
        let image = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in
            window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
        }
        window.isHidden = true
        let data = try XCTUnwrap(image.cgImage?.dataProvider?.data as Data?)
        let cgImage = try XCTUnwrap(image.cgImage)
        func pixel(_ y: CGFloat) -> [UInt8] {
            let row = Int(y * image.scale), column = cgImage.width / 2
            let start = row * cgImage.bytesPerRow + column * cgImage.bitsPerPixel / 8
            return Array(data[start..<start + 4])
        }
        // BGRA or RGBA: red is the one bright colour channel either way.
        func isRed(_ bytes: [UInt8]) -> Bool {
            bytes[1] < 40 && (bytes[0] > 200) != (bytes[2] > 200)
        }
        XCTAssertTrue(isRed(pixel(300)), "the middle of the screen: \(pixel(300))")
        XCTAssertTrue(isRed(pixel(590)), "the bottom safe area: \(pixel(590))")
        XCTAssertTrue(isRed(pixel(10)), "the top safe area: \(pixel(10))")
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
