import Foundation

/// mdoc's on-device folders (ADR 0085). Everything lives in the App Group
/// container so the Restore extension reads the same cases. The core's
/// `Library` keeps Markdown in `documents/` and cases in `cases/`; the app
/// keeps the source files in `originals/`. All of it is excluded from iCloud
/// and device backups, and everything but models and their downloads uses
/// Data Protection Complete. Models keep the default class so a background
/// download can install them while the phone is locked.
struct MdocStorage: Sendable {
    static let appGroup = "group.io.github.lawkitt.mdoc"

    enum Area: String, CaseIterable, Sendable {
        /// The core's Markdown documents.
        case documents
        /// The core's sealed cases.
        case cases
        /// Imported PDFs, DOCX files and page images, for the Original view.
        case originals
        case models
        /// Model files downloaded and waiting for the core to install them.
        case downloads
        /// The `.md` files Share writes; emptied after each share and at launch.
        case handOff = "hand-off"

        var protection: FileProtectionType? {
            self == .models || self == .downloads ? nil : .complete
        }
    }

    let root: URL

    init(root: URL) {
        self.root = root
    }

    /// `folder` in the App Group container, or in Application Support when
    /// the group is unavailable.
    static func shared(folder: String = "mdoc", fileManager: FileManager = .default) throws -> MdocStorage {
        if let group = fileManager.containerURL(forSecurityApplicationGroupIdentifier: appGroup) {
            return MdocStorage(root: group.appendingPathComponent(folder, isDirectory: true))
        }
        let support = try fileManager.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        return MdocStorage(root: support.appendingPathComponent(folder, isDirectory: true))
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

    /// Copies a file the user picked into an area, in a folder of its own
    /// so it keeps its name (the core titles a document after its file).
    /// `name` renames it, keeping the extension.
    func importCopy(of source: URL, in area: Area, named name: String? = nil, fileManager: FileManager = .default) throws -> URL {
        let folder = url(area).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try fileManager.createDirectory(at: folder, withIntermediateDirectories: true)
        let fileName = name.map { Self.fileName($0, extension: source.pathExtension) } ?? source.lastPathComponent
        let target = folder.appendingPathComponent(fileName)
        try fileManager.copyItem(at: source, to: target)
        if let protection = area.protection {
            try fileManager.setAttributes([.protectionKey: protection], ofItemAtPath: target.path)
        }
        try excludeFromBackup(target)
        return target
    }

    /// A file name from a title: no path separators, the extension kept.
    static func fileName(_ title: String, extension ext: String) -> String {
        let cleaned = title.trimmingCharacters(in: .whitespacesAndNewlines)
            .components(separatedBy: CharacterSet(charactersIn: "/:\\"))
            .joined(separator: "-")
        let base = cleaned.isEmpty ? "Document" : cleaned
        return ext.isEmpty ? base : "\(base).\(ext)"
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
