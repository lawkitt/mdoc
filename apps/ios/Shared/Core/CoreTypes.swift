import Foundation
import MdocFFI

// The app's ids and the snapshots its views show (ADR 0085). The core's own
// records and enums (`Category`, `MentionState`, `Scope`, `DocumentPhase`,
// `HandOff`, the Markdown styling, …) are used as UniFFI generates them
// (ADR 0104); their labels and icons are extensions. Swift keeps only these
// snapshots and its own view state; the core owns the rest.

/// The core's `Category`, named here because the Objective-C runtime has a
/// `Category` too.
typealias Category = MdocFFI.Category

struct ProjectID: Hashable, Sendable, Codable {
    let rawValue: String
}

struct DocumentID: Hashable, Sendable, Codable {
    let rawValue: String
}

struct CandidateID: Hashable, Sendable {
    let rawValue: String
}

struct IdentityID: Hashable, Sendable {
    let rawValue: String
}

extension IdentityID {
    /// The core's identity id.
    init(_ id: UInt64) {
        self.init(rawValue: String(id))
    }
}

/// A merge hint (ADR 0080): "Looks like `wording`". Linking them is the
/// user's choice; × hides the hint.
extension LinkHint {
    /// The shorter or later identity, which Same as links…
    var fromID: IdentityID { IdentityID(from) }
    /// …into this one, which keeps its alias and restore wording.
    var intoID: IdentityID { IdentityID(into) }
}

/// A range of UTF-8 bytes in a document's Markdown source. Every offset the
/// core returns is a source byte offset; `TextOffsets` converts to UTF-16.
struct SourceRange: Hashable, Sendable {
    var start: Int
    var end: Int

    var length: Int { end - start }
}

extension SourceRange {
    /// The core's range.
    init(_ range: TextRange) {
        self.init(start: Int(range.start), end: Int(range.end))
    }

    var textRange: TextRange {
        TextRange(start: UInt64(start), end: UInt64(end))
    }
}

/// The eight named case swatches (ADR 0077).
enum ProjectColour: String, CaseIterable, Sendable {
    case red, orange, amber, green, teal, blue, violet, pink
}

struct ProjectSummary: Identifiable, Hashable, Sendable {
    let id: ProjectID
    var name: String
    var colour: ProjectColour
}

enum SourceKind: String, Sendable {
    case pdf, docx, image, scan, text

    /// Scans and photos are read by text recognition; an interrupted import
    /// of anything else reads as an import not finished (ADR 0088).
    var needsRecognition: Bool { self == .scan || self == .image }
}

/// A document as the library lists it: a saved one from the core's
/// `DocumentSummary`, or an import the core has not read yet, with the id
/// this run gave it and its source's kind, which the core does not keep.
struct LibraryDocument: Identifiable, Hashable, Sendable {
    let id: DocumentID
    var title: String
    var projectID: ProjectID?
    var kind: SourceKind
    /// Nil while the import or its text recognition has not finished.
    var phase: DocumentPhase?
    var pendingCount: Int
    var updated: Date
    /// Every candidate, decided or not: none after a scan is "Nothing found".
    var candidateCount = 0
    /// How it began: Try a sample finds the sample by it (ADR 0109).
    var origin: DocumentOrigin = .ordinary
}

struct LibrarySnapshot: Sendable, Equatable {
    /// Pinned first, then most recently updated (#1091 Decision 28).
    var projects: [ProjectSummary]
    var documents: [LibraryDocument]
    /// Each Project's saved AI replies, newest first: its own and those of
    /// the documents in it now, for its page's Replies.
    var projectReplies: [ProjectID: [SavedReply]] = [:]

    static let empty = LibrarySnapshot(projects: [], documents: [])

    /// The Project's documents, most recently changed first.
    func documents(in projectID: ProjectID) -> [LibraryDocument] {
        documents.filter { $0.projectID == projectID }.sorted { $0.updated > $1.updated }
    }
}

/// One of a Project's names on its page: its alias, its category and the
/// wordings it stands for, the one restore puts back first.
struct ProjectNameRow: Identifiable, Hashable, Sendable {
    var alias: String
    var category: Category
    var originals: [String]
    var id: String { alias }
}

/// A wording on a Project's Always list, in the category scans propose it
/// in.
struct AlwaysWording: Hashable, Sendable {
    var wording: String
    var category: Category
}

