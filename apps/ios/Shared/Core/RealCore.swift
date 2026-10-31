import Foundation
import MdocFFI
#if canImport(UIKit)
import PDFKit
import UIKit
#endif

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
        return Core(library: library, setup: setup)
    }

    /// The Restore extension's library: cases and restore, no models.
    static func library(storage: MdocStorage, keys: any CaseKeyStore) throws -> any Library {
        try RealLibrary(storage: storage, keys: keys, setup: nil)
    }
}

// MARK: - Conversions

extension Category {
    init(_ category: MdocFFI.Category) {
        self = switch category {
        case .person: .person
        case .organization: .organization
        case .email: .email
        case .phone: .phone
        case .address: .address
        case .identity: .identity
        case .tax: .tax
        case .bank: .bank
        case .date: .date
        case .other: .other
        }
    }

    var ffi: MdocFFI.Category {
        switch self {
        case .person: .person
        case .organization: .organization
        case .email: .email
        case .phone: .phone
        case .address: .address
        case .identity: .identity
        case .tax: .tax
        case .bank: .bank
        case .date: .date
        case .other: .other
        }
    }
}

extension MentionState {
    init(_ state: MdocFFI.MentionState) {
        self = switch state {
        case .proposed: .proposed
        case .replaced: .replaced
        case .kept: .kept
        }
    }
}

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
            segments.append(mark.restored ? .restored(value: piece, alias: "") : .unresolved(piece))
            cursor = mark.range.upperBound
        }
        if cursor < bytes.count {
            segments.append(.text(String(decoding: bytes[cursor...], as: UTF8.self)))
        }
        self.init(segments: segments, restoredCount: Int(reply.values), unresolvedCount: Int(reply.unrecognised))
    }
}

