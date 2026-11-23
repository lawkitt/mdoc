import Foundation
import MdocFFI

/// mdoc's on-device folders (ADR 0085, ADR 0125). Everything lives in the
/// App Group container so the Restore extension reads the same store.
///
/// - `root` (`mdoc/`) keeps the models, their downloads and the `.md` file
///   Share writes. It is also the old library's folder: its `documents/`,
///   `cases/`, `imports/` and `originals/` stay untouched (#1091 Decision 2):
///   nothing reads or writes them.
/// - `vault` (`vault/`, `vault-dev/` for UI tests) is the core's encrypted
///   Vault, a sibling of the old folder, never inside it.
/// - `intake` (`vault-intake/`, `vault-dev-intake/`) holds what an import
///   reads until its first save keeps it in the Vault: the received files,
///   the record of imports not yet read, and OCR's finished pages.
/// - `viewing` is the app's own cache (not the App Group's) for viewing a
///   Vault document's original: a decrypted copy, wiped when the document
///   closes and at launch (#1091 Decision 24).
/// - `restorePlace` (`restore-place.json`, `restore-place-dev.json` for UI
///   tests) names the place of the last Copy for AI or Share, by id only,
///   which the Restore extension restores with by default.
///
/// All of it is excluded from iCloud and device backups, and everything but
/// models and their downloads uses Data Protection Complete. Models keep the
/// default class so a background download can install them while the phone
/// is locked.
struct MdocStorage: Sendable {
    static let appGroup = "group.io.github.lawkitt.mdoc"

    enum Area: String, CaseIterable, Sendable {
        case models
        /// Model files downloaded and waiting for the core to install them.
        case downloads
        /// The `.md` file Share writes; emptied at the next share and at launch.
        case handOff = "hand-off"
        /// Received files (PDFs, DOCX files, page images), a folder each,
        /// until their document's first save keeps them in the Vault.
        case intake = "files"
        /// OCR's finished pages of imports not yet complete
        /// (`DocumentSession::import_file_resuming`).
        case ocr

        var protection: FileProtectionType? {
            self == .models || self == .downloads ? nil : .complete
        }

        /// Under `intake`, not `root`.
        var isIntake: Bool { self == .intake || self == .ocr }
    }

    let root: URL
    /// The Vault's folder, which the core opens.
    let vault: URL
    /// What imports read before their first save.
    let intake: URL
    /// Decrypted viewing copies of originals.
    let viewing: URL
    /// The place of the last Copy for AI or Share (`RestorePlaceFile`).
    let restorePlace: URL

    /// Everything under `root`: tests and previews, where no old library
    /// lives.
    init(root: URL) {
        self.init(
            root: root, vault: root.appendingPathComponent("vault", isDirectory: true),
            intake: root.appendingPathComponent("intake", isDirectory: true),
            viewing: root.appendingPathComponent("original-view", isDirectory: true),
            restorePlace: root.appendingPathComponent("restore-place.json"))
    }

    init(root: URL, vault: URL, intake: URL, viewing: URL, restorePlace: URL) {
        self.root = root
        self.vault = vault
        self.intake = intake
        self.viewing = viewing
        self.restorePlace = restorePlace
    }

    /// `folder` in the App Group container, or in Application Support when
    /// the group is unavailable, with the Vault `vaultFolder` beside it and
    /// the restore place's file beside them: `restore-place.json` for the
    /// `vault` folder, `restore-place-dev.json` for `vault-dev`. Never inside
    /// `folder`, the old library's.
    static func shared(
        folder: String = "mdoc", vaultFolder: String = "vault", fileManager: FileManager = .default
    ) throws -> MdocStorage {
        let base: URL
        if let group = fileManager.containerURL(forSecurityApplicationGroupIdentifier: appGroup) {
            base = group
        } else {
            base = try fileManager.url(
                for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        }
        let caches = try fileManager.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        return MdocStorage(
            root: base.appendingPathComponent(folder, isDirectory: true),
            vault: base.appendingPathComponent(vaultFolder, isDirectory: true),
            intake: base.appendingPathComponent("\(vaultFolder)-intake", isDirectory: true),
            viewing: caches.appendingPathComponent("\(vaultFolder)-original-view", isDirectory: true),
            restorePlace: base.appendingPathComponent("restore-place\(vaultFolder.dropFirst("vault".count)).json"))
    }

    func url(_ area: Area) -> URL {
        (area.isIntake ? intake : root).appendingPathComponent(area.rawValue, isDirectory: true)
    }

    /// Creates the folders with their protection and backup exclusion. Safe
    /// to call at every launch.
    func prepare(fileManager: FileManager = .default) throws {
        try fileManager.createDirectory(at: root, withIntermediateDirectories: true)
        try excludeFromBackup(root)
        // The Vault's folder the core fills; the intake and viewing folders
        // the app does.
        for folder in [vault, intake, viewing] {
            try fileManager.createDirectory(
                at: folder, withIntermediateDirectories: true, attributes: [.protectionKey: FileProtectionType.complete])
            try fileManager.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: folder.path)
            try excludeFromBackup(folder)
        }
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

    /// A file name from a title, the extension kept, by the core's rule
    /// (`file_stem` in `crates/mdoc-ffi/src/library.rs`, #683): look-alikes
    /// for the characters a file name cannot hold, so "Claim: Bennett"
    /// stays that title; control characters as "-"; no leading dot; at
    /// most 240 bytes; "Document" when nothing is left.
    static func fileName(_ title: String, extension ext: String) -> String {
        let base = MdocFFI.fileNameStem(title: title)
        return ext.isEmpty ? base : "\(base).\(ext)"
    }

    /// Every viewing copy goes: at launch, after a stop that left one.
    func clearViewing(fileManager: FileManager = .default) {
        let items = (try? fileManager.contentsOfDirectory(at: viewing, includingPropertiesForKeys: nil)) ?? []
        for item in items {
            try? fileManager.removeItem(at: item)
        }
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
