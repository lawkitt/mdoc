import Foundation
import Observation

enum JobKind: Equatable, Sendable {
    case recognize
    case scan
    case install
}

enum CancelReason: Equatable, Sendable {
    /// The user tapped Cancel.
    case user
    /// mdoc left the screen and the background time ran out.
    case background
}

struct RunningJob: Equatable {
    let id: UUID
    let kind: JobKind
    let documentID: DocumentID?
    /// Nil until the core reports its first stage.
    var progress: JobUpdate?
}

/// A job that stopped because mdoc left the screen; the app offers to resume
/// on return (OCR keeps finished pages, the scan reruns).
struct InterruptedJob: Equatable {
    let kind: JobKind
    let documentID: DocumentID?
}

enum JobOutcome<Value: Sendable>: Sendable {
    case finished(Value)
    case cancelled(CancelReason)
    case failed(any Error)
    /// Another job holds the one slot (ADR 0006).
    case busy
}

/// Progress and cancellation for one job: the core polls it, or is told
/// through `onCancel`.
final class JobToken: JobObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var reason: CancelReason?
    private var handlers: [@Sendable () -> Void] = []
    private let onProgress: @Sendable (JobUpdate) -> Void

    init(onProgress: @escaping @Sendable (JobUpdate) -> Void) {
        self.onProgress = onProgress
    }

    func cancel(_ reason: CancelReason) {
        lock.lock()
        guard self.reason == nil else {
            lock.unlock()
            return
        }
        self.reason = reason
        let handlers = self.handlers
        self.handlers = []
        lock.unlock()
        handlers.forEach { $0() }
    }

    func onCancel(_ handler: @escaping @Sendable () -> Void) {
        lock.lock()
        if reason == nil {
            handlers.append(handler)
            lock.unlock()
        } else {
            lock.unlock()
            handler()
        }
    }

    var cancelReason: CancelReason? {
        lock.lock()
        defer { lock.unlock() }
        return reason
    }

    func progress(_ update: JobUpdate) {
        onProgress(update)
    }

    func isCancelled() -> Bool {
        cancelReason != nil
    }
}

/// Runs one model job at a time off the main thread, with progress and
/// cooperative cancellation (ADR 0006, ADR 0085). Nothing starts on its own:
/// every job comes from a user action.
@MainActor
@Observable
final class LongJobRunner {
    private(set) var current: RunningJob?
    private(set) var interrupted: InterruptedJob?
    private var token: JobToken?
    private let background: any BackgroundContinuation

    init(background: any BackgroundContinuation) {
        self.background = background
    }

    func run<Value: Sendable>(
        _ kind: JobKind,
        documentID: DocumentID?,
        title: String,
        work: @escaping @Sendable (any JobObserver) throws -> Value
    ) async -> JobOutcome<Value> {
        guard current == nil else { return .busy }
        let id = UUID()
        let lease = LeaseBox()
        let token = JobToken { [weak self] progress in
            if let fraction = progress.fraction {
                lease.lease?.progress(fraction)
            }
            Task { @MainActor in
                self?.update(id, progress)
            }
        }
        current = RunningJob(id: id, kind: kind, documentID: documentID, progress: nil)
        self.token = token
        if interrupted?.documentID == documentID {
            interrupted = nil
        }
        lease.lease = background.begin(title: title) {
            token.cancel(.background)
        }
        let result = await Task.detached(priority: .userInitiated) {
            Result { try work(token) }
        }.value
        lease.lease?.end(success: (try? result.get()) != nil)
        current = nil
        self.token = nil
        switch result {
        case .success(let value):
            return .finished(value)
        case .failure(let error):
            if let reason = token.cancelReason {
                if reason == .background {
                    interrupted = InterruptedJob(kind: kind, documentID: documentID)
                }
                return .cancelled(reason)
            }
            return .failed(error)
        }
    }

    func cancel() {
        token?.cancel(.user)
    }

    func dismissInterruption() {
        interrupted = nil
    }

    private func update(_ id: UUID, _ progress: JobUpdate) {
        guard current?.id == id else { return }
        current?.progress = progress
    }
}

/// Holds the lease the progress callback reports to, set once the job starts.
private final class LeaseBox: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: (any BackgroundLease)?

    var lease: (any BackgroundLease)? {
        get {
            lock.lock()
            defer { lock.unlock() }
            return stored
        }
        set {
            lock.lock()
            defer { lock.unlock() }
            stored = newValue
        }
    }
}
