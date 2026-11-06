import Foundation

/// The review bar's model (ADR 0098): which candidate is under review,
/// stepping in document order, deciding, undo, and the identity tools.
/// Every change goes through the session and comes back as a snapshot.
extension DocumentViewModel {
    enum RenameOutcome: Equatable {
        case done
        /// Another identity has the alias: offer Same as.
        case taken(Identity)
        case invalid(String)
    }

    struct SameAsTargets {
        var hinted: [Identity] = []
        var sameCategory: [Identity] = []
        var otherCategories: [Identity] = []
        var all: [Identity] { hinted + sameCategory + otherCategories }
    }

    var isReviewing: Bool { reviewing != nil }

    var reviewedCandidate: Candidate? { reviewing.flatMap { snapshot.candidate($0) } }

    var reviewOrder: ReviewOrder { ReviewOrder(snapshot, focus: focus) }

    // MARK: Starting and ending

    /// Review: the first proposal, or the first candidate when all are
    /// decided.
    func beginReview(focus: ReviewFocus? = nil) {
        guard !reviewWaitsForJob() else { return }
        self.focus = focus
        let order = reviewOrder
        guard let first = order.nextUndecided(after: nil) ?? order.candidates.first else {
            self.focus = nil
            return
        }
        select(first.id)
    }

    func endReview() {
        reviewing = nil
        focus = nil
        barExpanded = false
    }

    func clearFocus() {
        focus = nil
        if let reviewing, reviewOrder.index(of: reviewing) == nil {
            self.reviewing = reviewOrder.nextUndecided(after: nil)?.id
        }
    }

    /// Puts the bar on `id`, resetting the scope when the identity changes.
    func select(_ id: CandidateID) {
        guard let candidate = snapshot.candidate(id) else { return }
        // A name outside the focused group ends the focus.
        if let focus, !focus.contains(candidate) { self.focus = nil }
        if candidate.identityID != scopeIdentity {
            reviewScope = .allOccurrences
            scopeIdentity = candidate.identityID
        }
        reviewing = id
    }

    /// Scrolls the text back to the candidate under review.
    func revealReviewed() {
        revealRange = nil
        revealRequest += 1
    }

    /// The other mentions All N reaches, outlined in the text.
    var scopedMentions: Set<CandidateID> {
        guard reviewScope == .allOccurrences, let candidate = reviewedCandidate else { return [] }
        return Set(snapshot.mentions(of: candidate.identityID).map(\.id)).subtracting([candidate.id])
    }

    // MARK: Stepping

    /// Steps `delta` places in document order; false at either end.
    @discardableResult
    func step(_ delta: Int) -> Bool {
        let order = reviewOrder
        guard let index = order.index(of: reviewing) else { return false }
        let next = index + delta
        guard order.candidates.indices.contains(next) else { return false }
        select(order.candidates[next].id)
        return true
    }

    /// Where the candidate sits among its identity's mentions.
    func mentionPosition(of candidate: Candidate) -> (index: Int, count: Int) {
        let mentions = snapshot.mentions(of: candidate.identityID).sorted { $0.range.start < $1.range.start }
        let index = (mentions.firstIndex { $0.id == candidate.id } ?? 0) + 1
        return (index, mentions.count)
    }

    /// The mentions chip: the identity's next mention, cycling round.
    func nextMention() {
        guard let candidate = reviewedCandidate else { return }
        let mentions = snapshot.mentions(of: candidate.identityID).sorted { $0.range.start < $1.range.start }
        guard mentions.count > 1, let index = mentions.firstIndex(where: { $0.id == candidate.id }) else { return }
        reviewing = mentions[(index + 1) % mentions.count].id
        Haptics.selection()
    }

