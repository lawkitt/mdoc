import UIKit
import XCTest
@testable import mdoc

/// A cancelled multi-page OCR import resumes with the pages left and gives
/// the text an uninterrupted run gives (ADR 0085 Long jobs), on the
/// Simulator with the real core, the real OCR model and the app's scan path
/// (camera pages become one image-only PDF). It downloads the OCR model, so
/// it runs only when asked: `MDOC_OCR_TESTS=1`
/// (`TEST_RUNNER_MDOC_OCR_TESTS=1` through xcodebuild).
final class OCRResumeTests: XCTestCase {
    private var storage: MdocStorage!
    private var core: Core!

    override func setUp() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MDOC_OCR_TESTS"] == "1", "OCR tests not requested")
        storage = try Fixture.temporaryStorage()
        core = try RealCore.make(storage: storage, keys: FixedCaseKeyStore(), physicalMemory: 4 * Fixture.gigabyte)
        try await Self.installOCR(into: core)
    }

    override func tearDown() {
        if let storage {
            try? FileManager.default.removeItem(at: storage.root)
        }
    }

    func testACancelledScanResumesWithThePagesLeft() throws {
        let full = try core.library.importDocument(.scan(try pages("full")), title: "Full", into: .newCase(name: "Full"))
        let uninterrupted = try full.recognize(observer: TestObserver()).markdown
        XCTAssertTrue(uninterrupted.contains("Harbour Street"), uninterrupted)
        XCTAssertEqual(keptPages(), 0)

        // Cancel once the core has kept a page, as the Cancel button does.
        let stopped = try core.library.importDocument(.scan(try pages("stopped")), title: "Stopped", into: .newCase(name: "Stopped"))
        XCTAssertThrowsError(try stopped.recognize(observer: cancelAfterAPage())) { error in
            XCTAssertEqual(error as? CoreError, .cancelled)
        }
        let kept = keptPages()
        XCTAssertGreaterThan(kept, 0)
        XCTAssertLessThan(kept, Self.pageCount)
        XCTAssertTrue(stopped.snapshot().resumesOCR)

        // Resume, after mdoc stopped too: the next launch finds the import.
        core = try RealCore.make(storage: storage, keys: FixedCaseKeyStore(), physicalMemory: 4 * Fixture.gigabyte)
        let resumed = try core.library.session(for: stopped.id)
        XCTAssertTrue(resumed.snapshot().resumesOCR)
        let observer = TestObserver()
        let text = try resumed.recognize(observer: observer).markdown
        XCTAssertEqual(text, uninterrupted)
        XCTAssertFalse(resumed.snapshot().resumesOCR)
        XCTAssertEqual(keptPages(), 0)
    }

    func testDroppingAStoppedScanDiscardsItsPages() throws {
        let stopped = try core.library.importDocument(.scan(try pages("dropped")), title: "Dropped", into: .newCase(name: "Dropped"))
        XCTAssertThrowsError(try stopped.recognize(observer: cancelAfterAPage()))
        XCTAssertGreaterThan(keptPages(), 0)
        try core.library.deleteDocument(stopped.id)
        XCTAssertEqual(keptPages(), 0)
    }

    /// A scan asks for text recognition alone, and recognizes once it is in,
    /// with the name detector still missing (ADR 0090).
    @MainActor
    func testMissingOCRSetsUpThenRecognizes() async throws {
        let fresh = try Fixture.temporaryStorage()
        defer { try? FileManager.default.removeItem(at: fresh.root) }
        let without = try RealCore.make(storage: fresh, keys: FixedCaseKeyStore(), physicalMemory: Fixture.detectorTier)
        let session = try without.library.importDocument(.scan(try pages("setup")), title: "Setup", into: .newCase(name: "Setup"))
        let model = DocumentViewModel(
            session: session, jobs: LongJobRunner(background: NoBackgroundContinuation()), storage: fresh)
        await model.recognize()
        XCTAssertTrue(model.needsSetup)
        XCTAssertEqual(model.waitingForSetup, .recognize)
        XCTAssertEqual(model.setupNeeds, .ocr)
        XCTAssertNil(model.errorMessage)
        try await Self.installOCR(into: without)
        await model.continueAfterSetup()
        XCTAssertFalse(model.needsSetup)
        XCTAssertNil(model.errorMessage)
        XCTAssertEqual(model.snapshot.phase, .unscanned)
    }

    // MARK: Support

    private static let pageCount = 4

    /// Rendered pages of a fictional tenancy, as the camera would give them.
    private func pages(_ name: String) throws -> [URL] {
        let folder = storage.url(.originals).appendingPathComponent(name, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return try (1...Self.pageCount).map { number in
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
            let url = folder.appendingPathComponent("page-\(number).jpg")
            try XCTUnwrap(image.jpegData(compressionQuality: 0.9)).write(to: url)
            return url
        }
    }

    private func keptPages() -> Int {
        Self.keptPages(in: storage.url(.imports))
    }

    /// Pages the core keeps under `imports/`, across every import.
    private static func keptPages(in imports: URL) -> Int {
        let folders = (try? FileManager.default.contentsOfDirectory(at: imports, includingPropertiesForKeys: nil)) ?? []
        return folders.reduce(0) { count, folder in
            count + ((try? FileManager.default.contentsOfDirectory(atPath: folder.path))?.count ?? 0)
        }
    }

    /// An observer that cancels once a page is kept.
    private func cancelAfterAPage() -> TestObserver {
        let observer = TestObserver()
        let imports = storage.url(.imports)
        let watch = Thread {
            let deadline = Date().addingTimeInterval(300)
            while Date() < deadline, !observer.isCancelled() {
                if Self.keptPages(in: imports) > 0 {
                    observer.cancel()
                    return
                }
                Thread.sleep(forTimeInterval: 0.02)
            }
        }
        watch.start()
        return observer
    }

    /// Downloads the OCR model's files and has the core verify and install them.
    private static func installOCR(into core: Core) async throws {
        let setup = try XCTUnwrap(core.setup as? RealSetup)
        for request in try setup.downloadRequests(for: "ocr-english-russian") {
            let (file, response) = try await URLSession.shared.download(from: request.url)
            XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200, request.url.absoluteString)
            _ = try setup.install(request, from: file)
        }
        XCTAssertNotNil(setup.ocrPaths(), "OCR is not installed")
    }
}
