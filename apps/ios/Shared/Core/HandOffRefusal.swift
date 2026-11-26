import Foundation

/// Whether a saved document may leave for an AI now, as the Vault checks it
/// against its Project's names (#1091 Decision 19), in the app's terms.
enum SavedReadiness: Equatable, Sendable {
    case ready
    /// Never saved while Ready: not scanned, proposals left, or an edit's
    /// names not looked for yet.
    case notReviewed
    /// Its text is not the text it saved while Ready.
    case changedSinceReview
    /// Its text still holds this many names its Project knows, written out.
    case namesNotReplaced(Int)
    /// Marked "exclude from AI" (on a computer): mdoc's own exits refuse it.
    case excludedFromAI
}

/// Why Copy for AI and Share refuse, in plain words, with the one step that
/// resolves it (#1209 decisions 13 and 14). Nothing of the core's own
/// wording reaches the screen: each refusal has its own sentence.
enum HandOffRefusal: Equatable, Sendable {
    /// Proposals wait, of this many names: Replace N (proposals) and copy.
    case namesToDecide(names: Int, proposals: Int)
    /// Names were never looked for: Find names.
    case notScanned
    /// The text is not read yet: recognition or the import comes first.
    case notRead
    /// The Vault has not seen this text Ready: names in an edit are still
    /// to look for.
    case notChecked
    /// The text changed after its review: Find names again.
    case changedSinceReview
    /// Below the floor, on a tap only: nobody said this text's names were
    /// checked, which a phone that finds no names asks before the text
    /// leaves, and again after any change to it (item 48).
    case namesNotChecked
    /// Names the Project knows are written out in the text: Review.
    case namesWrittenOut(Int)
    /// Excluded from AI, which only a computer changes.
    case excludedFromAI
    /// Anything else: the text could not be prepared.
    case failed

    /// The step that resolves a refusal.
    enum Step: Equatable, Sendable {
        /// Accepts the pending proposals as the review does, then copies.
        case replaceAndCopy(Int)
        /// The review, on the names left.
        case review
        /// Find names, or Find names again.
        case findNames
        /// I checked the names myself: the text as it is now counts as
        /// checked, is saved, and the exit that asked goes on.
        case namesChecked

        var systemImage: String {
            switch self {
            case .replaceAndCopy: "eye.slash"
            case .review: "list.bullet.rectangle"
            case .findNames: "text.magnifyingglass"
            case .namesChecked: "checkmark.circle"
            }
        }

        var title: String {
            switch self {
            case .replaceAndCopy(let count): String(localized: "Replace \(count) and copy")
            case .review: String(localized: "Review")
            case .findNames: String(localized: "Find names")
            case .namesChecked: String(localized: "I checked the names myself")
            }
        }
    }

    /// The refusal of a saved document the session has no objection to,
    /// from what the Vault says of it; nil when it is Ready.
    init?(_ readiness: SavedReadiness) {
        switch readiness {
        case .ready: return nil
        case .notReviewed: self = .notChecked
        case .changedSinceReview: self = .changedSinceReview
        case .namesNotReplaced(let count): self = .namesWrittenOut(count)
        case .excludedFromAI: self = .excludedFromAI
        }
    }

    /// Short: under Copy for AI and Share in the document's menu, and an
    /// alert's title.
    var reason: String {
        switch self {
        case .namesToDecide(let count, _):
            count == 1
                ? String(localized: "1 name left to decide")
                : count > 1
                ? String(localized: "\(count) names left to decide")
                : String(localized: "Some names are left to decide")
        case .notScanned: String(localized: "Names not looked for yet")
        case .notRead: String(localized: "The document isn't ready yet")
        case .notChecked: String(localized: "The new text isn't checked yet")
        case .changedSinceReview: String(localized: "Changed since its review")
        case .namesNotChecked: String(localized: "Check the names yourself")
        case .namesWrittenOut(let count):
            count == 1
                ? String(localized: "1 known name is written out")
                : String(localized: "\(count) known names are written out")
        case .excludedFromAI: String(localized: "Excluded from AI")
        case .failed: String(localized: "The text couldn't be prepared")
        }
    }

    /// A sentence more, where a refusal shows on its own (the bar, an alert).
    var detail: String {
        switch self {
        case .namesToDecide:
            String(localized: "The text leaves once every name is replaced or kept.")
        case .notScanned:
            String(localized: "mdoc looks for names before the text leaves.")
        case .notRead:
            String(localized: "Its text is read first.")
        case .notChecked:
            String(localized: "Names in the edited text are looked for first.")
        case .changedSinceReview:
            String(localized: "The text changed after its review, so names are looked for again first.")
        case .namesNotChecked:
            String(localized: "This phone can't find names. Read the text and mark any name still in it before it goes to an AI.")
        case .namesWrittenOut:
            String(localized: "A name this document's Project knows is still written out. Replace it first.")
        case .excludedFromAI:
            String(localized: "This document is set to stay out of AI apps. Only mdoc on a computer changes that.")
        case .failed:
            String(localized: "Try again.")
        }
    }

    /// The one step that resolves it, where this phone can take it.
    /// `findsNames`: the phone runs the name detector. Below the floor no
    /// step looks for names again, so a text not checked since it changed
    /// has none there.
    func step(findsNames: Bool) -> Step? {
        switch self {
        case .namesToDecide(_, let proposals): proposals > 0 ? .replaceAndCopy(proposals) : .review
        case .notScanned, .notChecked, .changedSinceReview: findsNames ? .findNames : nil
        case .namesWrittenOut: .review
        case .namesNotChecked: .namesChecked
        case .notRead, .excludedFromAI, .failed: nil
        }
    }

    /// The alert's other button: a question is cancelled, a refusal only
    /// acknowledged.
    var dismissTitle: String {
        self == .namesNotChecked ? String(localized: "Cancel") : String(localized: "OK")
    }
}