    /// About 40 characters either side of a mention, on one line.
    func context(of candidate: Candidate) -> String {
        let bytes = Array(snapshot.markdown.utf8)
        let start = max(0, candidate.range.start - 60)
        let end = min(bytes.count, candidate.range.end + 60)
        guard start < end else { return candidate.original }
        let text = String(decoding: bytes[start..<end], as: UTF8.self)
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespaces)
        return (start > 0 ? "…" : "") + text + (end < bytes.count ? "…" : "")
    }

    // MARK: Deciding

    /// Replace or Keep in the bar's scope, then on to the next undecided
    /// candidate; the last decision ends review with All reviewed.
    func decideCurrent(_ decision: Decision) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        let index = reviewOrder.index(of: candidate.id)
        let ok = performReview { try session.decide(candidate.id, decision, scope: reviewScope) }
        guard ok else { return }
        UndoTip.decided = true
        Haptics.decision()
        advance(from: index)
    }

    /// The expanded view's Keep all n or Replace all n for the identity.
    func decideIdentity(_ decision: Decision) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        let index = reviewOrder.index(of: candidate.id)
        guard performReview({ try session.decide(candidate.id, decision, scope: .allOccurrences) }) else { return }
        Haptics.decision()
        advance(from: index)
    }

    /// After a decision: the next undecided candidate, or All reviewed.
    func advance(from index: Int?) {
        let order = reviewOrder
        if let next = order.nextUndecided(after: index) {
            select(next.id)
        } else if order.isDone {
            let count = order.count
            Haptics.success()
            toast = ToastMessage(
                text: focus == nil
                    ? String(localized: "All \(count) reviewed")
                    : String(localized: "\(count) reviewed"),
                actionTitle: String(localized: "Undo"), action: { [weak self] in self?.undoTapped() })
            if focus != nil {
                // The group is done: back to the whole document.
                focus = nil
                if let next = reviewOrder.nextUndecided(after: nil) {
                    select(next.id)
                    return
                }
            }
            endReview()
        } else if let index, let same = order.candidate(at: index) {
            select(same.id)
        }
    }

    /// Runs a review change; false (with the error shown) when it failed.
    @discardableResult
    func performReview(isStep: Bool = true, _ change: () throws -> DocumentSnapshot) -> Bool {
        do {
            let before = snapshot.undoStep
            applyReview(try change())
            // A change that made a step goes on the system's undo stack too.
            if isStep && snapshot.canUndo && snapshot.undoStep != before { registerSystemUndo() }
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    // MARK: Undo

    var canUndo: Bool { snapshot.canUndo }
    var canRedo: Bool { snapshot.canRedo }

    /// Undo: the bar moves back to what it undid, and the toast names it
    /// with Redo.
    func undo(fromSystem: Bool = false) {
        guard canUndo, !reviewWaitsForJob() else { return }
        guard performReview(isStep: false, { try session.undo() }) else { return }
        Haptics.warning()
        let step = snapshot.redoStep
        moveBar(to: step)
        toast = ToastMessage(
            text: step.map { String(localized: "Undid \($0.label)") } ?? String(localized: "Undone"),
            actionTitle: String(localized: "Redo"), action: { [weak self] in self?.redoTapped() })
    }

    func redo(fromSystem: Bool = false) {
        guard canRedo, !reviewWaitsForJob() else { return }
        guard performReview(isStep: false, { try session.redo() }) else { return }
        let step = snapshot.undoStep
        moveBar(to: step)
        toast = ToastMessage(
            text: step.map { String(localized: "Redid \($0.label)") } ?? String(localized: "Redone"),
            actionTitle: String(localized: "Undo"), action: { [weak self] in self?.undoTapped() })
    }

    /// After undo or redo: back onto the mention the step acted on, opening
    /// the bar if review had ended.
    private func moveBar(to step: HistoryStep?) {
        if let id = step?.candidateID, snapshot.candidate(id) != nil {
            if let focus, let candidate = snapshot.candidate(id), !focus.contains(candidate) { self.focus = nil }
            select(id)
            revealReviewed()
        } else if let range = step?.range, let near = ReviewOrder(snapshot).candidate(near: range.start) {
            focus = nil
            select(near.id)
            revealReviewed()
        } else if let reviewing, snapshot.candidate(reviewing) == nil {
            self.reviewing = reviewOrder.nextUndecided(after: nil)?.id
        }
    }

    // MARK: Identity tools

    /// The category menu: each category with the alias it would give.
    func categoryChoices(for candidate: Candidate) -> [CategoryChoice] {
        (try? session.categoryChoices(for: candidate.id, scope: reviewScope))
            ?? Category.allCases.map { CategoryChoice(category: $0, alias: nil, isCurrent: $0 == candidate.category) }
    }

    func setCategory(_ category: Category) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        let index = reviewOrder.index(of: candidate.id)
        let splits = reviewScope == .justThisOne && mentionPosition(of: candidate).count > 1
        guard performReview({ try session.setCategory(candidate.id, scope: reviewScope, to: category) }) else { return }
        Haptics.decision()
        reselect(near: candidate, index: index)
        if splits, let now = reviewedCandidate {
            toast = undoToast(String(localized: "Split off as \(now.alias)"))
        }
    }

    func rename(to alias: String) -> RenameOutcome {
        guard let candidate = reviewedCandidate else { return .done }
        guard !alias.isEmpty, alias != candidate.alias else { return .done }
        let index = reviewOrder.index(of: candidate.id)
        do {
            let before = snapshot.undoStep
            applyReview(try session.rename(candidate.id, scope: reviewScope, to: alias))
            if snapshot.canUndo && snapshot.undoStep != before { registerSystemUndo() }
        } catch CoreError.aliasTaken(_, let other) {
            if let identity = snapshot.identities.first(where: { $0.id == other }) {
                return .taken(identity)
            }
            return .invalid(String(localized: "\(alias) is another name's alias."))
        } catch {
            return .invalid(error.localizedDescription)
        }
        Haptics.decision()
        reselect(near: candidate, index: index)
        return .done
    }

    /// Same as…: in scope All the whole identity joins `target`, which keeps
    /// its alias; This one moves only this mention. Nothing is replaced.
    func sameAs(_ target: IdentityID) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        let index = reviewOrder.index(of: candidate.id)
        guard performReview({ try session.sameAs(candidate.id, scope: reviewScope, target: target) }) else { return }
        Haptics.decision()
        reselect(near: candidate, index: index)
        let name = snapshot.identities.first { $0.id == target }?.primaryWording ?? ""
        toast = undoToast(String(localized: "Same as \(name)"))
    }

    /// New alias: this mention splits off as an identity of its own.
    func newAlias() {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        let index = reviewOrder.index(of: candidate.id)
        guard performReview({ try session.newAlias(candidate.id) }) else { return }
        Haptics.decision()
        reselect(near: candidate, index: index)
        if let now = reviewedCandidate {
            toast = undoToast(String(localized: "Split off as \(now.alias)"))
        }
    }

    /// Same as for a whole identity, from Identities (a swipe or a drag).
    func link(_ identity: IdentityID, into target: IdentityID) {
        guard !reviewWaitsForJob() else { return }
        let reviewed = reviewedCandidate
        let reviewedIndex = reviewOrder.index(of: reviewing)
        guard performReview({ try session.link(identity, into: target) }) else { return }
        Haptics.decision()
        // The bar stays on the same mention, whose range may have moved.
        if let reviewed { reselect(near: reviewed, index: reviewedIndex) }
        let name = snapshot.identities.first { $0.id == target }?.primaryWording ?? ""
        toast = undoToast(String(localized: "Same as \(name)"))
    }

    /// Belongs to…: the identity's owner, or none.
    func setOwner(_ owner: IdentityID?) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        let index = reviewOrder.index(of: candidate.id)
        guard performReview({ try session.setOwner(candidate.identityID, to: owner) }) else { return }
        reselect(near: candidate, index: index)
    }

    /// Same as… targets: the core's hints first, then the same category in
    /// order of first mention, then the other categories.
    func sameAsTargets(for candidate: Candidate) -> SameAsTargets {
        sameAsTargets(forIdentity: candidate.identityID, category: candidate.category)
    }

    /// The same, for an identity: one picker order everywhere (the card,
    /// the long press and Identities).
    func sameAsTargets(forIdentity id: IdentityID, category: Category) -> SameAsTargets {
        let others = snapshot.identities.filter { $0.id != id }
        let hinted = mergeHints(for: id).compactMap { hint in
            others.first { $0.id == (hint.from == id ? hint.into : hint.from) }
        }
        let rest = others.filter { identity in !hinted.contains { $0.id == identity.id } }
            .sorted { firstMention(of: $0.id) < firstMention(of: $1.id) }
        return SameAsTargets(
            hinted: hinted,
            sameCategory: rest.filter { $0.category == category },
            otherCategories: rest.filter { $0.category != category })
    }

    /// The merge hint the card shows for this mention's identity, if any.
    func mergeHint(for candidate: Candidate) -> MergeHint? {
        mergeHints(for: candidate.identityID).first
    }

    /// The identity a hint points the card's identity at.
    func hintTarget(_ hint: MergeHint, for candidate: Candidate) -> IdentityID {
        hint.from == candidate.identityID ? hint.into : hint.from
    }

    func dismissHint(_ hint: MergeHint) {
        performReview(isStep: false) { try session.dismissHint(hint) }
    }

    private func mergeHints(for identity: IdentityID) -> [MergeHint] {
        snapshot.mergeHints.filter { $0.from == identity || $0.into == identity }
    }

    private func firstMention(of identity: IdentityID) -> Int {
        snapshot.mentions(of: identity).map(\.range.start).min() ?? .max
    }

    private func undoToast(_ text: String) -> ToastMessage {
        ToastMessage(text: text, actionTitle: String(localized: "Undo"), action: { [weak self] in self?.undoTapped() })
    }

    /// After an identity change moved ranges: the same mention by position,
    /// keeping the bar on it.
    private func reselect(near candidate: Candidate, index: Int?) {
        let order = reviewOrder
        if let index, let same = order.candidate(at: index) {
            select(same.id)
        } else if let near = ReviewOrder(snapshot).candidate(near: candidate.range.start) {
            focus = nil
            select(near.id)
        }
    }

    // MARK: Stored proposals

    /// Proposals that come with the document (Try a sample), as a scan's.
    /// Before the text is read they wait for recognition to finish.
    func proposeStored(_ names: [SampleDocument.Name]) {
        guard snapshot.phase != .recognizing else {
            storedNames = names
            return
        }
        storedNames = nil
        performReview(isStep: false) { try session.propose(names) }
    }

    // MARK: Find in document

    /// Every case-insensitive match of `query` in the text, as source
    /// byte ranges.
    func matches(of query: String) -> [SourceRange] {
        let needle = query.trimmingCharacters(in: .whitespaces)
        guard !needle.isEmpty else { return [] }
        let text = snapshot.markdown
        var found: [SourceRange] = []
        var searchStart = text.startIndex
        while let range = text.range(of: needle, options: [.caseInsensitive], range: searchStart..<text.endIndex) {
            let start = text.utf8.distance(from: text.startIndex, to: range.lowerBound)
            let end = text.utf8.distance(from: text.startIndex, to: range.upperBound)
            found.append(SourceRange(start: start, end: end))
            searchStart = range.upperBound
        }
        return found
    }

    /// The first match no candidate covers: what Find's Hide name marks.
    func firstUnmarked(of query: String) -> SourceRange? {
        matches(of: query).first { candidate(covering: $0) == nil }
    }

    func candidate(covering range: SourceRange) -> Candidate? {
        snapshot.candidates.first { $0.range.start <= range.start && $0.range.end >= range.end }
    }

    func context(of range: SourceRange) -> String {
        let bytes = Array(snapshot.markdown.utf8)
        let start = max(0, range.start - 50), end = min(bytes.count, range.end + 50)
        guard start < end else { return "" }
        let text = String(decoding: bytes[start..<end], as: UTF8.self)
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespaces)
        return (start > 0 ? "…" : "") + text + (end < bytes.count ? "…" : "")
    }

    /// Scrolls the text to a match: onto its candidate when it is one.
    func reveal(_ range: SourceRange) {
        if let candidate = candidate(covering: range) {
            select(candidate.id)
            revealReviewed()
        } else {
            revealRange = range
            revealRequest += 1
        }
    }

    // MARK: System undo

    /// Mirrors each core step in the window's undo manager, so shake to
    /// undo, the three-finger swipe and ⌘Z reach the same history as the
    /// bar's Undo (ADR 0098).
    func registerSystemUndo() {
        guard let manager = systemUndo else { return }
        manager.registerUndo(withTarget: self) { $0.undoFromSystem() }
        manager.setActionName(snapshot.undoStep?.label ?? "")
    }

    private func undoFromSystem() {
        undo(fromSystem: true)
        systemUndo?.registerUndo(withTarget: self) { $0.redoFromSystem() }
        systemUndo?.setActionName(snapshot.redoStep?.label ?? "")
    }

    private func redoFromSystem() {
        redo(fromSystem: true)
        systemUndo?.registerUndo(withTarget: self) { $0.undoFromSystem() }
        systemUndo?.setActionName(snapshot.undoStep?.label ?? "")
    }

    /// The bar's Undo and Redo go through the undo manager when it has the
    /// step, so its stacks stay in line with the core's.
    func undoTapped() {
        if let manager = systemUndo, manager.canUndo { manager.undo() } else { undo() }
    }

    func redoTapped() {
        if let manager = systemUndo, manager.canRedo { manager.redo() } else { redo() }
    }
}
