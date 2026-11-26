import Foundation
import MdocFFI
import os

/// The app's `Core` over `crates/mdoc-ffi` (ADR 0085). The Rust objects own
/// documents, review, Projects and restore, and the Vault keeps them
/// (ADR 0125); this file only converts their records into the snapshots
/// the views use, and keeps an import's received files until its first
/// save.
enum RealCore {
    /// The library and setup for the app. `physicalMemory` decides which
    /// models are offered; below the floor a scan runs the structured rules.
    static func make(storage: MdocStorage, keys: any VaultKeyStore, physicalMemory: UInt64) throws -> Core {
        let setup = RealSetup(storage: storage, physicalMemory: physicalMemory)
        let library = try RealLibrary(storage: storage, keys: keys, setup: setup)
        return Core(library: library, setup: setup)
    }

    /// The Restore extension's store: the Vault's Projects and Unfiled
    /// documents through the core's `ReplyVault`, no models.
    static func replyStore(storage: MdocStorage, keys: any VaultKeyStore) throws -> any ReplyStore {
        try VaultReplyStore(storage: storage, keys: keys)
    }
}

// MARK: - Conversions

extension ProjectColour {
    /// The swatch nearest a Project's stored hue (ADR 0077).
    init(hue: UInt32) {
        let hues: [(ProjectColour, Double)] = [
            (.red, 0), (.orange, 30), (.amber, 45), (.green, 130), (.teal, 175), (.blue, 215), (.violet, 270), (.pink, 330),
        ]
        let value = Double(hue % 360)
        self = hues.min { lhs, rhs in
            let left = min(abs(lhs.1 - value), 360 - abs(lhs.1 - value))
            let right = min(abs(rhs.1 - value), 360 - abs(rhs.1 - value))
            return left < right
        }!.0
    }
}

extension RestoreResult {
    /// A restored reply as display segments, from the UTF-8 ranges the core
    /// reports.
    init(_ reply: RestoredReply) {
        let bytes = Array(reply.text.utf8)
        var marks: [(range: Range<Int>, restored: Bool)] = []
        marks += reply.restored.map { (Int($0.start)..<Int($0.end), true) }
        marks += reply.unresolved.map { (Int($0.start)..<Int($0.end), false) }
        marks.sort { $0.range.lowerBound < $1.range.lowerBound }
        var segments: [RestoredSegment] = []
        var cursor = 0
        for mark in marks where mark.range.lowerBound >= cursor && mark.range.upperBound <= bytes.count {
            if mark.range.lowerBound > cursor {
                segments.append(.text(String(decoding: bytes[cursor..<mark.range.lowerBound], as: UTF8.self)))
            }
            let piece = String(decoding: bytes[mark.range], as: UTF8.self)
            segments.append(mark.restored ? .restored(piece) : .unresolved(piece))
            cursor = mark.range.upperBound
        }
        if cursor < bytes.count {
            segments.append(.text(String(decoding: bytes[cursor...], as: UTF8.self)))
        }
        self.init(segments: segments, restoredCount: Int(reply.values), unresolvedCount: Int(reply.unrecognised))
    }
}

extension CoreError {
    /// The core's failure in the app's terms: one case for each of its
    /// variants, so each reads with its own message (ADR 0104).
    init(_ error: any Error) {
        self = switch CoreFailure(error) {
        case .importFailed(let message): .importFailed(message)
        // Pages without text: reading them needs text recognition.
        case .needsOcr: .modelMissing
        case .needsPassword: .needsPassword
        case .wrongPassword: .wrongPassword
        case .ocr(let message): .recognitionFailed(message)
        case .detect(let message): .findNamesFailed(message)
        case .cancelled: .cancelled
        case .review(let message): .reviewChanged(message)
        case .project(let message): .projectRefused(message)
        case .setup(let message): .setupFailed(message)
        case .invalid(let message): .invalid(message)
        case .io(let message): .fileFailed(message)
        case .pendingProposals(let count): .pendingReview(Int(count))
        case .aliasTaken(let alias, let identity): .aliasTaken(alias: alias, by: IdentityID(rawValue: String(identity)))
        case .nameNotFound: .nameNotFound
        case .refused(let message): .refused(message)
        case .storage(let message): .saveFailed(message)
        case .other(let message): .other(message)
        }
    }
}

/// Runs a core call and turns its error into the app's.
private func core<T>(_ call: () throws -> T) throws -> T {
    do {
        return try call()
    } catch let error as CoreError {
        throw error
    } catch {
        throw CoreError(error)
    }
}

extension ProjectSummary {
    init(_ info: ProjectInfo) {
        self.init(id: ProjectID(rawValue: info.id), name: info.name, colour: ProjectColour(hue: info.hue))
    }
}

extension SourceKind {
    /// The app's kind of a Vault document's file family.
    init(_ kind: DocumentSourceKind) {
        self = switch kind {
        case .pdf: .pdf
        case .word: .docx
        case .image: .image
        case .scan: .scan
        case .spreadsheet, .presentation, .ebook, .markdown, .text: .text
        }
    }
}

/// Reports the core's job stages, how far each is and a scan's time left
/// to a `JobObserver`, and answers the core's questions about the phone.
final class StageReporter: MdocFFI.JobProgress, @unchecked Sendable {
    let observer: any JobObserver
    private let device: DeviceConditions
    private let lock = NSLock()
    private var last = JobUpdate(stage: .reading)

    init(_ observer: any JobObserver, device: DeviceConditions = .live) {
        self.observer = observer
        self.device = device
    }

    func stage(stage: JobStage) {
        let update = lock.withLock {
            // Pausing to cool down and going on keep the bar's fraction and time.
            let paused = stage == .coolingDown || last.stage == .coolingDown
            last = paused
                ? JobUpdate(stage: stage, fraction: last.fraction, secondsLeft: last.secondsLeft)
                : JobUpdate(stage: stage)
            return last
        }
        observer.progress(update)
    }

    /// The fraction also reaches the background task, whose progress iOS
    /// watches: a continued processing task that shows none is stopped.
    func advanced(done: UInt64, total: UInt64) {
        guard total > 0 else { return }
        let update = lock.withLock {
            last.fraction = min(1, Double(done) / Double(total))
            if last.stage == .recognizing {
                // Pages: the bar reads "Recognizing page 3 of 12".
                last.page = Int(done)
                last.pageCount = Int(total)
            }
            return last
        }
        observer.progress(update)
    }

    func remaining(seconds: UInt64) {
        let update = lock.withLock {
            last.secondsLeft = seconds
            return last
        }
        observer.progress(update)
    }

    func thermal() -> Thermal { device.thermal() }

    func freeMemory() -> UInt64? { device.freeMemory() }
}

/// What a scan asks about the phone (ADR 0120); tests stand in their own.
struct DeviceConditions: Sendable {
    var thermal: @Sendable () -> Thermal
    var freeMemory: @Sendable () -> UInt64?

    static let live = DeviceConditions(
        thermal: {
            switch ProcessInfo.processInfo.thermalState {
            case .nominal: .nominal
            case .fair: .fair
            case .serious: .serious
            case .critical: .critical
            @unknown default: .serious
            }
        },
        freeMemory: {
            // 0 where the app has no memory limit, as in the Simulator.
            let free = os_proc_available_memory()
            return free > 0 ? UInt64(free) : nil
        })
}


// MARK: - Library

/// The app's documents and Projects in the Vault (ADR 0125): opened on the
/// storage's `vault` folder with the Keychain key, known by the Vault's ids
/// (#1091). An import the core has not read yet is the one thing kept
/// outside it: its received files and its record wait in `intake` until its
/// first save.
final class RealLibrary: Library, @unchecked Sendable {
    let vault: MdocFFI.Vault
    let storage: MdocStorage
    let setup: RealSetup?
    private let lock = NSLock()
    private var sessions: [DocumentID: RealSession] = [:]
    /// Vault id → the id this run gave its document: an import keeps the
    /// id it had before its first save.
    private var ids: [String: DocumentID] = [:]
    private let pending: PendingImports
    /// Saves that wait for the phone to unlock, by document.
    private var unsaved: [DocumentID: () throws -> Void] = [:]
    /// Project names by id, read once each: every snapshot shows its
    /// Project's name.
    private var projectNames: [String: String] = [:]
    /// Bumped by every Vault change this handle makes or takes in, so a
    /// saved reply shown restored is restored again from the names as
    /// saved.
    private var changes = 0
    /// Where the saves that follow taps run (ADR 0104).
    fileprivate let saveQueue = DispatchQueue(label: "io.github.lawkitt.mdoc.save", qos: .utility)
    /// How long taps pause before their save; the tests shorten it.
    var savePause = SaveScheduler.pause
    /// How long typing pauses before its save (ADR 0109); the tests shorten it.
    var typingPause = SaveScheduler.typingPause
    /// What scans ask about the phone; tests stand in low memory or heat.
    var device = DeviceConditions.live
    #if DEBUG
    /// The unit tests never download the name finder: at the detector tier
    /// an edit's scan then runs the rules in its place, as Try a sample's
    /// proposals stand in for Find names. A phone never does (`scanEdits`).
    var rulesStandInForNameFinder = false
    #endif
    /// Runs once Delete Project has begun listing the Project's documents
    /// and imports, before it has both: a test saves an import there, as
    /// the save queue can (#1140).
    var listingProject: (() -> Void)?
    private var saveObserver: (@MainActor @Sendable (SaveEvent) -> Void)?

    init(storage: MdocStorage, keys: any VaultKeyStore, setup: RealSetup?) throws {
        self.storage = storage
        self.setup = setup
        pending = PendingImports(storage: storage)
        vault = try core { try MdocFFI.Vault.open(root: storage.vault.path, key: try keys.key()) }
        // Viewing copies a stop left behind go at launch (#1091 Decision 24).
        storage.clearViewing()
        // Imports not yet read when mdoc last stopped come back, ready to
        // recognize again.
        for entry in pending.all() {
            let session = RealSession(
                id: entry.id, library: self, source: entry.source, title: entry.title, projectID: entry.projectID,
                ocrStarted: entry.ocrStarted)
            sessions[entry.id] = session
        }
    }

