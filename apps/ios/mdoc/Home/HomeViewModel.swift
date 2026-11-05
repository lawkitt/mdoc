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

    /// Home's search, shown from `searchThreshold` documents (ADR 0099).
    var query = ""
    /// Home's case filter, offered from two cases.
    var caseFilter: CaseID?
    static let searchThreshold = 8

    /// Search shows from the threshold, and stays while a query is typed,
    /// so a query can always be cleared.
    var offersSearch: Bool { snapshot.documents.count >= Self.searchThreshold || !query.isEmpty }
    /// The filter shows from two cases, and stays while one is chosen.
    var offersCaseFilter: Bool { snapshot.cases.count >= 2 || caseFilter != nil }

    /// The documents Home lists: the case filter, then the search over
    /// titles and case names.
    var shownDocuments: [DocumentSummary] {
        let needle = query.trimmingCharacters(in: .whitespaces)
        return documents.filter { document in
            if let caseFilter, document.caseID != caseFilter { return false }
            guard !needle.isEmpty else { return true }
            return document.title.localizedStandardContains(needle)
                || (caseSummary(of: document)?.name.localizedStandardContains(needle) ?? false)
        }
    }

    func caseSummary(of document: DocumentSummary) -> CaseSummary? {
        snapshot.cases.first { $0.id == document.caseID }
    }

    var isEmpty: Bool { snapshot.documents.isEmpty }

    var cases: [CaseSummary] { snapshot.cases }

    func refresh() {
        snapshot = library.snapshot()
        // A filtered case that is gone filters nothing.
        if let caseFilter, !snapshot.cases.contains(where: { $0.id == caseFilter }) {
            self.caseFilter = nil
        }
    }

    /// Imports into the chosen case and returns the new document's session.
    func importDocument(_ pending: PendingImport, title: String, into caseChoice: CaseChoice) -> (any DocumentSession)? {
        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
        var caseChoice = caseChoice
        if case .newCase(let caseName) = caseChoice {
            // Trimmed first, as the core trims it: " Lease" is "Lease".
            let caseName = caseName.trimmingCharacters(in: .whitespacesAndNewlines)
            // Documents' titles count too, so a case named after its
            // document takes the number the core gives that title.
            caseChoice = .newCase(name: Self.unusedName(caseName, among: cases.map(\.name) + documents.map(\.title)))
        }
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

    /// `name`, or "name 2", "name 3"… when a case has it already, as the
    /// core numbers a document's title: two cases of one name could not be
    /// told apart in the case pickers.
    static func unusedName(_ name: String, among names: [String]) -> String {
        let taken = Set(names)
        guard taken.contains(name) else { return name }
        return (2...).lazy.map { "\(name) \($0)" }.first { !taken.contains($0) }!
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

    /// The status's color (ADR 0099): to review orange, ready green, the
    /// rest neutral.
    static func statusKind(of document: DocumentSummary) -> StatusPill.Kind {
        switch document.phase {
        case .reviewing: .toReview
        case .ready where document.candidateCount > 0: .ready
        default: .neutral
        }
    }

    /// The status's symbol, beside its words and color.
    static func statusSymbol(of document: DocumentSummary, findsNames: Bool = true) -> String {
        switch document.phase {
        case .recognizing where !document.kind.needsRecognition: "doc.badge.clock"
        case .recognizing: "text.viewfinder"
        case .unscanned where !findsNames: "character.cursor.ibeam"
        case .unscanned: "text.magnifyingglass"
        case .reviewing: "exclamationmark.circle"
        case .ready: "checkmark.circle"
        }
    }
}
