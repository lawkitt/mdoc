import Darwin
import Foundation
import MdocFFI
import XCTest
@testable import mdoc

/// OCR time and peak memory per render resolution on this device, for the
/// OCR corpus (`scripts/ios/ios-ocr-dpi.sh` runs it and scores the outputs on
/// the Mac). Measurement only: it downloads the OCR model and runs only when
/// `MDOC_OCR_DPI` lists resolutions, such as `150` or `150,200,300`
/// (`TEST_RUNNER_MDOC_OCR_DPI` through xcodebuild).
final class OCRResolutionTests: XCTestCase {
    func testResolution() async throws {
        let environment = ProcessInfo.processInfo.environment
        let resolutions = (environment["MDOC_OCR_DPI"] ?? "").split(separator: ",").compactMap { UInt16($0) }
        try XCTSkipIf(resolutions.isEmpty, "OCR resolution run not requested")
        let runs = Int(environment["MDOC_OCR_DPI_RUNS"] ?? "") ?? 3
        let storage = try Fixture.temporaryStorage()
        defer { try? FileManager.default.removeItem(at: storage.root) }
        let core = try RealCore.make(storage: storage, keys: FixedVaultKeyStore(), physicalMemory: Fixture.belowFloor)
        try await OCRResumeTests.installOCR(into: core)
        let paths = try XCTUnwrap((core.setup as? RealSetup)?.ocrPaths())

        let (corpus, output) = Self.folders(environment)
        let pdfs = try corpus.flatMap { folder in
            try FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
        }
        .filter { $0.pathExtension == "pdf" && !$0.lastPathComponent.hasSuffix(".native.pdf") }
        .sorted { $0.lastPathComponent < $1.lastPathComponent }
        XCTAssertFalse(pdfs.isEmpty, "no corpus PDFs in \(corpus.map(\.path))")

        var rows = ["page\tdpi\trun\tseconds\tpeak_footprint\tstart_footprint"]
        for run in 1...runs {
            for pdf in pdfs {
                for dpi in resolutions {
                    let stem = pdf.deletingPathExtension().lastPathComponent
                    let sampler = FootprintSampler()
                    let measured = try measureOcr(paths: paths, pdf: pdf.path, dpi: dpi)
                    let (start, peak) = sampler.stop()
                    let folder = output.appendingPathComponent(String(dpi), isDirectory: true)
                    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                    try measured.markdown.write(
                        to: folder.appendingPathComponent("\(stem).prepared.md"), atomically: true, encoding: .utf8)
                    try measured.recognition.write(
                        to: folder.appendingPathComponent("\(stem).recognition.txt"), atomically: true, encoding: .utf8)
                    let row = "\(stem)\t\(dpi)\t\(run)\t\(String(format: "%.3f", measured.seconds))\t\(peak)\t\(start)"
                    print("OCR-DPI\t\(row)")
                    rows.append(row)
                }
            }
        }
        let name = "runs-" + resolutions.map(String.init).joined(separator: "-") + ".tsv"
        try (rows.joined(separator: "\n") + "\n").write(
            to: output.appendingPathComponent(name), atomically: true, encoding: .utf8)
    }

    /// The corpus and the output folder: on the Simulator the Mac's
    /// repository and `MDOC_OCR_DPI_OUT`; on a phone the folders the script
    /// copies to and from the app's data container, the output one per
    /// script run (`MDOC_OCR_DPI_TAG`), so an earlier run's tables stay out.
    private static func folders(_ environment: [String: String]) -> ([URL], URL) {
        let fixtures = Fixture.repository.appendingPathComponent("tests/fixtures", isDirectory: true)
        let container = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("mdoc-ocr-dpi", isDirectory: true)
        let corpus: [URL]
        if let folders = environment["MDOC_OCR_DPI_CORPUS"] {
            corpus = folders.split(separator: ":").map { URL(fileURLWithPath: String($0), isDirectory: true) }
        } else if FileManager.default.fileExists(atPath: fixtures.path) {
            corpus = ["ocr-upstream", "ocr-qualification"].map { fixtures.appendingPathComponent($0, isDirectory: true) }
        } else {
            corpus = [container.appendingPathComponent("corpus", isDirectory: true)]
        }
        let output = environment["MDOC_OCR_DPI_OUT"].map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? container.appendingPathComponent("out", isDirectory: true)
                .appendingPathComponent(environment["MDOC_OCR_DPI_TAG"] ?? "untagged", isDirectory: true)
        return (corpus, output)
    }
}

/// This process's physical footprint (what iOS's memory limit counts),
/// sampled every 5 ms from creation until `stop`.
private final class FootprintSampler: @unchecked Sendable {
    private let lock = NSLock()
    private var peak: UInt64
    private var running = true
    private let start: UInt64

    init() {
        start = Self.footprint()
        peak = start
        let sampler = Thread { [self] in
            while self.sample() {
                Thread.sleep(forTimeInterval: 0.005)
            }
        }
        sampler.start()
    }

    private func sample() -> Bool {
        let now = Self.footprint()
        lock.lock()
        defer { lock.unlock() }
        peak = max(peak, now)
        return running
    }

    /// The footprint at creation and the highest sampled since, in bytes.
    func stop() -> (UInt64, UInt64) {
        let last = Self.footprint()
        lock.lock()
        defer { lock.unlock() }
        running = false
        peak = max(peak, last)
        return (start, peak)
    }

    static func footprint() -> UInt64 {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        return result == KERN_SUCCESS ? info.phys_footprint : 0
    }
}
