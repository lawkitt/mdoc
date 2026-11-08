import MdocFFI
import SwiftUI

/// Drag-to-merge (ADR 0106, desktop's ADR 0025): in Select each row has a ≡
/// handle at its trailing edge. Dragging it lifts a copy of the row that
/// follows the finger, and the row under the finger shows the ink outline
/// and "Same as PERSON_1"; near the list's top or bottom the list scrolls.
/// Letting go over a name is `DocumentViewModel.dragMerge` (one undo step, a
/// haptic, "PERSON_3 is now PERSON_1 · Undo"); anywhere else the copy
/// slides back. VoiceOver has Make one with… on the handle instead.
extension IdentitiesView {
    /// A row's ≡ handle, where the state mark is outside Select.
    func dragHandle(_ identity: Identity) -> some View {
        HandleSymbol()
            // The drag itself is on the list (`HandleDragRecognizer`).
            .background(HandleDragInstaller(recognizer: dragTracking.recognizer, onEvent: handleDrag))
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { dragTracking.handles[identity.id] = $0 }
            .onDisappear { dragTracking.handles[identity.id] = nil }
            .accessibilityElement()
            .accessibilityLabel(Text("Drag to make one with another name"))
            .accessibilityAction(named: Text("Make one with…")) { onPickSameAs(identity.id) }
            .accessibilityIdentifier("identity-drag-handle")
    }

    /// The lifted row over the list, following the finger.
    var liftedCopy: some View {
        GeometryReader { proxy in
            if let drag, let identity = identities.first(where: { $0.id == drag.dragged }) {
                let origin = proxy.frame(in: .global).origin
                let frame = drag.copyFrame
                HStack(spacing: Space.s) {
                    rowLine(identity, selected: selection.contains(identity.id))
                    HandleSymbol()
                }
                .padding(.vertical, Space.xxs)
                .frame(width: frame.width, height: frame.height)
                .background {
                    RoundedRectangle(cornerRadius: Radius.control, style: .continuous)
                        .fill(Palette.surface)
                        .padding(.horizontal, -Space.xs)
                        .shadow(color: .black.opacity(0.2), radius: 12, y: 4)
                }
                .scaleEffect(reduceMotion ? 1 : 1.02)
                .offset(x: frame.minX - origin.x, y: frame.minY - origin.y)
                .transition(.opacity)
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    /// The handle's finger, from down to up.
    func handleDrag(_ event: HandleDragEvent) {
        switch event {
        case let .began(id, point):
            guard selecting, drag == nil || drag?.returning == true, let frame = dragTracking.rows[id] else { return }
            dragTracking.autoScroll.onScroll = { retarget() }
            withAnimation(Motion.move(reduced: reduceMotion)) {
                drag = IdentityDragState(dragged: id, frame: frame, start: point, location: point)
            }
            Haptics.selection()
        case let .moved(point):
            guard var current = drag, !current.returning else { return }
            current.location = point
            drag = current
            retarget()
            dragTracking.autoScroll.speed = IdentityDrag.scrollSpeed(at: point.y, visible: dragTracking.visible)
        case .ended:
            dragTracking.autoScroll.speed = 0
            drop()
        case .cancelled:
            dragTracking.autoScroll.speed = 0
            slideBack()
        }
    }

    /// The row under the finger, after it moves or the list scrolls under it.
    private func retarget() {
        guard var current = drag, !current.returning else { return }
        let target = IdentityDrag.target(
            at: current.location, rows: dragTracking.rows, visible: dragTracking.visible, dragged: current.dragged)
        guard target != current.target else { return }
        current.target = target
        withAnimation(Motion.move(reduced: reduceMotion)) { drag = current }
        if target != nil { Haptics.selection() }
    }

    /// Let go: over a name, the two become one, and the selection loses the
    /// name that went; anywhere else, the copy goes back.
    private func drop() {
        guard let current = drag, !current.returning else { return }
        guard let target = current.target, model.canMerge(current.dragged, into: target) else {
            slideBack()
            return
        }
        withAnimation(Motion.move(reduced: reduceMotion)) {
            model.dragMerge(current.dragged, into: target)
            selection.remove(current.dragged)
            drag = nil
        }
    }

    /// The copy slides back onto its row, where the row is now (the list may
    /// have scrolled), then goes; under Reduce Motion it fades where it is.
    private func slideBack() {
        guard var current = drag else { return }
        guard !reduceMotion else {
            withAnimation(Motion.fade) { drag = nil }
            return
        }
        let now = dragTracking.rows[current.dragged] ?? current.frame
        current.target = nil
        current.returning = true
        current.location.y = current.start.y + (now.minY - current.frame.minY)
        let returning = current
        withAnimation(Motion.snappy, completionCriteria: .logicallyComplete) {
            drag = returning
        } completion: {
            if drag == returning { drag = nil }
        }
    }
}

/// ≡, at a full 44-point target.
private struct HandleSymbol: View {
    var body: some View {
        Image(systemName: "line.3.horizontal")
            .font(.body.weight(.medium))
            .foregroundStyle(.secondary)
            .frame(width: Size.target, height: Size.target)
            .contentShape(Rectangle())
    }
}

/// A row's part in a drag: it reports its frame, dims while lifted, and
/// takes the ink outline while the finger is over it.
struct DragMergeRow: ViewModifier {
    let id: IdentityID
    let lifted: Bool
    let targeted: Bool
    let tracking: IdentityDragTracking

    func body(content: Content) -> some View {
        content
            .opacity(lifted ? 0.35 : 1)
            .overlay {
                if targeted {
                    RoundedRectangle(cornerRadius: Radius.control, style: .continuous)
                        .strokeBorder(Palette.accent, lineWidth: 2)
                        .padding(.horizontal, -Space.xxs)
                        .accessibilityHidden(true)
                }
            }
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { tracking.rows[id] = $0 }
            .onDisappear { tracking.rows[id] = nil }
    }
}
