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

    /// "Same as PERSON_1" while a drag that this row can take is over it.
    func dropHint(on identity: Identity) -> String? {
        guard dropTarget == identity.id, model.canMerge(dragged, into: identity.id) else { return nil }
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
/// as a drag, and takes another row's drop, outlined while the drag is
/// over it. The row being dragged takes no drop of itself. The drop is the
/// core's link: one undo step, with a haptic and an Undo toast.
struct IdentityDrop: ViewModifier {
    let identity: Identity
    let model: DocumentViewModel
    @Binding var dragged: IdentityID?
    @Binding var target: IdentityID?

    private var takes: Bool {
        target == identity.id && model.canMerge(dragged, into: identity.id)
    }

    func body(content: Content) -> some View {
        content
            .overlay {
                if takes {
                    RoundedRectangle(cornerRadius: Radius.control, style: .continuous)
                        .strokeBorder(Palette.accent, lineWidth: 2)
                        .padding(.horizontal, -Space.xxs)
                        .accessibilityHidden(true)
                }
            }
            .onDrag {
                dragged = identity.id
                return NSItemProvider(object: identity.id.rawValue as NSString)
            }
            .dropDestination(for: String.self) { items, _ in
                let dropped = items.first.map(IdentityID.init(rawValue:))
                defer {
                    dragged = nil
                    target = nil
                }
                guard let dropped, model.canMerge(dropped, into: identity.id) else { return false }
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