/// What a Project's page shows beside its documents and replies: its names
/// and its Always and Never lists.
struct ProjectDetails: Equatable, Sendable {
    var names: [ProjectNameRow]
    var always: [AlwaysWording]
    var never: [String]

    static let empty = ProjectDetails(names: [], always: [], never: [])
}

/// What deleting a Project that has documents does with them (#1091
/// Decision 9): the app asks.
enum ProjectDeletion: Hashable, Sendable {
    /// Each goes to Unfiled with its names and its replies; the Project's
    /// own AI replies go with the Project.
    case moveToUnfiled
    /// They are deleted with it, and their replies.
    case deleteDocuments
}

/// What the user brought in. Files are already copied into mdoc's storage.
enum ImportSource: Sendable, Equatable {
    case file(URL, SourceKind)
    case scan([URL])
    case photos([URL])
    case text(String)
    /// New document: empty, named after its first line until renamed
    /// (ADR 0109).
    case newDocument
    /// Try a sample's text: the document Try a sample restores.
    case sample(String)
}

/// Move to Project's target: a Project, or a new one named so. Intake
/// takes a `ProjectID?` instead: Unfiled or the Project it was started
/// from, and no import makes a Project (#1091 Decision 35).
enum ProjectChoice: Hashable, Sendable {
    case newProject(name: String)
    case existing(ProjectID)
}

/// Where Move to Project took a document, for "Moved to <Project> · Undo".
struct ProjectMove: Hashable, Sendable {
    var document: DocumentID
    /// The Project it is in now; nil: Unfiled.
    var to: ProjectSummary?
    /// The Project it left, which keeps its names; nil: it was Unfiled.
    var from: ProjectID?
    /// The Vault's handle for Undo, good while the app runs.
    var undo: UInt64

    /// Where it is now, as the toast names it.
    var placeName: String {
        to?.name ?? String(localized: "Unfiled")
    }
}

/// The page the text came from.
enum OriginalSource: Equatable, Sendable {
    case none
    case pdf(OriginalBytes)
    /// Rendered in process while the Original pane shows it.
    case docx(OriginalBytes)
    case images([OriginalBytes])
}

extension OriginalSource {
    /// A Vault original's decrypted bytes go; received files stay.
    func close() {
        switch self {
        case .pdf(.stored(let stored)), .docx(.stored(let stored)): stored.close()
        case .images(let pages):
            for case .stored(let stored) in pages { stored.close() }
        default: break
        }
    }
}

/// An original's bytes as the Original pane reads them (#1091 Decision 24):
/// a file (an import's received file, or a viewing copy in the app's
/// private cache) or, under about 16 MiB, in memory.
enum OriginalBytes: Hashable, Sendable {
    case file(URL)
    case memory(OriginalData)
    /// A Vault document's original, decrypted when the pane first reads it.
    case stored(StoredOriginal)

    /// The file, when there is one.
    var fileURL: URL? {
        if case .file(let url) = self { url } else { nil }
    }

    /// A file or bytes in memory: a stored original decrypted now. Off the
    /// main thread for a large one.
    func resolved() throws -> OriginalBytes {
        if case .stored(let original) = self { try original.load() } else { self }
    }

    /// A stored original's decrypted bytes and viewing copy go: the view
    /// that read them went before they came (review 1).
    func closeIfStored() {
        if case .stored(let original) = self { original.close() }
    }

    func data() throws -> Data {
        switch try resolved() {
        case .file(let url): try Data(contentsOf: url, options: .mappedIfSafe)
        case .memory(let data): data.bytes
        case .stored: throw CoreError.notFound
        }
    }
}

/// A Vault document's original for the Original pane (#1091 Decision 24):
/// decrypted once, on first use, into memory when small or into the app's
/// private viewing cache, and let go when the document closes.
final class StoredOriginal: Hashable, @unchecked Sendable {
    private let lock = NSLock()
    private let decrypt: @Sendable () throws -> OriginalBytes
    private let remove: @Sendable () -> Void
    private var loaded: OriginalBytes?
    /// Bumped by each close: a decrypt that started before it throws its
    /// result away (review 1).
    private var generation = 0

    init(decrypt: @escaping @Sendable () throws -> OriginalBytes, remove: @escaping @Sendable () -> Void) {
        self.decrypt = decrypt
        self.remove = remove
    }

