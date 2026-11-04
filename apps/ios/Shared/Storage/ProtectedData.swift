import Foundation

/// Whether files under Data Protection Complete can be read and written:
/// false from the moment the phone starts locking until it unlocks. The app
/// sets it from its delegate; a save that fails while it is false waits for
/// the unlock rather than failing the job (ADR 0085). The Restore extension
/// runs only while the phone is unlocked, so there it stays true.
enum ProtectedData {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var available = true

    static var isAvailable: Bool {
        get { lock.withLock { available } }
        set { lock.withLock { available = newValue } }
    }
}
