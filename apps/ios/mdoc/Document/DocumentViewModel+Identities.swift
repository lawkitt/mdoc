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
            guard performReview({ try session.link(identity.id, into: target.id) }) else { break }
            if snapshot.undoStep != before { steps += 1 }
        }
        guard steps > 0 else { return }
        Haptics.decision()
        toast = undoToast(String(localized: "Same as \(target.primaryWording)"), steps: steps)
    }

    /// A tap on a name in the list: the bar's full view on its first
    /// undecided mention, or its first.
    func openIdentity(_ id: IdentityID) {
        let mentions = snapshot.mentions(of: id).sorted { $0.range.start < $1.range.start }
        guard let first = mentions.first(where: { $0.state == .proposed }) ?? mentions.first else { return }
        open(first.id)
        barExpanded = true
        revealReviewed()
    }

    private func undoToast(_ text: String, steps: Int) -> ToastMessage {
        ToastMessage(text: text, actionTitle: String(localized: "Undo")) { [weak self] in
            for _ in 0..<steps { self?.undoTapped() }
        }
    }
}
