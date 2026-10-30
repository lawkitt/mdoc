import Foundation

// Value types that cross the core boundary. They mirror the records and
// enums `crates/mdoc-ffi` will export through UniFFI (ADR 0085), so the
// real core can replace `FakeCore` without changing the views. Swift keeps
// only these snapshots and its own view state; the core owns the rest.

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

/// A range of UTF-8 bytes in a document's Markdown source. Every offset the
/// core returns is a source byte offset; `TextOffsets` converts to UTF-16.
struct SourceRange: Hashable, Sendable {
    var start: Int
    var end: Int

    var length: Int { end - start }
}

/// What a replacement stands for, in `mdoc_pii::Category` order.
enum Category: String, CaseIterable, Sendable {
    case person, organization, email, phone, address, date, identity, tax, bank, other

    var label: String {
        switch self {
        case .person: String(localized: "Person")
        case .organization: String(localized: "Organization")
        case .email: String(localized: "Email")
        case .phone: String(localized: "Phone")
        case .address: String(localized: "Address")
        case .date: String(localized: "Date")
        case .identity: String(localized: "Identity")
        case .tax: String(localized: "Tax identifier")
        case .bank: String(localized: "Bank details")
        case .other: String(localized: "Other")
        }
    }

    /// The alias prefix, as in `PERSON_1`.
    var token: String {
        switch self {
        case .person: "PERSON"
        case .organization: "ORG"
        case .email: "EMAIL"
        case .phone: "PHONE"
        case .address: "ADDRESS"
        case .date: "DATE"
        case .identity: "IDENTITY"
        case .tax: "TAX"
        case .bank: "BANK"
        case .other: "REDACTED"
        }
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
}

/// Where a document stands in the review-first flow.
enum DocumentPhase: Hashable, Sendable {
    /// Import or OCR has not finished; finished pages are kept.
    case recognizing
    /// Text is ready; no scan has run yet.
    case unscanned
    /// Proposals wait for a decision.
    case reviewing
    /// Nothing is pending: Copy for AI and Share are open.
    case ready
}

struct DocumentSummary: Identifiable, Hashable, Sendable {
    let id: DocumentID
    var title: String
    var caseID: CaseID?
    var kind: SourceKind
    var phase: DocumentPhase
    var pendingCount: Int
    var updated: Date
}

struct LibrarySnapshot: Sendable, Equatable {
    var cases: [CaseSummary]
    var documents: [DocumentSummary]
    /// The document whose text was shared or copied for AI last. Restore
    /// uses its case unless the user picks another (ADR 0084's rule).
    var lastSharedDocument: DocumentID?

    static let empty = LibrarySnapshot(cases: [], documents: [], lastSharedDocument: nil)

    func documents(in caseID: CaseID) -> [DocumentSummary] {
        documents.filter { $0.caseID == caseID }
    }

    var lastSharedCase: CaseID? {
        guard let id = lastSharedDocument else { return nil }
        return documents.first { $0.id == id }?.caseID
    }
}

/// What the user brought in. Files are already copied into mdoc's storage.
enum ImportSource: Sendable, Equatable {
    case file(URL, SourceKind)
    case scan([URL])
    case photos([URL])
    case text(String)
}

/// Each prepared document starts a new case unless the user adds it to an
/// existing one, so restore always has a mapping (ADR 0085).
enum CaseChoice: Hashable, Sendable {
    case newCase(name: String)
    case existing(CaseID)
}

/// The page the text came from.
enum OriginalSource: Equatable, Sendable {
    case none
    case pdf(URL)
    case images([URL])
}

enum MentionState: Hashable, Sendable {
    case proposed, replaced, kept
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
}

enum Decision: Hashable, Sendable {
    /// Replace with this alias (the proposed one, or one the user typed).
    case replace(alias: String)
    case keep
}

enum Scope: Hashable, Sendable {
    case allOccurrences
    case justThisOne
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

    var text: String {
        segments.map { segment in
            switch segment {
            case .text(let text): text
            case .restored(let value, _): value
            case .unresolved(let token): token
            }
        }.joined()
    }

    var restoredCount: Int {
        segments.filter { if case .restored = $0 { true } else { false } }.count
    }

    var unresolvedCount: Int {
        segments.filter { if case .unresolved = $0 { true } else { false } }.count
    }
}

/// The document's AI reply, kept in alias form with its restored reading.
struct ReplySnapshot: Hashable, Sendable {
    var aliasText: String
    var restored: RestoreResult
}

/// Copy for AI's output: the placeholder note plus the current Markdown,
/// and a file name with known originals pseudonymized.
struct HandOff: Hashable, Sendable {
    var text: String
    var fileName: String
}

struct DocumentSnapshot: Equatable, Sendable {
    let id: DocumentID
    var title: String
    var caseID: CaseID?
    var caseName: String?
    var kind: SourceKind
    var phase: DocumentPhase
    var markdown: String
    var spans: [MarkdownSpan]
    var candidates: [Candidate]
    var identities: [Identity]
    var original: OriginalSource
    /// OCR pages finished and total; equal once recognition is done.
    var pagesDone: Int
    var pageCount: Int
    var reply: ReplySnapshot?

    var pendingCount: Int {
        candidates.filter { $0.state == .proposed }.count
    }

    var replacedCount: Int {
        candidates.filter { $0.state == .replaced }.count
    }

    func candidate(_ id: CandidateID) -> Candidate? {
        candidates.first { $0.id == id }
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
    /// Below the floor: OCR, the structured rules and manual marking.
    case rulesOnly
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

/// A verified download the platform runs in the background; the core checks
/// the file's SHA-256 and installs it.
struct ModelDownloadRequest: Hashable, Sendable {
    var modelID: String
    var url: URL
    var sha256: String
    var byteSize: Int64
}

// MARK: - Errors

enum CoreError: Error, Equatable, Sendable, LocalizedError {
    case cancelled
    case notFound
    case pendingReview(Int)
    case emptySelection
    case noCase
    case modelMissing
    case importFailed(String)

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
            String(localized: "Download the name detector first.")
        case .importFailed(let reason):
            String(localized: "mdoc could not open this file: \(reason)")
        }
    }
}
