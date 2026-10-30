import Foundation
import Network

/// Whether the current connection costs the user data.
protocol NetworkConditions: Sendable {
    /// Cellular or a personal hotspot: a download asks first.
    var isExpensive: Bool { get }
}

final class NetworkMonitor: NetworkConditions, @unchecked Sendable {
    private let monitor = NWPathMonitor()
    private let lock = NSLock()
    private var expensive = false

    init() {
        monitor.pathUpdateHandler = { [weak self] path in
            guard let self else { return }
            lock.lock()
            expensive = path.status == .satisfied && path.isExpensive
            lock.unlock()
        }
        monitor.start(queue: DispatchQueue(label: "io.github.lawkitt.mdoc.network"))
    }

    deinit {
        monitor.cancel()
    }

    var isExpensive: Bool {
        lock.lock()
        defer { lock.unlock() }
        return expensive
    }
}

/// The platform half of model delivery: it fetches the file in the
/// background, on Wi-Fi unless the user allowed cellular; the core verifies
/// and installs it (ADR 0085).
protocol ModelDownloader: Sendable {
    func download(
        _ request: ModelDownloadRequest,
        allowCellular: Bool,
        progress: @escaping @Sendable (Double) -> Void
    ) async throws -> URL
}

/// Pretends to download until the background `URLSession` downloader comes
/// with the real core. It makes no network call.
struct FakeModelDownloader: ModelDownloader {
    var seconds: Double = 2

    func download(
        _ request: ModelDownloadRequest,
        allowCellular: Bool,
        progress: @escaping @Sendable (Double) -> Void
    ) async throws -> URL {
        let steps = 20
        for step in 1...steps {
            if seconds > 0 {
                try await Task.sleep(for: .seconds(seconds / Double(steps)))
            }
            try Task.checkCancellation()
            progress(Double(step) / Double(steps))
        }
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("\(request.modelID).download")
        try Data().write(to: file)
        return file
    }
}
