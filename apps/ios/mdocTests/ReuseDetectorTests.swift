import XCTest
@testable import mdoc

/// The device tests' detector comes from the phone's own library when it has
/// one, so the tests never download its 630 MB again (`CoreFactory.reuseDetector`).
final class ReuseDetectorTests: XCTestCase {
    private var folder: URL!

    override func setUpWithError() throws {
        folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: folder)
    }

    private func write(_ text: String, _ path: String, in storage: MdocStorage) throws {
        let file = storage.url(.models).appendingPathComponent(path)
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(text.utf8).write(to: file)
    }

    private func read(_ path: String, in storage: MdocStorage) -> String? {
        (try? Data(contentsOf: storage.url(.models).appendingPathComponent(path))).map { String(decoding: $0, as: UTF8.self) }
    }

    func testMissingFilesAreClonedAndKeptOnesStay() throws {
        let library = MdocStorage(root: folder.appendingPathComponent("mdoc"))
        let tests = MdocStorage(root: folder.appendingPathComponent("mdoc-ui-testing"))
        let model = "pseudonymization/gliner2/fp16_v2"
        try write("encoder", "\(model)/encoder.onnx", in: library)
        try write("classifier", "\(model)/classifier.onnx", in: library)
        try write("text recognition", "ocr/rec.onnx", in: library)
        try write("kept", "\(model)/classifier.onnx", in: tests)

        CoreFactory.reuseDetector(from: library, into: tests)

        XCTAssertEqual(read("\(model)/encoder.onnx", in: tests), "encoder")
        XCTAssertEqual(read("\(model)/classifier.onnx", in: tests), "kept")
        XCTAssertNil(read("ocr/rec.onnx", in: tests), "text recognition is downloaded each run")
        XCTAssertNil(read("\(model)/encoder.onnx.part", in: tests))
        XCTAssertEqual(read("\(model)/encoder.onnx", in: library), "encoder", "the library keeps its detector")
    }

    func testALibraryWithoutADetectorLeavesTheTestsToDownload() {
        let library = MdocStorage(root: folder.appendingPathComponent("mdoc"))
        let tests = MdocStorage(root: folder.appendingPathComponent("mdoc-ui-testing"))
        CoreFactory.reuseDetector(from: library, into: tests)
        XCTAssertFalse(FileManager.default.fileExists(atPath: tests.url(.models).path))
    }
}