    /// The bytes, decrypted the first time; never `.stored`. The decrypt
    /// runs outside the lock, so a close never waits for it; one the
    /// document closed during leaves nothing behind and throws
    /// `CoreError.cancelled`.
    func load() throws -> OriginalBytes {
        let (cached, started) = lock.withLock { (loaded, generation) }
        if let cached { return cached }
        let bytes = try decrypt()
        let kept = lock.withLock { () -> OriginalBytes? in
            guard generation == started else { return nil }
            if let loaded { return loaded }
            loaded = bytes
            return bytes
        }
        guard let kept else {
            remove()
            throw CoreError.cancelled
        }
        return kept
    }

    /// The document closed: the decrypted bytes go, and the viewing copy
    /// with them, also one a decrypt running now writes. The next view
    /// decrypts again.
    func close() {
        lock.withLock {
            generation += 1
            loaded = nil
        }
        remove()
    }

    static func == (lhs: StoredOriginal, rhs: StoredOriginal) -> Bool { lhs === rhs }

    func hash(into hasher: inout Hasher) {
        hasher.combine(ObjectIdentifier(self))
    }
}

/// Bytes held in memory, compared by identity: snapshots compare their
/// originals often, and never need to compare the bytes.
final class OriginalData: Hashable, @unchecked Sendable {
    let bytes: Data

    init(_ bytes: Data) {
        self.bytes = bytes
    }

    static func == (lhs: OriginalData, rhs: OriginalData) -> Bool { lhs === rhs }

    func hash(into hasher: inout Hasher) {
        hasher.combine(ObjectIdentifier(self))
    }
}

/// One underlined mention in the text.
struct Candidate: Identifiable, Hashable, Sendable {
    let id: CandidateID
    var identityID: IdentityID
    /// Where it sits in the current Markdown.
    var range: SourceRange
    /// The wording in the source document.
    var original: String
    var alias: String
    var category: Category
    var state: MentionState
}

struct Identity: Identifiable, Hashable, Sendable {
    let id: IdentityID
    var alias: String
    var category: Category
    /// The wording a restored reply uses.
    var primaryWording: String
    var variants: [String]
    var mentionCount: Int
    /// Proposed while any mention is; replaced or kept once all agree.
    var state: MentionState
    /// Belongs to…: the person or organization a contact or identifier
    /// belongs to.
    var owner: IdentityID? = nil
}

/// The candidate sheet's choice. Aliases come from the core and are not
/// edited (ADR 0085's review sheet shows the alias).
enum Decision: Hashable, Sendable {
    case replace
    case keep
}

/// A restored reply split for display: plain text, values put back, and
/// alias-shaped tokens no identity owns (never guessed).
enum RestoredSegment: Hashable, Sendable {
    case text(String)
    case restored(String)
    case unresolved(String)
}

struct RestoreResult: Hashable, Sendable {
    var segments: [RestoredSegment]
    /// Distinct identities restored and distinct unrecognized tokens, as
    /// "N values restored · M unrecognized" counts them (ADR 0078).
    var restoredCount: Int
    var unresolvedCount: Int

    /// Counts given by the core.
    init(segments: [RestoredSegment], restoredCount: Int, unresolvedCount: Int) {
        self.segments = segments
        self.restoredCount = restoredCount
        self.unresolvedCount = unresolvedCount
    }

    /// Counts distinct restored values and distinct tokens in `segments`.
    init(segments: [RestoredSegment]) {
        var values = Set<String>()
        var tokens = Set<String>()
        for segment in segments {
            switch segment {
            case .restored(let value): values.insert(value)
            case .unresolved(let token): tokens.insert(token)
            case .text: break
            }
        }
        self.init(segments: segments, restoredCount: values.count, unresolvedCount: tokens.count)
    }

    var text: String {
        segments.map { segment in
            switch segment {
            case .text(let text): text
            case .restored(let value): value
            case .unresolved(let token): token
            }
        }.joined()
    }
}

struct ReplyID: Hashable, Sendable {
    let rawValue: String
}

/// An AI reply saved in the Vault (#1091 Decision 18), as lists show it:
/// no text.
struct SavedReply: Identifiable, Hashable, Sendable {
    let id: ReplyID
    var title: String
    /// The app it came from, when known.
    var sourceApp: String?
    var created: Date
}

/// The document's AI reply shown in its Reply pane, kept in alias form with
/// its restored reading. `id` is the saved reply's.
struct ReplySnapshot: Hashable, Sendable {
    var id: ReplyID? = nil
    var aliasText: String
    var restored: RestoreResult
}

