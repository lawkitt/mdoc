import Foundation
import MdocFFI

/// Stepping limited to one identity or one category (ADR 0098's focus),
/// started from Identities.
enum ReviewFocus: Hashable, Sendable {
    case identity(IdentityID)
    case category(Category)

    func contains(_ candidate: Candidate) -> Bool {
        switch self {
        case .identity(let id): candidate.identityID == id
        case .category(let category): candidate.category == category
        }
    }
}

/// Where the review bar stands, in document order. Candidate ids are source
/// ranges, which move when a replacement changes the text, so the bar
/// keeps its place by position in the ordered list (decided mentions stay
/// in it).
struct ReviewOrder: Equatable {
    let candidates: [Candidate]

    init(_ snapshot: DocumentSnapshot, focus: ReviewFocus? = nil) {
        self.init(sorting: snapshot.candidates, focus: focus)
    }

    init(sorting candidates: [Candidate], focus: ReviewFocus? = nil) {
        let all = candidates.sorted { $0.range.start < $1.range.start }
        self.candidates = focus.map { focus in all.filter { focus.contains($0) } } ?? all
    }

    private init(ordered: [Candidate]) {
        candidates = ordered
    }

    /// This order within `focus`, without sorting again.
    func within(_ focus: ReviewFocus?) -> ReviewOrder {
        guard let focus else { return self }
        return ReviewOrder(ordered: candidates.filter { focus.contains($0) })
    }

    var count: Int { candidates.count }
    var decided: Int { candidates.filter { $0.state != .proposed }.count }
    var isDone: Bool { !candidates.isEmpty && candidates.allSatisfy { $0.state != .proposed } }

    func index(of id: CandidateID?) -> Int? {
        guard let id else { return nil }
        return candidates.firstIndex { $0.id == id }
    }

    func candidate(_ id: CandidateID) -> Candidate? {
        candidates.first { $0.id == id }
    }

    /// An identity's mentions, in document order.
    func mentions(of identity: IdentityID) -> [Candidate] {
        candidates.filter { $0.identityID == identity }
    }

    /// The first undecided candidate after `index`, wrapping round; nil when
    /// none is left.
    func nextUndecided(after index: Int?) -> Candidate? {
        guard !candidates.isEmpty else { return nil }
        let start = (index ?? -1) + 1
        for offset in 0..<candidates.count {
            let candidate = candidates[(start + offset) % candidates.count]
            if candidate.state == .proposed { return candidate }
        }
        return nil
    }

    /// The candidate at the same position, or the nearest one, after a
    /// change moved the ranges.
    func candidate(at index: Int) -> Candidate? {
        guard !candidates.isEmpty else { return nil }
        return candidates[min(max(index, 0), candidates.count - 1)]
    }

    /// The candidate whose range holds `offset`, else the first after it.
    func candidate(near offset: Int) -> Candidate? {
        candidates.first { $0.range.start <= offset && offset < max($0.range.end, $0.range.start + 1) }
            ?? candidates.first { $0.range.start >= offset }
            ?? candidates.last
    }
}

/// A scroll the text view runs once, after its layout is done: back to a
/// candidate, or to a Find match. A new id is a new request, even for the
/// same target, so a reveal is never lost because the target didn't change.
struct RevealRequest: Equatable, Sendable {
    enum Target: Equatable, Sendable {
        case candidate(CandidateID)
        case range(SourceRange)
    }

    let id: Int
    let target: Target
}

/// The review bar's state (ADR 0098, 0101, 0104): which candidate is under
/// review, the scope chip, the focus, whether the bar is pulled up, the hint
/// line and the pending reveal. A value with one way to change it: every
/// field is read-only outside, and each method is one transition, given the
/// core's snapshots through `apply`. It calls no core and shows nothing; the
/// view model runs the core and carries out what a transition returns
/// (toasts, haptics).
struct ReviewState: Equatable {
    /// The candidate on the bar; nil when not reviewing.
    private(set) var reviewing: CandidateID?
    /// The bar's scope chip: All by default, held within an identity and
    /// reset to All on the next one.
    private(set) var scope: Scope = .all
    /// The identity the scope was chosen for.
    private(set) var scopeIdentity: IdentityID?
    /// Pulled up: the identity's mentions, one row each.
    private(set) var expanded = false
    /// Stepping limited to one identity or category.
    private(set) var focus: ReviewFocus?
    /// The one hint line this document shows, while it shows.
    private(set) var hint: ReviewHint?
    /// A hint has shown in this document: no second one.
    private(set) var hintShownHere = false
    /// Cards and decisions so far in this document, which time the hints.
    private(set) var cardsSeen = 0
    private(set) var decisionsHere = 0
    /// The hints still to teach (`ReviewHint.teachable`), as the model last
    /// said.
    private(set) var teachable: Set<ReviewHint> = []
    /// The scroll the text view owes, until a newer one replaces it.
    private(set) var reveal: RevealRequest?
    /// Every candidate in document order, sorted once per snapshot.
    private(set) var document: ReviewOrder
    /// The order the bar steps in: the document's, within the focus.
    private(set) var order: ReviewOrder

