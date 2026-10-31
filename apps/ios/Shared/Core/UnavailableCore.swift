import Foundation

/// Stands in when the core cannot open (the Keychain refused the case key,
/// or storage is missing): nothing to list, every action explains why.
final class UnavailableLibrary: Library, @unchecked Sendable {
    func snapshot() -> LibrarySnapshot { .empty }
    func session(for document: DocumentID) throws -> any DocumentSession { throw CoreError.notFound }
    func importDocument(_ source: ImportSource, title: String, into caseChoice: CaseChoice) throws -> any DocumentSession {
        throw CoreError.noCase
    }
    func deleteDocument(_ document: DocumentID) throws { throw CoreError.notFound }
    func restore(reply: String, in caseID: CaseID) throws -> RestoreResult { throw CoreError.noCase }
    func saveReply(_ reply: String, to document: DocumentID) throws { throw CoreError.noCase }
}

final class UnavailableSetup: Setup, @unchecked Sendable {
    func snapshot() -> SetupSnapshot {
        SetupSnapshot(physicalMemory: ProcessInfo.processInfo.physicalMemory, tier: .belowFloor, offers: [])
    }
    func downloadRequests(for model: String) throws -> [ModelDownloadRequest] { [] }
    func install(_ request: ModelDownloadRequest, from file: URL) throws -> SetupSnapshot { snapshot() }
}
