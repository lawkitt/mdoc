import Foundation
import Observation
import UIKit

/// What the Restore with mdoc sheet shows.
enum RestoreSheet {
    case ready(RestoreViewModel)
    /// Nothing was shared as text.
    case noText
    /// mdoc's storage or its case key could not be opened: the error's own
    /// words.
    case failed(String)

    @MainActor
    static func make(text: String?, library: () throws -> any Library) -> RestoreSheet {
        guard let text else { return .noText }
        do {
            return .ready(RestoreViewModel(library: try library(), reply: text))
        } catch {
            return .failed(error.localizedDescription)
        }
    }
}

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
    /// The restored text shown was copied.
    private(set) var copied = false

    init(library: any Library, reply: String) {
        self.library = library
        self.reply = reply
        snapshot = library.snapshot()
        selectedCase = snapshot.currentCase ?? snapshot.cases.first?.id
        restore()
    }

    var cases: [CaseSummary] { snapshot.cases }

    var selectedCaseName: String? {
        cases.first { $0.id == selectedCase }?.name
    }

    var documentsInCase: [LibraryDocument] {
        guard let selectedCase else { return [] }
        return snapshot.documents(in: selectedCase)
    }

    /// Save to document's default: the case's most recently changed document.
    var defaultSaveTarget: LibraryDocument? {
        documentsInCase.first
    }

    func selectCase(_ id: CaseID) {
        guard id != selectedCase else { return }
        selectedCase = id
        savedTo = nil
        copied = false
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
            errorMessage = nil
            snapshot = library.snapshot()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func copy(to pasteboard: UIPasteboard = .general) {
        guard let result else { return }
        RestoredPasteboard.copy(result.text, to: pasteboard)
        copied = true
    }

    func title(of document: DocumentID) -> String? {
        snapshot.documents.first { $0.id == document }?.title
    }
}
