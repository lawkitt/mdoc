import MdocFFI
import SwiftUI

/// Identities' gestures and feedback (ADR 0106): a decided row's state mark
/// turns before the row moves to its new section; a name dragged onto
/// another makes them one (`IdentitiesView+DragMerge`).
extension IdentitiesView {
    /// The state whose section a row sits in: a name just decided stays in
    /// its old one while its mark turns.
    func placed(_ identity: Identity) -> MentionState {
        held[identity.id] ?? identity.state
    }

    /// A swipe's or a menu's Replace or Keep for one name.
    func decide(_ identity: Identity, _ decision: Decision) {
        deciding([identity.id]) { model.decide(identities: [identity.id], decision) }
    }

    /// Runs `change`, a decision on `ids`, so that their rows stay where they
    /// are while their state marks turn to the new marks; once that motion
    /// completes, the rows move to their new sections. Under Reduce Motion
    /// both cross-fade.
    func deciding(_ ids: [IdentityID], _ change: () -> Void) {
        // Names gone since (merged away) hold nothing.
        let present = Set(identities.map(\.id))
        held = held.filter { present.contains($0.key) }
        for identity in identities where ids.contains(identity.id) && held[identity.id] == nil {
            held[identity.id] = identity.state
        }
        let motion = Motion.move(reduced: reduceMotion)
        withAnimation(motion, completionCriteria: .logicallyComplete) {
            change()
        } completion: {
            withAnimation(motion) {
                for id in ids { held[id] = nil }
            }
        }
    }

    /// How a state mark turns into the next: the new one grows in as the
    /// old one shrinks away; a cross-fade under Reduce Motion.
    var markTransition: AnyTransition {
        reduceMotion ? .opacity : .scale(scale: 0.4).combined(with: .opacity)
    }

    /// "Same as PERSON_1" while a drag is over the row.
    func dropHint(on identity: Identity) -> String? {
        guard drag?.target == identity.id else { return nil }
        return String(localized: "Same as \(identity.alias)")
    }
}

/// A row's drop hint, in the alias line's place: one line in the accent.
struct DropHintLabel: View {
    let text: String

    var body: some View {
        Label(text, systemImage: "link")
            .font(Typography.caption.weight(.semibold))
            .foregroundStyle(Palette.accent)
            .lineLimit(1)
    }
}
