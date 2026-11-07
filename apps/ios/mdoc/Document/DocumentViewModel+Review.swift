import Foundation
import MdocFFI

/// The review bar's model (ADR 0098): which candidate is under review,
/// stepping in document order, deciding, undo, and the identity tools.
/// Every change goes through the session and comes back as a snapshot; the
/// bar's state is `review`, a `ReviewState` changed only here (ADR 0104).
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

    // MARK: Reading the bar

    var isReviewing: Bool { review.isReviewing }
    var reviewing: CandidateID? { review.reviewing }
    var reviewScope: Scope { review.scope }
    var focus: ReviewFocus? { review.focus }
    var barExpanded: Bool { review.expanded }
    var hint: ReviewHint? { review.hint }
    var reviewedCandidate: Candidate? { review.reviewedCandidate }
    var reviewOrder: ReviewOrder { review.order }
    /// Whether every candidate in the document is decided: Next then reads
    /// Done.
    var documentIsDone: Bool { review.documentIsDone }
    /// The other mentions All N reaches, outlined in the text.
    var scopedMentions: Set<CandidateID> { review.scopedMentions }

    // MARK: Starting and ending

    /// Review: the first proposal, or the first candidate when all are
    /// decided.
    func beginReview(focus: ReviewFocus? = nil) {
        guard !reviewWaitsForJob() else { return }
        updateReview { $0.begin(focus: focus) }
    }

    func endReview() {
        updateReview { $0.end() }
    }

    func clearFocus() {
        updateReview { $0.clearFocus() }
    }

    /// Puts the bar on `id`, resetting the scope when the identity changes.
    func select(_ id: CandidateID) {
        updateReview { $0.select(id) }
    }

    /// The scope chip: All or This one.
    func setScope(_ scope: Scope) {
        updateReview { $0.setScope(scope) }
    }

    /// The bar pulled up or let down; opening it keeps the name in view.
    func setBarExpanded(_ expanded: Bool) {
        updateReview { $0.setExpanded(expanded) }
    }

    /// The gesture was used, or × tapped: that hint never shows again.
    func learn(_ hint: ReviewHint) {
        hint.learn()
        updateReview { $0.learned(ReviewHint.teachable) }
    }

    /// Scrolls the text back to the candidate under review.
    func revealReviewed() {
        updateReview { $0.revealReviewed() }
    }

    // MARK: Stepping

    /// Steps `delta` places in document order; false at either end.
    @discardableResult
    func step(_ delta: Int) -> Bool {
        updateReview { $0.step(delta) }
    }

    /// Where the candidate sits among its identity's mentions.
    func mentionPosition(of candidate: Candidate) -> (index: Int, count: Int) {
        review.mentionPosition(of: candidate)
    }

    /// An identity's mentions in document order.
    func sortedMentions(of identity: IdentityID) -> [Candidate] {
        review.sortedMentions(of: identity)
    }

    /// The mentions chip: the identity's next mention, cycling round.
    func nextMention() {
        if updateReview({ $0.nextMention() }) { Haptics.selection() }
    }

    /// The full view's row for a mention: the name in its original
    /// wording, with the words around it and its heading.
    func snippet(of candidate: Candidate) -> MentionSnippet {
        MentionSnippet(markdown: snapshot.markdown, range: candidate.range, name: candidate.original)
    }

    /// A full view's row: the bar onto that mention, and the text there.
    func openMention(_ id: CandidateID) {
        updateReview {
            $0.select(id)
            $0.revealReviewed()
        }
    }

    // MARK: Deciding

    /// Replace or Keep in the bar's scope, then on to the next undecided
    /// candidate; the last decision ends review with All reviewed.
    func decideCurrent(_ decision: Decision) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        // A decided name changes in place, as the card's Keep instead and
        // Replace instead do (ADR 0101), from the long press, K and Return
        // too (#678); deciding it as it is changes nothing.
        guard candidate.state == .proposed else {
            if candidate.state != (decision == .keep ? .kept : .replaced) { changeCurrent(decision) }
            return
        }
        let index = reviewOrder.index(of: candidate.id)
        guard perform({ try session.decide(candidate.id, decision, scope: reviewScope) }) else { return }
        Haptics.decision()
        let advanced = updateReview { state -> ReviewState.Advance in
            state.decided()
            return state.advance(from: index)
        }
        allReviewed(advanced)
    }

    /// A state pill in the full view: that one mention changes, and the bar
    /// stays where it is.
    func decideMention(_ id: CandidateID, _ decision: Decision) {
        guard let current = reviewedCandidate, !reviewWaitsForJob() else { return }
        let index = reviewOrder.index(of: current.id)
        guard perform({ try session.decide(id, decision, scope: .this) }) else { return }
        Haptics.decision()
        updateReview {
            $0.decided()
            $0.reselect(near: current, index: index)
        }
    }

    /// Return, the card's filled button: Replace on a proposed name, Replace
    /// instead on a kept one, and Next on a replaced one, as the decided
    /// card's filled Next (ADR 0101).
    func returnKey() {
        if let candidate = reviewedCandidate, let action = keyAction(.return, for: candidate) { perform(action) }
    }

    /// "Keep instead" or "Replace instead" on a decided card, in the bar's
    /// scope: the card stays, showing its new state, until Next.
    func changeCurrent(_ decision: Decision) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        let index = reviewOrder.index(of: candidate.id)
        guard perform({ try session.decide(candidate.id, decision, scope: reviewScope) }) else { return }
        Haptics.decision()
        updateReview {
            $0.decided()
            $0.reselect(near: candidate, index: index)
        }
    }

    /// Next on a decided card: the next undecided candidate, or the end of
    /// review when none is left.
    func goToNextUndecided() {
        // As after a decision: the group's next, then the document's, then
        // All reviewed.
        let index = reviewOrder.index(of: reviewing)
        allReviewed(updateReview { $0.advance(from: index) })
        Haptics.selection()
    }

    /// "All 15 reviewed · Undo", or "3 reviewed" at the end of a focus.
    private func allReviewed(_ advance: ReviewState.Advance) {
        guard case .allReviewed(let count, let focused) = advance else { return }
        Haptics.success()
        toast = undoToast(focused ? String(localized: "\(count) reviewed") : String(localized: "All \(count) reviewed"))
    }

    // MARK: Undo

    var canUndo: Bool { snapshot.canUndo }
    var canRedo: Bool { snapshot.canRedo }

    /// Undo: the bar moves back to what it undid, and the toast names it
    /// with Redo.
    func undo(fromSystem: Bool = false) {
        guard canUndo, !reviewWaitsForJob() else { return }
        guard perform(undoable: false, { try session.undo() }) else { return }
        Haptics.warning()
        let step = snapshot.redoStep
        updateReview { $0.follow(step) }
        toast = ToastMessage(
            text: step.map { String(localized: "Undid \($0.label)") } ?? String(localized: "Undone"),
            actionTitle: String(localized: "Redo"), action: { [weak self] in self?.redoTapped() })
    }

    func redo(fromSystem: Bool = false) {
        guard canRedo, !reviewWaitsForJob() else { return }
        guard perform(undoable: false, { try session.redo() }) else { return }
        let step = snapshot.undoStep
        updateReview { $0.follow(step) }
        toast = ToastMessage(
            text: step.map { String(localized: "Redid \($0.label)") } ?? String(localized: "Redone"),
            actionTitle: String(localized: "Undo"), action: { [weak self] in self?.undoTapped() })
    }

    // MARK: Identity tools

    /// The category menu: each category with the alias it would give.
    func categoryChoices(for candidate: Candidate) -> [CategoryChoice] {
        (try? session.categoryChoices(for: candidate, scope: reviewScope))
            ?? Category.allCases.map { CategoryChoice(category: $0, alias: nil, current: $0 == candidate.category) }
    }

    func setCategory(_ category: Category) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        let splits = reviewScope == .this && mentionPosition(of: candidate).count > 1
        guard changeIdentity(of: candidate, { try session.setCategory(candidate.id, scope: reviewScope, to: category) }) else { return }
        if splits, let now = reviewedCandidate {
            toast = undoToast(String(localized: "Split off as \(now.alias)"))
        }
    }

    func rename(to alias: String) -> RenameOutcome {
        guard let candidate = reviewedCandidate else { return .done }
        guard !alias.isEmpty, alias != candidate.alias else { return .done }
        let index = reviewOrder.index(of: candidate.id)
        do {
            // Rename shows its own errors: an alias taken offers Same as.
            try change(rethrowing: true) { try session.rename(candidate.id, scope: reviewScope, to: alias) }
        } catch CoreError.aliasTaken(_, let other) {
            if let identity = snapshot.identities.first(where: { $0.id == other }) {
                return .taken(identity)
            }
            return .invalid(String(localized: "\(alias) is another name's alias."))
        } catch {
            return .invalid(error.localizedDescription)
        }
        Haptics.decision()
        updateReview { $0.reselect(near: candidate, index: index) }
        return .done
    }

    /// Same as…: in scope All the whole identity joins `target`, which keeps
    /// its alias; This one moves only this mention. Nothing is replaced.
    func sameAs(_ target: IdentityID) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        guard changeIdentity(of: candidate, { try session.sameAs(candidate.id, scope: reviewScope, target: target) }) else { return }
        let name = snapshot.identities.first { $0.id == target }?.primaryWording ?? ""
        toast = undoToast(String(localized: "Same as \(name)"))
    }

    /// New alias: this mention splits off as an identity of its own.
    func newAlias() {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        guard changeIdentity(of: candidate, { try session.newAlias(candidate.id) }) else { return }
        if let now = reviewedCandidate {
            toast = undoToast(String(localized: "Split off as \(now.alias)"))
        }
    }

    /// Same as for a whole identity, from Identities (a swipe or a drag).
    func link(_ identity: IdentityID, into target: IdentityID) {
        guard !reviewWaitsForJob() else { return }
        let reviewed = reviewedCandidate
        let reviewedIndex = reviewOrder.index(of: reviewing)
        guard perform({ try session.link(identity, into: target) }) else { return }
        Haptics.decision()
        // The bar stays on the same mention, whose range may have moved.
        if let reviewed { updateReview { $0.reselect(near: reviewed, index: reviewedIndex) } }
        let name = snapshot.identities.first { $0.id == target }?.primaryWording ?? ""
        toast = undoToast(String(localized: "Same as \(name)"))
    }

    /// Belongs to…: the identity's owner, or none.
    func setOwner(_ owner: IdentityID?) {
        guard let candidate = reviewedCandidate, !reviewWaitsForJob() else { return }
        changeIdentity(of: candidate, haptic: false) { try session.setOwner(candidate.identityID, to: owner) }
    }

    /// An identity tool's change to the name under review, through the one
    /// runner; the bar stays on the same mention by position after.
    @discardableResult
    private func changeIdentity(
        of candidate: Candidate, haptic: Bool = true, _ body: () throws -> DocumentSnapshot
    ) -> Bool {
        let index = reviewOrder.index(of: candidate.id)
        guard perform(body) else { return false }
        if haptic { Haptics.decision() }
        updateReview { $0.reselect(near: candidate, index: index) }
        return true
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
            others.first { $0.id == (hint.fromID == id ? hint.intoID : hint.fromID) }
        }
        let rest = others.filter { identity in !hinted.contains { $0.id == identity.id } }
            .sorted { firstMention(of: $0.id) < firstMention(of: $1.id) }
        return SameAsTargets(
            hinted: hinted,
            sameCategory: rest.filter { $0.category == category },
            otherCategories: rest.filter { $0.category != category })
    }

    /// The merge hint the card shows for this mention's identity, if any.
    func mergeHint(for candidate: Candidate) -> LinkHint? {
        mergeHints(for: candidate.identityID).first
    }

    /// The identity a hint points the card's identity at.
    func hintTarget(_ hint: LinkHint, for candidate: Candidate) -> IdentityID {
        hint.fromID == candidate.identityID ? hint.intoID : hint.fromID
    }

    func dismissHint(_ hint: LinkHint) {
        perform(undoable: false) { try session.dismissHint(hint) }
    }

    private func mergeHints(for identity: IdentityID) -> [LinkHint] {
        snapshot.mergeHints.filter { $0.fromID == identity || $0.intoID == identity }
    }

    private func firstMention(of identity: IdentityID) -> Int {
        sortedMentions(of: identity).first?.range.start ?? .max
    }

    func undoToast(_ text: String) -> ToastMessage {
        ToastMessage(text: text, actionTitle: String(localized: "Undo"), action: { [weak self] in self?.undoTapped() })
    }

    // MARK: Stored proposals

    /// Proposals that come with the document (Try a sample), as a scan's.
    /// Before the text is read they wait for recognition to finish.
    func proposeStored(_ names: [SampleDocument.Name]) {
        guard snapshot.phase != nil else {
            storedNames = names
            return
        }
        storedNames = nil
        perform(undoable: false) { try session.propose(names) }
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

    /// A match with up to 50 characters either side, cut between
    /// characters, never inside a letter (#679).
    func context(of range: SourceRange) -> String {
        let text = snapshot.markdown
        let utf8 = text.utf8
        guard range.start >= 0, range.start <= range.end, range.end <= utf8.count else { return "" }
        let lower = utf8.index(utf8.startIndex, offsetBy: range.start)
        let upper = utf8.index(utf8.startIndex, offsetBy: range.end)
        let before = text[..<lower].suffix(50), after = text[upper...].prefix(50)
        let shown = (before + text[lower..<upper] + after)
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespaces)
        return (before.startIndex > text.startIndex ? "…" : "") + shown + (after.endIndex < text.endIndex ? "…" : "")
    }

    /// Scrolls the text to a match: onto its candidate when it is one.
    func reveal(_ range: SourceRange) {
        let covering = candidate(covering: range)
        updateReview { state in
            if let covering {
                state.select(covering.id)
                state.revealReviewed()
            } else {
                state.reveal(range)
            }
        }
    }

    // MARK: System undo

    /// Mirrors each core step in the window's undo manager, so shake to
    /// undo, the three-finger swipe and ⌘Z reach the same history as the
    /// bar's Undo (ADR 0098).
    func registerSystemUndo() {
        systemUndoSteps = min(systemUndoSteps + 1, Self.systemUndoLevels)
        guard let manager = systemUndo else { return }
        manager.registerUndo(withTarget: self) { $0.undoFromSystem() }
        manager.setActionName(snapshot.undoStep?.label ?? "")
    }

    /// As many steps as the core keeps (`DocumentSession`, 256).
    static let systemUndoLevels = 256

    /// The document shows in a window, on `screen`: its steps go back on the
    /// window's undo stack, each its own group, so ⌘Z and shake reach them
    /// again. Redo starts empty there; the bar's Redo still reaches the core.
    /// A new screen for the same window (a size-class change rebuilds it)
    /// takes the steps over as they are.
    func attachSystemUndo(_ manager: UndoManager?, for screen: UUID) {
        systemUndoScreen = screen
        guard manager !== systemUndo else { return }
        releaseSystemUndo()
        systemUndo = manager
        guard let manager else { return }
        manager.levelsOfUndo = Self.systemUndoLevels
        // Inside an open group the steps would join it as one.
        guard systemUndoSteps > 0, manager.groupingLevel == 0 else {
            systemUndoSteps = 0
            return
        }
        let byEvent = manager.groupsByEvent
        manager.groupsByEvent = false
        for step in 0..<systemUndoSteps {
            manager.beginUndoGrouping()
            manager.registerUndo(withTarget: self) { $0.undoFromSystem() }
            if step == systemUndoSteps - 1 { manager.setActionName(snapshot.undoStep?.label ?? "") }
            manager.endUndoGrouping()
        }
        manager.groupsByEvent = byEvent
    }

    /// `screen` leaves the window: unless a newer screen of this document
    /// took it over, the steps leave the window's stacks, so another
    /// document's ⌘Z or Undo never reaches them (#672).
    func detachSystemUndo(for screen: UUID) {
        guard screen == systemUndoScreen else { return }
        systemUndoScreen = nil
        releaseSystemUndo()
    }

    private func releaseSystemUndo() {
        systemUndo?.removeAllActions(withTarget: self)
        systemUndo = nil
    }

    private func undoFromSystem() {
        systemUndoSteps = max(systemUndoSteps - 1, 0)
        undo(fromSystem: true)
        systemUndo?.registerUndo(withTarget: self) { $0.redoFromSystem() }
        systemUndo?.setActionName(snapshot.redoStep?.label ?? "")
    }

    private func redoFromSystem() {
        systemUndoSteps = min(systemUndoSteps + 1, Self.systemUndoLevels)
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