    /// The phone is below the name finder's floor (ADR 0088, ADR 0120): a
    /// Vault save with no proposals left then records the document Ready,
    /// its names decided by hand, and it stays unscanned (#1091 Q4, the
    /// maintainer's (b)). A phone at the detector tier is offered Find
    /// names, even before the name finder is installed, as today.
    var findsNoNames: Bool {
        guard let setup else { return true }
        return setup.snapshot().tier == .belowFloor
    }

    /// The saved documents from the Vault's manifest, which opens none of
    /// them (ADR 0104), then the imports not saved yet. A document open
    /// this run lists as its session has it, as it shows when opened: a
    /// save a locked phone held back has not landed yet.
    func snapshot() -> LibrarySnapshot {
        let projects = vault.projects().map(ProjectSummary.init)
        let stored = vault.documents()
        let rows = lock.withLock {
            stored.map { info in
                let id = ids[info.id] ?? DocumentID(rawValue: info.id)
                return (info, id, sessions[id])
            }
        }
        var documents = rows.map { info, id, session in
            let updated = Date(timeIntervalSince1970: TimeInterval(info.updated))
            return session?.loadedSummary(updated: updated) ?? LibraryDocument(info, id: id)
        }
        let listed = Set(documents.map(\.id))
        let pending = lock.withLock { sessions.values.filter { !listed.contains($0.id) } }
        documents += pending.map { $0.summary(updated: $0.created) }
        let replies = Dictionary(uniqueKeysWithValues: projects.map { project in
            (project.id, vault.projectReplies(projectId: project.id.rawValue).map(SavedReply.init))
        })
        return LibrarySnapshot(
            projects: projects, documents: documents.sorted { $0.updated > $1.updated }, projectReplies: replies)
    }

    /// Takes in what another handle wrote meanwhile, the Restore
    /// extension's replies above all (#1091 Decision 20): true when
    /// anything changed. mdoc comes to the foreground, a list is pulled, or
    /// a share sheet closes.
    @discardableResult
    func takeInChanges() -> Bool {
        let changed = (try? vault.refresh()) ?? false
        if changed { noteChange() }
        return changed
    }

    /// The Vault changed: what the names restore is read again.
    fileprivate func noteChange() {
        lock.withLock { changes += 1 }
    }

    /// How many Vault changes this handle has seen (`changes`).
    fileprivate var changeCount: Int { lock.withLock { changes } }

    /// Saved reply `id` restored with the people it named when saved, as
    /// they are now (#1091 Decision 18).
    func restoreSavedReply(_ id: ReplyID) throws -> RestoreResult {
        RestoreResult(try core { try vault.restoreReply(id: id.rawValue) })
    }

    func savedReply(_ id: ReplyID) -> SavedReply? {
        vault.replyInfo(id: id.rawValue).map(SavedReply.init)
    }

    /// Promote to Document: the reply restores with the names as the taps
    /// left them, so those are saved first.
    func promoteReply(_ id: ReplyID) throws -> DocumentID {
        saveChangesReporting()
        let promoted = try core { try vault.promoteReply(id: id.rawValue) }
        noteChange()
        return lock.withLock { ids[promoted.id] ?? DocumentID(rawValue: promoted.id) }
    }

    /// The saved replies that restore with a saved document's names: its
    /// Project's while it is in one, else its own (#1091 Decision 4).
    fileprivate func replies(vaultID: String, projectID: String?) -> [SavedReply] {
        let replies = if let projectID {
            vault.projectReplies(projectId: projectID)
        } else {
            vault.documentReplies(documentId: vaultID)
        }
        return replies.map(SavedReply.init)
    }

    /// Copy for AI or Share left from this saved document: the Restore
    /// extension restores with its Project, or with it while it is Unfiled,
    /// until the next one (#1091, the maintainer's (a)). Only the id is
    /// written. A file that can't be written leaves the extension its
    /// fallback, so it fails nothing.
    fileprivate func rememberShared(vaultID: String) {
        guard let info = try? vault.document(id: vaultID) else { return }
        let place: RestorePlace = if let project = info.projectId {
            .project(ProjectID(rawValue: project))
        } else {
            .document(DocumentID(rawValue: vaultID))
        }
        try? RestorePlaceFile(storage).write(place)
    }

    /// The documents this run has a session for: opened, or imported.
    var openSessionCount: Int { lock.withLock { sessions.count } }

    func session(for document: DocumentID) throws -> any DocumentSession {
        if let session = lock.withLock({ sessions[document] }) {
            return session
        }
        return try session(vaultID: document.rawValue)
    }

    /// The session of a saved document, opened from the Vault the first
    /// time.
    private func session(vaultID: String) throws -> RealSession {
        if let id = lock.withLock({ ids[vaultID] }), let session = lock.withLock({ sessions[id] }) {
            return session
        }
        let info: DocumentInfo
        do {
            info = try vault.document(id: vaultID)
        } catch {
            throw CoreError.notFound
        }
        // It opens in its Project as stored: what the other documents'
        // taps added there (a name marked and replaced) is saved first, so
        // it is proposed here too.
        saveChangesReporting()
        let session = RealSession(
            id: DocumentID(rawValue: vaultID), library: self, source: nil, title: info.title, projectID: nil)
        try session.reopen(vaultID: vaultID, kind: SourceKind(info.sourceKind))
        lock.withLock {
            sessions[session.id] = session
            ids[vaultID] = session.id
        }
        return session
    }

    func importDocument(_ source: ImportSource, title: String, into project: ProjectID?) throws -> any DocumentSession {
        // Its job joins the Project as stored: the taps' changes first.
        saveChangesReporting()
        let prepared = try IntakeFiles.prepare(source, title: title, storage: storage)
        let id = DocumentID(rawValue: "new:\(UUID().uuidString)")
        // Kept in the intake folder until its first save, so it survives
        // mdoc stopping.
        pending.add(PendingImports.Entry(id: id, title: title, projectID: project?.rawValue, source: prepared))
        let session = RealSession(id: id, library: self, source: prepared, title: title, projectID: project?.rawValue)
        lock.withLock { sessions[session.id] = session }
        return session
    }

    func deleteDocument(_ document: DocumentID) throws {
        guard let session = lock.withLock({ sessions[document] }) else {
            // A listed document not opened this run has no session: its id
            // is the Vault's.
            try core { try vault.deleteDocument(id: document.rawValue) }
            forget(document, vaultID: document.rawValue)
            return
        }
        // Deleted once no save or rename runs, and none starts until the
        // delete ends: once it succeeds, nothing writes the document again
        // (not its taps' save, its job's, or a save waiting for the phone
        // to unlock) and its job stops. A delete that fails leaves the
        // session as it was.
        let vaultID = try session.delete { [self] vaultID in
            if let vaultID {
                try core { try vault.deleteDocument(id: vaultID) }
            } else if let input = session.pendingInput {
                // An import not yet read: OCR's kept pages go first, found by
                // the source files' digests, then the files.
                let kept = storage.url(.ocr).path
                switch input {
                case .file(let url, _): try core { try discardKeptImport(kept: kept, source: url.path) }
                case .images(let urls): try core { try discardKeptImages(kept: kept, images: urls.map(\.path)) }
                case .text: break
                }
            }
        }
        forget(document, vaultID: vaultID)
    }

    /// A deleted or discarded document's session goes, with the save that
    /// waited for the phone to unlock: together, so no failed save queues
    /// one again after (`saveOnUnlock`). An import's intake files go too.
    private func forget(_ document: DocumentID, vaultID: String?) {
        lock.withLock {
            _ = unsaved.removeValue(forKey: document)
            _ = sessions.removeValue(forKey: document)
            if let vaultID { _ = ids.removeValue(forKey: vaultID) }
        }
        pending.remove(document)
    }

    func discardIfEmpty(_ document: DocumentID) throws -> Bool {
        guard let session = lock.withLock({ sessions[document] }),
              let discarded = try session.discardIfEmpty() else { return false }
        // Gone in the Vault: its saves have nothing left to write.
        forget(document, vaultID: discarded)
        return true
    }

    func renameDocument(_ document: DocumentID, to title: String) throws {
        guard let session = try session(for: document) as? RealSession else { throw CoreError.notFound }
        try session.rename(to: title)
    }

    func renameProject(_ projectID: ProjectID, to name: String) throws {
        _ = try core { try vault.renameProject(id: projectID.rawValue, name: name) }
        lock.withLock { _ = projectNames.removeValue(forKey: projectID.rawValue) }
    }

    func moveDocument(_ document: DocumentID, to target: ProjectChoice) throws -> ProjectMove {
        try move(document) { id in
            switch target {
            case .existing(let project): try vault.moveDocument(id: id, projectId: project.rawValue)
            case .newProject(let name): try vault.moveDocumentToNewProject(id: id, name: name)
            }
        }
    }

    func undoMove(_ move: ProjectMove) throws -> ProjectMove {
        try self.move(move.document) { _ in try vault.undoMove(undo: move.undo) }
    }

    /// A saved document's move through the Vault. The move has happened
    /// once the core returns, so the result stands even if the session
    /// cannot open the changed document at once: it opens it on its next
    /// use.
    private func move(_ document: DocumentID, _ call: (String) throws -> DocumentMove) throws -> ProjectMove {
        guard let session = try session(for: document) as? RealSession else { throw CoreError.notFound }
        guard let vaultID = session.vaultID else {
            throw CoreError.refused(String(localized: "Move the document once it is read."))
        }
        // The move rewrites the document and the session opens it again:
        // what the taps changed is saved first.
        try session.saveChanges()
        let moved = try core { try call(vaultID) }
        lock.withLock { projectNames.removeAll() }
        noteChange()
        // Its text and Project changed in the Vault: the session opens it
        // again.
        do {
            try session.reopen(vaultID: vaultID, kind: nil)
        } catch {
            session.reopenLater()
        }
        return ProjectMove(
            document: document, to: moved.project.map(ProjectSummary.init),
            from: moved.left.map { ProjectID(rawValue: $0.id) }, undo: moved.undo)
    }

    func createProject(named name: String) throws -> ProjectSummary {
        ProjectSummary(try core { try vault.createProject(name: name) })
    }

