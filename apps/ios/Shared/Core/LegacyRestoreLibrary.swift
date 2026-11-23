import Foundation
import MdocFFI

/// The Restore extension's library until Vault & Projects item 10 moves it
/// onto the Vault: the old library's cases, read to restore a reply and to
/// keep it for one of their documents, as before the app cut over (#1091
/// Q3). The app no longer writes there, so it holds only what was shared
/// before the cutover. Nothing else is offered: the app's documents are the
/// Vault's (`RealLibrary`).
final class LegacyRestoreLibrary: Library, @unchecked Sendable {
    private let ffi: MdocFFI.Library

    init(storage: MdocStorage, keys: any VaultKeyStore) throws {
        ffi = try legacyCore { try MdocFFI.Library(root: storage.root.path, caseKey: try keys.key()) }
    }

    func snapshot() -> LibrarySnapshot {
        let projects = ffi.cases().cases.map {
            ProjectSummary(id: ProjectID(rawValue: $0.id), name: $0.name, colour: ProjectColour(hue: $0.hue))
        }
        let summaries = (try? ffi.documentSummaries()) ?? []
        let documents: [LibraryDocument] = summaries.map { summary -> LibraryDocument in
            // Broken up: Xcode 26.2 can't type-check it as one expression.
            let pending = Int(summary.proposed)
            let candidates = pending + Int(summary.replaced) + Int(summary.kept)
            let updated = Date(timeIntervalSince1970: TimeInterval(summary.modified))
            let projectID: ProjectID? = summary.caseId.map { ProjectID(rawValue: $0) }
            return LibraryDocument(
                id: DocumentID(rawValue: summary.path), title: summary.title,
                projectID: projectID, kind: .text, phase: summary.phase,
                pendingCount: pending, updated: updated,
                candidateCount: candidates, origin: summary.origin)
        }
        return LibrarySnapshot(
            projects: projects, documents: documents.sorted { $0.updated > $1.updated },
            currentProject: ffi.currentCase().map { ProjectID(rawValue: $0.id) })
    }

    func session(for document: DocumentID) throws -> any DocumentSession { throw CoreError.notFound }

    func importDocument(_ source: ImportSource, title: String, into project: ProjectID?) throws -> any DocumentSession {
        throw CoreError.refused(String(localized: "Open mdoc to add documents."))
    }

    func deleteDocument(_ document: DocumentID) throws { throw CoreError.notFound }

    func restore(reply: String, in projectID: ProjectID) throws -> RestoreResult {
        RestoreResult(try legacyCore { try ffi.restore(caseId: projectID.rawValue, reply: reply) })
    }

    /// Keeps the reply for one of the old library's documents, in its case.
    func saveReply(_ reply: String, to document: DocumentID) throws {
        guard let summary = try legacyCore({ try ffi.documentSummaries() }).first(where: { $0.path == document.rawValue }),
              let caseID = summary.caseId
        else { throw CoreError.noProject }
        try legacyCore { try ffi.saveReply(caseId: caseID, document: summary.path, reply: reply) }
    }
}

/// Runs an old-library call and turns its error into the app's.
private func legacyCore<T>(_ call: () throws -> T) throws -> T {
    do {
        return try call()
    } catch let error as CoreError {
        throw error
    } catch {
        throw CoreError(error)
    }
}
