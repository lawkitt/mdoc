import Foundation

/// The one place that picks the core: the Rust core through `MdocFFI` for
/// the app and the extension; `FakeCore` only for previews and view-model
/// tests.
enum CoreFactory {
    /// Launch argument for UI tests: a separate, fresh storage root and case
    /// key, and the structured rules instead of the detector, so no model is
    /// needed.
    static let uiTestingArgument = "-mdoc-ui-testing"
    /// Set in the App Group's defaults while the app runs for UI tests, so
    /// the Restore extension opens the same test library.
    private static let uiTestingKey = "uiTestingLibrary"
    private static let uiTestingFolder = "mdoc-ui-testing"
    /// Below the detector floor: a scan runs the structured rules.
    static let rulesOnlyMemory: UInt64 = 4 * 1_073_741_824

    static var isUITesting: Bool {
        ProcessInfo.processInfo.arguments.contains(uiTestingArgument)
    }

    /// The app's storage. UI tests get a fresh folder of their own; a normal
    /// launch clears the test flag.
    static func appStorage() throws -> MdocStorage {
        let defaults = UserDefaults(suiteName: MdocStorage.appGroup)
        if isUITesting {
            let storage = try MdocStorage.shared(folder: uiTestingFolder)
            try? FileManager.default.removeItem(at: storage.root)
            defaults?.set(true, forKey: uiTestingKey)
            return storage
        }
        defaults?.removeObject(forKey: uiTestingKey)
        return try MdocStorage.shared()
    }

    static func appCore(storage: MdocStorage) throws -> Core {
        try RealCore.make(
            storage: storage, keys: keyStore(testing: isUITesting),
            physicalMemory: isUITesting ? rulesOnlyMemory : ProcessInfo.processInfo.physicalMemory)
    }

    /// The extension gets the library only: restoring needs no model.
    static func restoreExtension() throws -> any Library {
        let testing = UserDefaults(suiteName: MdocStorage.appGroup)?.bool(forKey: uiTestingKey) ?? false
        let storage = try testing ? MdocStorage.shared(folder: uiTestingFolder) : MdocStorage.shared()
        try storage.prepare()
        return try RealCore.library(storage: storage, keys: keyStore(testing: testing))
    }

    private static func keyStore(testing: Bool) -> any CaseKeyStore {
        testing ? KeychainCaseKeyStore(account: "case-key-ui-testing") : KeychainCaseKeyStore()
    }
}