    func projectDetails(_ projectID: ProjectID) throws -> ProjectDetails {
        let page = try core { try vault.projectPage(id: projectID.rawValue) }
        return ProjectDetails(
            names: page.names.map { ProjectNameRow(alias: $0.alias, category: $0.category, originals: $0.originals) },
            always: page.always.map { AlwaysWording(wording: $0.wording, category: $0.category) },
            never: page.never)
    }

    func removeListed(_ wording: String, always: Bool, from projectID: ProjectID) throws {
        _ = try core { try vault.removeListed(projectId: projectID.rawValue, wording: wording, always: always) }
    }

    func documentsFromSource(_ file: URL) -> [DocumentID] {
        let found = (try? vault.documentsFromSource(path: file.path)) ?? []
        return lock.withLock { found.map { ids[$0.id] ?? DocumentID(rawValue: $0.id) } }
    }

    /// Delete Project. Its documents' sessions follow as a move's and a
    /// delete's do: a moved one saves what its taps changed first and opens
    /// again in Unfiled; a deleted one goes through `deleteDocument`, so
    /// nothing saves it again. An import not read yet that was to join the
    /// Project lands in Unfiled, or is deleted with the rest.
    func deleteProject(_ projectID: ProjectID, documents: ProjectDeletion) throws {
        let id = projectID.rawValue
        // Even a delete that fails may have moved or deleted some.
        defer {
            lock.withLock { projectNames.removeAll() }
            noteChange()
        }
        let (members, imports) = contents(of: id)
        switch documents {
        case .deleteDocuments:
            for document in imports.map(\.id) + members.map(\.1) {
                try deleteDocument(document)
            }
            try core { try vault.deleteProject(id: id, documents: .delete) }
        case .moveToUnfiled:
            var open = lock.withLock { members.compactMap { vaultID, document in sessions[document].map { (vaultID, $0) } } }
            for (_, session) in open {
                try session.saveChanges()
            }
            do {
                // No import's join and first save runs meanwhile: each lands
                // in Unfiled, or saved in the Project since it was listed,
                // moves with the rest.
                try holdingSaves(imports[...]) {
                    let leaving = imports.filter { $0.leaveProject(id) }
                    open += imports.filter { session in !leaving.contains { $0 === session } }
                        .compactMap { session in session.vaultID.map { ($0, session) } }
                    do {
                        // Their records land in Unfiled first: a stop before
                        // the Project goes leaves none naming it once gone.
                        try pending.place(leaving.map(\.id), in: nil)
                        try core { try vault.deleteProject(id: id, documents: .moveToUnfiled) }
                    } catch {
                        leaving.forEach { $0.rejoin(id) }
                        try? pending.place(leaving.map(\.id), in: id)
                        throw error
                    }
                }
            } catch {
                // A delete that failed part way moved some to Unfiled: they
                // show it before the error does.
                let staying = Set(vault.documents().filter { $0.projectId == id }.map(\.id))
                reopen(open.filter { !staying.contains($0.0) })
                throw error
            }
            reopen(open)
        }
    }

    /// A Project's saved documents, by Vault id with this run's id, and the
    /// imports not saved yet that are to join it, listed while none of
    /// those imports saves: a first save lands it in one list, never
    /// between the two.
    private func contents(of id: String) -> (members: [(String, DocumentID)], imports: [RealSession]) {
        let joining = lock.withLock { sessions.values.filter { $0.joins(id) } }
        listingProject?()
        return holdingSaves(joining[...]) {
            let stored = vault.documents().filter { $0.projectId == id }.map(\.id)
            return lock.withLock {
                (
                    stored.map { vaultID in (vaultID, ids[vaultID] ?? DocumentID(rawValue: vaultID)) },
                    sessions.values.filter { $0.joins(id) }
                )
            }
        }
    }

    /// Moved documents' sessions, by Vault id, open them again; one that
    /// fails opens it on its next use.
    private func reopen(_ moved: [(String, RealSession)]) {
        for (vaultID, session) in moved {
            do {
                try session.reopen(vaultID: vaultID, kind: nil)
            } catch {
                session.reopenLater()
            }
        }
    }

    /// Runs `body` while none of `sessions` saves.
    private func holdingSaves<T>(_ sessions: ArraySlice<RealSession>, _ body: () throws -> T) rethrows -> T {
        guard let first = sessions.first else { return try body() }
        return try first.holdingSave { try holdingSaves(sessions.dropFirst(), body) }
    }

    func deleteDocuments(_ documents: [DocumentID]) throws {
        // Those this run holds a session for go one by one, as a single
        // Delete does; the rest in one call.
        let (held, rest) = lock.withLock {
            (documents.filter { sessions[$0] != nil }, documents.filter { sessions[$0] == nil })
        }
        for document in held {
            try deleteDocument(document)
        }
        guard !rest.isEmpty else { return }
        try core { try vault.deleteDocuments(ids: rest.map(\.rawValue)) }
        for document in rest {
            forget(document, vaultID: document.rawValue)
        }
    }

    /// A save that waits for the phone to unlock. A job's save replaces
    /// what waited for the document before it (it saves the taps' changes
    /// too); the taps' save never replaces a job's, which may join the
    /// Project first.
    fileprivate func saveOnUnlock(_ id: DocumentID, replacing: Bool = true, _ save: @escaping () throws -> Void) {
        lock.withLock {
            // A document deleted or discarded meanwhile has nothing to save.
            guard sessions[id] != nil else { return }
            if replacing || unsaved[id] == nil {
                unsaved[id] = save
            }
        }
    }

    /// Every document's taps saved now; the first failure is thrown once
    /// all were tried.
    func saveChanges() throws {
        let open = lock.withLock { Array(sessions.values) }
        var failure: (any Error)?
        for session in open {
            do {
                try session.saveChanges()
            } catch {
                failure = failure ?? error
            }
        }
        if let failure {
            throw failure
        }
    }

    /// `saveChanges`, each failure told to the app as a failed save that
    /// followed taps: the call that needed the saves goes on.
    private func saveChangesReporting() {
        for session in lock.withLock({ Array(sessions.values) }) {
            do {
                try session.saveChanges()
            } catch {
                report(.failed(session.id, CoreError(error)))
            }
        }
    }

    func observeSaves(_ observer: @escaping @MainActor @Sendable (SaveEvent) -> Void) {
        lock.withLock { saveObserver = observer }
    }

    /// Tells the app how a save that followed taps went.
    fileprivate func report(_ event: SaveEvent) {
        guard let observer = lock.withLock({ saveObserver }) else { return }
        DispatchQueue.main.async {
            MainActor.assumeIsolated { observer(event) }
        }
    }

    /// The save that waits for `id` to unlock, run now if there is one;
    /// kept if it fails.
    fileprivate func saveDeferred(_ id: DocumentID) throws {
        guard let save = lock.withLock({ unsaved.removeValue(forKey: id) }) else { return }
        do {
            try save()
        } catch {
            lock.withLock { unsaved[id] = unsaved[id] ?? save }
            throw error
        }
    }

    func saveDeferred() throws -> [DocumentID] {
        let waiting = lock.withLock {
            let waiting = unsaved
            unsaved = [:]
            return waiting
        }
        var saved: [DocumentID] = []
        var failure: (any Error)?
        for (id, save) in waiting {
            do {
                try save()
                saved.append(id)
            } catch {
                // Kept until it saves: the result exists nowhere else. A
                // failure with the phone unlocked (a full disk) is reported
                // too, and tried again next time.
                lock.withLock { unsaved[id] = unsaved[id] ?? save }
                if ProtectedData.isAvailable {
                    failure = failure ?? error
                }
            }
        }
        if let failure {
            throw failure
        }
        return saved
    }

    fileprivate func ocrStarted(_ session: RealSession) {
        pending.markOCRStarted(session.id)
    }

    /// A saved document keeps its id for the rest of the run. An import is
    /// known by it before it stops joining its Project, so a Project's
    /// delete lists it as one of the Project's documents or as an import,
    /// never neither (#1140).
    fileprivate func keepID(_ session: RealSession, vaultID: String) {
        lock.withLock { ids[vaultID] = session.id }
    }

    /// An import's first save ends it, and its received files go: the
    /// Vault keeps what it keeps of them.
    fileprivate func importSaved(_ session: RealSession) {
        pending.remove(session.id)
    }

    /// What the Original pane shows of a Vault document's original: its
    /// kind from the name it was imported with, decrypted when the pane
    /// first reads it, into memory when 16 MiB or less, else into a folder
    /// of its own in the viewing cache (#1091 Decision 24). `.none` for a
    /// document that keeps no original: pasted, begun in mdoc, or pages
    /// picked together.
    fileprivate func storedOriginal(_ vaultID: String) -> OriginalSource {
        guard let file = try? vault.original(id: vaultID) else { return .none }
        let vault = vault
        let folder = storage.viewing.appendingPathComponent(vaultID, isDirectory: true)
        let inMemory = originalsInMemory
        let stored = StoredOriginal(
            decrypt: {
                if file.length <= inMemory {
                    return .memory(OriginalData(try core { try vault.originalBytes(id: vaultID) }))
                }
                try? FileManager.default.removeItem(at: folder)
                try FileManager.default.createDirectory(
                    at: folder, withIntermediateDirectories: true,
                    attributes: [.protectionKey: FileProtectionType.complete])
                let path = try core { try vault.copyOriginal(id: vaultID, folder: folder.path) }
                var url = URL(fileURLWithPath: path)
                try FileManager.default.setAttributes([.protectionKey: FileProtectionType.complete], ofItemAtPath: path)
                var values = URLResourceValues()
                values.isExcludedFromBackup = true
                try url.setResourceValues(values)
                return .file(url)
            },
            remove: { try? FileManager.default.removeItem(at: folder) })
        switch URL(fileURLWithPath: file.fileName).pathExtension.lowercased() {
        case "pdf": return .pdf(.stored(stored))
        case "docx": return .docx(.stored(stored))
        case "jpg", "jpeg", "png", "heic", "heif", "webp", "gif": return .images([.stored(stored)])
        default: return .none
        }
    }

