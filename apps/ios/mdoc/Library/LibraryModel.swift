import Foundation
import Observation

/// Something brought in that waits for its title and case.
struct PendingImport: Identifiable, Equatable {
    let id = UUID()
    var source: ImportSource
    var suggestedTitle: String
}

/// The library as the sidebar, Start, Cases and the Documents page show it
/// (ADR 0102): every document, newest first, each with its case, and the
/// library's changes. A change throws; `AppModel` shows the error.
@MainActor
@Observable
final class LibraryModel {
    private let library: any Library
    /// The device runs the name detector (ADR 0088).
    let findsNames: Bool
    private(set) var snapshot: LibrarySnapshot = .empty
    /// Deleted documents still within their toast's Undo (Q17): left out of
    /// every list until the delete happens or is undone.
    private(set) var hidden: Set<DocumentID> = []

    init(library: any Library, findsNames: Bool = true) {
        self.library = library
        self.findsNames = findsNames
        refresh()
    }

    /// Every document, most recently changed first.
    var documents: [DocumentSummary] {
        snapshot.documents.filter { !hidden.contains($0.id) }.sorted { $0.updated > $1.updated }
    }

    var cases: [CaseSummary] { snapshot.cases }

    var isEmpty: Bool { documents.isEmpty }

    func caseSummary(of document: DocumentSummary) -> CaseSummary? {
        snapshot.cases.first { $0.id == document.caseID }
    }

    func document(_ id: DocumentID) -> DocumentSummary? {
        documents.first { $0.id == id }
    }

    // MARK: Lists

    /// The sidebar's *To review*.
    var toReview: [DocumentSummary] {
        documents.filter { $0.phase == .reviewing }
    }

    /// The document most worth going back to: the newest with names left to
    /// review (Start's Continue).
    var continueDocument: DocumentSummary? {
        toReview.max { $0.updated < $1.updated }
    }

    /// The five cases most recently created, without the one the document is
    /// in: Move to case's quick list.
    func recentCases(excluding current: CaseID?) -> [CaseSummary] {
        Array(cases.filter { $0.id != current }.prefix(5))
    }

    /// The one search over titles and case names, for the sidebar and the
    /// Documents page: the documents whose title or case matches, and the
    /// cases whose name does. An empty query matches nothing.
    func search(_ query: String) -> (documents: [DocumentSummary], cases: [CaseSummary]) {
        let needle = query.trimmingCharacters(in: .whitespaces)
        guard !needle.isEmpty else { return ([], []) }
        let documents = documents.filter { document in
            document.title.localizedStandardContains(needle)
                || (caseSummary(of: document)?.name.localizedStandardContains(needle) ?? false)
        }
        return (documents, cases.filter { $0.name.localizedStandardContains(needle) })
    }

    // MARK: The Documents page

    /// The Documents page's search, shown from `searchThreshold` documents
    /// (ADR 0099).
    var query = ""
    /// The Documents page's case filter, offered from two cases.
    var caseFilter: CaseID?
    static let searchThreshold = 8

    /// Search shows from the threshold, and stays while a query is typed,
    /// so a query can always be cleared.
    var offersSearch: Bool { documents.count >= Self.searchThreshold || !query.isEmpty }
    /// The filter shows from two cases, and stays while one is chosen.
    var offersCaseFilter: Bool { cases.count >= 2 || caseFilter != nil }

    /// The documents the page lists: the case filter, then the search.
    var shownDocuments: [DocumentSummary] {
        let filtered = documents.filter { caseFilter == nil || $0.caseID == caseFilter }
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else { return filtered }
        let found = Set(search(query).documents.map(\.id))
        return filtered.filter { found.contains($0.id) }
    }

    // MARK: Changes

    func refresh() {
        snapshot = library.snapshot()
        // A filtered case that is gone filters nothing.
        if let caseFilter, !snapshot.cases.contains(where: { $0.id == caseFilter }) {
            self.caseFilter = nil
        }
    }

    /// Imports into the chosen case and returns the new document's session.
    func importDocument(_ pending: PendingImport, title: String, into caseChoice: CaseChoice) throws -> any DocumentSession {
        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
        var caseChoice = caseChoice
        if case .newCase(let caseName) = caseChoice {
            // Trimmed first, as the core trims it: " Lease" is "Lease".
            let caseName = caseName.trimmingCharacters(in: .whitespacesAndNewlines)
            // Documents' titles count too, so a case named after its
            // document takes the number the core gives that title.
            caseChoice = .newCase(name: Self.unusedName(caseName, among: cases.map(\.name) + documents.map(\.title)))
        }
        let session = try library.importDocument(
            pending.source, title: name.isEmpty ? pending.suggestedTitle : name, into: caseChoice)
        refresh()
        return session
    }

    /// `name`, or "name 2", "name 3"… when a case has it already, as the
    /// core numbers a document's title: two cases of one name could not be
    /// told apart in the case pickers.
    static func unusedName(_ name: String, among names: [String]) -> String {
        let taken = Set(names)
        guard taken.contains(name) else { return name }
        return (2...).lazy.map { "\(name) \($0)" }.first { !taken.contains($0) }!
    }

    /// Renames a document; a blank title changes nothing.
    func rename(_ id: DocumentID, to title: String) throws {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        try library.renameDocument(id, to: title)
        refresh()
    }

    /// Renames a case; a blank name changes nothing.
    func renameCase(_ id: CaseID, to name: String) throws {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return }
        try library.renameCase(id, to: name)
        refresh()
    }

    /// Move to case. A new case is named as every new case is (#677):
    /// trimmed, never blank (nil, nothing moves), and numbered when a case
    /// has the name already.
    func move(_ id: DocumentID, to target: CaseChoice) throws -> CaseMove? {
        var target = target
        if case .newCase(let name) = target {
            let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !name.isEmpty else { return nil }
            target = .newCase(name: Self.unusedName(name, among: cases.map(\.name)))
        }
        defer { refresh() }
        return try library.moveDocument(id, to: target)
    }

    func undoMove(_ move: CaseMove) throws -> CaseMove {
        defer { refresh() }
        return try library.undoMove(move)
    }

    /// Delete's first half: the document leaves the lists while its toast
    /// offers Undo.
    func hide(_ id: DocumentID) {
        hidden.insert(id)
    }

    /// Undo of Delete: the document is back in place.
    func unhide(_ id: DocumentID) {
        hidden.remove(id)
    }

    /// Deletes the document through the core, which also takes it out of
    /// its case.
    func delete(_ id: DocumentID) throws {
        defer {
            hidden.remove(id)
            refresh()
        }
        try library.deleteDocument(id)
    }

    /// Saves what finished while the phone was locked; returns its documents.
    func saveDeferred() throws -> [DocumentID] {
        defer { refresh() }
        return try library.saveDeferred()
    }

    // MARK: Status

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
