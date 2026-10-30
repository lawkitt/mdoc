import Foundation
import Observation

/// Restores a shared AI reply in place for the Restore with mdoc extension.
/// It uses one case at a time, the case of the document shared last unless
/// the user picks another, so aliases from different cases never mix
/// (ADR 0084's rule). It needs no model.
@MainActor
@Observable
final class RestoreViewModel {
    let reply: String
    private let library: any Library
    private(set) var snapshot: LibrarySnapshot
    private(set) var selectedCase: CaseID?
    private(set) var result: RestoreResult?
    private(set) var errorMessage: String?
    private(set) var savedTo: DocumentID?

    init(library: any Library, reply: String) {
        self.library = library
        self.reply = reply
        snapshot = library.snapshot()
        selectedCase = snapshot.lastSharedCase ?? snapshot.cases.first?.id
        restore()
    }

    var cases: [CaseSummary] { snapshot.cases }

    var selectedCaseName: String? {
        cases.first { $0.id == selectedCase }?.name
    }

    var documentsInCase: [DocumentSummary] {
        guard let selectedCase else { return [] }
        return snapshot.documents(in: selectedCase)
    }

    /// Save to document's default: the document shared last when it is in
    /// this case, else the case's most recent one.
    var defaultSaveTarget: DocumentSummary? {
        let documents = documentsInCase
        return documents.first { $0.id == snapshot.lastSharedDocument } ?? documents.first
    }

    func selectCase(_ id: CaseID) {
        guard id != selectedCase else { return }
        selectedCase = id
        savedTo = nil
        restore()
    }

    private func restore() {
        guard let selectedCase else {
            result = nil
            errorMessage = CoreError.noCase.errorDescription
            return
        }
        do {
            result = try library.restore(reply: reply, in: selectedCase)
            errorMessage = nil
        } catch {
            result = nil
            errorMessage = error.localizedDescription
        }
    }

    /// Keeps the reply, in alias form, as the document's reply (ADR 0078).
    func save(to document: DocumentID) {
        do {
            try library.saveReply(reply, to: document)
            savedTo = document
            snapshot = library.snapshot()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func title(of document: DocumentID) -> String? {
        snapshot.documents.first { $0.id == document }?.title
    }
}