    /// The largest original viewed from memory, about 16 MiB (#1091
    /// Decision 24); the tests lower it.
    var originalsInMemory: UInt64 = 16 * 1024 * 1024

    fileprivate func projectName(_ id: String?) -> String? {
        guard let id else { return nil }
        if let name = lock.withLock({ projectNames[id] }) { return name }
        // Not kept when it cannot be read now (the phone locked).
        guard let name = try? vault.project(id: id).name else { return nil }
        lock.withLock { projectNames[id] = name }
        return name
    }
}

// MARK: - Session

/// One review tap, as Instruments shows it (refactor grill Q12): the core
/// call and the snapshot the app makes of its result.
private let reviewSignposter = OSSignposter(subsystem: "io.github.lawkitt.mdoc", category: "Review")

/// One typed edit's core call in edit mode (ADR 0109), beside the app's
/// "Restyle" and "Structural" intervals.
private let typingSignposter = OSSignposter(subsystem: "io.github.lawkitt.mdoc", category: "Editing")

/// One document over the core's `DocumentSession`. What the document is
/// (its title, Project, review and whether it was scanned) is the core's,
/// read with `info()` and `snapshot()` each time (#681, ADR 0104), and the
/// Vault keeps it. This side keeps only what the core has no record of: an
/// import's source before it is read, with the title and Project chosen for
/// it, the document's Vault id once saved, and which saved reply its Reply
/// pane shows.
final class RealSession: DocumentSession, @unchecked Sendable {
    let id: DocumentID
    let created = Date()
    private let ffi = MdocFFI.DocumentSession()
    // Strong: a session keeps its library open while it is used.
    private let library: RealLibrary
    private let lock = NSLock()
    /// What an import reads; nil once it is read, or for a saved document.
    private var source: PreparedSource?
    /// The title and Project chosen at import, until the core holds the
    /// document: it reads the title from the file and joins the Project
    /// only once it is read. Nil: Unfiled.
    private var chosen: (title: String, projectID: String?) {
        lock.withLock { chosenPlace }
    }
    private var chosenPlace: (title: String, projectID: String?)
    private var kind: SourceKind = .text
    private var original: OriginalSource = .none
    /// Its id in the Vault, once its first save made it or it was opened
    /// from there.
    private var savedID: String?
    /// OCR began on this import and has not finished: the core keeps its
    /// finished pages, so recognition resumes with the rest.
    private var ocrStarted: Bool
    /// A move changed the saved document and opening it again failed: the
    /// next use opens it.
    private var reopenPending = false
    /// The Markdown styling of the text with this revision: a snapshot whose
    /// `text_revision` is the same reuses it instead of parsing again.
    private var styled: (revision: UInt64, styling: MarkdownStyling)?
    /// The save that follows taps (ADR 0104).
    private let saver: SaveScheduler
    /// Held while a save records the document, and while a rename or a
    /// delete runs.
    private let saving = NSLock()
    /// Deleted or discarded (`end`): nothing saves it again.
    private var ended = false
    /// The core's id of the long job running now, which `end` stops.
    private var runningJob: UInt64?
    /// The text Copy for AI last asked "I checked the names myself" of:
    /// the answer is about that text only (item 48).
    private var namesAskedOf: String?
    /// The saved reply the Reply pane shows, picked or pasted this
    /// session; nil shows the newest.
    private var shownReply: ReplyID?
    /// The shown reply as restored, while it and the Vault stay.
    private var restoredReply: (key: RestoredReplyKey, reply: ReplySnapshot)?

    private struct RestoredReplyKey: Equatable {
        let id: ReplyID
        let vaultChanges: Int
    }

    fileprivate init(
        id: DocumentID, library: RealLibrary, source: PreparedSource?, title: String, projectID: String?,
        ocrStarted: Bool = false
    ) {
        self.id = id
        self.library = library
        self.source = source
        chosenPlace = (title, projectID)
        self.ocrStarted = ocrStarted
        saver = SaveScheduler(queue: library.saveQueue)
        if let source {
            kind = source.kind
            original = source.original
        }
    }

    /// The core's title, Project and phase; nil before the import is read.
    var info: MdocFFI.SessionInfo? { ffi.info() }

    /// Its Vault id, once saved.
    var vaultID: String? { lock.withLock { savedID } }

    /// Opens a saved document from the Vault: its review as saved, its
    /// names lined up with its Project's as they are now. `kind` nil keeps
    /// the one it has.
    fileprivate func reopen(vaultID: String, kind: SourceKind?) throws {
        _ = try core { try ffi.openFromVault(vault: library.vault, id: vaultID) }
        let stored = library.storedOriginal(vaultID)
        lock.withLock {
            savedID = vaultID
            // The core holds its Project now; the one chosen at import
            // would show it in that Project after a move to Unfiled.
            chosenPlace.projectID = nil
            if let kind { self.kind = kind }
            original.close()
            original = stored
            reopenPending = false
        }
    }

    /// The document closed: its decrypted original goes (#1091 Decision 24).
    func closeOriginal() {
        lock.withLock { original }.close()
    }

    /// Rename: the core renames the document, in the Vault too once saved,
    /// keeping its review and history.
    fileprivate func rename(to title: String) throws {
        guard ffi.info() != nil else {
            throw CoreError.refused(String(localized: "Rename the document once it is read."))
        }
        _ = try saving.withLock { try core { try ffi.renameInVault(vault: library.vault, title: title) } }
    }

    /// `RealLibrary.discardIfEmpty`: the core removes the document, with no
    /// save or rename running; nil when it was not empty. Ended (`end`)
    /// once discarded, with the Vault id it had.
    fileprivate func discardIfEmpty() throws -> String?? {
        try end { [self] _ in try core { try ffi.discardIfEmptyInVault(vault: library.vault) } }
    }

    /// `RealLibrary.deleteDocument`: `remove` deletes the document by its
    /// Vault id (nil before its first save), read once no save or rename
    /// runs; none starts until it returns. Ended (`end`) once it succeeds;
    /// returns the Vault id.
    fileprivate func delete(_ remove: (String?) throws -> Void) throws -> String? {
        try end { vaultID in
            try remove(vaultID)
            return true
        } ?? nil
    }

    /// Runs `remove` on the document's Vault id under the save lock; once
    /// it returns true, the session ends: its scheduled save does not run,
    /// no later save writes it, and its running job stops. Nil when
    /// `remove` returned false, else the Vault id.
    private func end(_ remove: (String?) throws -> Bool) throws -> String?? {
        let outcome: (vaultID: String?, job: UInt64?)? = try saving.withLock {
            let vaultID = lock.withLock { savedID }
            guard try remove(vaultID) else { return nil }
            return lock.withLock {
                ended = true
                return (vaultID, runningJob)
            }
        }
        guard let outcome else { return nil }
        saver.cancel()
        if let job = outcome.job { ffi.cancelJob(job: job) }
        return .some(outcome.vaultID)
    }

    func followFirstLine() throws -> String? {
        try saving.withLock { try core { try ffi.followFirstLineInVault(vault: library.vault) } }
    }

    /// What an import not yet read reads.
    fileprivate var pendingInput: PreparedSource.Input? {
        lock.withLock { source?.input }
    }

    /// The list row of an import not saved yet.
    fileprivate func summary(updated: Date) -> LibraryDocument {
        let snapshot = snapshot()
        return LibraryDocument(
            id: id, title: snapshot.title, projectID: snapshot.projectID, kind: snapshot.kind,
            phase: snapshot.phase, pendingCount: snapshot.pendingCount, updated: updated,
            candidateCount: snapshot.candidates.count, origin: snapshot.origin)
    }

    /// The list row of a document this session holds, from the core's
    /// review without the reply or the styling; nil before it is read.
    fileprivate func loadedSummary(updated: Date) -> LibraryDocument? {
        guard let review = try? ffi.snapshot() else { return nil }
        let projectID = review.projectId ?? chosen.projectID
        return LibraryDocument(
            id: id, title: review.title, projectID: projectID.map(ProjectID.init), kind: lock.withLock { kind },
            phase: review.phase, pendingCount: Int(review.proposed), updated: updated,
            candidateCount: review.mentions.count, origin: ffi.info()?.origin ?? .ordinary)
    }

    /// The Project an import not yet read was to join is deleted, its
    /// documents moved to Unfiled: the import lands there too. True when it
    /// was to join `projectID`.
    fileprivate func leaveProject(_ projectID: String) -> Bool {
        lock.withLock {
            guard savedID == nil, chosenPlace.projectID == projectID else { return false }
            chosenPlace.projectID = nil
            return true
        }
    }

    /// Undoes `leaveProject` when the Project's delete failed.
    fileprivate func rejoin(_ projectID: String) {
        lock.withLock {
            if savedID == nil { chosenPlace.projectID = projectID }
        }
    }

    /// Whether this import, not yet saved, was to join `projectID`.
    fileprivate func joins(_ projectID: String) -> Bool {
        lock.withLock { savedID == nil && chosenPlace.projectID == projectID }
    }

    fileprivate func reopenLater() {
        lock.withLock { reopenPending = true }
    }

    /// Opens the document again when a move left that for later; a failure
    /// leaves it pending for the next try.
    private func reopenIfPending() {
        guard let vaultID = lock.withLock({ reopenPending ? savedID : nil }) else { return }
        try? reopen(vaultID: vaultID, kind: nil)
    }

    func snapshot() -> DocumentSnapshot {
        reopenIfPending()
        guard let review = try? ffi.snapshot() else {
            let (kind, original, ocrStarted, source) = lock.withLock { (self.kind, self.original, self.ocrStarted, self.source) }
            // Before it is read, its origin is the import's.
            let origin: DocumentOrigin = if case .text(_, let origin) = source?.input { origin } else { .ordinary }
            return DocumentSnapshot(
                id: id, title: chosen.title, projectID: chosen.projectID.map(ProjectID.init),
                projectName: library.projectName(chosen.projectID),
                kind: kind, phase: nil, markdown: "", styling: .empty, candidates: [], identities: [],
                original: original, reply: nil, resumesOCR: ocrStarted, origin: origin)
        }
        return snapshot(of: review)
    }

