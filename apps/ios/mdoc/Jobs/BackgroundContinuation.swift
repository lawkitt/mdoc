import BackgroundTasks
import Foundation
import UIKit

/// Keeps a user-started job alive when mdoc leaves the screen (ADR 0085):
/// `BGContinuedProcessingTask` on iOS 26 and later, the short background
/// grace before that. When the time runs out it calls `expire`, which
/// cancels the job; the app offers to resume on return.
protocol BackgroundContinuation: Sendable {
    @MainActor
    func begin(title: String, expire: @escaping @Sendable () -> Void) -> any BackgroundLease
}

protocol BackgroundLease: Sendable {
    func progress(_ fraction: Double)
    func end(success: Bool)
}

enum BackgroundContinuations {
    @MainActor
    static func system() -> any BackgroundContinuation {
        if #available(iOS 26, *) {
            return ContinuedProcessing(fallback: GraceTime())
        }
        return GraceTime()
    }
}

/// Tests and previews: the job simply runs.
struct NoBackgroundContinuation: BackgroundContinuation {
    @MainActor
    func begin(title: String, expire: @escaping @Sendable () -> Void) -> any BackgroundLease {
        NoLease()
    }

    private struct NoLease: BackgroundLease {
        func progress(_ fraction: Double) {}
        func end(success: Bool) {}
    }
}

/// iOS 17–25: the background grace `beginBackgroundTask` gives, then cancel.
struct GraceTime: BackgroundContinuation {
    @MainActor
    func begin(title: String, expire: @escaping @Sendable () -> Void) -> any BackgroundLease {
        let lease = GraceLease()
        let id = UIApplication.shared.beginBackgroundTask(withName: title) {
            expire()
            // The task must end before the handler returns.
            let id = lease.take()
            if id != .invalid {
                UIApplication.shared.endBackgroundTask(id)
            }
        }
        lease.start(id)
        return lease
    }
}

private final class GraceLease: BackgroundLease, @unchecked Sendable {
    private let lock = NSLock()
    private var id: UIBackgroundTaskIdentifier = .invalid

    func start(_ id: UIBackgroundTaskIdentifier) {
        lock.lock()
        defer { lock.unlock() }
        self.id = id
    }

    func take() -> UIBackgroundTaskIdentifier {
        lock.lock()
        defer { lock.unlock() }
        let id = self.id
        self.id = .invalid
        return id
    }

    func progress(_ fraction: Double) {}

    func end(success: Bool) {
        let id = take()
        guard id != .invalid else { return }
        Task { @MainActor in
            UIApplication.shared.endBackgroundTask(id)
        }
    }
}

/// iOS 26: a continued processing task shows the job's progress while mdoc
/// is in the background. If the system refuses the request, the grace time
/// stands in.
@available(iOS 26, *)
struct ContinuedProcessing: BackgroundContinuation {
    /// Matches `BGTaskSchedulerPermittedIdentifiers` in Info.plist.
    static let prefix = "io.github.lawkitt.mdoc.job."
    let fallback: GraceTime

    @MainActor
    func begin(title: String, expire: @escaping @Sendable () -> Void) -> any BackgroundLease {
        let identifier = Self.prefix + UUID().uuidString
        let lease = ContinuedLease()
        let registered = BGTaskScheduler.shared.register(forTaskWithIdentifier: identifier, using: nil) { task in
            guard let task = task as? BGContinuedProcessingTask else {
                task.setTaskCompleted(success: false)
                return
            }
            task.expirationHandler = {
                expire()
            }
            lease.attach(task)
        }
        let request = BGContinuedProcessingTaskRequest(
            identifier: identifier, title: title,
            subtitle: String(localized: "mdoc keeps working while you use other apps."))
        request.strategy = .fail
        do {
            guard registered else { throw CocoaError(.featureUnsupported) }
            try BGTaskScheduler.shared.submit(request)
            return lease
        } catch {
            return fallback.begin(title: title, expire: expire)
        }
    }
}

@available(iOS 26, *)
private final class ContinuedLease: BackgroundLease, @unchecked Sendable {
    private let lock = NSLock()
    private var task: BGContinuedProcessingTask?
    private var ended: Bool?
    private var lastFraction = 0.0

    func attach(_ task: BGContinuedProcessingTask) {
        lock.lock()
        defer { lock.unlock() }
        task.progress.totalUnitCount = 100
        task.progress.completedUnitCount = Int64(lastFraction * 100)
        if let ended {
            task.setTaskCompleted(success: ended)
        } else {
            self.task = task
        }
    }

    func progress(_ fraction: Double) {
        lock.lock()
        defer { lock.unlock() }
        lastFraction = fraction
        task?.progress.completedUnitCount = Int64(fraction * 100)
    }

    func end(success: Bool) {
        lock.lock()
        defer { lock.unlock() }
        ended = success
        task?.setTaskCompleted(success: success)
        task = nil
    }
}
