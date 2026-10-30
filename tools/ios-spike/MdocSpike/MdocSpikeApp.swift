// The feasibility spike's throwaway harness (ADR 0085): runs the core's
// `spike.run` on the bundled copies of the fixtures and models, adds what
// only the platform knows (device, thermal state, memory headroom) and
// writes Documents/spike-report.json. Launched with `--exit-when-done`, it
// prints the report between markers and exits, so scripts can wait for it.
import Darwin
import Foundation
import SwiftUI
import UIKit
import os

@main
struct MdocSpikeApp: App {
    @State private var status = "Running the feasibility spike…"

    var body: some Scene {
        WindowGroup {
            ScrollView {
                Text(status)
                    .font(.system(.footnote, design: .monospaced))
                    .padding()
            }
            .task {
                status = await Harness.measure()
                if CommandLine.arguments.contains("--exit-when-done") {
                    exit(0)
                }
            }
        }
    }
}

enum Harness {
    static func measure() async -> String {
        let thermal = ThermalRecorder()
        let availableAtStart = os_proc_available_memory()
        let started = Date()
        let outcome: Result<String, Error> = await Task.detached(priority: .userInitiated) {
            // The generated bindings compile into this module.
            Result { try reportJson(report: MdocSpike.run(input: input())) }
        }.value
        let seconds = Date().timeIntervalSince(started)
        let samples = thermal.stop()

        var report: [String: Any]
        switch outcome {
        case .success(let json):
            report = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any] ?? [:]
        case .failure(let error):
            report = ["error": "\(error)"]
        }
        let worst = samples.map(\.state).max() ?? ProcessInfo.processInfo.thermalState.rawValue
        report["platform"] = [
            "device": machine(),
            "system": "\(UIDevice.current.systemName) \(UIDevice.current.systemVersion)",
            "simulator": isSimulator,
            "physical_memory_bytes": ProcessInfo.processInfo.physicalMemory,
            "available_memory_at_start_bytes": availableAtStart,
            "wall_seconds": seconds,
            "thermal_worst": thermalName(worst),
            "thermal_samples": samples.map { ["t": $0.time, "state": thermalName($0.state)] },
        ] as [String: Any]
        var budgets = report["budgets"] as? [[String: Any]] ?? []
        // "No serious thermal throttling" (ADR 0085). The Simulator reports
        // the Mac's state, so only a device row counts.
        budgets.append([
            "name": "thermal state below serious",
            "limit": "fair",
            "measured": thermalName(worst),
            "pass": !isSimulator && worst < ProcessInfo.ThermalState.serious.rawValue,
        ])
        report["budgets"] = budgets

        let data = (try? JSONSerialization.data(
            withJSONObject: report, options: [.prettyPrinted, .sortedKeys])) ?? Data()
        let text = String(decoding: data, as: UTF8.self)
        let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        try? data.write(to: documents.appendingPathComponent("spike-report.json"))
        print("MDOC_SPIKE_REPORT_BEGIN\n\(text)\nMDOC_SPIKE_REPORT_END")
        return text
    }

    static func input() throws -> SpikeInput {
        let data = Bundle.main.resourceURL!.appendingPathComponent("data")
        let models = data.appendingPathComponent("pseudonymization")
        let fp16 = try FileManager.default.contentsOfDirectory(atPath: models.path)
            .first { $0.hasPrefix("gliner2-pii-fp16-") }
        return SpikeInput(
            fixtures: data.appendingPathComponent("fixtures").path,
            ocrRoot: data.appendingPathComponent("ocr").path,
            modelRoot: models.appendingPathComponent(fp16 ?? "missing-fp16-model").path,
            runtimes: try linkedRuntimes())
    }

    static var isSimulator: Bool {
        #if targetEnvironment(simulator)
            true
        #else
            false
        #endif
    }

    static func machine() -> String {
        if let model = ProcessInfo.processInfo.environment["SIMULATOR_MODEL_IDENTIFIER"] {
            return "\(model) (Simulator)"
        }
        var info = utsname()
        uname(&info)
        return withUnsafeBytes(of: &info.machine) { String(decoding: $0.prefix { $0 != 0 }, as: UTF8.self) }
    }

    static func thermalName(_ state: Int) -> String {
        switch ProcessInfo.ThermalState(rawValue: state) {
        case .nominal: "nominal"
        case .fair: "fair"
        case .serious: "serious"
        case .critical: "critical"
        default: "unknown"
        }
    }
}

/// The thermal state once a second, and at every change, while the spike runs.
final class ThermalRecorder: @unchecked Sendable {
    struct Sample {
        let time: Double
        let state: Int
    }

    private let started = Date()
    private let lock = NSLock()
    private var samples: [Sample] = []
    private var timer: DispatchSourceTimer?
    private var observer: NSObjectProtocol?

    init() {
        record()
        let timer = DispatchSource.makeTimerSource(queue: .global(qos: .utility))
        timer.schedule(deadline: .now() + 1, repeating: 1)
        timer.setEventHandler { [weak self] in self?.record() }
        timer.resume()
        self.timer = timer
        observer = NotificationCenter.default.addObserver(
            forName: ProcessInfo.thermalStateDidChangeNotification, object: nil, queue: nil
        ) { [weak self] _ in self?.record() }
    }

    private func record() {
        let sample = Sample(
            time: Date().timeIntervalSince(started),
            state: ProcessInfo.processInfo.thermalState.rawValue)
        lock.lock()
        defer { lock.unlock() }
        if samples.last?.state != sample.state || Int(sample.time) % 10 == 0 {
            samples.append(sample)
        }
    }

    func stop() -> [Sample] {
        timer?.cancel()
        if let observer { NotificationCenter.default.removeObserver(observer) }
        record()
        lock.lock()
        defer { lock.unlock() }
        return samples
    }
}
