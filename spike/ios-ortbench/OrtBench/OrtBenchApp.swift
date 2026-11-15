// SPIKE (lawkitt/mdoc#903): one bench per launch, configured by environment, result appended to Documents/results.jsonl.
import SwiftUI

@main
struct OrtBenchApp: App {
    @State private var status = "running…"
    var body: some Scene {
        WindowGroup {
            Text(status).font(.footnote.monospaced()).padding()
                .task { status = await Task.detached { Self.runFromEnvironment() }.value }
        }
    }

    nonisolated static func runFromEnvironment() -> String {
        let env = ProcessInfo.processInfo.environment
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        guard let model = env["BENCH_MODEL"] else { return "set BENCH_MODEL" }
        let path = docs.appendingPathComponent("models").appendingPathComponent(model).path
        let cache = docs.appendingPathComponent("coreml-cache").path
        try? FileManager.default.createDirectory(atPath: cache, withIntermediateDirectories: true)
        var out = [CChar](repeating: 0, count: 4096)
        let dump = env["BENCH_DUMP"].map { docs.appendingPathComponent($0).path } ?? ""
        _ = ort_bench(path, env["BENCH_EP"] ?? "cpu", env["BENCH_FORMAT"] ?? "MLProgram", env["BENCH_UNITS"] ?? "ALL",
                      env["BENCH_CACHE"] == "1" ? cache : "", Int32(env["BENCH_SEQ"] ?? "512") ?? 512,
                      Int32(env["BENCH_RUNS"] ?? "5") ?? 5, Int32(env["BENCH_THREADS"] ?? "4") ?? 4, dump, &out, out.count)
        let line = String(cString: out)
        let results = docs.appendingPathComponent("results.jsonl")
        if let h = try? FileHandle(forWritingTo: results) { h.seekToEndOfFile(); h.write((line + "\n").data(using: .utf8)!); try? h.close() }
        else { try? (line + "\n").write(to: results, atomically: true, encoding: .utf8) }
        if env["BENCH_EXIT"] == "1" { exit(0) }
        return line
    }
}