/// Where the Restore extension restores a reply and saves it (#1091, the
/// maintainer's (a)): a Project, or an Unfiled document, which restores with
/// its own names.
enum RestorePlace: Hashable, Sendable {
    case project(ProjectID)
    case document(DocumentID)
}

/// Mentions by state, as the core counts them.
struct MentionCounts: Hashable, Sendable {
    var proposed: Int
    var replaced: Int
    var kept: Int
}

struct DocumentSnapshot: Equatable, Sendable {
    let id: DocumentID
    var title: String
    var projectID: ProjectID?
    var projectName: String?
    var kind: SourceKind
    /// Nil while the import or its text recognition has not finished:
    /// finished pages are kept.
    var phase: DocumentPhase?
    var markdown: String
    var styling: MarkdownStyling
    /// The core's `text_revision`: the same while `markdown` is, so the
    /// text view lays the text out again only when it changes. Nil for a
    /// snapshot the core did not make.
    var textRevision: UInt64? = nil
    var candidates: [Candidate]
    var identities: [Identity]
    var original: OriginalSource
    /// The reply the Reply pane shows: the one picked, else the newest of
    /// `replies`.
    var reply: ReplySnapshot?
    /// The saved replies that restore with this document's names, newest
    /// first: its Project's while it is in one, else its own (#1091
    /// Decision 4). Today's Reply pane lists them.
    var replies: [SavedReply] = []
    /// A partial import (pages OCR skipped or may have misread): the banner
    /// over the text.
    var notice: String? = nil
    /// What the Markdown leaves out that no view shows, as counts: the
    /// quiet line under the title, never orange.
    var leftOut: String? = nil
    /// OCR stopped part-way (a cancel, or mdoc stopping): its finished pages
    /// are kept and recognition resumes with the rest.
    var resumesOCR = false
    /// Merge hints (ADR 0080), without the dismissed ones.
    var mergeHints: [LinkHint] = []
    var canUndo = false
    var canRedo = false
    /// What Undo would revert; after a redo, what it made again.
    var undoStep: HistoryStep? = nil
    /// What Redo would make again; after an undo, what it undid.
    var redoStep: HistoryStep? = nil

    /// The core's count of mentions in each state; nil for a snapshot the
    /// core did not make, which counts `candidates`.
    var counts: MentionCounts? = nil
    /// How it began: a new document follows its first line and is
    /// discarded when left empty; the sample restores (ADR 0109).
    var origin: DocumentOrigin = .ordinary

    var pendingCount: Int {
        counts?.proposed ?? candidates.filter { $0.state == .proposed }.count
    }

    var replacedCount: Int {
        counts?.replaced ?? candidates.filter { $0.state == .replaced }.count
    }

    func candidate(_ id: CandidateID) -> Candidate? {
        candidates.first { $0.id == id }
    }

    /// The candidate an undo or redo step acted on, when one is at its range.
    func candidate(of step: HistoryStep?) -> Candidate? {
        guard let range = step?.range.map(SourceRange.init) else { return nil }
        return candidates.first { $0.range == range }
    }

    func mentions(of identity: IdentityID) -> [Candidate] {
        candidates.filter { $0.identityID == identity }
    }
}

/// Fix the words' draft (ADR 0114): the document as it shows while a
/// name's words are fixed, where its handles start and how far they reach.
struct WordsDraft: Equatable, Sendable {
    var snapshot: DocumentSnapshot
    /// The name's words in `snapshot.markdown`.
    var wording: SourceRange
    /// The paragraph or cell around them, up to Markdown formatting or
    /// another name's alias: the handles stay inside.
    var bounds: SourceRange
}

// MARK: - Setup

enum ModelPurpose: Hashable, Sendable {
    case detector
    case ocr
}

enum ModelState: Hashable, Sendable {
    case notInstalled
    case downloading(fraction: Double)
    /// Held by the Wi-Fi-only session on cellular or offline (ADR 0088).
    case waitingForWiFi
    case verifying
    case installed
    case failed(String)
}

struct ModelOffer: Identifiable, Hashable, Sendable {
    let id: String
    var name: String
    var purpose: ModelPurpose
    var byteSize: Int64
    var state: ModelState
}

/// What the device can run, decided by physical RAM and the detector's
/// timed run rather than OS version.
enum DeviceTier: Hashable, Sendable {
    /// The GLiNER2 detector fits.
    case detector
    /// Below the floor: OCR and manual marking; the app finds no names
    /// (ADR 0088).
    case belowFloor
}

