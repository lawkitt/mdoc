import Foundation
import Observation

/// Something brought in that waits for its title and case.
struct PendingImport: Identifiable, Equatable, Sendable {
    let id = UUID()
    var source: ImportSource
    var suggestedTitle: String
    /// Pages of a camera or Photos batch that couldn't be read and were
    /// left out; a note says so.
    var unreadPages = 0
    /// The name of the file it came from, from Files or "Open in mdoc".
    var fileName: String? = nil
    /// The Project it was started from, where the import sheet starts;
    /// nil, Unfiled (#1091 Decision 35).
    var project: ProjectID? = nil

    /// The one file whose bytes a saved document records as its source
    /// (#1091 Decision 7): a file, or a single photo. Camera pages and
    /// photos picked together become a PDF the core makes, and a paste
    /// keeps no source.
    var sourceFile: URL? {
        switch source {
        case .file(let url, _): url
        case .photos(let urls) where urls.count == 1: urls[0]
        default: nil
        }
    }
}

/// An import whose file is already in mdoc (#1091 Decision 7): it asks
/// Open Existing or Import Again.
struct RepeatedImport: Identifiable, Equatable, Sendable {
    let pending: PendingImport
    /// The document to open instead: the most recently updated of those
    /// imported from the same bytes.
    let existing: DocumentID
    /// The question's name for it: the file's, else the document's title.
    let name: String
    var id: UUID { pending.id }
}

/// The library as the sidebar, Start, Cases and the Documents page show it
/// (ADR 0102): every document, newest first, each with its case, and the
/// library's changes. A change throws; `AppModel` shows the error.
@MainActor
@Observable
final class LibraryModel {
    private let library: any Library
    /// The device runs the name detector (ADR 0088), as setup says now:
    /// timing the detector can put the phone below the floor (ADR 0120).
    var findsNames: Bool { canFindNames() }
    private let canFindNames: @MainActor () -> Bool
    private(set) var snapshot: LibrarySnapshot = .empty
    /// Deleted documents still within their toast's Undo (Q17): left out of
    /// every list until the delete happens or is undone.
    private(set) var hidden: Set<DocumentID> = []

    init(library: any Library, findsNames: Bool = true, canFindNames: (@MainActor () -> Bool)? = nil) {
        self.library = library
        self.canFindNames = canFindNames ?? { findsNames }
        refresh()
    }

    /// Every document, most recently changed first.
    var documents: [LibraryDocument] {
        snapshot.documents.filter { !hidden.contains($0.id) }.sorted { $0.updated > $1.updated }
    }

    /// Pinned first, then most recently updated (#1091 Decision 28).
    var projects: [ProjectSummary] { snapshot.projects }

    /// The sidebar's Projects: the first five.
    var topProjects: [ProjectSummary] { Array(projects.prefix(Self.sidebarProjects)) }
    static let sidebarProjects = 5

    func project(_ id: ProjectID) -> ProjectSummary? {
        projects.first { $0.id == id }
    }

    /// A Project's documents, most recently changed first.
    func documents(in project: ProjectID) -> [LibraryDocument] {
        documents.filter { $0.projectID == project }
    }

    /// Unfiled: the documents in no Project, most recently changed first.
    var unfiled: [LibraryDocument] {
        documents.filter { $0.projectID == nil }
    }

    var isEmpty: Bool { documents.isEmpty }

    func projectSummary(of document: LibraryDocument) -> ProjectSummary? {
        snapshot.projects.first { $0.id == document.projectID }
    }

    func document(_ id: DocumentID) -> LibraryDocument? {
        documents.first { $0.id == id }
    }

    // MARK: Lists

    /// The sidebar's *To review*.
    var toReview: [LibraryDocument] {
        documents.filter { $0.phase == .reviewing }
    }

    /// The document most worth going back to: the newest with names left to
    /// review (Start's Continue).
    var continueDocument: LibraryDocument? {
        toReview.max { $0.updated < $1.updated }
    }

    /// The five projects most recently created, without the one the document is
    /// in: Move to case's quick list.
    func recentProjects(excluding current: ProjectID?) -> [ProjectSummary] {
        Array(projects.filter { $0.id != current }.prefix(5))
    }

