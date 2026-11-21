import Foundation
import ImageIO
import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import mdoc

/// Photos and camera pages as the app keeps them (#1024 item 10, #1021):
/// formats the core reads go to it as they are, without their metadata;
/// HEIC is drawn upright as a JPEG by iOS; several are one document.
final class ImporterTests: XCTestCase {
    private var storage: MdocStorage!

    override func setUpWithError() throws {
        storage = try Fixture.temporaryStorage()
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: storage.root)
    }

    private func properties(_ url: URL) throws -> [CFString: Any] {
        let source = try XCTUnwrap(CGImageSourceCreateWithURL(url as CFURL, nil))
        return try XCTUnwrap(CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any])
    }

    /// A camera JPEG with a GPS position, a camera and orientation 6, as
    /// ImageIO writes one.
    private func cameraJPEG() throws -> Data {
        let image = UIGraphicsImageRenderer(size: CGSize(width: 40, height: 30)).image { context in
            UIColor.red.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 10, height: 10))
        }
        let data = NSMutableData()
        let destination = try XCTUnwrap(
            CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil))
        let metadata: [CFString: Any] = [
            kCGImagePropertyOrientation: 6,
            kCGImagePropertyTIFFDictionary: [kCGImagePropertyTIFFMake: "Fictional Camera Co"],
            kCGImagePropertyGPSDictionary: [
                kCGImagePropertyGPSLatitude: 12.3456, kCGImagePropertyGPSLatitudeRef: "N",
            ],
        ]
        CGImageDestinationAddImage(destination, try XCTUnwrap(image.cgImage), metadata as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        return data as Data
    }

    /// A camera JPEG is kept as it is, its GPS position and camera gone and
    /// its orientation kept, so the Original shows it upright.
    func testAPhotoIsKeptWithoutItsMetadata() throws {
        let importer = Importer(storage: storage)
        let pending = try XCTUnwrap(try importer.images([try cameraJPEG()], scanned: false, title: "Receipt"))
        guard case .photos(let pages) = pending.source else { return XCTFail("\(pending.source)") }
        let page = try XCTUnwrap(pages.first)
        XCTAssertEqual(page.pathExtension, "jpg")
        let kept = try properties(page)
        XCTAssertNil(kept[kCGImagePropertyGPSDictionary])
        XCTAssertNil((kept[kCGImagePropertyTIFFDictionary] as? [CFString: Any])?[kCGImagePropertyTIFFMake])
        XCTAssertEqual(kept[kCGImagePropertyOrientation] as? Int, 6)
        XCTAssertEqual(kept[kCGImagePropertyPixelWidth] as? Int, 40)
    }

    /// A PNG goes to the core as a PNG, never "save it as JPEG".
    func testAPNGStaysAPNG() throws {
        let png = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).pngData { _ in })
        let pending = try XCTUnwrap(try Importer(storage: storage).images([png], scanned: false))
        guard case .photos(let pages) = pending.source else { return XCTFail("\(pending.source)") }
        XCTAssertEqual(pages.map(\.pathExtension), ["png"])
    }

    /// A HEIC is drawn by iOS as an upright JPEG without its GPS position,
    /// camera or date: the fixture stored turned with orientation 6 comes
    /// out 1200 × 1600, and the GPS one loses its metadata.
    func testAHEICBecomesAnUprightJPEGWithoutMetadata() throws {
        let fixtures = Fixture.repository.appendingPathComponent("tests/fixtures/heic")
        let turned = try Data(contentsOf: fixtures.appendingPathComponent("page-orientation-6.heic"))
        let gps = try Data(contentsOf: fixtures.appendingPathComponent("page-gps.heic"))
        let pending = try XCTUnwrap(try Importer(storage: storage).images([turned, gps], scanned: false))
        guard case .photos(let pages) = pending.source else { return XCTFail("\(pending.source)") }
        XCTAssertEqual(pages.map(\.pathExtension), ["jpg", "jpg"])
        for page in pages {
            let kept = try properties(page)
            XCTAssertEqual(kept[kCGImagePropertyPixelWidth] as? Int, 1200)
            XCTAssertEqual(kept[kCGImagePropertyPixelHeight] as? Int, 1600)
            XCTAssertEqual(kept[kCGImagePropertyOrientation] as? Int ?? 1, 1)
            XCTAssertNil(kept[kCGImagePropertyGPSDictionary])
            XCTAssertNil((kept[kCGImagePropertyTIFFDictionary] as? [CFString: Any])?[kCGImagePropertyTIFFMake])
            XCTAssertNil((kept[kCGImagePropertyExifDictionary] as? [CFString: Any])?[kCGImagePropertyExifDateTimeOriginal])
        }
    }

    /// Photos picked together stay one document of a page each, read by
    /// the core without any PDF written beside them; deleting the import
    /// removes them.
    func testPhotosPickedTogetherAreOneDocument() throws {
        let core = try RealCore.make(storage: storage, keys: FixedCaseKeyStore(), physicalMemory: Fixture.belowFloor)
        let png = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).pngData { _ in })
        let pending = try XCTUnwrap(try Importer(storage: storage).images([try cameraJPEG(), png], scanned: false))
        guard case .photos(let pages) = pending.source else { return XCTFail("\(pending.source)") }
        let session = try core.library.importDocument(pending.source, title: "Receipts", into: .newCase(name: "Receipts"))
        XCTAssertEqual(session.snapshot().original, .images(pages))
        XCTAssertThrowsError(try session.recognize(observer: TestObserver())) { error in
            XCTAssertEqual(error as? CoreError, .modelMissing)
        }
        let folder = pages[0].deletingLastPathComponent()
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: folder.path).sorted(), ["page-1.jpg", "page-2.png"])
        try core.library.deleteDocument(session.id)
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.path))
    }
}
