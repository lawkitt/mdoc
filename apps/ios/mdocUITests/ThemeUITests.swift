import UIKit
import XCTest

/// A theme picked in Settings shows in Settings at once, not only once it
/// is opened again (#1036); Automatic follows the system after Light or
/// Dark, and the sidebar's theme button flips Light and Dark (#1070).
final class ThemeUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testTheThemePickedShowsInTheOpenSettingsSheet() throws {
        let app = MdocUITest.launch()
        MdocUITest.revealSidebar(app)
        let settings = app.buttons["settings-button"].firstMatch
        XCTAssertTrue(settings.waitUntilHittable(timeout: MdocUITest.timeout))
        settings.tap()
        let theme = app.descendants(matching: .any)["settings-theme"].firstMatch
        XCTAssertTrue(theme.waitForExistence(timeout: MdocUITest.timeout))
        // Show Markdown is the text's, in its title menu.
        XCTAssertFalse(app.descendants(matching: .any)["settings-show-markdown"].firstMatch.exists)
        for (choice, dark) in [("Dark", true), ("Light", false), ("Dark", true)] {
            theme.buttons[choice].tap()
            // Just above the picker: its row's card, away from any text.
            let frame = theme.frame
            let point = CGPoint(x: frame.minX + frame.width * 0.75, y: frame.minY - 10)
            let shown = MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: point) == dark }
            XCTAssertTrue(shown, "\(choice) shows in the open sheet")
        }
    }

    /// Automatic after Dark shows the system's appearance at once: light
    /// on the Simulator, whose appearance a test leaves alone.
    @MainActor
    func testAutomaticFollowsTheSystemAfterDark() throws {
        let app = MdocUITest.launch()
        MdocUITest.revealSidebar(app)
        let settings = app.buttons["settings-button"].firstMatch
        XCTAssertTrue(settings.waitUntilHittable(timeout: MdocUITest.timeout))
        settings.tap()
        let theme = app.descendants(matching: .any)["settings-theme"].firstMatch
        XCTAssertTrue(theme.waitForExistence(timeout: MdocUITest.timeout))
        let frame = theme.frame
        let point = CGPoint(x: frame.minX + frame.width * 0.75, y: frame.minY - 10)
        let systemDark = try XCTUnwrap(MdocUITest.isDark(at: point), "the sheet as the system draws it")
        theme.buttons[systemDark ? "Light" : "Dark"].tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: point) == !systemDark }, "the other theme")
        theme.buttons["Automatic"].tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: point) == systemDark }, "Automatic is the system's again")
        app.buttons["Done"].firstMatch.tap()
        XCTAssertTrue(theme.waitForNonExistence(timeout: MdocUITest.timeout))
        // The screen under the sheet follows too.
        MdocUITest.revealSidebar(app)
        let search = app.descendants(matching: .any)["sidebar-search"].firstMatch
        XCTAssertTrue(search.waitUntilHittable(timeout: MdocUITest.timeout))
        let beside = CGPoint(x: search.frame.minX + 8, y: search.frame.maxY + 24)
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: beside) == systemDark }, "the sidebar too")
    }

    /// The sidebar's theme button flips to the other theme, and its value
    /// says which is chosen.
    @MainActor
    func testTheSidebarThemeButtonFlipsLightAndDark() throws {
        let app = MdocUITest.launch()
        MdocUITest.revealSidebar(app)
        let button = app.buttons["theme-button"].firstMatch
        XCTAssertTrue(button.waitUntilHittable(timeout: MdocUITest.timeout))
        XCTAssertEqual(button.value as? String, "Automatic")
        let search = app.descendants(matching: .any)["sidebar-search"].firstMatch
        let beside = CGPoint(x: search.frame.minX + 8, y: search.frame.maxY + 24)
        let startsDark = try XCTUnwrap(MdocUITest.isDark(at: beside))
        button.tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: beside) == !startsDark }, "flipped")
        XCTAssertEqual(button.value as? String, startsDark ? "Light" : "Dark")
        button.tap()
        XCTAssertTrue(MdocUITest.wait(timeout: 5) { MdocUITest.isDark(at: beside) == startsDark }, "flipped back")
        XCTAssertEqual(button.value as? String, startsDark ? "Dark" : "Light")
    }
}

extension MdocUITest {
    /// Whether the screen is dark at `point`, in points.
    @MainActor
    static func isDark(at point: CGPoint) -> Bool? {
        let image = XCUIScreen.main.screenshot().image
        guard let cgImage = image.cgImage else { return nil }
        let scale = CGFloat(cgImage.width) / image.size.width
        let x = Int(point.x * scale), y = Int(point.y * scale)
        guard x >= 0, y >= 0, x < cgImage.width, y < cgImage.height else { return nil }
        var pixel = [UInt8](repeating: 0, count: 4)
        guard let context = CGContext(
            data: &pixel, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        context.draw(cgImage, in: CGRect(x: -x, y: y + 1 - cgImage.height, width: cgImage.width, height: cgImage.height))
        let luminance = (0.299 * Double(pixel[0]) + 0.587 * Double(pixel[1]) + 0.114 * Double(pixel[2])) / 255
        return luminance < 0.5
    }
}