    /// The one search over titles and case names, for the sidebar and the
    /// Documents page: the documents whose title or case matches, and the
    /// projects whose name does. An empty query matches nothing.
    func search(_ query: String) -> (documents: [LibraryDocument], projects: [ProjectSummary]) {
        let needle = query.trimmingCharacters(in: .whitespaces)
        guard !needle.isEmpty else { return ([], []) }
        let documents = documents.filter { document in
            document.title.localizedStandardContains(needle)
                || (projectSummary(of: document)?.name.localizedStandardContains(needle) ?? false)
        }
        return (documents, projects.filter { $0.name.localizedStandardContains(needle) })
    }

    // MARK: All Documents

    /// All Documents' search, shown from `searchThreshold` documents
    /// (ADR 0099).
    var query = ""
    static let searchThreshold = 8

    /// Search shows from the threshold, and stays while a query is typed,
    /// so a query can always be cleared.
    var offersSearch: Bool { documents.count >= Self.searchThreshold || !query.isEmpty }

    /// The documents All Documents lists: every one the search lets through.
    var shownDocuments: [LibraryDocument] {
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else { return documents }
        let found = Set(search(query).documents.map(\.id))
        return documents.filter { found.contains($0.id) }
    }

    // MARK: A Project's page

    /// A Project's saved replies, newest first, for its page's Replies.
    func replies(in project: ProjectID) -> [SavedReply] {
        snapshot.projectReplies[project] ?? []
    }

    /// A Project's names and its Always and Never lists, read from the
    /// Vault as they are now; empty when it can't be read. Read again on
    /// every change the lists see.
    func details(of project: ProjectID) -> ProjectDetails {
        _ = snapshot
        return (try? library.projectDetails(project)) ?? .empty
    }

    /// A Project's names matching `query` (by alias or wording), grouped by
    /// category in the categories' order, each group by alias.
    static func names(_ names: [ProjectNameRow], matching query: String) -> [(Category, [ProjectNameRow])] {
        let needle = query.trimmingCharacters(in: .whitespaces)
        let found = names.filter { name in
            needle.isEmpty || name.alias.localizedStandardContains(needle)
                || name.originals.contains { $0.localizedStandardContains(needle) }
        }
        return Category.allCases.compactMap { category in
            let group = found.filter { $0.category == category }
                .sorted { $0.alias.localizedStandardCompare($1.alias) == .orderedAscending }
            return group.isEmpty ? nil : (category, group)
        }
    }

    /// The wordings of a list matching `query`.
    static func wordings(_ wordings: [String], matching query: String) -> [String] {
        let needle = query.trimmingCharacters(in: .whitespaces)
        return needle.isEmpty ? wordings : wordings.filter { $0.localizedStandardContains(needle) }
    }

    /// A saved reply restored, or why it can't be.
    func restore(_ reply: ReplyID) -> Result<RestoreResult, CoreError> {
        do {
            return .success(try library.restoreSavedReply(reply))
        } catch {
            return .failure(error as? CoreError ?? CoreError(error))
        }
    }

    /// Promote to Document: the new document, listed (#1091 Decision 3).
    func promote(_ reply: ReplyID) throws -> DocumentID {
        defer { refresh() }
        return try library.promoteReply(reply)
    }

    // MARK: Changes

    func refresh() {
        snapshot = library.snapshot()
    }

    /// The saved document an import repeats, when its file's bytes are
    /// those of one already in mdoc (#1091 Decision 7): the most recently
    /// updated, among those listed.
    func repeatOf(_ pending: PendingImport) -> RepeatedImport? {
        guard let file = pending.sourceFile else { return nil }
        let listed = Set(documents.map(\.id))
        guard let existing = library.documentsFromSource(file).first(where: listed.contains),
              let document = document(existing) else { return nil }
        return RepeatedImport(pending: pending, existing: existing, name: pending.fileName ?? document.title)
    }

