import Foundation
import Observation

/// Something brought in that waits for its title and case.
struct PendingImport: Identifiable, Equatable {
    let id = UUID()
    var source: ImportSource
    var suggestedTitle: String
}

/// Home: every document, newest first, each with its case (ADR 0087).
@MainActor
@Observable
final class HomeViewModel {
    private let library: any Library
    private(set) var snapshot: LibrarySnapshot = .empty
    var pendingImport: PendingImport?
    var errorMessage: String?

    init(library: any Library) {
        self.library = library
        refresh()
    }

    /// Every document, most recently changed first.
    var documents: [DocumentSummary] {
        snapshot.documents.sorted { $0.updated > $1.updated }
    }

    func caseSummary(of document: DocumentSummary) -> CaseSummary? {
        snapshot.cases.first { $0.id == document.caseID }
    }

    var isEmpty: Bool { snapshot.documents.isEmpty }

    var cases: [CaseSummary] { snapshot.cases }

    func refresh() {
        snapshot = library.snapshot()
    }

    /// Imports into the chosen case and returns the new document's session.
    func importDocument(_ pending: PendingImport, title: String, into caseChoice: CaseChoice) -> (any DocumentSession)? {
        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            let session = try library.importDocument(
                pending.source, title: name.isEmpty ? pending.suggestedTitle : name, into: caseChoice)
            pendingImport = nil
            refresh()
            return session
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func delete(_ document: DocumentID) {
        do {
            try library.deleteDocument(document)
        } catch {
            errorMessage = error.localizedDescription
        }
        refresh()
    }

    /// One line under a document's title.
    static func status(of document: DocumentSummary) -> String {
        switch document.phase {
        case .recognizing: String(localized: "Text not recognized yet")
        case .unscanned: String(localized: "Ready to find names")
        case .reviewing: String(localized: "\(document.pendingCount) to review")
        case .ready where document.candidateCount == 0: String(localized: "Nothing found")
        case .ready: String(localized: "Ready to share")
        }
    }

    /// The status's symbol: state is shown by symbol and words, never by a
    /// colour, so the case colour stays the only colour on a row.
    static func statusSymbol(of document: DocumentSummary) -> String {
        switch document.phase {
        case .recognizing: "text.viewfinder"
        case .unscanned: "person.text.rectangle"
        case .reviewing: "exclamationmark.circle"
        case .ready: "checkmark.circle"
        }
    }
}
