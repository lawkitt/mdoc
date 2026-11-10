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

struct CaseID: Hashable, Sendable, Codable {
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
enum CaseColour: String, CaseIterable, Sendable {
    case red, orange, amber, green, teal, blue, violet, pink
}

struct CaseSummary: Identifiable, Hashable, Sendable {
    let id: CaseID
    var name: String
    var colour: CaseColour
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
    var caseID: CaseID?
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
    var cases: [CaseSummary]
    var documents: [LibraryDocument]
    /// The case of the document shared or copied for AI last. Restore uses
    /// it unless the user picks another (ADR 0084's current case).
    var currentCase: CaseID?

    static let empty = LibrarySnapshot(cases: [], documents: [], currentCase: nil)

    /// The case's documents, most recently changed first.
    func documents(in caseID: CaseID) -> [LibraryDocument] {
        documents.filter { $0.caseID == caseID }.sorted { $0.updated > $1.updated }
    }
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

/// Each prepared document starts a new case unless the user adds it to an
/// existing one, so restore always has a mapping (ADR 0085).
enum CaseChoice: Hashable, Sendable {
    case newCase(name: String)
    case existing(CaseID)
}

/// Where Move to case took a document, for "Moved to <case> · Undo".
struct CaseMove: Hashable, Sendable {
    var document: DocumentID
    /// The case it is in now.
    var to: CaseSummary
    /// The case it left, if it was in one; it stays, even empty. Undo moves
    /// the document back there.
    var from: CaseID?
    /// Aliases of the names the move added to `to`; Undo takes them out
    /// again where nothing else uses them.
    var added: [String]
}

/// The page the text came from.
enum OriginalSource: Equatable, Sendable {
    case none
    case pdf(URL)
    case images([URL])
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
    case restored(value: String, alias: String)
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
            case .restored(let value, _): values.insert(value)
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
            case .restored(let value, _): value
            case .unresolved(let token): token
            }
        }.joined()
    }
}

/// The document's AI reply, kept in alias form with its restored reading.
struct ReplySnapshot: Hashable, Sendable {
    var aliasText: String
    var restored: RestoreResult
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
    var caseID: CaseID?
    var caseName: String?
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
    /// OCR pages finished and total; equal once recognition is done.
    var pagesDone: Int
    var pageCount: Int
    var reply: ReplySnapshot?
    /// The import's partial-result warning (pages OCR could not read, …),
    /// shown with the text.
    var warning: String? = nil
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

/// What the device can run, decided by physical RAM rather than OS version.
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
    case noCase
    /// Scanning needs the detector, or recognition needs OCR, and it is not
    /// installed.
    case modelMissing
    /// Reading or converting the file.
    case importFailed(String)
    /// Text recognition failed.
    case recognitionFailed(String)
    /// Finding names failed.
    case findNamesFailed(String)
    /// A review change no longer applies: the text or the mention changed.
    case reviewChanged(String)
    /// A case rule (one case per document), or the case could not be read.
    case caseRefused(String)
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
        case .noCase:
            String(localized: "Share a document from mdoc first; replies restore with its case.")
        case .modelMissing:
            String(localized: "Download the models first.")
        case .importFailed(let reason):
            String(localized: "mdoc could not open this file: \(reason)")
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
        case .reviewChanged(let reason), .caseRefused(let reason), .invalid(let reason), .refused(let reason),
             .other(let reason):
            reason
        }
    }
}
