import Darwin
import Foundation
import PDFKit
import XCTest
@testable import mdoc

/// Time through the first PDFKit page draw and peak memory on this device
/// (`scripts/ios/ios-docx-measure.sh` runs it once per document, so each
/// peak is its own). Measurement only: it runs only when `MDOC_DOCX_MEASURE`
/// names a document of the corpus (`TEST_RUNNER_MDOC_DOCX_MEASURE` through
/// xcodebuild). The first run includes the bundled fonts' one scan.
@MainActor
final class DocxOriginalMeasureTests: XCTestCase {
    func testRender() async throws {
        let environment = ProcessInfo.processInfo.environment
        let name = environment["MDOC_DOCX_MEASURE"] ?? ""
        try XCTSkipIf(name.isEmpty, "DOCX measurement not requested")
        let runs = Int(environment["MDOC_DOCX_MEASURE_RUNS"] ?? "") ?? 3
        let container = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("mdoc-docx-measure", isDirectory: true)
        let corpus = environment["MDOC_DOCX_MEASURE_CORPUS"].map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? container.appendingPathComponent("corpus", isDirectory: true)
        let output = environment["MDOC_DOCX_MEASURE_OUT"].map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? container.appendingPathComponent("out", isDirectory: true)
                .appendingPathComponent(environment["MDOC_DOCX_MEASURE_TAG"] ?? "untagged", isDirectory: true)
        let source = corpus.appendingPathComponent(name)
        XCTAssertTrue(FileManager.default.fileExists(atPath: source.path), "no \(source.path)")

        var rows: [String] = []
        for run in 1...runs {
            let sampler = FootprintSampler()
            defer { _ = sampler.stop() }
            let clock = ContinuousClock()
            let started = clock.now
            let result = await RenderedDocx.render(source).rendering
            guard case .ready(let rendered) = result else {
                XCTFail("The Original did not render: \(result)")
                return
            }
            let row = try autoreleasepool {
                let view = CommentedPDFContainer(frame: CGRect(x: 0, y: 0, width: 390, height: 600))
                view.load(rendered)
                let document = try XCTUnwrap(view.pdfView.document)
                let page = try XCTUnwrap(document.page(at: 0))
                _ = page.thumbnail(of: CGSize(width: 390, height: 600), for: .cropBox)
                let elapsed = started.duration(to: clock.now).components
                let seconds = Double(elapsed.seconds) + Double(elapsed.attoseconds) / 1e18
                let (start, peak) = sampler.stop()
                return [
                    name, String(run), String(document.pageCount), String(rendered.comments.all.count),
                    String(rendered.comments.markers.reduce(0) { $0 + $1.threads.count }),
                    String(format: "%.6f", seconds), String(peak), String(start),
                    String(FootprintSampler.lifetimePeak()),
                ].joined(separator: "\t")
            }
            print("DOCX-MEASURE\t\(row)")
            rows.append(row)
        }
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        let header = "document\trun\tpages\tcomments\tplaced_threads\tseconds\tpeak_footprint\tstart_footprint\tlifetime_peak"
        try ([header] + rows).joined(separator: "\n").appending("\n").write(
            to: output.appendingPathComponent("\(name).tsv"), atomically: true, encoding: .utf8)
    }
}

/// This process's physical footprint (what iOS's memory limit counts),
/// sampled every 2 ms from creation until `stop`.
private final class FootprintSampler: @unchecked Sendable {
    private let lock = NSLock()
    private var peak: UInt64
    private var running = true
    private let start: UInt64

    init() {
        start = Self.info().phys_footprint
        peak = start
        Thread { [self] in
            while self.sample() {
                Thread.sleep(forTimeInterval: 0.002)
            }
        }.start()
    }

    private func sample() -> Bool {
        let now = Self.info().phys_footprint
        lock.lock()
        defer { lock.unlock() }
        peak = max(peak, now)
        return running
    }

    /// The footprint at creation and the highest sampled since, in bytes.
    func stop() -> (UInt64, UInt64) {
        let last = Self.info().phys_footprint
        lock.lock()
        defer { lock.unlock() }
        running = false
        peak = max(peak, last)
        return (start, peak)
    }

    /// The kernel's own highest footprint for the process's life, in bytes.
    static func lifetimePeak() -> UInt64 {
        UInt64(max(info().ledger_phys_footprint_peak, 0))
    }

    private static func info() -> task_vm_info_data_t {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        _ = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        return info
    }
}
