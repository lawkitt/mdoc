import Foundation

/// mdoc's on-device folders (ADR 0085). Everything lives in the App Group
/// container so the Restore extension reads the same cases. Documents,
/// Markdown, cases and models are excluded from iCloud and device backups;
/// documents, Markdown and cases use Data Protection Complete. Models keep
/// the default class so a background download can install them while the
/// phone is locked.
struct MdocStorage: Sendable {
    static let appGroup = "group.io.github.lawkitt.mdoc"

    enum Area: String, CaseIterable, Sendable {
        case documents, markdown, cases, models, handOff = "hand-off"

        /// Hand-off holds the `.md` files Share writes; it is emptied after
        /// each share and at launch.
        var protection: FileProtectionType? {
            self == .models ? nil : .complete
        }
    }

    let root: URL

    init(root: URL) {
        self.root = root
    }

    /// The App Group container, or Application Support when the group is
    /// unavailable (unit tests run without entitlements).
    static func shared(fileManager: FileManager = .default) throws -> MdocStorage {
        if let group = fileManager.containerURL(forSecurityApplicationGroupIdentifier: appGroup) {
            return MdocStorage(root: group.appendingPathComponent("mdoc", isDirectory: true))
        }
        let support = try fileManager.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        return MdocStorage(root: support.appendingPathComponent("mdoc", isDirectory: true))
    }

    func url(_ area: Area) -> URL {
        root.appendingPathComponent(area.rawValue, isDirectory: true)
    }

    /// Creates the folders with their protection and backup exclusion. Safe
    /// to call at every launch.
    func prepare(fileManager: FileManager = .default) throws {
        try fileManager.createDirectory(at: root, withIntermediateDirectories: true)
        try excludeFromBackup(root)
        for area in Area.allCases {
            let folder = url(area)
            var attributes: [FileAttributeKey: Any] = [:]
            if let protection = area.protection {
                attributes[.protectionKey] = protection
            }
            try fileManager.createDirectory(at: folder, withIntermediateDirectories: true, attributes: attributes)
            if let protection = area.protection {
                // New files inherit the folder's class; set it on folders that
                // existed before too.
                try fileManager.setAttributes([.protectionKey: protection], ofItemAtPath: folder.path)
            }
            try excludeFromBackup(folder)
        }
    }

    /// Writes a file atomically with its area's protection.
    @discardableResult
    func write(_ data: Data, named name: String, in area: Area) throws -> URL {
        let target = url(area).appendingPathComponent(name)
        var options: Data.WritingOptions = [.atomic]
        if area.protection == .complete {
            options.insert(.completeFileProtection)
        }
        try data.write(to: target, options: options)
        try excludeFromBackup(target)
        return target
    }

    /// Copies a file the user picked into an area under a fresh name.
    func importCopy(of source: URL, in area: Area, fileManager: FileManager = .default) throws -> URL {
        let name = "\(UUID().uuidString)-\(source.lastPathComponent)"
        let target = url(area).appendingPathComponent(name)
        try fileManager.copyItem(at: source, to: target)
        if let protection = area.protection {
            try fileManager.setAttributes([.protectionKey: protection], ofItemAtPath: target.path)
        }
        try excludeFromBackup(target)
        return target
    }

    func clear(_ area: Area, fileManager: FileManager = .default) {
        let folder = url(area)
        let items = (try? fileManager.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
        for item in items {
            try? fileManager.removeItem(at: item)
        }
    }

    private func excludeFromBackup(_ url: URL) throws {
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var target = url
        try target.setResourceValues(values)
    }
}
