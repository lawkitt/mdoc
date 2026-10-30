import Foundation
import Observation

/// Something brought in that waits for its title and case.
struct PendingImport: Identifiable, Equatable {
    let id = UUID()
    var source: ImportSource
    var suggestedTitle: String
}

/// Home: documents grouped by case, newest case first.
@MainActor
@Observable
final class HomeViewModel {
    struct Section: Identifiable, Equatable {
        let caseSummary: CaseSummary
        var documents: [DocumentSummary]
        var id: CaseID { caseSummary.id }
    }

    private let library: any Library
    private(set) var snapshot: LibrarySnapshot = .empty
    var pendingImport: PendingImport?
    var errorMessage: String?

    init(library: any Library) {
        self.library = library
        refresh()
    }

    var sections: [Section] {
        let sections = snapshot.cases.compactMap { item -> Section? in
            let documents = snapshot.documents(in: item.id).sorted { $0.updated > $1.updated }
            return documents.isEmpty ? nil : Section(caseSummary: item, documents: documents)
        }
        return sections.sorted { ($0.documents.first?.updated ?? .distantPast) > ($1.documents.first?.updated ?? .distantPast) }
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
        case .ready: String(localized: "Ready to share")
        }
    }
}
