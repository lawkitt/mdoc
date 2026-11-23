import Foundation
import MdocFFI

/// What the Restore with mdoc extension reads and writes (#1091 Decisions
/// 20 and 37): the Vault's Projects and Unfiled documents, a reply restored
/// with one of them, and the reply saved as a new reply file. Nothing else
/// is written, not even the manifest; the app picks a saved reply up
/// through `Library.takeInChanges`.
protocol ReplyStore: AnyObject, Sendable {
    /// The places a reply can restore with, as the picker lists them.
    func places() -> RestorePlaces
    /// `reply` restored as saving it with `place` would restore it.
    func restore(_ reply: String, with place: RestorePlace) throws -> RestoreResult
    /// Saves `reply`, as received, for `place`: a Project's reply, or an
    /// Unfiled document's.
    func save(_ reply: String, with place: RestorePlace, title: String) throws -> SavedReply
}

/// The Restore picker's places: Projects, pinned first, then most recently
/// updated (#1091 Decision 28), then Unfiled documents, most recently
/// updated first.
struct RestorePlaces: Equatable, Sendable {
    var projects: [ProjectSummary]
    var unfiled: [LibraryDocument]

    static let empty = RestorePlaces(projects: [], unfiled: [])

    var all: [RestorePlace] {
        projects.map { .project($0.id) } + unfiled.map { .document($0.id) }
    }

    var isEmpty: Bool { projects.isEmpty && unfiled.isEmpty }

    func contains(_ place: RestorePlace) -> Bool { all.contains(place) }

    func name(of place: RestorePlace) -> String? {
        switch place {
        case .project(let id): projects.first { $0.id == id }?.name
        case .document(let id): unfiled.first { $0.id == id }?.title
        }
    }
}

/// The extension's store over the core's `ReplyVault`: the Vault folder the
/// app made, opened with the shared Keychain key. A Vault folder that does
/// not exist yet is refused, never made.
final class VaultReplyStore: ReplyStore, @unchecked Sendable {
    private let vault: MdocFFI.ReplyVault

    init(storage: MdocStorage, keys: any VaultKeyStore) throws {
        guard FileManager.default.fileExists(atPath: storage.vault.path) else {
            throw CoreError.refused(String(localized: "Open mdoc first: it has nothing to restore with yet."))
        }
        vault = try vaultCall { try MdocFFI.ReplyVault.open(root: storage.vault.path, key: try keys.key()) }
    }

    func places() -> RestorePlaces {
        RestorePlaces(
            projects: vault.projects().map(ProjectSummary.init),
            unfiled: vault.unfiled().map { LibraryDocument($0, id: DocumentID(rawValue: $0.id)) })
    }

    func restore(_ reply: String, with place: RestorePlace) throws -> RestoreResult {
        RestoreResult(try vaultCall { try vault.restoreText(sharedFrom: place.sharedFrom, markdown: reply) })
    }

    func save(_ reply: String, with place: RestorePlace, title: String) throws -> SavedReply {
        SavedReply(try vaultCall {
            try vault.saveReply(sharedFrom: place.sharedFrom, title: title, sourceApp: nil, markdown: reply)
        })
    }
}

extension RestorePlace {
    /// What the core restores and saves a reply with: a Project, or an
    /// Unfiled document.
    var sharedFrom: SharedFrom {
        switch self {
        case .project(let id): .project(id: id.rawValue)
        case .document(let id): .document(id: id.rawValue)
        }
    }
}

extension SavedReply {
    init(_ info: ReplyInfo) {
        self.init(
            id: ReplyID(rawValue: info.id), title: info.title, sourceApp: info.sourceApp,
            created: Date(timeIntervalSince1970: TimeInterval(info.created)))
    }
}

extension LibraryDocument {
    /// A saved document as the Vault's manifest lists it, by `id`.
    init(_ info: DocumentInfo, id: DocumentID) {
        self.init(
            id: id, title: info.title, projectID: info.projectId.map(ProjectID.init),
            kind: SourceKind(info.sourceKind), phase: info.phase, pendingCount: Int(info.proposed),
            updated: Date(timeIntervalSince1970: TimeInterval(info.updated)),
            candidateCount: Int(info.proposed + info.replaced + info.kept), origin: info.origin)
    }
}

/// Runs a core call and turns its error into the app's.
private func vaultCall<T>(_ call: () throws -> T) throws -> T {
    do {
        return try call()
    } catch let error as CoreError {
        throw error
    } catch {
        throw CoreError(error)
    }
}
