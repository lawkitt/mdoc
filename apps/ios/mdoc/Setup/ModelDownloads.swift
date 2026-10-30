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

/// Fetches model files with a background `URLSession`, so a download goes
/// on while mdoc is suspended. Wi-Fi only unless the user allowed cellular.
/// A finished file waits in `staging` until the core installs it; one that
/// finished while mdoc was not running is picked up there next time.
final class BackgroundModelDownloader: NSObject, ModelDownloader, URLSessionDownloadDelegate, @unchecked Sendable {
    static let wifiIdentifier = "io.github.lawkitt.mdoc.models"
    static let cellularIdentifier = "io.github.lawkitt.mdoc.models.cellular"

    private struct Waiter {
        let continuation: CheckedContinuation<URL, any Error>
        let progress: @Sendable (Double) -> Void
    }

    private let staging: URL
    private let lock = NSLock()
    private var waiters: [String: Waiter] = [:]
    private var finished: [String: URL] = [:]
    /// The system's completion handlers, called on the main queue once a
    /// session's events are delivered.
    private struct Completion: @unchecked Sendable {
        let call: () -> Void
    }
    private var completions: [String: Completion] = [:]
    private var sessions: [Bool: URLSession] = [:]
    private let makeConfiguration: @Sendable (_ identifier: String, _ cellular: Bool) -> URLSessionConfiguration

    /// `configuration` is replaced in tests, which cannot use a background session.
    init(
        staging: URL,
        configuration: @escaping @Sendable (_ identifier: String, _ cellular: Bool) -> URLSessionConfiguration = BackgroundModelDownloader.background
    ) {
        self.staging = staging
        makeConfiguration = configuration
        super.init()
    }

    static func background(identifier: String, cellular: Bool) -> URLSessionConfiguration {
        let configuration = URLSessionConfiguration.background(withIdentifier: identifier)
        configuration.allowsCellularAccess = cellular
        configuration.allowsExpensiveNetworkAccess = cellular
        configuration.isDiscretionary = false
        configuration.sessionSendsLaunchEvents = true
        return configuration
    }

    /// Reconnects to downloads that went on while mdoc was not running.
    func resumeSessions() {
        _ = session(cellular: false)
        _ = session(cellular: true)
    }

    /// The system woke mdoc for a background session's events.
    func handleEvents(for identifier: String, completion: @escaping () -> Void) {
        lock.lock()
        completions[identifier] = Completion(call: completion)
        lock.unlock()
        resumeSessions()
    }

    func download(
        _ request: ModelDownloadRequest,
        allowCellular: Bool,
        progress: @escaping @Sendable (Double) -> Void
    ) async throws -> URL {
        let target = staging.appendingPathComponent(request.modelID, isDirectory: true)
            .appendingPathComponent(request.file)
        if FileManager.default.fileExists(atPath: target.path) {
            return target
        }
        let task = session(cellular: allowCellular).downloadTask(with: request.url)
        task.taskDescription = "\(request.modelID)/\(request.file)"
        let key = Self.key(task, cellular: allowCellular)
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                lock.lock()
                waiters[key] = Waiter(continuation: continuation, progress: progress)
                lock.unlock()
                task.resume()
            }
        } onCancel: {
            task.cancel()
        }
    }

    private func session(cellular: Bool) -> URLSession {
        lock.lock()
        defer { lock.unlock() }
        if let session = sessions[cellular] {
            return session
        }
        let identifier = cellular ? Self.cellularIdentifier : Self.wifiIdentifier
        let session = URLSession(configuration: makeConfiguration(identifier, cellular), delegate: self, delegateQueue: nil)
        sessions[cellular] = session
        return session
    }

    private static func key(_ task: URLSessionTask, cellular: Bool) -> String {
        "\(cellular ? "cellular" : "wifi")-\(task.taskIdentifier)"
    }

    private func key(_ session: URLSession, _ task: URLSessionTask) -> String {
        lock.lock()
        defer { lock.unlock() }
        return Self.key(task, cellular: sessions[true] === session)
    }

    // MARK: URLSessionDownloadDelegate

    func urlSession(
        _ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64,
        totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64
    ) {
        guard totalBytesExpectedToWrite > 0 else { return }
        let key = key(session, downloadTask)
        lock.lock()
        let waiter = waiters[key]
        lock.unlock()
        waiter?.progress(Double(totalBytesWritten) / Double(totalBytesExpectedToWrite))
    }

    /// The file must move before this returns; it lands in staging by its
    /// model and file name, even when no caller is waiting any more.
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        guard let name = downloadTask.taskDescription else { return }
        if let status = (downloadTask.response as? HTTPURLResponse)?.statusCode, !(200..<300).contains(status) {
            return
        }
        let target = staging.appendingPathComponent(name)
        do {
            try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? FileManager.default.removeItem(at: target)
            try FileManager.default.moveItem(at: location, to: target)
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            var excluded = target
            try? excluded.setResourceValues(values)
            let key = key(session, downloadTask)
            lock.lock()
            finished[key] = target
            lock.unlock()
        } catch {
            // Reported by didCompleteWithError as a missing file.
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?) {
        let key = key(session, task)
        lock.lock()
        let waiter = waiters.removeValue(forKey: key)
        let file = finished.removeValue(forKey: key)
        lock.unlock()
        guard let waiter else { return }
        if let error {
            let cancelled = (error as? URLError)?.code == .cancelled
            waiter.continuation.resume(throwing: cancelled ? CancellationError() : error)
        } else if let file {
            waiter.continuation.resume(returning: file)
        } else {
            let status = (task.response as? HTTPURLResponse)?.statusCode ?? 0
            waiter.continuation.resume(throwing: URLError(.badServerResponse, userInfo: [
                NSLocalizedDescriptionKey: String(localized: "The download failed (HTTP \(status))."),
            ]))
        }
    }

    func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        guard let identifier = session.configuration.identifier else { return }
        lock.lock()
        let completion = completions.removeValue(forKey: identifier)
        lock.unlock()
        if let completion {
            DispatchQueue.main.async { completion.call() }
        }
    }
}

/// Pretends to download, for previews and tests: no network call.
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
