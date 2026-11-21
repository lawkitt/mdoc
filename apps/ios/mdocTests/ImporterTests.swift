import Foundation
import ImageIO
import class MdocFFI.DocumentSession
import enum MdocFFI.CoreFailure
import struct MdocFFI.ImportOptions
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
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let image = UIGraphicsImageRenderer(size: CGSize(width: 40, height: 30), format: format).image { context in
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
    /// A file's first bytes decide what it is: a photo named `.pdf`, or
    /// text named `.jpg`, is refused before any copy is kept.
    func testAFilesItemIsKnownByItsFirstBytes() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let png = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).pngData { _ in })
        let importer = Importer(storage: storage)
        // A photo whose early comment says `%PDF-`: still a photo, never a PDF.
        let jpeg = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).jpegData(withCompressionQuality: 0.9) { _ in })
        let comment = Data([0xff, 0xfe, 0x00, 0x0b]) + Data("%PDF-1.7 x".utf8)
        let disguised = jpeg.prefix(2) + comment + jpeg.dropFirst(2)
        XCTAssertNotNil(disguised.prefix(1024).range(of: Data("%PDF-".utf8)))
        for (name, data) in [
            ("photo.pdf", png), ("notes.jpg", Data("Not a photo.".utf8)), ("letter.docx", png), ("scan.pdf", disguised),
        ] {
            let url = folder.appendingPathComponent(name)
            try data.write(to: url)
            XCTAssertThrowsError(try importer.file(url), name)
        }
        let photo = folder.appendingPathComponent("photo.png")
        try png.write(to: photo)
        let pending = try XCTUnwrap(try importer.file(photo))
        guard case .photos(let pages) = pending.source else { return XCTFail("\(pending.source)") }
        XCTAssertEqual(pages.map(\.pathExtension), ["png"])
        let left = try FileManager.default.contentsOfDirectory(at: storage.url(.originals), includingPropertiesForKeys: nil)
        XCTAssertEqual(left.count, 1, "\(left)")
    }

    /// A TIFF is kept as a TIFF without its GPS position, camera or private
    /// tags (#1021), and the core reads every page of it.
    func testATIFFImportsEveryPageThroughTheCore() throws {
        let core = try RealCore.make(storage: storage, keys: FixedCaseKeyStore(), physicalMemory: Fixture.belowFloor)
        let library = try XCTUnwrap(core.library as? RealLibrary)
        let tagged = try Data(contentsOf: Fixture.repository.appendingPathComponent("tests/fixtures/tiff/tagged-gps.tif"))
        XCTAssertNotNil(tagged.range(of: Data("FictionalCam".utf8)))
        let pending = try XCTUnwrap(try Importer(storage: storage).images([tagged], scanned: false))
        guard case .photos(let pages) = pending.source else { return XCTFail("\(pending.source)") }
        let page = try XCTUnwrap(pages.first)
        XCTAssertEqual(page.pathExtension, "tiff")
        let kept = try Data(contentsOf: page)
        for needle in ["FictionalCam", "Fictional Studio", "xmpmeta", "A fictional comment", "2031:03:14"] {
            XCTAssertNil(kept.range(of: Data(needle.utf8)), needle)
        }
        let source = try XCTUnwrap(CGImageSourceCreateWithData(kept as CFData, nil))
        XCTAssertEqual(CGImageSourceGetCount(source), 2)
        let properties = try XCTUnwrap(CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any])
        XCTAssertNil(properties[kCGImagePropertyGPSDictionary])
        XCTAssertEqual(properties[kCGImagePropertyOrientation] as? Int, 6)
        XCTAssertThrowsError(
            try MdocFFI.DocumentSession().importImagesIn(
                library: library.ffi, images: [page.path], title: "Fax",
                options: MdocFFI.ImportOptions(ocr: nil, nativeTextOnly: false, password: nil), progress: nil)
        ) { error in
            XCTAssertEqual(CoreFailure(error), .needsOcr(pages: [1, 2], pageCount: 2))
        }
    }
    /// A two-page 8-bit gray BigTIFF, 4 × 4 pixels a page, little-endian.
    private func bigTIFF() -> Data {
        var out = Data([0x49, 0x49, 0x2b, 0x00, 0x08, 0x00, 0x00, 0x00])
        func u16(_ value: UInt16) -> Data { withUnsafeBytes(of: value.littleEndian) { Data($0) } }
        func u64(_ value: UInt64) -> Data { withUnsafeBytes(of: value.littleEndian) { Data($0) } }
        let linkAt = out.count
        out += u64(0)
        var link = linkAt
        for page in 0..<2 {
            let strip = UInt64(out.count)
            out += Data((0..<16).map { UInt8(truncatingIfNeeded: $0 * 15 + page * 40) })
            let entries: [(UInt16, UInt16, UInt64)] = [
                (256, 3, 4), (257, 3, 4), (258, 3, 8), (259, 3, 1), (262, 3, 1),
                (273, 16, strip), (277, 3, 1), (278, 3, 4), (279, 16, 16),
            ]
            let at = UInt64(out.count)
            out.replaceSubrange(link..<link + 8, with: u64(at))
            out += u64(UInt64(entries.count))
            for (tag, kind, value) in entries {
                out += u16(tag) + u16(kind) + u64(1) + u64(value)
            }
            link = out.count
            out += u64(0)
        }
        return out
    }

    /// A BigTIFF isn't taken: the core can't yet keep it without its
    /// metadata, and iOS would draw only its first page.
    func testABigTIFFIsRefused() throws {
        let big = bigTIFF()
        let source = try XCTUnwrap(CGImageSourceCreateWithData(big as CFData, nil))
        XCTAssertEqual(CGImageSourceGetCount(source), 2)
        XCTAssertThrowsError(try Importer(storage: storage).images([big], scanned: false))
    }

    /// A batch with a page that can't be kept is refused whole, with the
    /// formats mdoc opens: no page goes missing without a word.
    func testAMixedBatchWithAPageItCantKeepIsRefusedWhole() throws {
        let jpeg = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).jpegData(withCompressionQuality: 0.9) { _ in })
        XCTAssertThrowsError(try Importer(storage: storage).images([jpeg, bigTIFF()], scanned: false)) { error in
            XCTAssertEqual(error.localizedDescription, "mdoc opens PDF, DOCX and image files.")
        }
        let left = try FileManager.default.contentsOfDirectory(at: storage.url(.originals), includingPropertiesForKeys: nil)
        XCTAssertTrue(left.isEmpty, "\(left)")
    }
}
