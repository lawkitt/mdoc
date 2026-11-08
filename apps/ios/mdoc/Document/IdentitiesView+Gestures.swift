import MdocFFI
import SwiftUI

/// Identities' gestures and feedback (ADR 0106): a decided row's state mark
/// turns before the row moves to its new section, and a name dragged onto
/// another makes them one.
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
        guard dropTarget == identity.id else { return nil }
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

/// A row's part in drag-to-merge (ADR 0106, desktop's ADR 0025): it lifts
/// as a drag and takes another row's drop, outlined while a drag is over
/// it. The drop is `DocumentViewModel.dragMerge`: the core's link, one undo
/// step, a haptic and an Undo toast; a row's drop of itself does nothing.
///
/// On iOS 26 a List keeps a drag that starts in it to itself, so the drop
/// never reaches a row, nor the list's or the panel's own drop destination
/// (seen on the iPhone 17 Pro Simulator, 2026-11-08): the hint line that
/// teaches the gesture waits until it does.
struct IdentityDrop: ViewModifier {
    /// Whether a drop reaches the row under the finger; the hint line shows
    /// only then.
    static let reachesRows = false

    let identity: Identity
    let model: DocumentViewModel
    @Binding var target: IdentityID?

    func body(content: Content) -> some View {
        content
            .overlay {
                if target == identity.id {
                    RoundedRectangle(cornerRadius: Radius.control, style: .continuous)
                        .strokeBorder(Palette.accent, lineWidth: 2)
                        .padding(.horizontal, -Space.xxs)
                        .accessibilityHidden(true)
                }
            }
            .draggable(identity.id.rawValue)
            .dropDestination(for: String.self) { items, _ in
                target = nil
                guard let dropped = items.first.map(IdentityID.init(rawValue:)),
                      model.canMerge(dropped, into: identity.id) else { return false }
                model.dragMerge(dropped, into: identity.id)
                return true
            } isTargeted: { over in
                if over {
                    target = identity.id
                } else if target == identity.id {
                    target = nil
                }
            }
    }
}
