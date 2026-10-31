import Foundation

/// The one place that picks the core: the Rust core through `MdocFFI` for
/// the app and the extension; `FakeCore` only for previews and view-model
/// tests.
enum CoreFactory {
    /// Launch argument for UI tests: a separate, fresh storage root and case
    /// key, and a device below the detector floor, so no model is needed;
    /// the seeded document is scanned with the structured rules.
    static let uiTestingArgument = "-mdoc-ui-testing"
    /// Set in the App Group's defaults while the app runs for UI tests, so
    /// the Restore extension opens the same test library.
    private static let uiTestingKey = "uiTestingLibrary"
    private static let uiTestingFolder = "mdoc-ui-testing"
    /// Below the detector floor: the app offers no Find names, and only the
    /// UI tests' seed scans, with the structured rules.
    static let belowFloorMemory: UInt64 = 4 * 1_073_741_824

    static var isUITesting: Bool {
        ProcessInfo.processInfo.arguments.contains(uiTestingArgument)
    }

    /// The app's storage. UI tests get a fresh folder of their own; a normal
    /// launch clears the test flag.
    static func appStorage() throws -> MdocStorage {
        let defaults = UserDefaults(suiteName: MdocStorage.appGroup)
        if isUITesting {
            let storage = try MdocStorage.shared(folder: uiTestingFolder)
            clearKeepingDetector(storage)
            defaults?.set(true, forKey: uiTestingKey)
            return storage
        }
        defaults?.removeObject(forKey: uiTestingKey)
        return try MdocStorage.shared()
    }

    /// A fresh test library, all but a downloaded name detector: only the
    /// device tests download one (hundreds of megabytes), and their next run
    /// reuses it. Text recognition is downloaded again each time.
    private static func clearKeepingDetector(_ storage: MdocStorage) {
        let files = FileManager.default
        let models = storage.url(.models)
        // `mdoc-ffi`'s Setup keeps the detector in `models/pseudonymization`.
        let detector = "pseudonymization"
        guard files.fileExists(atPath: models.appendingPathComponent(detector).path) else {
            try? files.removeItem(at: storage.root)
            return
        }
        for (folder, kept) in [(storage.root, models.lastPathComponent), (models, detector)] {
            let items = (try? files.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
            for item in items where item.lastPathComponent != kept {
                try? files.removeItem(at: item)
            }
        }
    }

    static func appCore(storage: MdocStorage) throws -> Core {
        try RealCore.make(
            storage: storage, keys: keyStore(testing: isUITesting),
            physicalMemory: isUITesting && !detectorTierInUITests ? belowFloorMemory : ProcessInfo.processInfo.physicalMemory)
    }

    /// The UI survey shows the detector tier's setup screens too; nothing
    /// is downloaded, as UI tests use a fake downloader.
    private static var detectorTierInUITests: Bool {
        ProcessInfo.processInfo.environment["MDOC_UI_DETECTOR_TIER"] == "1"
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
