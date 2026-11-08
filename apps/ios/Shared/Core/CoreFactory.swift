import Foundation

/// The one place that opens the core for the app and the extension: the
/// Rust core through `MdocFFI`, over the App Group's library or, for UI
/// tests, a fresh one of their own.
enum CoreFactory {
    /// Set in the App Group's defaults while the app runs for UI tests, so
    /// the Restore extension opens the same test library.
    private static let uiTestingKey = "uiTestingLibrary"
    private static let uiTestingFolder = "mdoc-ui-testing"

    /// The process opens the UI tests' library and preferences: the app
    /// launched for UI tests, or the Restore extension while the app runs
    /// for them.
    static var opensUITestLibrary: Bool {
        if Bundle.main.bundleURL.pathExtension == "appex" {
            return UserDefaults(suiteName: MdocStorage.appGroup)?.bool(forKey: uiTestingKey) ?? false
        }
        return LaunchConfiguration.current.isUITesting
    }

    /// The app's storage. UI tests get a fresh folder of their own; a normal
    /// launch clears the test flag.
    static func appStorage(_ launch: LaunchConfiguration = .current) throws -> MdocStorage {
        let defaults = UserDefaults(suiteName: MdocStorage.appGroup)
        if launch.isUITesting {
            let storage = try MdocStorage.shared(folder: uiTestingFolder)
            clearKeepingDetector(storage)
            if launch.detectorTier && launch.realDownloads, let library = try? MdocStorage.shared() {
                reuseDetector(from: library, into: storage)
            }
            defaults?.set(true, forKey: uiTestingKey)
            return storage
        }
        defaults?.removeObject(forKey: uiTestingKey)
        return try MdocStorage.shared()
    }

    /// A fresh test library, all but a downloaded name detector: only the
    /// device tests need one (hundreds of megabytes), cloned from the phone's
    /// own library or downloaded, and their next run keeps it. Text
    /// recognition is downloaded again each time.
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

    /// The device tests' name detector, from the phone's own library when it
    /// has one: each file the test library lacks is cloned (on APFS, no
    /// copy and no extra space), so a phone that downloaded the detector
    /// never downloads its 630 MB again for the tests. Only files installed
    /// there are read, each checked by the core when it was installed; the
    /// core counts a file by its pinned size, so a cut clone is fetched again.
    static func reuseDetector(from library: MdocStorage, into storage: MdocStorage) {
        let files = FileManager.default
        let detector = "pseudonymization"
        let source = library.url(.models).appendingPathComponent(detector, isDirectory: true).resolvingSymlinksInPath()
        let target = storage.url(.models).appendingPathComponent(detector, isDirectory: true)
        guard
            source != target.resolvingSymlinksInPath(),
            let items = files.enumerator(at: source, includingPropertiesForKeys: [.isRegularFileKey])
        else { return }
        for case let item as URL in items {
            guard (try? item.resourceValues(forKeys: [.isRegularFileKey]))?.isRegularFile == true else { continue }
            let relative = String(item.resolvingSymlinksInPath().path.dropFirst(source.path.count + 1))
            let copy = target.appendingPathComponent(relative)
            guard !relative.isEmpty, !files.fileExists(atPath: copy.path) else { continue }
            let partial = copy.appendingPathExtension("part")
            try? files.removeItem(at: partial)
            do {
                try files.createDirectory(at: copy.deletingLastPathComponent(), withIntermediateDirectories: true)
                try files.copyItem(at: item, to: partial)
                try files.moveItem(at: partial, to: copy)
            } catch {
                try? files.removeItem(at: partial)
            }
        }
    }

    static func appCore(storage: MdocStorage, _ launch: LaunchConfiguration = .current) throws -> Core {
        try RealCore.make(
            storage: storage, keys: keyStore(testing: launch.isUITesting), physicalMemory: launch.physicalMemory)
    }

    /// The extension gets the library only: restoring needs no model.
    static func restoreExtension() throws -> any Library {
        let testing = opensUITestLibrary
        let storage = try testing ? MdocStorage.shared(folder: uiTestingFolder) : MdocStorage.shared()
        try storage.prepare()
        return try RealCore.library(storage: storage, keys: keyStore(testing: testing))
    }

    private static func keyStore(testing: Bool) -> any CaseKeyStore {
        testing ? KeychainCaseKeyStore(account: "case-key-ui-testing") : KeychainCaseKeyStore()
    }
}