    /// The app's snapshot of the core's review. It derives nothing: states,
    /// counts, the kept names, the phase and the ids are the core's.
    private func snapshot(of review: ReviewSnapshot) -> DocumentSnapshot {
        // Before it joins its Project the core has none: the one chosen.
        let projectID = review.projectId ?? chosen.projectID
        let (kind, original) = lock.withLock { (self.kind, self.original) }
        let candidates = review.mentions.map { mention in
            Candidate(
                id: CandidateID(rawValue: String(mention.id)),
                identityID: mention.identity.map(IdentityID.init) ?? IdentityID(rawValue: "mention-\(mention.id)"),
                range: SourceRange(mention.range), original: mention.original, alias: mention.alias ?? "",
                category: mention.category, state: mention.state)
        }
        // Names kept everywhere follow the active identities, under Kept
        // (ADR 0098's Show filter).
        let identities = (review.identities + review.keptIdentities).map { row in
            Identity(
                id: IdentityID(row.id), alias: row.alias, category: row.category,
                primaryWording: row.originals.first ?? row.alias, variants: row.originals,
                mentionCount: Int(row.mentionCount), state: row.state, owner: row.owner.map(IdentityID.init))
        }
        let replies = savedReplies(projectID: projectID)
        let reply = reply(of: replies)
        return DocumentSnapshot(
            id: id, title: review.title, projectID: projectID.map(ProjectID.init),
            projectName: library.projectName(projectID), kind: kind, phase: review.phase,
            markdown: review.text, styling: styling(review), textRevision: review.textRevision,
            candidates: candidates, identities: identities, original: original,
            reply: reply, replies: replies, notice: review.notice, leftOut: review.leftOut,
            mergeHints: review.linkHints, canUndo: review.canUndo, canRedo: review.canRedo,
            undoStep: review.undoStep, redoStep: review.redoStep,
            counts: MentionCounts(proposed: Int(review.proposed), replaced: Int(review.replaced), kept: Int(review.kept)),
            origin: ffi.info()?.origin ?? .ordinary, promotedFrom: promotedFrom())
    }

    /// The saved reply this document was promoted from, from the Vault's
    /// manifest: deleted once it is not listed there.
    private func promotedFrom() -> PromotedFrom? {
        guard let reply = ffi.fromReply() else { return nil }
        return library.savedReply(ReplyID(rawValue: reply)).map(PromotedFrom.reply) ?? .deletedReply
    }

    /// The saved replies that restore with this document's names, from
    /// the Vault's manifest: none before its first save.
    private func savedReplies(projectID: String?) -> [SavedReply] {
        guard let vaultID else { return [] }
        return library.replies(vaultID: vaultID, projectID: projectID)
    }

    /// The reply the pane shows, the one picked or else the newest,
    /// restored from the Vault with the people it named when saved, as they
    /// are now (#1091 Decision 18); restored again only once another is
    /// shown or the Vault changed: a tap's change shows once it is saved.
    private func reply(of replies: [SavedReply]) -> ReplySnapshot? {
        let picked = lock.withLock { shownReply }
        guard let shown = replies.first(where: { $0.id == picked }) ?? replies.first else { return nil }
        // Read before the restore: a save meanwhile restores it again.
        let restoredKey = RestoredReplyKey(id: shown.id, vaultChanges: library.changeCount)
        if let restored = lock.withLock({ restoredReply }), restored.key == restoredKey {
            return restored.reply
        }
        let vault = library.vault
        // One that can't be read now (a locked phone) is read again next
        // time, not kept as none.
        guard let reply = try? ReplySnapshot(
            id: shown.id, aliasText: vault.reply(id: shown.id.rawValue).markdown,
            restored: RestoreResult(vault.restoreReply(id: shown.id.rawValue)))
        else { return nil }
        lock.withLock { restoredReply = (restoredKey, reply) }
        return reply
    }

    /// The text's Markdown styling, parsed again only when the text changed.
    private func styling(_ review: ReviewSnapshot) -> MarkdownStyling {
        if let styled = lock.withLock({ styled }), styled.revision == review.textRevision {
            return styled.styling
        }
        let styling = MarkdownStyling(markdown: review.text)
        lock.withLock { styled = (review.textRevision, styling) }
        return styling
    }

    /// A long core call, cancellable by its job id: the id comes from the
    /// core just before the call, so a cancel before the job starts stops
    /// it as it starts, and one after it ended never stops the next (#680).
    private func job<T>(_ observer: any JobObserver, _ call: () throws -> T) throws -> T {
        let job = ffi.nextJob()
        observer.onCancel { [ffi] in ffi.cancelJob(job: job) }
        // A document deleted before or while its job runs: the job stops,
        // and what it finished is not saved.
        let ended = lock.withLock {
            runningJob = job
            return self.ended
        }
        defer { lock.withLock { runningJob = nil } }
        if ended { throw CoreError.cancelled }
        let value = try core(call)
        if lock.withLock({ self.ended }) { throw CoreError.cancelled }
        return value
    }

    func takeUnlockedDocx() -> UnlockedDocx? { ffi.takeUnlockedDocx() }

    func recognize(observer: any JobObserver, password: String?) throws -> DocumentSnapshot {
        guard let source = lock.withLock({ self.source }) else { return snapshot() }
        let reporter = StageReporter(observer)
        observer.progress(JobUpdate(stage: .reading))
        switch source.input {
        case .text(let text, let origin):
            _ = try job(observer) { try ffi.importTextAs(text: text, title: chosen.title, origin: origin) }
        case .file(let url, let needsOcr):
            let ocr = library.setup?.ocrPaths()
            if needsOcr && ocr == nil {
                throw CoreError.modelMissing
            }
            if ocr != nil && (needsOcr || source.kind == .pdf) && !lock.withLock({ ocrStarted }) {
                lock.withLock { ocrStarted = true }
                library.ocrStarted(self)
            }
            // OCR keeps its finished pages in the intake folder (ADR 0085
            // Long jobs): after a cancel, or mdoc stopping, this resumes
            // with the pages left.
            _ = try job(observer) {
                try ffi.importFileResuming(
                    kept: library.storage.url(.ocr).path, path: url.path,
                    options: ImportOptions(ocr: ocr, nativeTextOnly: false, password: password), progress: reporter)
            }
        case .images(let urls):
            guard let ocr = library.setup?.ocrPaths() else { throw CoreError.modelMissing }
            if !lock.withLock({ ocrStarted }) {
                lock.withLock { ocrStarted = true }
                library.ocrStarted(self)
            }
            // One document, a page each, built in memory by the core; its
            // kept pages resume as a file's do.
            _ = try job(observer) {
                try ffi.importImagesResuming(
                    kept: library.storage.url(.ocr).path, images: urls.map(\.path), title: chosen.title,
                    options: ImportOptions(ocr: ocr, nativeTextOnly: false, password: nil), progress: reporter)
            }
        }
        lock.withLock {
            self.source = nil
            ocrStarted = false
        }
        // Camera pages are a scan; the core takes every image as a photo.
        if kind == .scan {
            try core { try ffi.setSourceKind(kind: .scan) }
        }
        try saveAfterJob { [self] in
            // Joining and the first save are one step under the save lock:
            // a Project deleted meanwhile (`leaveProject`) is deleted
            // either before it, which leaves the import Unfiled, or after.
            try saving.withLock {
                // Deleted meanwhile: nothing joins the Project.
                guard !lock.withLock({ ended }) else { return }
                // Started from a Project: it joins it before its first save.
                if let projectID = chosen.projectID, vaultID == nil {
                    _ = try core { try ffi.joinProject(vault: library.vault, projectId: projectID) }
                }
                try saveHeld()
            }
        }
        return snapshot()
    }

    func scanEstimate() -> Int? {
        guard let paths = library.setup?.detectorPaths() else { return nil }
        return ffi.scanEstimate(detector: paths).map(Int.init)
    }

    func scan(observer: any JobObserver) throws -> DocumentSnapshot {
        guard let setup = library.setup else { throw CoreError.modelMissing }
        var state = setup.snapshot()
        if state.tier == .detector, state.tenPagesSeconds == nil, setup.detectorPaths() != nil {
            // A detector installed before mdoc timed it is timed first, in
            // this job, so the scan has its estimate (ADR 0120).
            observer.progress(JobUpdate(stage: .scanning))
            state = try setup.timeDetector(observer: observer)
        }
        let detector: DetectorPaths?
        switch state.tier {
        case .detector:
            guard let paths = setup.detectorPaths() else { throw CoreError.modelMissing }
            detector = paths
        case .belowFloor:
            // Below the floor the app offers no Find names (ADR 0088). Only
            // the UI tests' seed (`seedForUITesting`), on a stand-in memory
            // size, gets here: the core's structured rules give it candidates
            // without a model. A phone too slow for the detector never scans
            // with the rules alone, which would miss names (ADR 0120).
            if state.belowFloor == .speed {
                throw CoreError.findNamesFailed(
                    String(localized: "The name finder is too slow on this phone. Select text to mark a name."))
            }
            #if DEBUG
            detector = nil
            #else
            // The seed runs only in Debug builds, as the UI tests do.
            throw CoreError.modelMissing
            #endif
        }
        _ = try job(observer) { try ffi.scan(detector: detector, progress: StageReporter(observer, device: library.device)) }
        try saveAfterJob { [self] in try save() }
        return snapshot()
    }

    /// The save that ends a job. A job left running when the phone locks
    /// finishes with the library's files unwritable (Data Protection
    /// Complete), so its result stays in this session and the library saves
    /// it once the phone unlocks.
    private func saveAfterJob(_ finish: @escaping () throws -> Void) throws {
        do {
            try finish()
        } catch {
            guard !ProtectedData.isAvailable else { throw error }
            library.saveOnUnlock(id, finish)
            // The phone may have unlocked since the check, its save already
            // run with nothing queued: then this one saves now.
            if ProtectedData.isAvailable {
                _ = try? library.saveDeferred()
            }
        }
    }

    func decide(_ candidate: CandidateID, _ decision: Decision, scope: Scope) throws -> DocumentSnapshot {
        let mention = try Self.mention(candidate)
        return try review(decision == .replace
            ? .replace(mention: mention, scope: scope)
            : .keep(mention: mention, scope: scope))
    }

