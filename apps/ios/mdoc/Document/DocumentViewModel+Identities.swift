import Foundation
import MdocFFI

/// Identities as a drawer (2026-11-06 grill, round 2): the list's state
/// buttons, Replace N, Looks like, and Select's bulk Replace, Keep and Same
/// as. Every change goes through the same core calls as the review bar and
/// offers Undo in a toast.
extension DocumentViewModel {
    /// "15 names · 4 to review".
    var identityCounts: (total: Int, toReview: Int) {
        let identities = snapshot.identities
        return (identities.count, identities.filter { $0.state == .proposed }.count)
    }

    /// Why Copy for AI and Share wait, shown under them in the document's
    /// menu (ADR 0115): "32 names left to decide", or names not found yet.
    /// Nil once the document is ready.
    var handOffBlockedReason: String? {
        switch snapshot.phase {
        case .ready?:
            return nil
        case .reviewing?:
            let left = identityCounts.toReview
            if left == 1 { return String(localized: "1 name left to decide") }
            if left > 1 { return String(localized: "\(left) names left to decide") }
            return String(localized: "Some names are left to decide")
        case .unscanned?:
            return String(localized: "Find names first")
        default:
            return String(localized: "The document isn't ready yet")
        }
    }

    /// Merge hints whose two identities are both still there: the Looks
    /// like cards.
    var identityHints: [(hint: LinkHint, from: Identity, into: Identity)] {
        snapshot.mergeHints.compactMap { hint in
            guard let from = snapshot.identities.first(where: { $0.id == hint.fromID }),
                  let into = snapshot.identities.first(where: { $0.id == hint.intoID }) else { return nil }
            return (hint, from, into)
        }
    }

    /// The round state button: a proposed name is replaced; after that it
    /// switches between Replaced and Kept.
    func toggle(_ identity: Identity) {
        let decision: Decision = identity.state == .replaced ? .keep : .replace
        decide(identities: [identity.id], decision)
    }

    /// Replace or Keep every mention of each identity, one undo step each;
    /// the toast's Undo takes them all back.
    func decide(identities ids: [IdentityID], _ decision: Decision) {
        guard !reviewWaitsForJob(), !ids.isEmpty else { return }
        let names = ids.compactMap { id in snapshot.identities.first { $0.id == id }?.primaryWording }
        var steps = 0
        for id in ids {
            let before = snapshot.undoStep
            decide(identity: id, decision)
            if errorMessage != nil { break }
            if snapshot.undoStep != before { steps += 1 }
        }
        guard steps > 0 else { return }
        Haptics.decision()
        let verb = decision == .replace ? String(localized: "Replaced") : String(localized: "Kept")
        let text = names.count == 1
            ? "\(verb) \(names[0])"
            : (decision == .replace ? String(localized: "\(steps) replaced") : String(localized: "\(steps) kept"))
        toast = undoToast(text, steps: steps)
    }

    /// Select › Same as: every selected identity joins the one mentioned
    /// most, which keeps its alias and restore wording.
    func merge(_ ids: [IdentityID]) {
        let identities = ids.compactMap { id in snapshot.identities.first { $0.id == id } }
        guard identities.count > 1, !reviewWaitsForJob(),
              let target = identities.max(by: { $0.mentionCount < $1.mentionCount }) else { return }
        var steps = 0
        for identity in identities where identity.id != target.id {
            let before = snapshot.undoStep
            guard perform({ try session.link(identity.id, into: target.id) }) else { break }
            if snapshot.undoStep != before { steps += 1 }
        }
        guard steps > 0 else { return }
        Haptics.decision()
        ReviewHint.dragMerge.learn()
        toast = undoToast(String(localized: "Same as \(target.primaryWording)"), steps: steps)
    }

    // MARK: Drag-to-merge

    /// Whether `dragged`, a row being dragged, may drop on `target`'s row:
    /// another name of this document (ADR 0106). A row takes no drop of
    /// itself, nor anything that isn't one of the names.
    func canMerge(_ dragged: IdentityID?, into target: IdentityID) -> Bool {
        guard let dragged, dragged != target else { return false }
        let ids = snapshot.identities.map(\.id)
        return ids.contains(dragged) && ids.contains(target)
    }

    /// A name dropped on another: it joins the target, which keeps its alias
    /// and restore wording, and another category takes the target's (the
    /// core's link, as desktop's drag, ADR 0025). One undo step, no
    /// confirmation: "PERSON_3 is now PERSON_1 · Undo".
    func dragMerge(_ dragged: IdentityID, into target: IdentityID) {
        guard canMerge(dragged, into: target) else { return }
        link(dragged, into: target) { from, into in String(localized: "\(from.alias) is now \(into.alias)") }
    }

    /// The Select demo (ADR 0114): the first time Select opens with two
    /// names or more, until the first drag-merge or Same as, or until it is
    /// dismissed.
    var offersSelectDemo: Bool {
        snapshot.identities.count >= 2 && ReviewHint.teachable.contains(.dragMerge)
    }

    /// A tap on a name in the list: the bar's full view on its first
    /// undecided mention, or its first.
    func openIdentity(_ id: IdentityID) {
        let mentions = sortedMentions(of: id)
        guard let first = mentions.first(where: { $0.state == .proposed }) ?? mentions.first else { return }
        open(first.id)
        updateReview {
            $0.setExpanded(true)
            $0.revealReviewed()
        }
    }

    private func undoToast(_ text: String, steps: Int) -> ToastMessage {
        ToastMessage(text: text, actionTitle: String(localized: "Undo")) { [weak self] in
            for _ in 0..<steps { self?.undoTapped() }
        }
    }
}
