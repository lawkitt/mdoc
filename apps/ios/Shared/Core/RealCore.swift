import Foundation
import MdocFFI
import notify
import os

/// The app's `Core` over `crates/mdoc-ffi` (ADR 0085). The Rust objects own
/// documents, review, cases and restore; this file only converts their
/// records into the snapshots the views use, and keeps which source file a
/// document came from for the Original view.
enum RealCore {
    /// The library and setup for the app. `physicalMemory` decides which
    /// models are offered; below the floor a scan runs the structured rules.
    static func make(storage: MdocStorage, keys: any CaseKeyStore, physicalMemory: UInt64) throws -> Core {
        let setup = RealSetup(storage: storage, physicalMemory: physicalMemory)
        let library = try RealLibrary(storage: storage, keys: keys, setup: setup)
        library.migrateScanned()
        return Core(library: library, setup: setup)
    }

    /// The Restore extension's library: cases and restore, no models.
    static func library(storage: MdocStorage, keys: any CaseKeyStore) throws -> any Library {
        try RealLibrary(storage: storage, keys: keys, setup: nil)
    }
}

// MARK: - Conversions

extension CaseColour {
    /// The swatch nearest a case's stored hue (ADR 0077).
    init(hue: UInt32) {
        let hues: [(CaseColour, Double)] = [
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
        case .caseFailed(let message): .caseRefused(message)
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

extension CaseSummary {
    init(_ info: CaseInfo) {
        self.init(id: CaseID(rawValue: info.id), name: info.name, colour: CaseColour(hue: info.hue))
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

final class RealLibrary: Library, @unchecked Sendable {
    let ffi: MdocFFI.Library
    let storage: MdocStorage
    let setup: RealSetup?
    private let lock = NSLock()
    private var sessions: [DocumentID: RealSession] = [:]
    /// Saved document path → the id this run gave it.
    private var ids: [String: DocumentID] = [:]
    private let originals: OriginalsIndex
    private let pending: PendingImports
    /// Saves that wait for the phone to unlock, by document.
    private var unsaved: [DocumentID: () throws -> Void] = [:]
    /// Case names by id, read once each: every snapshot shows its case's
    /// name, and reading it opens the case.
    private var caseNames: [String: String] = [:]
    /// Told of each reply saved, here or in the Restore extension, so the
    /// sessions read their kept reply again (`replyGeneration`).
    private var replyToken: Int32 = 0
    private let replyWatch: Bool
    private var replyGenerationValue = 0
    /// Where the saves that follow taps run (ADR 0104).
    fileprivate let saveQueue = DispatchQueue(label: "io.github.lawkitt.mdoc.save", qos: .utility)
    /// How long taps pause before their save; the tests shorten it.
    var savePause = SaveScheduler.pause
    /// How long typing pauses before its save (ADR 0109); the tests shorten it.
    var typingPause = SaveScheduler.typingPause
    /// What scans ask about the phone; tests stand in low memory or heat.
    var device = DeviceConditions.live
    private var saveObserver: (@MainActor @Sendable (SaveEvent) -> Void)?

    init(storage: MdocStorage, keys: any CaseKeyStore, setup: RealSetup?) throws {
        self.storage = storage
        self.setup = setup
        originals = OriginalsIndex(storage: storage)
        pending = PendingImports(storage: storage)
        ffi = try core { try MdocFFI.Library(root: storage.root.path, caseKey: try keys.key()) }
        replyWatch = notify_register_check(Self.replySaved, &replyToken) == NOTIFY_STATUS_OK
        // Imports not yet read when mdoc last stopped come back, ready to
        // recognize again.
        for entry in pending.all() {
            let session = RealSession(
                id: entry.id, library: self, source: entry.source, title: entry.title, caseID: entry.caseID,
                ocrStarted: entry.ocrStarted)
            sessions[entry.id] = session
        }
    }

    deinit {
        if replyWatch { notify_cancel(replyToken) }
    }

    /// The saved documents from the core's summaries, which open none of
    /// them (ADR 0104), then the imports not saved yet. A document open
    /// this run lists as its session has it, as it shows when opened: a
    /// Keep is not saved, and a save a locked phone held back has not
    /// landed yet.
    func snapshot() -> LibrarySnapshot {
        let cases = ffi.cases().cases.map(CaseSummary.init)
        let stored = (try? ffi.documentSummaries()) ?? []
        let kinds = originals.kinds()
        let rows = lock.withLock {
            stored.map { summary in
                let id = ids[summary.path] ?? DocumentID(rawValue: summary.path)
                return (summary, id, sessions[id])
            }
        }
        var documents = rows.map { summary, id, session in
            let updated = Date(timeIntervalSince1970: TimeInterval(summary.modified))
            return session?.loadedSummary(updated: updated) ?? LibraryDocument(
                id: id, title: summary.title, caseID: summary.caseId.map(CaseID.init), kind: kinds(summary.path),
                phase: summary.phase, pendingCount: Int(summary.proposed), updated: updated,
                candidateCount: Int(summary.proposed + summary.replaced + summary.kept), origin: summary.origin)
        }
        let listed = Set(documents.map(\.id))
        let pending = lock.withLock { sessions.values.filter { !listed.contains($0.id) } }
        documents += pending.map { $0.summary(updated: $0.created) }
        return LibrarySnapshot(
            cases: cases,
            documents: documents.sorted { $0.updated > $1.updated },
            currentCase: ffi.currentCase().map { CaseID(rawValue: $0.id) })
    }

    /// The documents this run has a session for: opened, or imported.
    var openSessionCount: Int { lock.withLock { sessions.count } }

    func session(for document: DocumentID) throws -> any DocumentSession {
        if let session = lock.withLock({ sessions[document] }) {
            return session
        }
        return try session(path: document.rawValue)
    }

    /// The session of a saved document, opened in its case the first time.
    private func session(path: String) throws -> RealSession {
        if let id = lock.withLock({ ids[path] }), let session = lock.withLock({ sessions[id] }) {
            return session
        }
        guard FileManager.default.fileExists(atPath: path) else { throw CoreError.notFound }
        // It opens in its case as stored: what the other documents' taps
        // added to the case (a name marked and replaced) is saved first,
        // so it is proposed here too.
        saveChangesReporting()
        let session = RealSession(id: DocumentID(rawValue: path), library: self, source: nil, title: "", caseID: nil)
        try session.reopen(path: path, original: originals.original(for: path))
        lock.withLock {
            sessions[session.id] = session
            ids[path] = session.id
        }
        return session
    }

    func importDocument(_ source: ImportSource, title: String, into caseChoice: CaseChoice) throws -> any DocumentSession {
        // The case exists from the start, so the document lists under it
        // while it is read (ADR 0085: a new case unless one is chosen).
        let caseID: String = switch caseChoice {
        case .existing(let id): id.rawValue
        case .newCase(let name): try core { try ffi.createCase(name: name) }.id
        }
        // Its job joins the case as stored: the taps' changes first.
        saveChangesReporting()
        let prepared = try originals.prepare(source, title: title)
        let id = DocumentID(rawValue: "new:\(UUID().uuidString)")
        // Kept on disk until its first save, so it survives mdoc stopping.
        pending.add(PendingImports.Entry(id: id, title: title, caseID: caseID, source: prepared))
        let session = RealSession(id: id, library: self, source: prepared, title: title, caseID: caseID)
        lock.withLock { sessions[session.id] = session }
        return session
    }

    func deleteDocument(_ document: DocumentID) throws {
        let session = lock.withLock { sessions[document] }
        // A listed document not opened this run has no session: its id is
        // its file.
        let path = session.map(\.path) ?? (FileManager.default.fileExists(atPath: document.rawValue) ? document.rawValue : nil)
        guard session != nil || path != nil else { throw CoreError.notFound }
        // Nothing writes it again once it is gone: not its taps' save, its
        // job's, or a save that waits for the phone to unlock; its job stops.
        session?.end()
        lock.withLock { _ = unsaved.removeValue(forKey: document) }
        if let path {
            // The core takes it out of its case (and the reply kept for it),
            // then removes the file; a case it cannot save keeps it.
            _ = try core { try ffi.deleteDocument(path: path) }
            lock.withLock { _ = ids.removeValue(forKey: path) }
            originals.remove(for: path)
        } else if let input = session?.pendingInput {
            // An import not yet read: OCR's kept pages go first, found by the
            // source files' digests, then the files.
            switch input {
            case .file(let url, _): try core { try ffi.discardImport(source: url.path) }
            case .images(let urls): try core { try ffi.discardImages(images: urls.map(\.path)) }
            case .text: break
            }
        }
        lock.withLock { _ = sessions.removeValue(forKey: document) }
        pending.remove(document)
    }

    func discardIfEmpty(_ document: DocumentID) throws -> Bool {
        guard let session = lock.withLock({ sessions[document] }) else { return false }
        let path = session.path
        guard try session.discardIfEmpty() else { return false }
        // Gone in the core: its saves have nothing left to write.
        session.end()
        lock.withLock { _ = unsaved.removeValue(forKey: document) }
        if let path {
            lock.withLock { _ = ids.removeValue(forKey: path) }
            originals.remove(for: path)
        }
        lock.withLock { _ = sessions.removeValue(forKey: document) }
        pending.remove(document)
        return true
    }

    func renameDocument(_ document: DocumentID, to title: String) throws {
        guard let session = try session(for: document) as? RealSession else { throw CoreError.notFound }
        let (old, new) = try session.rename(to: title)
        renamed(session, from: old, to: new)
    }

    /// What this side keeps by the file follows a renamed document.
    fileprivate func renamed(_ session: RealSession, from old: String?, to new: String?) {
        if let old, let new, old != new {
            lock.withLock {
                ids[old] = nil
                ids[new] = session.id
            }
            originals.move(from: old, to: new)
        }
    }

    func renameCase(_ caseID: CaseID, to name: String) throws {
        _ = try core { try ffi.renameCase(caseId: caseID.rawValue, name: name) }
        lock.withLock { _ = caseNames.removeValue(forKey: caseID.rawValue) }
    }

    func moveDocument(_ document: DocumentID, to target: CaseChoice) throws -> CaseMove {
        try move(document) { path in
            switch target {
            case .existing(let id): try ffi.moveDocument(path: path, caseId: id.rawValue)
            case .newCase(let name): try ffi.moveDocumentToNewCase(path: path, name: name)
            }
        }
    }

    func undoMove(_ move: CaseMove) throws -> CaseMove {
        guard let from = move.from else { throw CoreError.noCase }
        return try self.move(move.document) { path in
            try ffi.moveBack(path: path, caseId: from.rawValue, added: move.added)
        }
    }

    /// A saved document's move through the core. The move has happened once
    /// the core returns, so the result stands even if the session cannot
    /// open the changed file at once: it opens it on its next use.
    private func move(_ document: DocumentID, _ call: (String) throws -> MovedDocument) throws -> CaseMove {
        guard let session = try session(for: document) as? RealSession else { throw CoreError.notFound }
        guard let path = session.path else {
            throw CoreError.refused(String(localized: "Move the document once it is read."))
        }
        // The move rewrites the file and the session opens it again: what
        // the taps changed is in the file first.
        try session.saveChanges()
        let moved = try core { try call(path) }
        // Its text and case changed on disk: the session opens it again.
        let original = originals.original(for: path)
        do {
            try session.reopen(path: path, original: original)
        } catch {
            session.reopenLater(original: original)
        }
        return CaseMove(
            document: document, to: CaseSummary(moved.case), from: moved.left.map { CaseID(rawValue: $0.id) },
            added: moved.added)
    }

    func restore(reply: String, in caseID: CaseID) throws -> RestoreResult {
        RestoreResult(try core { try ffi.restore(caseId: caseID.rawValue, reply: reply) })
    }

    func saveReply(_ reply: String, to document: DocumentID) throws {
        guard let session = try session(for: document) as? RealSession, let info = session.info, let path = info.path,
              let caseID = info.caseId else {
            throw CoreError.noCase
        }
        try core { try ffi.saveReply(caseId: caseID, document: path, reply: reply) }
        replySaved()
    }

    /// Posted with each reply saved, in this process or the other (the app
    /// and its Restore extension share the library).
    private static let replySaved = "io.github.lawkitt.mdoc.reply-saved"

    /// A reply was saved: every session reads its kept reply again.
    fileprivate func replySaved() {
        lock.withLock { replyGenerationValue += 1 }
        notify_post(Self.replySaved)
    }

    /// Changes with each reply saved, here or in the Restore extension. If
    /// the system cannot tell, it changes on every call: nothing is cached.
    fileprivate func replyGeneration() -> Int {
        lock.withLock {
            var posted: Int32 = 1
            if replyWatch {
                notify_check(replyToken, &posted)
            }
            if posted != 0 { replyGenerationValue += 1 }
            return replyGenerationValue
        }
    }

    /// A save that waits for the phone to unlock. A job's save replaces
    /// what waited for the document before it (it saves the taps' changes
    /// too); the taps' save never replaces a job's, which may join the case
    /// first.
    fileprivate func saveOnUnlock(_ id: DocumentID, replacing: Bool = true, _ save: @escaping () throws -> Void) {
        lock.withLock {
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

    /// A saved document keeps its id for the rest of the run, and its
    /// source files; the first save ends its import. Later saves of the
    /// same file find it recorded and write nothing.
    fileprivate func saved(_ session: RealSession, path: String, original: OriginalSource, owned: [URL]) {
        let first = lock.withLock {
            let first = ids[path] != session.id
            ids[path] = session.id
            return first
        }
        guard first else { return }
        originals.record(original, owned: owned, for: path)
        pending.remove(session.id, keepingFiles: true)
    }

    /// Once, for a library from before the core kept whether a document
    /// was scanned (#684): a document this app recorded as scanned (#685),
    /// which the core does not count as scanned, is told so, so a scan that
    /// found nothing still reads as scanned. Then the app's record goes. A
    /// document that cannot be opened now (the phone locked) keeps its
    /// record for the next launch.
    func migrateScanned() {
        let recorded = originals.scannedKeys()
        guard !recorded.isEmpty, let summaries = try? ffi.documentSummaries() else { return }
        var kept: Set<String> = []
        for summary in summaries where !summary.scanned && recorded.contains(originals.key(summary.path)) {
            // A session of its own, let go after: the document saves once,
            // its time changing as any save's does.
            let session = RealSession(id: DocumentID(rawValue: summary.path), library: self, source: nil, title: "", caseID: nil)
            do {
                try session.reopen(path: summary.path, original: (.none, .text))
                try session.markScanned()
            } catch {
                kept.insert(originals.key(summary.path))
            }
        }
        originals.forgetScanned(except: kept)
    }

    fileprivate func caseName(_ id: String?) -> String? {
        guard let id else { return nil }
        if let name = lock.withLock({ caseNames[id] }) { return name }
        // Not kept when it cannot be read now (the phone locked).
        guard let name = try? ffi.caseInfo(id: id).name else { return nil }
        lock.withLock { caseNames[id] = name }
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
/// (its title, file, case, review and whether it was scanned) is the core's,
/// read with `info()` and `snapshot()` each time (#681, ADR 0104). This side
/// keeps only what the core has no record of: an import's source before it
/// is read, with the title and case chosen for it, and which source file a
/// document came from, for the Original view.
final class RealSession: DocumentSession, @unchecked Sendable {
    let id: DocumentID
    let created = Date()
    private let ffi = MdocFFI.DocumentSession()
    // Strong: a session keeps its library open while it is used.
    private let library: RealLibrary
    private let lock = NSLock()
    /// What an import reads; nil once it is read, or for a saved document.
    private var source: PreparedSource?
    /// The title and case chosen at import, until the core holds the
    /// document: it reads the title from the file and has no case before
    /// its first save.
    private let chosen: (title: String, caseID: String?)
    private var kind: SourceKind = .text
    private var original: OriginalSource = .none
    private var owned: [URL] = []
    /// OCR began on this import and has not finished: the core keeps its
    /// finished pages, so recognition resumes with the rest.
    private var ocrStarted: Bool
    /// A move changed the saved file and opening it again failed: the next
    /// use opens it, with this original.
    private var reopenPending: (OriginalSource, SourceKind)?
    /// The Markdown styling of the text with this revision: a snapshot whose
    /// `text_revision` is the same reuses it instead of parsing again.
    private var styled: (revision: UInt64, styling: MarkdownStyling)?
    /// The save that follows taps (ADR 0104).
    private let saver: SaveScheduler
    /// Held while a save records its file, and while a rename moves it:
    /// the file a save records is the session's.
    private let saving = NSLock()
    /// Deleted or discarded (`end`): nothing saves it again.
    private var ended = false
    /// The core's id of the long job running now, which `end` stops.
    private var runningJob: UInt64?
    /// The reply kept for the document, read from its case again only
    /// once a reply was saved or the document moved.
    private var keptReply: (key: KeptReplyKey, text: String?)?
    /// The kept reply as restored, while its text and the names stay.
    private var restoredReply: (key: RestoredReplyKey, reply: ReplySnapshot?)?

    private struct KeptReplyKey: Equatable {
        let caseID: String
        let path: String
        let generation: Int
    }

    private struct RestoredReplyKey: Equatable {
        let text: String
        let textRevision: UInt64
        let identities: [Identity]
    }

    fileprivate init(
        id: DocumentID, library: RealLibrary, source: PreparedSource?, title: String, caseID: String?,
        ocrStarted: Bool = false
    ) {
        self.id = id
        self.library = library
        self.source = source
        chosen = (title, caseID)
        self.ocrStarted = ocrStarted
        saver = SaveScheduler(queue: library.saveQueue)
        if let source {
            kind = source.kind
            original = source.original
            owned = source.owned
        }
    }

    /// The core's title, file, case and phase; nil before the import is read.
    var info: MdocFFI.SessionInfo? { ffi.info() }

    /// The library file, once saved.
    var path: String? { ffi.info()?.path }

    /// Opens a saved document in its case.
    fileprivate func reopen(path: String, original: (OriginalSource, SourceKind)) throws {
        _ = try core { try ffi.openDocument(library: library.ffi, path: path) }
        lock.withLock {
            self.original = original.0
            kind = original.1
        }
    }

    /// Rename: the core renames the file and the session follows it,
    /// keeping its review and history. Returns the path before and after.
    fileprivate func rename(to title: String) throws -> (old: String?, new: String?) {
        guard let before = ffi.info() else {
            throw CoreError.refused(String(localized: "Rename the document once it is read."))
        }
        let renamed = try saving.withLock { try core { try ffi.renameDocument(library: library.ffi, title: title) } }
        return (before.path, renamed.path ?? before.path)
    }

    /// `RealLibrary.discardIfEmpty`: the core removes the file and its case.
    /// A save waiting for the lock writes nothing after it.
    fileprivate func discardIfEmpty() throws -> Bool {
        try saving.withLock {
            guard try core({ try ffi.discardIfEmpty(library: library.ffi) }) else { return false }
            lock.withLock { ended = true }
            return true
        }
    }

    func followFirstLine() throws -> String? {
        let before = ffi.info()?.path
        let renamed = try saving.withLock { try core { try ffi.followFirstLine(library: library.ffi) } }
        guard let renamed else { return nil }
        library.renamed(self, from: before, to: renamed.path ?? before)
        return renamed.title
    }

    /// What an import not yet read reads.
    fileprivate var pendingInput: PreparedSource.Input? {
        lock.withLock { source?.input }
    }

    /// The list row of an import not saved yet.
    fileprivate func summary(updated: Date) -> LibraryDocument {
        let snapshot = snapshot()
        return LibraryDocument(
            id: id, title: snapshot.title, caseID: snapshot.caseID, kind: snapshot.kind,
            phase: snapshot.phase, pendingCount: snapshot.pendingCount, updated: updated,
            candidateCount: snapshot.candidates.count, origin: snapshot.origin)
    }

    /// The list row of a document this session holds, from the core's
    /// review without the reply or the styling; nil before it is read.
    fileprivate func loadedSummary(updated: Date) -> LibraryDocument? {
        guard let review = try? ffi.snapshot() else { return nil }
        let caseID = review.caseId ?? chosen.caseID
        return LibraryDocument(
            id: id, title: review.title, caseID: caseID.map(CaseID.init), kind: lock.withLock { kind },
            phase: review.phase, pendingCount: Int(review.proposed), updated: updated,
            candidateCount: review.mentions.count, origin: ffi.info()?.origin ?? .ordinary)
    }

    fileprivate func reopenLater(original: (OriginalSource, SourceKind)) {
        lock.withLock { reopenPending = original }
    }

    /// Opens the file again when a move left that for later; a failure
    /// leaves it pending for the next try.
    private func reopenIfPending() {
        guard let pending = lock.withLock({ reopenPending }), let path = ffi.info()?.path else { return }
        if (try? reopen(path: path, original: pending)) != nil {
            lock.withLock { reopenPending = nil }
        }
    }

    /// `migrateScanned`: the core counts the document as scanned, as the
    /// app recorded it, and saves that.
    fileprivate func markScanned() throws {
        _ = try core { try ffi.review(action: .proposeNames(names: []), library: library.ffi) }
    }

    func snapshot() -> DocumentSnapshot {
        reopenIfPending()
        guard let review = try? ffi.snapshot() else {
            let (kind, original, ocrStarted, source) = lock.withLock { (self.kind, self.original, self.ocrStarted, self.source) }
            // Before it is read, its origin is the import's.
            let origin: DocumentOrigin = if case .text(_, let origin) = source?.input { origin } else { .ordinary }
            return DocumentSnapshot(
                id: id, title: chosen.title, caseID: chosen.caseID.map(CaseID.init), caseName: library.caseName(chosen.caseID),
                kind: kind, phase: nil, markdown: "", styling: .empty, candidates: [], identities: [],
                original: original, reply: nil, resumesOCR: ocrStarted, origin: origin)
        }
        return snapshot(of: review, path: ffi.info()?.path)
    }

    /// The app's snapshot of the core's review, the document saved at
    /// `path`. It derives nothing: states, counts, the kept names, the
    /// phase and the ids are the core's.
    private func snapshot(of review: ReviewSnapshot, path: String?) -> DocumentSnapshot {
        // Before its first save the core has no case: the one chosen. A
        // saved document is always in one.
        let caseID = review.caseId ?? chosen.caseID
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
        let reply = reply(caseID: caseID, path: path, textRevision: review.textRevision, identities: identities)
        return DocumentSnapshot(
            id: id, title: review.title, caseID: caseID.map(CaseID.init),
            caseName: library.caseName(caseID), kind: kind, phase: review.phase,
            markdown: review.text, styling: styling(review), textRevision: review.textRevision,
            candidates: candidates, identities: identities, original: original,
            reply: reply, notice: review.notice, leftOut: review.leftOut,
            mergeHints: review.linkHints, canUndo: review.canUndo, canRedo: review.canRedo,
            undoStep: review.undoStep, redoStep: review.redoStep,
            counts: MentionCounts(proposed: Int(review.proposed), replaced: Int(review.replaced), kept: Int(review.kept)),
            origin: ffi.info()?.origin ?? .ordinary)
    }

    /// The reply kept for the document and its restored reading. Reading
    /// it opens the case, so it is read again only once a reply was saved
    /// (here or in the Restore extension) or the document's case or file
    /// changed; it is restored again only once its text or the names did.
    private func reply(caseID: String?, path: String?, textRevision: UInt64, identities: [Identity]) -> ReplySnapshot? {
        guard let caseID, let path else { return nil }
        let key = KeptReplyKey(caseID: caseID, path: path, generation: library.replyGeneration())
        let text: String?
        if let kept = lock.withLock({ keptReply }), kept.key == key {
            text = kept.text
        } else {
            // No reply is kept too: most documents have none.
            do {
                text = try library.ffi.reply(caseId: caseID, document: path)
            } catch {
                return nil
            }
            lock.withLock { keptReply = (key, text) }
        }
        guard let text else { return nil }
        let restoredKey = RestoredReplyKey(text: text, textRevision: textRevision, identities: identities)
        if let restored = lock.withLock({ restoredReply }), restored.key == restoredKey {
            return restored.reply
        }
        let reply = (try? ffi.restoreReply(reply: text)).map { ReplySnapshot(aliasText: text, restored: RestoreResult($0)) }
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
            // OCR keeps its finished pages in the library (ADR 0085 Long
            // jobs): after a cancel, or mdoc stopping, this resumes with the
            // pages left.
            _ = try job(observer) {
                try ffi.importFileIn(
                    library: library.ffi, path: url.path,
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
                try ffi.importImagesIn(
                    library: library.ffi, images: urls.map(\.path), title: chosen.title,
                    options: ImportOptions(ocr: ocr, nativeTextOnly: false, password: nil), progress: reporter)
            }
        }
        lock.withLock {
            self.source = nil
            ocrStarted = false
        }
        let caseID = chosen.caseID
        try saveAfterJob { [self] in
            if let caseID {
                _ = try core { try ffi.joinCase(library: library.ffi, caseId: caseID) }
            }
            try save()
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

    /// The core refuses while proposals are pending, and makes the
    /// document's case the current one. Saved first: a reply pasted into
    /// the Restore extension restores with the case as saved.
    func copyForAI() throws -> HandOff {
        try saveChanges()
        return try core { try ffi.handOff(library: library.ffi) }
    }

    func restoreReply(_ reply: String) throws -> DocumentSnapshot {
        guard let info = ffi.info(), let caseID = info.caseId, let path = info.path else { throw CoreError.noCase }
        _ = try core { try ffi.restoreReply(reply: reply) }
        try core { try library.ffi.saveReply(caseId: caseID, document: path, reply: reply) }
        library.replySaved()
        return snapshot()
    }

    func clearReply() throws -> DocumentSnapshot {
        guard let info = ffi.info(), let caseID = info.caseId, let path = info.path else { throw CoreError.noCase }
        try core { try library.ffi.saveReply(caseId: caseID, document: path, reply: "") }
        library.replySaved()
        return snapshot()
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
        _ = try core { try ffi.review(action: .proposeNames(names: names), library: nil) }
        // Stored proposals stand in for the scan, and save as its result
        // does: when the phone locks first, once it unlocks.
        try saveAfterJob { [self] in try save() }
        return snapshot()
    }

    func hideTarget(_ range: SourceRange) throws -> WordsTarget {
        try core { try ffi.hideTarget(range: range.textRange) }
    }

    func markSelection(_ range: SourceRange, as category: Category?) throws -> DocumentSnapshot {
        guard range.length > 0 else { throw CoreError.emptySelection }
        // No category: the core's guess, as desktop's Replace on a selection.
        return try review(.mark(range: range.textRange, category: category))
    }

    func fixWordsDraft(_ candidate: CandidateID) throws -> WordsDraft {
        let draft = try core { try ffi.fixWordsDraft(mention: Self.mention(candidate)) }
        return WordsDraft(
            snapshot: snapshot(of: draft.snapshot, path: ffi.info()?.path),
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
    /// the library and its case right after, off the main thread, as a
    /// case member autosaves on desktop.
    private func review(_ action: ReviewAction) throws -> DocumentSnapshot {
        reopenIfPending()
        defer { scheduleSave() }
        return try tap(action)
    }

    /// The tap itself, as the "Review tap" signpost times it.
    private func tap(_ action: ReviewAction) throws -> DocumentSnapshot {
        let interval = reviewSignposter.beginInterval("Review tap", id: reviewSignposter.makeSignpostID())
        defer { reviewSignposter.endInterval("Review tap", interval) }
        let review = try core { try ffi.review(action: action, library: nil) }
        return snapshot(of: review, path: ffi.info()?.path)
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
        return snapshot(of: review, path: ffi.info()?.path)
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
            // the case chosen at import, and saves the taps too.
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
            guard !lock.withLock({ ended }) else { return nil as SavedChange? }
            let saved = try core { try ffi.savePending(library: library.ffi) }
            if let saved { recordSaved(path: saved.path) }
            return saved
        }
        guard let saved else { return }
        library.report(.saved(id, rejoined: saved.rejoined))
    }

    /// The document goes (deleted or discarded): its scheduled save does
    /// not run, one running now ends first and no later save writes it; its
    /// running job stops.
    fileprivate func end() {
        saver.cancel()
        let job = saving.withLock {
            lock.withLock {
                ended = true
                return runningJob
            }
        }
        if let job { ffi.cancelJob(job: job) }
    }

    private func save() throws {
        try saving.withLock {
            guard !lock.withLock({ ended }) else { return }
            let saved = try core { try ffi.save(library: library.ffi) }
            recordSaved(path: saved.path)
        }
    }

    private func recordSaved(path: String) {
        let (original, owned) = lock.withLock { (self.original, self.owned) }
        library.saved(self, path: path, original: original, owned: owned)
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

// MARK: - Originals

/// What `DocumentSession.import_file` reads: a file in `originals/`, named
/// after the title the user chose (the core titles a document by its file),
/// or pasted text.
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

/// The app's record of which source file each saved document came from,
/// for the Original view: `originals.json` beside the core's folders, with
/// paths relative to the storage root.
final class OriginalsIndex: @unchecked Sendable {
    private struct Entry: Codable {
        var kind: String
        var files: [String]
        /// Intake folders the document owns; older entries have none.
        var folders: [String]?
        /// Find names ran, as #685 recorded it before the core kept it
        /// (#684); `RealLibrary.migrateScanned` hands it to the core once.
        var scanned: Bool?
    }

    private let storage: MdocStorage
    private let lock = NSLock()
    private var url: URL { storage.root.appendingPathComponent("originals.json") }

    init(storage: MdocStorage) {
        self.storage = storage
    }

    /// Moves a received file under its title. Page images go to the core
    /// as they are, together as one document, a single one named after the
    /// title.
    func prepare(_ source: ImportSource, title: String) throws -> PreparedSource {
        switch source {
        case .text(let text):
            return PreparedSource(input: .text(text, .ordinary), kind: .text, original: .none)
        case .newDocument:
            return PreparedSource(input: .text("", .new), kind: .text, original: .none)
        case .sample(let text):
            return PreparedSource(input: .text(text, .sample), kind: .text, original: .none)
        case .file(let url, let kind):
            let named = try rename(url, to: title)
            if kind == .image {
                return PreparedSource(input: .images([named]), kind: kind, original: .images([named]), owned: owned(named))
            }
            let original: OriginalSource = switch kind {
            case .pdf: .pdf(named)
            case .docx: .docx(named)
            default: .none
            }
            return PreparedSource(input: .file(named, needsOcr: false), kind: kind, original: original, owned: owned(named))
        case .scan(let pages), .photos(let pages):
            let kind: SourceKind = if case .scan = source { .scan } else { .image }
            guard let first = pages.first else { throw CoreError.emptySelection }
            let named = pages.count == 1 ? [try rename(first, to: title)] : pages
            return PreparedSource(input: .images(named), kind: kind, original: .images(named), owned: owned(named[0]))
        }
    }

    /// The import's own folder under `originals/`, when it has one.
    private func owned(_ file: URL) -> [URL] {
        let folder = file.deletingLastPathComponent().standardizedFileURL
        let originals = storage.url(.originals).standardizedFileURL
        return folder.deletingLastPathComponent().path == originals.path ? [folder] : []
    }

    private func rename(_ url: URL, to title: String) throws -> URL {
        let target = url.deletingLastPathComponent()
            .appendingPathComponent(MdocStorage.fileName(title, extension: url.pathExtension))
        guard target != url else { return url }
        try? FileManager.default.removeItem(at: target)
        try FileManager.default.moveItem(at: url, to: target)
        return target
    }

    func record(_ original: OriginalSource, owned: [URL], for path: String) {
        let files: [URL]
        let kind: String
        switch original {
        case .none: files = []; kind = "none"
        case .pdf(let url): files = [url]; kind = "pdf"
        case .docx(let url): files = [url]; kind = "docx"
        case .images(let urls): files = urls; kind = "images"
        }
        guard !files.isEmpty || !owned.isEmpty else { return }
        lock.withLock {
            var entries = load()
            let key = relative(path)
            entries[key] = Entry(
                kind: kind, files: files.map { relative($0.path) }, folders: owned.map { relative($0.path) },
                scanned: entries[key]?.scanned)
            save(entries)
        }
    }

    /// The documents #685 recorded as scanned, by `key`.
    func scannedKeys() -> Set<String> {
        lock.withLock { Set(load().filter { $0.value.scanned == true }.keys) }
    }

    /// Forgets #685's scanned records, all but `kept`'s.
    func forgetScanned(except kept: Set<String>) {
        lock.withLock {
            var entries = load()
            guard entries.contains(where: { $0.value.scanned != nil && !kept.contains($0.key) }) else { return }
            for key in entries.keys where !kept.contains(key) {
                entries[key]?.scanned = nil
            }
            // An entry that only kept the record goes with it.
            entries = entries.filter { $0.value.scanned != nil || !$0.value.files.isEmpty || !($0.value.folders ?? []).isEmpty }
            save(entries)
        }
    }

    func original(for path: String) -> (OriginalSource, SourceKind) {
        guard let entry = lock.withLock({ load()[relative(path)] }) else { return (.none, .text) }
        return original(of: entry)
    }

    /// Each document's kind, read once for a whole list.
    func kinds() -> (String) -> SourceKind {
        let entries = lock.withLock { load() }
        return { [self] path in entries[relative(path)].map { original(of: $0).1 } ?? .text }
    }

    private func original(of entry: Entry) -> (OriginalSource, SourceKind) {
        let urls = entry.files.map { storage.root.appendingPathComponent($0) }
        switch entry.kind {
        case "pdf": return (urls.first.map(OriginalSource.pdf) ?? .none, .pdf)
        case "docx": return (urls.first.map(OriginalSource.docx) ?? .none, .docx)
        case "images": return (.images(urls), .scan)
        default:
            // A DOCX imported before the Original pane showed DOCX files was
            // recorded by its folder only.
            let docx = (entry.folders ?? []).lazy.compactMap { folder in
                try? FileManager.default.contentsOfDirectory(
                    at: self.storage.root.appendingPathComponent(folder), includingPropertiesForKeys: nil
                ).first { $0.pathExtension.lowercased() == "docx" }
            }.first
            return docx.map { (.docx($0), .docx) } ?? (.none, .text)
        }
    }

    /// A document's key in the index: its path relative to the root.
    func key(_ path: String) -> String {
        relative(path)
    }

    /// A renamed document keeps its source files.
    func move(from old: String, to new: String) {
        lock.withLock {
            var entries = load()
            guard let entry = entries.removeValue(forKey: relative(old)) else { return }
            entries[relative(new)] = entry
            save(entries)
        }
    }

    func remove(for path: String) {
        lock.withLock {
            var entries = load()
            if let entry = entries.removeValue(forKey: relative(path)) {
                for item in entry.files + (entry.folders ?? []) {
                    try? FileManager.default.removeItem(at: storage.root.appendingPathComponent(item))
                }
            }
            save(entries)
        }
    }

    private func relative(_ path: String) -> String {
        let root = storage.root.standardizedFileURL.path + "/"
        let standard = URL(fileURLWithPath: path).standardizedFileURL.path
        return standard.hasPrefix(root) ? String(standard.dropFirst(root.count)) : standard
    }

    private func load() -> [String: Entry] {
        guard let data = try? Data(contentsOf: url) else { return [:] }
        return (try? JSONDecoder().decode([String: Entry].self, from: data)) ?? [:]
    }

    private func save(_ entries: [String: Entry]) {
        guard let data = try? JSONEncoder().encode(entries) else { return }
        try? data.write(to: url, options: [.atomic, .completeFileProtection])
    }
}

/// Imports chosen but not yet read and saved: `pending.json` beside the
/// core's folders, so an import survives mdoc being stopped (by the user or
/// by iOS) before recognition finishes. Paths are relative to the root.
final class PendingImports: @unchecked Sendable {
    struct Entry {
        var id: DocumentID
        var title: String
        var caseID: String
        var source: PreparedSource
        var ocrStarted = false
    }

    private struct Stored: Codable {
        var title: String
        var caseID: String
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
    private var url: URL { storage.root.appendingPathComponent("pending.json") }

    init(storage: MdocStorage) {
        self.storage = storage
    }

    func add(_ entry: Entry) {
        let source = entry.source
        var stored = Stored(
            title: entry.title, caseID: entry.caseID, kind: source.kind.rawValue, text: nil, file: nil,
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
            stored.originalFiles = [relative(file)]
        case .docx(let file):
            stored.originalKind = "docx"
            stored.originalFiles = [relative(file)]
        case .images(let files):
            stored.originalKind = "images"
            stored.originalFiles = files.map { relative($0) }
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
            case "pdf": files.first.map(OriginalSource.pdf) ?? .none
            case "docx": files.first.map(OriginalSource.docx) ?? .none
            case "images": .images(files)
            default: .none
            }
            let source = PreparedSource(
                input: input, kind: SourceKind(rawValue: stored.kind) ?? .text, original: original,
                owned: stored.owned.map { absolute($0) })
            return Entry(
                id: DocumentID(rawValue: id), title: stored.title, caseID: stored.caseID, source: source,
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

    /// Forgets an import; unless it was saved, its intake files go too.
    func remove(_ id: DocumentID, keepingFiles: Bool = false) {
        lock.withLock {
            var entries = load()
            guard let stored = entries.removeValue(forKey: id.rawValue) else { return }
            if !keepingFiles {
                for folder in stored.owned {
                    try? FileManager.default.removeItem(at: absolute(folder))
                }
            }
            save(entries)
        }
    }

    private func relative(_ url: URL) -> String {
        let root = storage.root.standardizedFileURL.path + "/"
        let path = url.standardizedFileURL.path
        return path.hasPrefix(root) ? String(path.dropFirst(root.count)) : path
    }

    private func absolute(_ path: String) -> URL {
        path.hasPrefix("/") ? URL(fileURLWithPath: path) : storage.root.appendingPathComponent(path)
    }

    private func load() -> [String: Stored] {
        guard let data = try? Data(contentsOf: url) else { return [:] }
        return (try? JSONDecoder().decode([String: Stored].self, from: data)) ?? [:]
    }

    private func save(_ entries: [String: Stored]) {
        guard let data = try? JSONEncoder().encode(entries) else { return }
        try? data.write(to: url, options: [.atomic, .completeFileProtection])
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var excluded = url
        try? excluded.setResourceValues(values)
    }
}