    /// New Project: named as every new Project is (#677), trimmed and
    /// numbered when a Project has the name already; nil for a blank name.
    func createProject(named name: String) throws -> ProjectSummary? {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return nil }
        defer { refresh() }
        return try library.createProject(named: Self.unusedName(name, among: projects.map(\.name)))
    }

    func deleteProject(_ id: ProjectID, documents: ProjectDeletion) throws {
        defer { refresh() }
        try library.deleteProject(id, documents: documents)
    }

    /// Takes a wording off the Project's Always or Never list.
    func removeListed(_ wording: String, always: Bool, from project: ProjectID) throws {
        defer { refresh() }
        try library.removeListed(wording, always: always, from: project)
    }

    /// Imports into Unfiled or the chosen Project and returns the new
    /// document's session.
    func importDocument(_ pending: PendingImport, title: String, into project: ProjectID?) throws -> any DocumentSession {
        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
        let session = try library.importDocument(
            pending.source, title: name.isEmpty ? pending.suggestedTitle : name, into: project)
        refresh()
        return session
    }

    /// `name`, or "name 2", "name 3"… when a case has it already, as the
    /// core numbers a document's title: two projects of one name could not be
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
    func renameProject(_ id: ProjectID, to name: String) throws {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return }
        try library.renameProject(id, to: name)
        refresh()
    }

    /// Move to case. A new case is named as every new case is (#677):
    /// trimmed, never blank (nil, nothing moves), and numbered when a case
    /// has the name already.
    func move(_ id: DocumentID, to target: ProjectChoice) throws -> ProjectMove? {
        var target = target
        if case .newProject(let name) = target {
            let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !name.isEmpty else { return nil }
            target = .newProject(name: Self.unusedName(name, among: projects.map(\.name)))
        }
        defer { refresh() }
        return try library.moveDocument(id, to: target)
    }

    func undoMove(_ move: ProjectMove) throws -> ProjectMove {
        defer { refresh() }
        return try library.undoMove(move)
    }

    /// A new document left empty goes, with the case made for it
    /// (ADR 0109); true when it went.
    func discardIfEmpty(_ id: DocumentID) throws -> Bool {
        let gone = try library.discardIfEmpty(id)
        if gone { refresh() }
        return gone
    }

    /// Delete's first half: the documents leave the lists while their toast
    /// offers Undo.
    func hide(_ ids: [DocumentID]) {
        hidden.formUnion(ids)
    }

    /// Undo of Delete: the documents are back in place.
    func unhide(_ ids: [DocumentID]) {
        hidden.subtract(ids)
    }

    /// Deletes the documents through the core, with their replies.
    func delete(_ ids: [DocumentID]) throws {
        defer {
            hidden.subtract(ids)
            refresh()
        }
        if ids.count == 1 {
            try library.deleteDocument(ids[0])
        } else {
            try library.deleteDocuments(ids)
        }
    }

    /// Saves what finished while the phone was locked; returns its documents.
    func saveDeferred() throws -> [DocumentID] {
        do {
            let saved = try library.saveDeferred()
            // Every activation asks; most find nothing to save.
            if !saved.isEmpty { refresh() }
            return saved
        } catch {
            refresh()
            throw error
        }
    }

    // MARK: Status

    /// One line under a document's title. An interrupted import of a file
    /// that needs no recognition says so (ADR 0088); below the detector
    /// floor nothing waits for Find names.
    static func status(of document: LibraryDocument, findsNames: Bool = true) -> String {
        switch document.phase {
        case nil where !document.kind.needsRecognition: String(localized: "Import not finished")
        case nil: String(localized: "Text not recognized yet")
        case .unscanned where !findsNames: String(localized: "Ready to mark names")
        case .unscanned: String(localized: "Ready to find names")
        case .reviewing: String(localized: "\(document.pendingCount) to review")
        case .ready where document.candidateCount == 0: String(localized: "Nothing found")
        case .ready: String(localized: "Ready to share")
        }
    }

    /// The status's color (ADR 0099): to review orange, ready green, the
    /// rest neutral.
    static func statusKind(of document: LibraryDocument) -> StatusPill.Kind {
        switch document.phase {
        case .reviewing: .toReview
        case .ready where document.candidateCount > 0: .ready
        default: .neutral
        }
    }

    /// The status's symbol, beside its words and color.
    static func statusSymbol(of document: LibraryDocument, findsNames: Bool = true) -> String {
        switch document.phase {
        case nil where !document.kind.needsRecognition: "doc.badge.clock"
        case nil: "text.viewfinder"
        case .unscanned where !findsNames: "character.cursor.ibeam"
        case .unscanned: "text.magnifyingglass"
        case .reviewing: "exclamationmark.circle"
        case .ready: "checkmark.circle"
        }
    }
}
