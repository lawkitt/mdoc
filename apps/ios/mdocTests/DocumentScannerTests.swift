import XCTest
@testable import mdoc

/// VisionKit's document camera, as its delegate hears it.
@MainActor
final class DocumentScannerTests: XCTestCase {
    private struct CameraBusy: LocalizedError {
        var errorDescription: String? { "The camera is in use by another app." }
    }

    /// A camera that fails says so: the sheet closes with its error, not
    /// as if the user cancelled, and the app shows the error.
    func testAScannerErrorIsShownNotTakenForACancel() throws {
        var cancelled = false
        var failed: Error?
        let scanner = DocumentScanner(onScan: { _ in XCTFail("no scan") }, onCancel: { cancelled = true }, onFail: { failed = $0 })
        let coordinator = scanner.makeCoordinator()
        coordinator.cameraFailed(CameraBusy())
        XCTAssertFalse(cancelled)
        XCTAssertEqual(failed?.localizedDescription, "The camera is in use by another app.")

        // The Presenter hands it to the app, whose error replaces the camera.
        let app = Fixture.app(try Fixture.seeded())
        app.navigator.present(.intake(.scan))
        app.present(try XCTUnwrap(failed))
        guard case .error(let message) = app.navigator.presentation else { return XCTFail("the error shows") }
        XCTAssertEqual(message, "The camera is in use by another app.")
    }
}