    init(_ snapshot: DocumentSnapshot) {
        document = ReviewOrder(snapshot)
        order = document
    }

    var isReviewing: Bool { reviewing != nil }

    var reviewedCandidate: Candidate? { reviewing.flatMap { document.candidate($0) } }

    /// Whether every candidate in the document is decided: Next then reads
    /// Done.
    var documentIsDone: Bool { document.isDone }

    /// An identity's mentions in document order: the card's "2 of 3", the
    /// mentions chip, the full view's rows and Identities' open.
    func sortedMentions(of identity: IdentityID) -> [Candidate] {
        document.mentions(of: identity)
    }

    /// Where the candidate sits among its identity's mentions.
    func mentionPosition(of candidate: Candidate) -> (index: Int, count: Int) {
        let mentions = sortedMentions(of: candidate.identityID)
        let index = (mentions.firstIndex { $0.id == candidate.id } ?? 0) + 1
        return (index, mentions.count)
    }

    /// The other mentions All N reaches, outlined in the text.
    var scopedMentions: Set<CandidateID> {
        guard scope == .all, let candidate = reviewedCandidate else { return [] }
        return Set(sortedMentions(of: candidate.identityID).map(\.id)).subtracting([candidate.id])
    }

    // MARK: Snapshots

    /// The core's new snapshot: the orders follow it, and a candidate that
    /// is gone leaves the bar.
    mutating func apply(_ snapshot: DocumentSnapshot) {
        if snapshot.candidates != document.candidates {
            let sorted = ReviewOrder(snapshot)
            if sorted.candidates != document.candidates {
                document = sorted
                order = sorted.within(focus)
            }
        }
        if let reviewing, document.candidate(reviewing) == nil {
            self.reviewing = nil
        }
    }

    /// The hints still to teach, given before each transition so the hint
    /// line follows Settings › Reset tips and hints learned elsewhere.
    mutating func setTeachable(_ hints: Set<ReviewHint>) {
        teachable = hints
    }

    // MARK: Starting and ending

    /// Review: the first proposal, or the first candidate when all are
    /// decided. False, with no focus, when there is nothing to review.
    @discardableResult
    mutating func begin(focus: ReviewFocus? = nil) -> Bool {
        setFocus(focus)
        guard let first = order.nextUndecided(after: nil) ?? order.candidates.first else {
            setFocus(nil)
            return false
        }
        select(first.id)
        return true
    }

    mutating func end() {
        reviewing = nil
        setFocus(nil)
        expanded = false
        hint = nil
    }

    mutating func clearFocus() {
        setFocus(nil)
        if let reviewing, order.index(of: reviewing) == nil {
            self.reviewing = order.nextUndecided(after: nil)?.id
        }
    }

    private mutating func setFocus(_ focus: ReviewFocus?) {
        guard focus != self.focus else { return }
        self.focus = focus
        order = document.within(focus)
    }

    // MARK: Selecting

    /// Puts the bar on `id`, resetting the scope when the identity changes.
    /// `counts` is false when it is the same card found again after a
    /// change moved its range.
    mutating func select(_ id: CandidateID, counts: Bool = true) {
        guard let candidate = document.candidate(id) else { return }
        // A name outside the focused group ends the focus.
        if let focus, !focus.contains(candidate) { setFocus(nil) }
        if candidate.identityID != scopeIdentity {
            scope = .all
            scopeIdentity = candidate.identityID
        }
        // A new card, not the same one found again after its range moved.
        if counts && id != reviewing { cardsSeen += 1 }
        reviewing = id
        updateHint()
    }

    /// The scope chip.
    mutating func setScope(_ scope: Scope) {
        self.scope = scope
    }

    /// The bar pulled up or let down. Opening it asks the text to keep the
    /// name in view above the taller bar.
    mutating func setExpanded(_ expanded: Bool) {
        guard expanded != self.expanded else { return }
        self.expanded = expanded
        if expanded { revealReviewed() }
    }

    /// Steps `delta` places in the bar's order; false at either end.
    @discardableResult
    mutating func step(_ delta: Int) -> Bool {
        guard let index = order.index(of: reviewing) else { return false }
        let next = index + delta
        guard order.candidates.indices.contains(next) else { return false }
        select(order.candidates[next].id)
        return true
    }

    /// The first proposal after the one under review, wrapping around.
    mutating func reviewNext() {
        if let next = order.nextUndecided(after: order.index(of: reviewing)) {
            select(next.id)
        }
    }

