import Foundation

/// A UI test run's prebuilt library (`LaunchConfiguration.prebuiltLibrary`):
/// the run's first launch with a seed imports and scans it as ever, then
/// copies the library that made beside the test library; each later launch
/// of the run with the same seed copies it back into the cleared test
/// library, before the core opens it, instead of importing the seed again.
/// The case key is the test library's Keychain item, which a launch keeps,
/// so the copy opens. Libraries of earlier runs go when a run builds its
/// first. Only UI tests that ask for it take this path; the app never does.
struct UITestPrebuiltLibrary: Sendable {
    /// Beside the test library in the App Group container.
    static let folderName = "mdoc-ui-testing-prebuilt"
    /// In the test library, once a launch has copied the prebuilt one in.
    static let markerName = ".prebuilt"

    /// The test library.
    let root: URL
    /// Where every run's prebuilt libraries are.
    let folder: URL
    /// This run's, for this seed.
    let library: URL
    /// The run's own prefix of `library`'s name.
    private let run: String

    init?(_ launch: LaunchConfiguration, storage: MdocStorage) {
        guard launch.isUITesting, !launch.keepsLibrary, let run = launch.prebuiltLibrary, let seed = launch.seedFile else {
            return nil
        }
        root = storage.root
        folder = root.deletingLastPathComponent().appendingPathComponent(Self.folderName, isDirectory: true)
        self.run = Self.hash(run)
        library = folder.appendingPathComponent("\(self.run)-\(Self.hash(seed))", isDirectory: true)
    }

    /// The test library was copied from the prebuilt one at this launch.
    var isRestored: Bool {
        FileManager.default.fileExists(atPath: root.appendingPathComponent(Self.markerName).path)
    }

    /// Copies the run's library for this seed into the test library, which
    /// was just cleared (all but a downloaded detector, which the copy
    /// lacks). False, having copied nothing, when the run has none yet:
    /// then libraries of earlier runs go.
    @discardableResult
    func restore() throws -> Bool {
        let files = FileManager.default
        guard files.fileExists(atPath: library.path) else {
            let earlier = (try? files.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
            for item in earlier where !item.lastPathComponent.hasPrefix("\(run)-") {
                try? files.removeItem(at: item)
            }
            return false
        }
        try files.createDirectory(at: root, withIntermediateDirectories: true)
        for item in try files.contentsOfDirectory(at: library, includingPropertiesForKeys: nil) {
            try files.copyItem(at: item, to: root.appendingPathComponent(item.lastPathComponent))
        }
        try Data().write(to: root.appendingPathComponent(Self.markerName))
        return true
    }

    /// Keeps the test library as the run's prebuilt one for this seed: the
    /// seed imported, scanned and saved. Models, downloads and the share's
    /// hand-off stay out. Written whole or not at all.
    func save() {
        let files = FileManager.default
        guard !files.fileExists(atPath: library.path) else { return }
        let partial = library.appendingPathExtension("part")
        try? files.removeItem(at: partial)
        let left: Set = [MdocStorage.Area.models.rawValue, MdocStorage.Area.downloads.rawValue,
                         MdocStorage.Area.handOff.rawValue, Self.markerName]
        do {
            try files.createDirectory(at: partial, withIntermediateDirectories: true)
            for item in try files.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
            where !left.contains(item.lastPathComponent) {
                try files.copyItem(at: item, to: partial.appendingPathComponent(item.lastPathComponent))
            }
            try files.moveItem(at: partial, to: library)
        } catch {
            try? files.removeItem(at: partial)
        }
    }

    /// FNV-1a, 64 bits, in hex: a short file name for a run or a seed path.
    static func hash(_ text: String) -> String {
        var value: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in text.utf8 {
            value = (value ^ UInt64(byte)) &* 0x0000_0100_0000_01b3
        }
        return String(value, radix: 16)
    }
}