    func notAName(_ candidate: CandidateID, scope: Scope) throws -> DocumentSnapshot {
        try review(.notAName(mention: try Self.mention(candidate), scope: scope))
    }

    func replaceAllPending() throws -> DocumentSnapshot {
        try review(.replaceAll)
    }

    /// Copy for AI and Share (#1091 Decision 19): saved first, then the
    /// session refuses while proposals are pending, the Vault gives the
    /// file a neutral name, nothing of the title (#1205),
    /// and the text comes from the Vault, which checks the document is Ready
    /// against its Project's names as they are now. The Vault also notes
    /// the share, so a move's Undo keeps the names a reply may use.
    /// A refusal is thrown as `CoreError.handOffRefused`, in the app's own
    /// words: never the core's (#1209).
    func copyForAI() throws -> HandOff {
        // A tap right after Done can come before Done's work has run off
        // the main thread: run it now, as Done does, so the hand-off sees
        // the state Done leaves. The edit scan runs discovery first, then
        // scans the paragraphs edited: a new name is proposed, and the save
        // records the edited text as reviewed only once it was scanned
        // (#1209 decision 15, item 44). It cancels Done's scan if that runs.
        _ = try? scanEdits()
        do {
            try saveChanges()
        } catch {
            // The save's own words name the Vault's records: not on screen.
            throw CoreError.handOffRefused(.failed)
        }
        let handOff: MdocFFI.HandOff
        do {
            handOff = try core { try ffi.handOff(vault: library.vault) }
        } catch CoreError.pendingReview(let count) {
            let names = snapshot().identities.filter { $0.state == .proposed }.count
            throw CoreError.handOffRefused(.namesToDecide(names: names, proposals: count))
        } catch {
            // Not saved into the Vault yet, or its file name unreadable.
            throw CoreError.handOffRefused(vaultID == nil ? .notRead : .failed)
        }
        guard let vaultID else { throw CoreError.handOffRefused(.notRead) }
        // Below the floor names are decided by hand, and the text leaves
        // only once the user said they checked them (item 48): asked after
        // any other refusal the Vault can tell now, before its "not
        // reviewed", which is what an unchecked record reads as.
        let readiness = savedReadiness()
        if readiness != .notReviewed, let refusal = readiness.flatMap(HandOffRefusal.init) {
            throw CoreError.handOffRefused(refusal)
        }
        if let asked = textOwingNamesCheck() {
            lock.withLock { namesAskedOf = asked }
            throw CoreError.handOffRefused(.namesNotChecked)
        }
        if let refusal = readiness.flatMap(HandOffRefusal.init) {
            throw CoreError.handOffRefused(refusal)
        }
        let text: String
        do {
            text = try core { try library.vault.textForAi(id: vaultID) }
        } catch {
            // A change between the check and the read: why, as it is now.
            throw CoreError.handOffRefused(savedReadiness().flatMap(HandOffRefusal.init) ?? .failed)
        }
        // The Restore extension restores with where it went.
        library.rememberShared(vaultID: vaultID)
        return HandOff(text: text, fileName: handOff.fileName)
    }

    /// Below the floor, the text of a document never scanned that nobody
    /// said was checked, as it is now: Copy for AI and Share ask first. A
    /// scanned one needs no check, nor does any above the floor.
    private func textOwingNamesCheck() -> String? {
        guard library.findsNoNames, !ffi.namesChecked(), let review = try? ffi.snapshot(),
              review.phase == .unscanned else { return nil }
        return review.text
    }

    /// The text last asked of counts as checked, and the save that makes
    /// owed runs now, so the Vault records it Ready once no proposals are
    /// left. False, with nothing checked, when the text changed since it
    /// was asked (a join or a move meanwhile): the exit asks again.
    func confirmNamesChecked() throws -> Bool {
        guard let asked = lock.withLock({ namesAskedOf }) else { return false }
        guard try core({ try ffi.confirmNamesChecked(text: asked) }) else { return false }
        lock.withLock { namesAskedOf = nil }
        try saveChanges()
        return true
    }

    /// Saves, then opens the document again from the Vault: its names
    /// lined up with its Project's as they are now, so a name another
    /// document added since is proposed. Undo starts afresh.
    func lineUpWithProject() throws -> DocumentSnapshot {
        try saveChanges()
        guard let vaultID else { return snapshot() }
        try holdingSave { try reopen(vaultID: vaultID, kind: nil) }
        return snapshot()
    }

    func savedReadiness() -> SavedReadiness? {
        guard let vaultID else { return nil }
        #if DEBUG
        // A UI test's stand-in for a state only a computer makes today.
        if let readiness = LaunchConfiguration.current.uiReadiness { return readiness }
        #endif
        if (try? library.vault.document(id: vaultID))?.excludeFromAi == true { return .excludedFromAI }
        return switch try? library.vault.readiness(id: vaultID) {
        case .ready?: .ready
        case .notReviewed?: .notReviewed
        case .changedSinceReview?: .changedSinceReview
        case .namesNotReplaced(let count)?: .namesNotReplaced(Int(count))
        case nil: nil
        }
    }

    /// Saves `reply` in the Vault as an AI Reply to text shared from this
    /// document: its Project's, or its own while Unfiled (#1091 Decision
    /// 4), and the Reply pane shows it.
    func restoreReply(_ reply: String) throws -> DocumentSnapshot {
        _ = try core { try ffi.restoreReply(reply: reply) }
        // What the reply restores with is saved first, a first save the
        // locked phone held back too, which makes its Vault id (review 4).
        try saveChanges()
        guard let vaultID else { throw CoreError.notFound }
        let title = ffi.info()?.title ?? chosen.title
        let saved = try core {
            try library.vault.saveReply(
                sharedFrom: .document(id: vaultID), title: String(localized: "Reply to \(title)"), sourceApp: nil,
                markdown: reply)
        }
        lock.withLock { shownReply = ReplyID(rawValue: saved.id) }
        return snapshot()
    }

    /// The Reply pane shows saved reply `id`.
    func showReply(_ id: ReplyID) -> DocumentSnapshot {
        lock.withLock { shownReply = id }
        return snapshot()
    }

    /// Remove reply: the reply the pane shows is deleted from the Vault,
    /// and the pane shows the newest one left.
    func clearReply() throws -> DocumentSnapshot {
        if let shown = snapshot().reply?.id {
            try core { try library.vault.deleteReply(id: shown.rawValue) }
        }
        lock.withLock { shownReply = nil }
        return snapshot()
    }

    // MARK: Notes (#1091 Decision 3)

    func notes() -> String {
        reopenIfPending()
        return ffi.notes()
    }

    /// The core holds the notes with the document; they save once typing
    /// pauses, as a typed edit does.
    func setNotes(_ notes: String) throws {
        reopenIfPending()
        try core { try ffi.setNotes(notes: notes) }
        scheduleSave(pause: library.typingPause)
    }

    // MARK: Review bar (ADR 0098)

    func undo() throws -> DocumentSnapshot {
        try review(.undo)
    }

    func redo() throws -> DocumentSnapshot {
        try review(.redo)
    }

    /// Try a sample's names, proposed where they occur, as a scan's: the
    /// core finds them and counts the document as scanned.
    func propose(_ names: [SampleDocument.Name]) throws -> DocumentSnapshot {
        let names = names.map { ProposedName(wording: $0.wording, category: $0.category) }
        _ = try core { try ffi.review(action: .proposeNames(names: names)) }
        // Stored proposals stand in for the scan, and save as its result
        // does: when the phone locks first, once it unlocks.
        try saveAfterJob { [self] in try save() }
        return snapshot()
    }

    func proposeRules() throws -> DocumentSnapshot {
        reopenIfPending()
        // Saved as a tap is. It stays unscanned: Ready only below the floor
        // (`findsNoNames`), as for names marked by hand.
        defer { scheduleSave() }
        return snapshot(of: try core { try ffi.proposeRules() })
    }

    func hideTarget(_ range: SourceRange) throws -> WordsTarget {
        try core { try ffi.hideTarget(range: range.textRange) }
    }

    func markSelection(_ range: SourceRange, as category: Category?) throws -> DocumentSnapshot {
        guard range.length > 0 else { throw CoreError.emptySelection }
        // No category: the core's guess, as desktop's Replace on a selection.
        return try review(.mark(range: range.textRange, category: category))
    }

    /// Off the main thread: the core reads the Vault for the names the
    /// document's Project knows.
    func checkSelection(_ range: SourceRange) throws -> SelectionCheck {
        try core { try ffi.checkSelection(vault: library.vault, range: range.textRange) }
    }

    func replaceSelection(_ range: SourceRange) throws -> SelectionReplacement {
        reopenIfPending()
        defer { scheduleSave() }
        let replaced = try core { try ffi.replaceSelection(range: range.textRange) }
        return SelectionReplacement(
            snapshot: snapshot(of: replaced.snapshot), range: SourceRange(replaced.range),
            replaced: Int(replaced.replaced))
    }

    func fixWordsDraft(_ candidate: CandidateID) throws -> WordsDraft {
        let draft = try core { try ffi.fixWordsDraft(mention: Self.mention(candidate)) }
        return WordsDraft(
            snapshot: snapshot(of: draft.snapshot),
            wording: SourceRange(draft.wording), bounds: SourceRange(draft.bounds))
    }

    func fixWordsTarget(_ candidate: CandidateID, words: SourceRange) throws -> WordsTarget {
        try core { try ffi.fixWordsTarget(mention: Self.mention(candidate), words: words.textRange) }
    }

    func fixWords(_ candidate: CandidateID, words: SourceRange) throws -> DocumentSnapshot {
        try review(.fixWords(mention: Self.mention(candidate), words: words.textRange))
    }

    func rename(_ candidate: CandidateID, scope: Scope, to alias: String) throws -> DocumentSnapshot {
        try review(.rename(mention: Self.mention(candidate), scope: scope, alias: alias))
    }

    func categoryChoices(for candidate: Candidate, scope: Scope) throws -> [CategoryChoice] {
        try core { try ffi.categoryChoices(range: candidate.range.textRange, scope: scope) }
    }

