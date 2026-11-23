import Foundation

/// Stands in when the core cannot open (the Keychain refused the Vault key,
/// or storage is missing): nothing to list, every action explains why.
final class UnavailableLibrary: Library, @unchecked Sendable {
    func snapshot() -> LibrarySnapshot { .empty }
    func session(for document: DocumentID) throws -> any DocumentSession { throw CoreError.notFound }
    func importDocument(_ source: ImportSource, title: String, into project: ProjectID?) throws -> any DocumentSession {
        throw CoreError.noProject
    }
    func deleteDocument(_ document: DocumentID) throws { throw CoreError.notFound }
    func restore(reply: String, in projectID: ProjectID) throws -> RestoreResult { throw CoreError.noProject }
    func saveReply(_ reply: String, to document: DocumentID) throws { throw CoreError.noProject }
}

final class UnavailableSetup: Setup, @unchecked Sendable {
    func snapshot() -> SetupSnapshot {
        SetupSnapshot(physicalMemory: ProcessInfo.processInfo.physicalMemory, tier: .belowFloor, offers: [])
    }
    func downloadRequests(for model: String) throws -> [ModelDownloadRequest] { [] }
    func install(_ request: ModelDownloadRequest, from file: URL) throws -> SetupSnapshot { snapshot() }
}