struct SetupSnapshot: Hashable, Sendable {
    var physicalMemory: UInt64
    var tier: DeviceTier
    var offers: [ModelOffer]
    /// Why the device is below the floor: too little memory, or too slow.
    var belowFloor: FloorReason? = nil
    /// Seconds 10 pages take on this device, once the detector was timed.
    var tenPagesSeconds: UInt64? = nil

    var missing: [ModelOffer] {
        offers.filter { $0.state != .installed }
    }

    var needsSetup: Bool { !missing.isEmpty }

    var missingBytes: Int64 {
        missing.reduce(0) { $0 + $1.byteSize }
    }
}

/// One pinned file of a model: the platform downloads it in the background,
/// the core checks its size and SHA-256 and installs it.
struct ModelDownloadRequest: Hashable, Sendable {
    var modelID: String
    /// The file's path inside the model's folder.
    var file: String
    var url: URL
    var byteSize: Int64
}

/// One progress report: the stage, and how far when the core knows.
struct JobUpdate: Hashable, Sendable {
    var stage: JobStage
    var fraction: Double?
    var page: Int?
    var pageCount: Int?
    /// A scan's estimate of the seconds it has left.
    var secondsLeft: UInt64?
}

// MARK: - Errors

/// Why a call failed, each with its own message (ADR 0104). The core's
/// failures map one to one from its `CoreError` and `ReviewError`
/// (`CoreFailure`); the rest are the app's own.
enum CoreError: Error, Equatable, Sendable, LocalizedError {
    case cancelled
    /// The document is no longer in the library.
    case notFound
    /// Copy for AI and Share wait until every proposal is replaced or kept.
    case pendingReview(Int)
    case emptySelection
    case noProject
    /// Scanning needs the detector, or recognition needs OCR, and it is not
    /// installed.
    case modelMissing
    /// Reading or converting the file.
    case importFailed(String)
    /// A locked DOCX or PDF: its password is needed, or the one given was wrong.
    case needsPassword
    case wrongPassword
    /// Text recognition failed.
    case recognitionFailed(String)
    /// Finding names failed.
    case findNamesFailed(String)
    /// A review change no longer applies: the text or the mention changed.
    case reviewChanged(String)
    /// A case rule (one case per document), or the case could not be read.
    case projectRefused(String)
    /// A model file could not be verified or installed.
    case setupFailed(String)
    /// Something the core does not accept, such as a blank title.
    case invalid(String)
    /// Reading or writing a file failed.
    case fileFailed(String)
    /// Rename: another identity has this alias. Same as is the way to it.
    case aliasTaken(alias: String, by: IdentityID)
    /// The mention or identity is no longer in the review.
    case nameNotFound
    /// A change refused, in the core's words or the app's.
    case refused(String)
    /// A review change could not be saved.
    case saveFailed(String)
    /// Anything else, in its own words.
    case other(String)

    var errorDescription: String? {
        switch self {
        case .cancelled:
            String(localized: "Stopped.")
        case .notFound:
            String(localized: "This document is no longer in mdoc.")
        case .pendingReview(let count):
            String(localized: "Review \(count) proposals before sharing.")
        case .emptySelection:
            String(localized: "Select the words to replace first.")
        case .noProject:
            String(localized: "Share a document from mdoc first; replies restore with its Project.")
        case .modelMissing:
            String(localized: "Download the models first.")
        case .importFailed(let reason):
            String(localized: "mdoc could not open this file: \(reason)")
        case .needsPassword:
            String(localized: "This document is password-protected")
        case .wrongPassword:
            String(localized: "The password is incorrect. Check it and try again.")
        case .recognitionFailed(let reason):
            String(localized: "Text recognition stopped: \(reason)")
        case .findNamesFailed(let reason):
            String(localized: "Find names stopped: \(reason)")
        case .setupFailed(let reason):
            String(localized: "The model could not be installed: \(reason)")
        case .fileFailed(let reason):
            String(localized: "mdoc could not read or write a file: \(reason)")
        case .saveFailed(let reason):
            String(localized: "mdoc could not save this change: \(reason)")
        case .aliasTaken(let alias, _):
            String(localized: "\(alias) is another name's alias.")
        case .nameNotFound:
            String(localized: "That name is no longer in the document.")
        // Sentences of the core's (a blank title, the case a document is
        // in already, a mention that moved) or of the app's, as they are.
        case .reviewChanged(let reason), .projectRefused(let reason), .invalid(let reason), .refused(let reason),
             .other(let reason):
            reason
        }
    }
}