    /// The mentions chip: the identity's next mention, cycling round; false
    /// when it has no other.
    @discardableResult
    mutating func nextMention() -> Bool {
        guard let candidate = reviewedCandidate else { return false }
        let mentions = sortedMentions(of: candidate.identityID)
        guard mentions.count > 1, let index = mentions.firstIndex(where: { $0.id == candidate.id }) else { return false }
        reviewing = mentions[(index + 1) % mentions.count].id
        return true
    }

    /// Hide name on a selection: the bar on the new mark, out of any focus;
    /// false when there is none.
    @discardableResult
    mutating func selectMarked(_ selection: SourceRange) -> Bool {
        let marked = document.candidates.first { $0.range.start <= selection.start && $0.range.end >= selection.end }
            ?? document.candidate(near: selection.start)
        guard let marked else { return false }
        setFocus(nil)
        select(marked.id)
        return true
    }

    // MARK: After a change

    /// What the bar did after a decision, for the toast and haptics.
    enum Advance: Equatable {
        /// On to another card, or the same one.
        case moved
        /// The order is all decided: "All N reviewed" (or "N reviewed" in a
        /// focus, which then ends). The bar either moved on into the whole
        /// document or review ended.
        case allReviewed(count: Int, focused: Bool)
        /// Nowhere to go.
        case stayed
    }

    /// A decision made; it times the hints.
    mutating func decided() {
        decisionsHere += 1
    }

    /// After a decision: the next undecided candidate, or All reviewed.
    /// `index` is where the decided card stood before the change.
    mutating func advance(from index: Int?) -> Advance {
        if let next = order.nextUndecided(after: index) {
            select(next.id)
            return .moved
        }
        if order.isDone {
            let count = order.count
            let focused = focus != nil
            if focused {
                // The group is done: back to the whole document.
                setFocus(nil)
                if let next = order.nextUndecided(after: nil) {
                    select(next.id)
                    return .allReviewed(count: count, focused: true)
                }
            }
            end()
            return .allReviewed(count: count, focused: focused)
        }
        if let index, let same = order.candidate(at: index) {
            select(same.id)
            return .moved
        }
        return .stayed
    }

    /// After an identity change moved ranges: the same mention by position,
    /// keeping the bar on it.
    mutating func reselect(near candidate: Candidate, index: Int?) {
        if let index, let same = order.candidate(at: index) {
            select(same.id, counts: false)
        } else if let near = document.candidate(near: candidate.range.start) {
            setFocus(nil)
            select(near.id, counts: false)
        }
    }

    /// After undo or redo: back onto the mention the step acted on, opening
    /// the bar if review had ended, and scrolled to it. `step` is what was
    /// undone or made again.
    mutating func follow(_ step: HistoryStep?) {
        guard let range = step?.range.map(SourceRange.init) else { return }
        if let candidate = document.candidates.first(where: { $0.range == range }) {
            select(candidate.id, counts: false)
            revealReviewed()
        } else if let near = document.candidate(near: range.start) {
            setFocus(nil)
            select(near.id, counts: false)
            revealReviewed()
        }
    }

    // MARK: Hints

    /// Shows at most one hint line per document, once its moment comes:
    /// swipe after the second card, the long press after three decisions.
    /// A learned hint goes at once.
    private mutating func updateHint() {
        if let hint, !teachable.contains(hint) { self.hint = nil }
        guard isReviewing, hint == nil, !hintShownHere else { return }
        if teachable.contains(.swipe), cardsSeen >= 2, order.count > 2 {
            hint = .swipe
        } else if teachable.contains(.longPress), decisionsHere >= 3 {
            hint = .longPress
        }
        if hint != nil { hintShownHere = true }
    }

    /// A hint's gesture was used, or × tapped: `teachable` no longer has it.
    mutating func learned(_ hints: Set<ReviewHint>) {
        teachable = hints
        updateHint()
    }

    // MARK: Reveal

    /// Scrolls the text back to the candidate under review.
    mutating func revealReviewed() {
        guard let reviewing else { return }
        request(.candidate(reviewing))
    }

    /// Scrolls the text to a range no candidate covers (a Find match).
    mutating func reveal(_ range: SourceRange) {
        request(.range(range))
    }

    private mutating func request(_ target: RevealRequest.Target) {
        reveal = RevealRequest(id: (reveal?.id ?? 0) + 1, target: target)
    }
}

extension Category {
    /// Contacts and identifiers, which can belong to a person or company
    /// (desktop's Belongs to…, ADR 0032).
    var hasOwner: Bool {
        switch self {
        case .email, .url, .phone, .address, .identity, .tax, .bank: true
        case .person, .organization, .date, .caseNumber, .other: false
        }
    }
}
