import Foundation

/// The place of the last Copy for AI or Share, which the Restore extension
/// restores with by default (#1091, the maintainer's (a)): a Project's id,
/// or an Unfiled document's, and nothing else. The app writes it; the
/// extension reads it. A small file in the App Group beside the Vault, not
/// in it (the Vault keeps no "current" pointer) and not in the old
/// library's folder, with complete file protection and no backup.
struct RestorePlaceFile: Sendable {
    let url: URL

    init(_ storage: MdocStorage) {
        url = storage.restorePlace
    }

    /// The place written last; nil when there is none or it can't be read.
    func read() -> RestorePlace? {
        guard let data = try? Data(contentsOf: url),
              let stored = try? JSONDecoder().decode(Stored.self, from: data)
        else { return nil }
        if let project = stored.project { return .project(ProjectID(rawValue: project)) }
        if let document = stored.document { return .document(DocumentID(rawValue: document)) }
        return nil
    }

    func write(_ place: RestorePlace) throws {
        let stored = switch place {
        case .project(let id): Stored(project: id.rawValue, document: nil)
        case .document(let id): Stored(project: nil, document: id.rawValue)
        }
        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys
        try encoder.encode(stored).write(to: url, options: [.atomic, .completeFileProtection])
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var target = url
        try target.setResourceValues(values)
    }

    /// `{"project": "<id>"}` or `{"document": "<id>"}`.
    private struct Stored: Codable {
        var project: String?
        var document: String?
    }
}
