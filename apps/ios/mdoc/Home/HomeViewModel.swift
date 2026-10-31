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
    /// The device runs the name detector (ADR 0088).
    let findsNames: Bool
    private(set) var snapshot: LibrarySnapshot = .empty
    var pendingImport: PendingImport?
    var errorMessage: String?

    init(library: any Library, findsNames: Bool = true) {
        self.library = library
        self.findsNames = findsNames
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

    /// One line under a document's title. An interrupted import of a file
    /// that needs no recognition says so (ADR 0088); below the detector
    /// floor nothing waits for Find names.
    static func status(of document: DocumentSummary, findsNames: Bool = true) -> String {
        switch document.phase {
        case .recognizing where !document.kind.needsRecognition: String(localized: "Import not finished")
        case .recognizing: String(localized: "Text not recognized yet")
        case .unscanned where !findsNames: String(localized: "Ready to mark names")
        case .unscanned: String(localized: "Ready to find names")
        case .reviewing: String(localized: "\(document.pendingCount) to review")
        case .ready where document.candidateCount == 0: String(localized: "Nothing found")
        case .ready: String(localized: "Ready to share")
        }
    }

    /// The status's symbol: state is shown by symbol and words, never by a
    /// colour, so the case colour stays the only colour on a row.
    static func statusSymbol(of document: DocumentSummary, findsNames: Bool = true) -> String {
        switch document.phase {
        case .recognizing where !document.kind.needsRecognition: "doc.badge.clock"
        case .recognizing: "text.viewfinder"
        case .unscanned where !findsNames: "character.cursor.ibeam"
        case .unscanned: "person.text.rectangle"
        case .reviewing: "exclamationmark.circle"
        case .ready: "checkmark.circle"
        }
    }
}