extension CoreError {
    /// The core's failure in the app's terms.
    init(_ error: any Error) {
        switch CoreFailure(error) {
        case .cancelled: self = .cancelled
        case .needsOcr: self = .modelMissing
        case .failed(let message): self = .importFailed(message)
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

/// Reports the core's job stages to a `JobObserver`.
private final class StageReporter: MdocFFI.JobProgress, @unchecked Sendable {
    let observer: any JobObserver

    init(_ observer: any JobObserver) {
        self.observer = observer
    }

    func stage(stage: JobStage) {
        let mapped: CoreStage = switch stage {
        case .reading: .reading
        case .converting: .converting
        case .recognizing: .recognizing
        case .scanning: .scanning
        }
        observer.progress(JobUpdate(stage: mapped))
    }
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

    init(storage: MdocStorage, keys: any CaseKeyStore, setup: RealSetup?) throws {
        self.storage = storage
        self.setup = setup
        originals = OriginalsIndex(storage: storage)
        pending = PendingImports(storage: storage)
        ffi = try core { try MdocFFI.Library(root: storage.root.path, caseKey: try keys.key()) }
        // Imports not yet read when mdoc last stopped come back, ready to
        // recognize again.
        for entry in pending.all() {
            let session = RealSession(
                id: entry.id, library: self, source: entry.source, title: entry.title, caseID: entry.caseID,
                ocrStarted: entry.ocrStarted)
            sessions[entry.id] = session
        }
    }

    func snapshot() -> LibrarySnapshot {
        let cases = ffi.cases().cases.map { CaseSummary(id: CaseID(rawValue: $0.id), name: $0.name, colour: CaseColour(hue: $0.hue)) }
        let stored = (try? ffi.documents()) ?? []
        var documents: [DocumentSummary] = []
        var seen = Set<DocumentID>()
        for document in stored {
            guard let session = try? session(path: document.path) else { continue }
            seen.insert(session.id)
            documents.append(session.summary(updated: Date(timeIntervalSince1970: TimeInterval(document.modified))))
        }
        let pending = lock.withLock { sessions.values.filter { !seen.contains($0.id) } }
        documents += pending.map { $0.summary(updated: $0.created) }
        return LibrarySnapshot(
            cases: cases,
            documents: documents.sorted { $0.updated > $1.updated },
            currentCase: ffi.currentCase().map { CaseID(rawValue: $0.id) })
    }

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
        let prepared = try originals.prepare(source, title: title)
        let id = DocumentID(rawValue: "new:\(UUID().uuidString)")
        // Kept on disk until its first save, so it survives mdoc stopping.
        pending.add(PendingImports.Entry(id: id, title: title, caseID: caseID, source: prepared))
        let session = RealSession(id: id, library: self, source: prepared, title: title, caseID: caseID)
        lock.withLock { sessions[session.id] = session }
        return session
    }

    func deleteDocument(_ document: DocumentID) throws {
        guard let session = lock.withLock({ sessions[document] }) else { throw CoreError.notFound }
        if let path = session.path {
            // The core takes it out of its case (and the reply kept for it),
            // then removes the file; a case it cannot save keeps it.
            _ = try core { try ffi.deleteDocument(path: path) }
            lock.withLock { _ = ids.removeValue(forKey: path) }
            originals.remove(for: path)
        } else if let source = session.sourceFile {
            // An import not yet read: OCR's kept pages go first, found by the
            // source file's digest, then the file.
            try core { try ffi.discardImport(source: source.path) }
        }
        lock.withLock { _ = sessions.removeValue(forKey: document) }
        pending.remove(document)
    }

    func restore(reply: String, in caseID: CaseID) throws -> RestoreResult {
        RestoreResult(try core { try ffi.restore(caseId: caseID.rawValue, reply: reply) })
    }

    func saveReply(_ reply: String, to document: DocumentID) throws {
        guard let session = try session(for: document) as? RealSession, let path = session.path, let caseID = session.caseID else {
            throw CoreError.noCase
        }
        try core { try ffi.saveReply(caseId: caseID, document: path, reply: reply) }
    }

    /// OCR began on an import: once stopped, it resumes with the pages left.
    fileprivate func ocrStarted(_ session: RealSession) {
        pending.markOCRStarted(session.id)
    }

    /// A saved document keeps its id for the rest of the run.
    fileprivate func saved(_ session: RealSession, path: String, original: OriginalSource, owned: [URL]) {
        lock.withLock { ids[path] = session.id }
        originals.record(original, owned: owned, for: path)
        pending.remove(session.id, keepingFiles: true)
    }

    fileprivate func caseName(_ id: String?) -> String? {
        guard let id else { return nil }
        return try? ffi.caseInfo(id: id).name
    }
}

// MARK: - Session

final class RealSession: DocumentSession, @unchecked Sendable {
    let id: DocumentID
    let created = Date()
    private let ffi = MdocFFI.DocumentSession()
    // Strong: a session keeps its library open while it is used.
    private let library: RealLibrary
    private let lock = NSLock()
    private var source: PreparedSource?
    private var title: String
    private var review: ReviewSnapshot?
    private var scanned = false
    private(set) var caseID: String?
    private(set) var path: String?
    private var kind: SourceKind = .text
    private var original: OriginalSource = .none
    private var owned: [URL] = []
    /// OCR began on this import and has not finished: the core keeps its
    /// finished pages, so recognition resumes with the rest.
    private var ocrStarted: Bool

    fileprivate init(
        id: DocumentID, library: RealLibrary, source: PreparedSource?, title: String, caseID: String?,
        ocrStarted: Bool = false
    ) {
        self.id = id
        self.library = library
        self.source = source
        self.title = title
        self.caseID = caseID
        self.ocrStarted = ocrStarted
        if let source {
            kind = source.kind
            original = source.original
            owned = source.owned
        }
    }

    /// Opens a saved document in its case.
    fileprivate func reopen(path: String, original: (OriginalSource, SourceKind)) throws {
        let opened = try core { try ffi.openDocument(library: library.ffi, path: path) }
        lock.withLock {
            self.path = path
            review = opened
            title = opened.title
            caseID = opened.caseId
            scanned = !opened.mentions.isEmpty
            self.original = original.0
            kind = original.1
        }
    }

    /// The source file of an import not yet read.
    fileprivate var sourceFile: URL? {
        guard case .file(let url, _) = lock.withLock({ source?.input }) else { return nil }
        return url
    }

    fileprivate func summary(updated: Date) -> DocumentSummary {
        let snapshot = snapshot()
        return DocumentSummary(
            id: id, title: snapshot.title, caseID: snapshot.caseID, kind: snapshot.kind,
            phase: snapshot.phase, pendingCount: snapshot.pendingCount, updated: updated,
            candidateCount: snapshot.candidates.count)
    }

    func snapshot() -> DocumentSnapshot {
        let (review, scanned, caseID, path, title, kind, original, ocrStarted) = lock.withLock {
            (self.review, self.scanned, self.caseID, self.path, self.title, self.kind, self.original, self.ocrStarted)
        }
        let reply: ReplySnapshot? = {
            guard let caseID, let path,
                  let text = try? library.ffi.reply(caseId: caseID, document: path),
                  let restored = try? ffi.restoreReply(reply: text) else { return nil }
            return ReplySnapshot(aliasText: text, restored: RestoreResult(restored))
        }()
        guard let review else {
            return DocumentSnapshot(
                id: id, title: title, caseID: caseID.map(CaseID.init), caseName: library.caseName(caseID),
                kind: kind, phase: .recognizing, markdown: "", styling: .empty, candidates: [], identities: [],
                original: original, pagesDone: 0, pageCount: 0, reply: nil, resumesOCR: ocrStarted)
        }
        let candidates = review.mentions.map { mention in
            Candidate(
                id: CandidateID(rawValue: "\(mention.range.start)-\(mention.range.end)"),
                identityID: IdentityID(rawValue: mention.identity.map(String.init) ?? "mention-\(mention.range.start)"),
                range: SourceRange(start: Int(mention.range.start), end: Int(mention.range.end)),
                original: mention.original, alias: mention.alias ?? "",
                category: Category(mention.category), state: MentionState(mention.state))
        }
        let identities = review.identities.map { row in
            let id = IdentityID(rawValue: String(row.id))
            let states = Set(candidates.filter { $0.identityID == id }.map(\.state))
            let state: MentionState = row.proposed > 0 ? .proposed : (states == [.kept] ? .kept : .replaced)
            return Identity(
                id: id, alias: row.alias, category: Category(row.category),
                primaryWording: row.originals.first ?? row.alias, variants: row.originals,
                mentionCount: max(Int(row.mentions), candidates.filter { $0.identityID == id }.count), state: state)
        }
        let phase: DocumentPhase = if !scanned {
            .unscanned
        } else if review.proposed > 0 {
            .reviewing
        } else {
            .ready
        }
        return DocumentSnapshot(
            id: id, title: review.title, caseID: (review.caseId ?? caseID).map(CaseID.init),
            caseName: library.caseName(review.caseId ?? caseID), kind: kind, phase: phase,
            markdown: review.text, styling: MarkdownStyling(markdown: review.text),
            candidates: candidates, identities: identities, original: original,
            pagesDone: 0, pageCount: 0, reply: reply, warning: review.warning)
    }

    func recognize(observer: any JobObserver) throws -> DocumentSnapshot {
        guard let source = lock.withLock({ self.source }) else { return snapshot() }
        observer.onCancel { [ffi] in ffi.cancel() }
        let reporter = StageReporter(observer)
        observer.progress(JobUpdate(stage: .reading))
        let imported: ReviewSnapshot
        switch source.input {
        case .text(let text):
            imported = try core { try ffi.importText(text: text, title: lock.withLock { title }) }
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
            imported = try core {
                try ffi.importFileIn(
                    library: library.ffi, path: url.path, options: ImportOptions(ocr: ocr, nativeTextOnly: false),
                    progress: reporter)
            }
        }
        let caseID = lock.withLock { self.caseID }
        if let caseID {
            _ = try core { try ffi.joinCase(library: library.ffi, caseId: caseID) }
        }
        lock.withLock {
            review = imported
            self.source = nil
            ocrStarted = false
        }
        try save()
        return snapshot()
    }

    func scan(observer: any JobObserver) throws -> DocumentSnapshot {
        guard let setup = library.setup else { throw CoreError.modelMissing }
        let detector: DetectorPaths?
        switch setup.snapshot().tier {
        case .detector:
            guard let paths = setup.detectorPaths() else { throw CoreError.modelMissing }
            detector = paths
        case .rulesOnly:
            detector = nil
        }
        observer.onCancel { [ffi] in ffi.cancel() }
        let scanned = try core { try ffi.scan(detector: detector, progress: StageReporter(observer)) }
        lock.withLock {
            review = scanned
            self.scanned = true
        }
        try save()
        return snapshot()
    }

    func decide(_ candidate: CandidateID, _ decision: Decision, scope: Scope) throws -> DocumentSnapshot {
        guard let mention = snapshot().candidate(candidate) else { throw CoreError.notFound }
        let range = TextRange(start: UInt64(mention.range.start), end: UInt64(mention.range.end))
        let reach: MdocFFI.Scope = scope == .allOccurrences ? .all : .this
        return try change {
            switch decision {
            case .replace: try ffi.replace(range: range, scope: reach)
            case .keep: try ffi.keep(range: range, scope: reach)
            }
        }
    }

    func replaceAllPending() throws -> DocumentSnapshot {
        try change { try ffi.replaceAll() }
    }

    func addReplacement(_ range: SourceRange, category: Category) throws -> DocumentSnapshot {
        guard range.length > 0 else { throw CoreError.emptySelection }
        let selection = TextRange(start: UInt64(range.start), end: UInt64(range.end))
        return try change {
            let proposed = try ffi.addSelection(range: selection, category: category.ffi)
            guard let mention = proposed.mentions.first(where: { $0.range.start <= selection.start && $0.range.end >= selection.end }) else {
                return proposed
            }
            return try ffi.replace(range: mention.range, scope: .all)
        }
    }

    func copyForAI() throws -> HandOff {
        let current = snapshot()
        guard current.pendingCount == 0 else { throw CoreError.pendingReview(current.pendingCount) }
        let text = try core { try ffi.copyForAi() }
        let fileName = try core { try ffi.shareFileName() }
        if let caseID {
            try core { try library.ffi.noteShared(caseId: caseID) }
        }
        return HandOff(text: text, fileName: fileName)
    }

    func restoreReply(_ reply: String) throws -> DocumentSnapshot {
        guard let caseID, let path else { throw CoreError.noCase }
        _ = try core { try ffi.restoreReply(reply: reply) }
        try core { try library.ffi.saveReply(caseId: caseID, document: path, reply: reply) }
        return snapshot()
    }

    func clearReply() throws -> DocumentSnapshot {
        guard let caseID, let path else { throw CoreError.noCase }
        try core { try library.ffi.saveReply(caseId: caseID, document: path, reply: "") }
        return snapshot()
    }

    /// A review transition, then the document and its mapping saved into
    /// its case, as a case member autosaves on desktop.
    private func change(_ transition: () throws -> ReviewSnapshot) throws -> DocumentSnapshot {
        let changed = try core(transition)
        lock.withLock { review = changed }
        try save()
        return snapshot()
    }

    private func save() throws {
        let saved = try core { try ffi.save(library: library.ffi) }
        let (original, owned) = lock.withLock {
            path = saved.path
            caseID = saved.case.id
            review = saved.review
            return (self.original, self.owned)
        }
        library.saved(self, path: saved.path, original: original, owned: owned)
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
        let tier: DeviceTier = ffi.offers(model: .gliner2Fp16) ? .detector : .rulesOnly
        let offers = catalog
            .filter { $0.offered && ($0.id == .gliner2Fp16 || $0.id == Self.ocrModel) }
            .map { info in
                ModelOffer(
                    id: Self.key(info.id), name: info.title.isEmpty ? info.name : "\(info.name) (\(info.title))",
                    purpose: info.id == .gliner2Fp16 ? .detector : .ocr,
                    byteSize: Int64(info.installed ? info.bytes : info.missingBytes),
                    state: info.installed ? .installed : .notInstalled)
            }
        return SetupSnapshot(physicalMemory: physicalMemory, tier: tier, offers: offers)
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
        case text(String)
        /// `needsOcr`: page images, which only OCR can read.
        case file(URL, needsOcr: Bool)
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
    }

    private let storage: MdocStorage
    private let lock = NSLock()
    private var url: URL { storage.root.appendingPathComponent("originals.json") }

    init(storage: MdocStorage) {
        self.storage = storage
    }

    /// Moves a received file under its title and, for page images, builds
    /// the one file the core imports: the JPEG itself, or a PDF of the pages.
    func prepare(_ source: ImportSource, title: String) throws -> PreparedSource {
        switch source {
        case .text(let text):
            return PreparedSource(input: .text(text), kind: .text, original: .none)
        case .file(let url, let kind):
            let named = try rename(url, to: title)
            let original: OriginalSource = kind == .pdf ? .pdf(named) : (kind == .image ? .images([named]) : .none)
            return PreparedSource(
                input: .file(named, needsOcr: kind == .image), kind: kind, original: original,
                owned: owned(named))
        case .scan(let pages), .photos(let pages):
            let kind: SourceKind = if case .scan = source { .scan } else { .image }
            guard let first = pages.first else { throw CoreError.emptySelection }
            if pages.count == 1 {
                let named = try rename(first, to: title)
                return PreparedSource(
                    input: .file(named, needsOcr: true), kind: kind, original: .images([named]), owned: owned(named))
            }
            let pdf = try pagesPDF(pages, title: title, folder: first.deletingLastPathComponent())
            return PreparedSource(
                input: .file(pdf, needsOcr: true), kind: kind, original: .images(pages), owned: owned(pdf))
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

    /// Several scanned pages as one image-only PDF, which the core reads
    /// with OCR as a scanned document.
    private func pagesPDF(_ pages: [URL], title: String, folder: URL) throws -> URL {
        #if canImport(UIKit)
        let document = PDFDocument()
        for (index, url) in pages.enumerated() {
            guard let image = UIImage(contentsOfFile: url.path), let page = PDFPage(image: image) else { continue }
            document.insert(page, at: index)
        }
        let target = folder.appendingPathComponent(MdocStorage.fileName(title, extension: "pdf"))
        guard document.pageCount > 0, document.write(to: target) else {
            throw CoreError.importFailed(String(localized: "The scanned pages could not be read."))
        }
        return target
        #else
        throw CoreError.importFailed("PDFKit is unavailable.")
        #endif
    }

    func record(_ original: OriginalSource, owned: [URL], for path: String) {
        let files: [URL]
        let kind: String
        switch original {
        case .none: files = []; kind = "none"
        case .pdf(let url): files = [url]; kind = "pdf"
        case .images(let urls): files = urls; kind = "images"
        }
        guard !files.isEmpty || !owned.isEmpty else { return }
        lock.withLock {
            var entries = load()
            entries[relative(path)] = Entry(
                kind: kind, files: files.map { relative($0.path) }, folders: owned.map { relative($0.path) })
            save(entries)
        }
    }

    func original(for path: String) -> (OriginalSource, SourceKind) {
        guard let entry = lock.withLock({ load()[relative(path)] }) else { return (.none, .text) }
        let urls = entry.files.map { storage.root.appendingPathComponent($0) }
        switch entry.kind {
        case "pdf": return (urls.first.map(OriginalSource.pdf) ?? .none, .pdf)
        case "images": return (.images(urls), .scan)
        default: return (.none, .text)
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
        var needsOcr: Bool
        var originalKind: String
        var originalFiles: [String]
        var owned: [String]
        /// OCR began: the core keeps its finished pages. Older entries have none.
        var ocrStarted: Bool?
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
        case .text(let text): stored.text = text
        case .file(let file, let needsOcr):
            stored.file = relative(file)
            stored.needsOcr = needsOcr
        }
        switch source.original {
        case .none: break
        case .pdf(let file):
            stored.originalKind = "pdf"
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
                input = .text(text)
            } else if let file = stored.file {
                let url = absolute(file)
                // A source file that is gone cannot be read again.
                guard FileManager.default.fileExists(atPath: url.path) else { return nil }
                input = .file(url, needsOcr: stored.needsOcr)
            } else {
                return nil
            }
            let files = stored.originalFiles.map { absolute($0) }
            let original: OriginalSource = switch stored.originalKind {
            case "pdf": files.first.map(OriginalSource.pdf) ?? .none
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