    func setCategory(_ candidate: CandidateID, scope: Scope, to category: Category) throws -> DocumentSnapshot {
        try review(.setCategory(mention: Self.mention(candidate), scope: scope, category: category))
    }

    func sameAs(_ candidate: CandidateID, scope: Scope, target: IdentityID) throws -> DocumentSnapshot {
        try review(.sameAs(mention: Self.mention(candidate), scope: scope, target: Self.identity(target)))
    }

    func link(_ identity: IdentityID, into target: IdentityID) throws -> DocumentSnapshot {
        try review(.link(from: Self.identity(identity), into: Self.identity(target)))
    }

    func newAlias(_ candidate: CandidateID) throws -> DocumentSnapshot {
        try review(.newAlias(mention: Self.mention(candidate)))
    }

    func setOwner(_ identity: IdentityID, to owner: IdentityID?) throws -> DocumentSnapshot {
        try review(.setOwner(identity: Self.identity(identity), owner: owner.map(Self.identity)))
    }

    func dismissHint(_ hint: LinkHint) throws -> DocumentSnapshot {
        try review(.dismissHint(from: hint.from, into: hint.into))
    }

    /// The core's id of a mention.
    private static func mention(_ id: CandidateID) throws -> UInt64 {
        guard let value = UInt64(id.rawValue) else { throw CoreError.nameNotFound }
        return value
    }

    /// The core's id of an identity; a mention without one has none.
    private static func identity(_ id: IdentityID) throws -> UInt64 {
        guard let value = UInt64(id.rawValue) else { throw CoreError.nameNotFound }
        return value
    }

    /// A review tap in one core call (ADR 0104): the action by mention or
    /// identity id, and the snapshot after it. The change is saved into
    /// the library and its Project right after, off the main thread, as a
    /// Project member autosaves on desktop.
    private func review(_ action: ReviewAction) throws -> DocumentSnapshot {
        reopenIfPending()
        defer { scheduleSave() }
        return try tap(action)
    }

    /// The tap itself, as the "Review tap" signpost times it.
    private func tap(_ action: ReviewAction) throws -> DocumentSnapshot {
        let interval = reviewSignposter.beginInterval("Review tap", id: reviewSignposter.makeSignpostID())
        defer { reviewSignposter.endInterval("Review tap", interval) }
        let review = try core { try ffi.review(action: action) }
        return snapshot(of: review)
    }

    // MARK: Edit mode (ADR 0109)

    func type(_ range: SourceRange, _ text: String) throws -> Typed {
        let interval = typingSignposter.beginInterval("Typing", id: typingSignposter.makeSignpostID())
        defer { typingSignposter.endInterval("Typing", interval) }
        let typed = try core { try ffi.typeText(range: range.textRange, text: text) }
        // Typing saves once it pauses, as taps do, only later (ADR 0109).
        scheduleSave(pause: library.typingPause)
        return typed
    }

    func format(_ edits: [EditorEdit]) throws -> Typed {
        let typed = try core { try ffi.formatText(edits: edits) }
        scheduleSave(pause: library.typingPause)
        return typed
    }

    func endTyping() {
        ffi.endTyping()
    }

    func discover() throws -> DocumentSnapshot {
        let review = try core { try ffi.discover() }
        return snapshot(of: review)
    }

    func scanEdits() throws -> DocumentSnapshot {
        let paths = library.setup?.detectorPaths()
        var rules = library.setup?.snapshot().tier == .belowFloor
        #if DEBUG
        if library.rulesStandInForNameFinder { rules = true }
        #endif
        // Without the name finder above the floor the edits stay owed: the
        // rules alone would miss names.
        guard paths != nil || rules else {
            return try discover()
        }
        let review = try core { try ffi.scanEdits(detector: paths, progress: nil) }
        return snapshot(of: review)
    }

    /// The taps' save, once they pause (`SaveScheduler`). Told to the app:
    /// a failure shows as "could not save this change" and stays to save.
    private func scheduleSave(pause: DispatchTimeInterval? = nil) {
        saver.schedule(pause: pause ?? library.savePause) { [weak self] in
            guard let self else { return }
            do {
                try saveChanges()
            } catch {
                library.report(.failed(id, CoreError(error)))
            }
        }
    }

    /// What the taps changed, saved now. While the phone is locked the
    /// library's files are unwritable: it saves once the phone unlocks.
    func saveChanges() throws {
        saver.cancel()
        do {
            // A job's save a locked phone held back goes first: it joins
            // the Project chosen at import, and saves the taps too.
            try library.saveDeferred(id)
            try savePendingNow()
        } catch {
            guard !ProtectedData.isAvailable else { throw error }
            library.saveOnUnlock(id, replacing: false) { [self] in try savePendingNow() }
            // The phone may have unlocked since the check, its save already
            // run with nothing queued: then this one saves now.
            if ProtectedData.isAvailable {
                _ = try? library.saveDeferred()
            }
        }
    }

    private func savePendingNow() throws {
        let saved = try saving.withLock {
            guard !lock.withLock({ ended }) else { return nil as SavedVaultChange? }
            ffi.setNoNameFinder(noNameFinder: library.findsNoNames)
            let saved = try core { try ffi.savePendingIntoVault(vault: library.vault) }
            if let saved { recordSaved(vaultID: saved.id) }
            return saved
        }
        guard let saved else { return }
        library.report(.saved(id, rejoined: saved.rejoined))
    }

    /// The save that ends a job: the first one makes the document in the
    /// Vault, Unfiled unless it joined a Project.
    private func save() throws {
        try saving.withLock { try saveHeld() }
    }

    /// `save`, for a caller that holds `saving`.
    private func saveHeld() throws {
        guard !lock.withLock({ ended }) else { return }
        ffi.setNoNameFinder(noNameFinder: library.findsNoNames)
        let saved = try core { try ffi.saveIntoVault(vault: library.vault) }
        recordSaved(vaultID: saved.id)
    }

    /// Runs `body` while no save of this session runs, nor starts.
    fileprivate func holdingSave<T>(_ body: () throws -> T) rethrows -> T {
        try saving.withLock(body)
    }

    private func recordSaved(vaultID: String) {
        library.noteChange()
        library.keepID(self, vaultID: vaultID)
        let first = lock.withLock {
            let first = savedID == nil
            savedID = vaultID
            // The core holds its Project from now on.
            chosenPlace.projectID = nil
            if first {
                // The received files go with the import: the original is
                // viewed from the Vault.
                original = library.storedOriginal(vaultID)
            }
            return first
        }
        if first { library.importSaved(self) }
    }
}

// MARK: - Setup

final class RealSetup: Setup, @unchecked Sendable {
    let ffi: MdocFFI.Setup
    private let physicalMemory: UInt64
    /// Files the core installed, by model, so a retry after a failed file
    /// fetches only the rest.
    private let installedRecord: URL
    private let lock = NSLock()
    /// The OCR bundle setup offers: English and Russian, the desktop's languages.
    static let ocrModel: ModelId = .ocrEnglishRussian

    init(storage: MdocStorage, physicalMemory: UInt64) {
        self.physicalMemory = physicalMemory
        installedRecord = storage.url(.downloads).appendingPathComponent("installed.json")
        ffi = MdocFFI.Setup(modelsRoot: storage.url(.models).path, physicalMemoryBytes: physicalMemory)
    }

    func snapshot() -> SetupSnapshot {
        let catalog = ffi.catalog()
        let below = ffi.belowFloor()
        let tier: DeviceTier = below == nil ? .detector : .belowFloor
        let offers = catalog
            .filter { $0.offered && ($0.id == .gliner2Fp16 || $0.id == Self.ocrModel) }
            .map { info in
                ModelOffer(
                    id: Self.key(info.id), name: info.title.isEmpty ? info.name : "\(info.name) (\(info.title))",
                    purpose: info.id == .gliner2Fp16 ? .detector : .ocr,
                    byteSize: Int64(info.installed ? info.bytes : info.missingBytes),
                    state: info.installed ? .installed : .notInstalled)
            }
        return SetupSnapshot(
            physicalMemory: physicalMemory, tier: tier, offers: offers, belowFloor: below,
            tenPagesSeconds: ffi.tenPagesSeconds())
    }

    func timeDetector(observer: any JobObserver) throws -> SetupSnapshot {
        let runtimes = try core { try linkedRuntimes() }
        observer.onCancel { [ffi] in ffi.cancelTiming() }
        _ = try core { try ffi.timeDetector(runtimes: runtimes, progress: StageReporter(observer)) }
        return snapshot()
    }

    func downloadRequests(for model: String) throws -> [ModelDownloadRequest] {
        guard let id = Self.model(model), let info = ffi.catalog().first(where: { $0.id == id }) else {
            throw CoreError.notFound
        }
        // Skip files installed by an earlier, interrupted run, as long as
        // what is left matches the bytes the core still misses.
        let done = lock.withLock { loadInstalled()[model] ?? [] }
        return Self.remaining(info.files, installed: done, missingBytes: info.missingBytes).compactMap { file in
            URL(string: file.url).map {
                ModelDownloadRequest(modelID: model, file: file.path, url: $0, byteSize: Int64(file.bytes))
            }
        }
    }

    func install(_ request: ModelDownloadRequest, from file: URL) throws -> SetupSnapshot {
        guard let id = Self.model(request.modelID) else { throw CoreError.notFound }
        let info = try core { try ffi.install(model: id, file: request.file, downloaded: file.path) }
        lock.withLock {
            var installed = loadInstalled()
            installed[request.modelID] = info.installed ? nil : (installed[request.modelID] ?? []).union([request.file])
            saveInstalled(installed)
        }
        return snapshot()
    }

    /// The files still to fetch: all but those recorded as installed, unless
    /// the record disagrees with the bytes the core still misses.
    static func remaining(_ files: [ModelFile], installed: Set<String>, missingBytes: UInt64) -> [ModelFile] {
        let left = files.filter { !installed.contains($0.path) }
        return left.reduce(0) { $0 + $1.bytes } == missingBytes ? left : files
    }

