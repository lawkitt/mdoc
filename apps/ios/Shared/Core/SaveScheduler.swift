import Foundation

/// The save that follows review taps (ADR 0104, Calls stay synchronous
/// while they are cheap): a tap returns its snapshot at once and its change
/// is saved on a utility queue once the taps pause, several taps making one
/// save. A flush point (leaving the foreground, sharing, switching
/// documents) saves at once instead, cancelling the scheduled save.
final class SaveScheduler: @unchecked Sendable {
    /// How long taps pause before their save.
    static let pause: DispatchTimeInterval = .milliseconds(300)

    private let queue: DispatchQueue
    private let lock = NSLock()
    private var scheduled: DispatchWorkItem?

    init(queue: DispatchQueue) {
        self.queue = queue
    }

    /// Runs `save` on the queue once `pause` passes without another call;
    /// a later call replaces this one's save.
    func schedule(pause: DispatchTimeInterval = SaveScheduler.pause, _ save: @escaping @Sendable () -> Void) {
        let item = DispatchWorkItem(block: save)
        lock.withLock {
            scheduled?.cancel()
            scheduled = item
        }
        // timing: taps coalesce this long before their save (ADR 0104)
        queue.asyncAfter(deadline: .now() + pause, execute: item)
    }

    /// The scheduled save will not run: its caller saves now, or the
    /// document went.
    func cancel() {
        lock.withLock {
            scheduled?.cancel()
            scheduled = nil
        }
    }
}