    private func loadInstalled() -> [String: Set<String>] {
        guard let data = try? Data(contentsOf: installedRecord) else { return [:] }
        return (try? JSONDecoder().decode([String: Set<String>].self, from: data)) ?? [:]
    }

    private func saveInstalled(_ installed: [String: Set<String>]) {
        try? FileManager.default.createDirectory(at: installedRecord.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? JSONEncoder().encode(installed).write(to: installedRecord, options: .atomic)
    }

    func detectorPaths() -> DetectorPaths? {
        guard let runtimes = try? linkedRuntimes() else { return nil }
        return ffi.detectorPaths(runtimes: runtimes)
    }

    func ocrPaths() -> OcrPaths? {
        guard let runtimes = try? linkedRuntimes() else { return nil }
        return ffi.ocrPaths(model: Self.ocrModel, runtimes: runtimes)
    }

    static func key(_ model: ModelId) -> String {
        switch model {
        case .gliner2Fp16: "gliner2-fp16"
        case .ocrEnglish: "ocr-english"
        case .ocrEnglishRussian: "ocr-english-russian"
        }
    }

    static func model(_ key: String) -> ModelId? {
        [ModelId.gliner2Fp16, .ocrEnglish, .ocrEnglishRussian].first { Self.key($0) == key }
    }
}

// MARK: - Intake

/// What an import reads: a received file in the intake folder, named after
/// the title the user chose (the core titles a document by its file), page
/// images, or pasted text.
struct PreparedSource: Sendable {
    enum Input: Sendable {
        /// Text taken as Markdown, and how its document began.
        case text(String, DocumentOrigin)
        /// `needsOcr`: page images, which only OCR can read (imports kept
        /// from before `images`).
        case file(URL, needsOcr: Bool)
        /// Photos or camera pages, one document of a page each, which only
        /// OCR can read.
        case images([URL])
    }

    var input: Input
    var kind: SourceKind
    var original: OriginalSource
    /// The intake folders this import owns, removed with the document.
    var owned: [URL] = []
}

/// Received files made ready for the core: renamed after their title, in
/// their own folder under the intake folder, which the import owns until
/// its first save.
enum IntakeFiles {
    /// Moves a received file under its title. Page images go to the core
    /// as they are, together as one document, a single one named after the
    /// title.
    static func prepare(_ source: ImportSource, title: String, storage: MdocStorage) throws -> PreparedSource {
        switch source {
        case .text(let text):
            return PreparedSource(input: .text(text, .ordinary), kind: .text, original: .none)
        case .newDocument:
            return PreparedSource(input: .text("", .new), kind: .text, original: .none)
        case .sample(let text):
            return PreparedSource(input: .text(text, .sample), kind: .text, original: .none)
        case .file(let url, let kind):
            let named = try rename(url, to: title)
            let folders = owned(named, storage: storage)
            if kind == .image {
                return PreparedSource(input: .images([named]), kind: kind, original: .images([.file(named)]), owned: folders)
            }
            let original: OriginalSource = switch kind {
            case .pdf: .pdf(.file(named))
            case .docx: .docx(.file(named))
            default: .none
            }
            return PreparedSource(input: .file(named, needsOcr: false), kind: kind, original: original, owned: folders)
        case .scan(let pages), .photos(let pages):
            let kind: SourceKind = if case .scan = source { .scan } else { .image }
            guard let first = pages.first else { throw CoreError.emptySelection }
            let named = pages.count == 1 ? [try rename(first, to: title)] : pages
            return PreparedSource(
                input: .images(named), kind: kind, original: .images(named.map(OriginalBytes.file)),
                owned: owned(named[0], storage: storage))
        }
    }

    /// The import's own folder under the intake folder, when it has one.
    private static func owned(_ file: URL, storage: MdocStorage) -> [URL] {
        let folder = file.deletingLastPathComponent().standardizedFileURL
        let intake = storage.url(.intake).standardizedFileURL
        return folder.deletingLastPathComponent().path == intake.path ? [folder] : []
    }

    private static func rename(_ url: URL, to title: String) throws -> URL {
        let target = url.deletingLastPathComponent()
            .appendingPathComponent(MdocStorage.fileName(title, extension: url.pathExtension))
        guard target != url else { return url }
        try? FileManager.default.removeItem(at: target)
        try FileManager.default.moveItem(at: url, to: target)
        return target
    }
}

final class PendingImports: @unchecked Sendable {
    struct Entry {
        var id: DocumentID
        var title: String
        /// Nil: Unfiled.
        var projectID: String?
        var source: PreparedSource
        var ocrStarted = false
    }

    private struct Stored: Codable {
        var title: String
        var projectID: String?
        var kind: String
        var text: String?
        var file: String?
        /// Page images imported together; older entries have none.
        var images: [String]?
        var needsOcr: Bool
        var originalKind: String
        var originalFiles: [String]
        var owned: [String]
        /// OCR began: the core keeps its finished pages. Older entries have none.
        var ocrStarted: Bool?
        /// A text that began in mdoc: "new" or "sample" (ADR 0109).
        var origin: String?
    }

    private let storage: MdocStorage
    private let lock = NSLock()
    private var url: URL { storage.intake.appendingPathComponent("pending.json") }

    init(storage: MdocStorage) {
        self.storage = storage
    }

    func add(_ entry: Entry) {
        let source = entry.source
        var stored = Stored(
            title: entry.title, projectID: entry.projectID, kind: source.kind.rawValue, text: nil, file: nil,
            needsOcr: false, originalKind: "none", originalFiles: [], owned: source.owned.map { relative($0) })
        switch source.input {
        case .text(let text, let origin):
            stored.text = text
            stored.origin = switch origin {
            case .new: "new"
            case .sample: "sample"
            case .ordinary: nil
            }
        case .file(let file, let needsOcr):
            stored.file = relative(file)
            stored.needsOcr = needsOcr
        case .images(let files):
            stored.images = files.map { relative($0) }
        }
        switch source.original {
        case .none: break
        case .pdf(let file):
            stored.originalKind = "pdf"
            stored.originalFiles = file.fileURL.map { [relative($0)] } ?? []
        case .docx(let file):
            stored.originalKind = "docx"
            stored.originalFiles = file.fileURL.map { [relative($0)] } ?? []
        case .images(let files):
            stored.originalKind = "images"
            stored.originalFiles = files.compactMap(\.fileURL).map { relative($0) }
        }
        lock.withLock {
            var entries = load()
            entries[entry.id.rawValue] = stored
            save(entries)
        }
    }

    func all() -> [Entry] {
        lock.withLock { load() }.compactMap { id, stored in
            let input: PreparedSource.Input
            if let text = stored.text {
                let origin: DocumentOrigin = switch stored.origin {
                case "new": .new
                case "sample": .sample
                default: .ordinary
                }
                input = .text(text, origin)
            } else if let file = stored.file {
                let url = absolute(file)
                // A source file that is gone cannot be read again.
                guard FileManager.default.fileExists(atPath: url.path) else { return nil }
                input = .file(url, needsOcr: stored.needsOcr)
            } else if let images = stored.images, !images.isEmpty {
                let urls = images.map { absolute($0) }
                guard urls.allSatisfy({ FileManager.default.fileExists(atPath: $0.path) }) else { return nil }
                input = .images(urls)
            } else {
                return nil
            }
            let files = stored.originalFiles.map { absolute($0) }
            let original: OriginalSource = switch stored.originalKind {
            case "pdf": files.first.map { .pdf(.file($0)) } ?? .none
            case "docx": files.first.map { .docx(.file($0)) } ?? .none
            case "images": .images(files.map(OriginalBytes.file))
            default: .none
            }
            let source = PreparedSource(
                input: input, kind: SourceKind(rawValue: stored.kind) ?? .text, original: original,
                owned: stored.owned.map { absolute($0) })
            return Entry(
                id: DocumentID(rawValue: id), title: stored.title, projectID: stored.projectID, source: source,
                ocrStarted: stored.ocrStarted ?? false)
        }
    }

    func markOCRStarted(_ id: DocumentID) {
        lock.withLock {
            var entries = load()
            guard entries[id.rawValue] != nil else { return }
            entries[id.rawValue]?.ocrStarted = true
            save(entries)
        }
    }

    /// The Project imports are to join, `nil` Unfiled, in one write: their
    /// Project is being deleted, its documents moved, or that delete
    /// failed. Throws when the record can't be read or written.
    func place(_ ids: [DocumentID], in projectID: String?) throws {
        guard !ids.isEmpty else { return }
        try lock.withLock {
            var entries = try read()
            for id in ids where entries[id.rawValue] != nil {
                entries[id.rawValue]?.projectID = projectID
            }
            try write(entries)
        }
    }

    /// Forgets an import, and its intake files.
    func remove(_ id: DocumentID) {
        lock.withLock {
            var entries = load()
            guard let stored = entries.removeValue(forKey: id.rawValue) else { return }
            for folder in stored.owned {
                try? FileManager.default.removeItem(at: absolute(folder))
            }
            save(entries)
        }
    }

    private func relative(_ url: URL) -> String {
        let root = storage.intake.standardizedFileURL.path + "/"
        let path = url.standardizedFileURL.path
        return path.hasPrefix(root) ? String(path.dropFirst(root.count)) : path
    }

    private func absolute(_ path: String) -> URL {
        path.hasPrefix("/") ? URL(fileURLWithPath: path) : storage.intake.appendingPathComponent(path)
    }

    private func load() -> [String: Stored] {
        guard let data = try? Data(contentsOf: url) else { return [:] }
        return (try? JSONDecoder().decode([String: Stored].self, from: data)) ?? [:]
    }

    /// `load`, throwing when the record is there but can't be read.
    private func read() throws -> [String: Stored] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [:] }
        return try JSONDecoder().decode([String: Stored].self, from: Data(contentsOf: url))
    }

    private func save(_ entries: [String: Stored]) {
        try? write(entries)
    }

    /// `save`, throwing when the record can't be written.
    private func write(_ entries: [String: Stored]) throws {
        let data = try JSONEncoder().encode(entries)
        try data.write(to: url, options: [.atomic, .completeFileProtection])
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var excluded = url
        try? excluded.setResourceValues(values)
    }
}
