import SwiftUI
import TipKit

/// The docked review bar (ADR 0098): it takes the bottom bar's place while
/// reviewing, leaves the text scrollable and tappable above it, and never
/// closes to step. Top to bottom: grabber, progress line, the swipeable
/// review card with peeks of its neighbors, then Undo, Keep and Replace.
struct ReviewBar: View {
    @Bindable var model: DocumentViewModel
    /// The most the expanded details may take, so the active name stays in
    /// view (about 45% of the screen with the compact bar).
    var detailsHeight: CGFloat = 260

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var drag: CGFloat = 0
    /// +1 stepping forward, -1 back: where the next card comes from.
    @State private var direction = 1
    /// The one-time nudge that shows the card moves (ADR 0098).
    @AppStorage("reviewNudged") private var nudged = false

    var body: some View {
        if let candidate = model.reviewedCandidate {
            VStack(spacing: 6) {
                grabber
                ProgressLine(model: model)
                pager(candidate)
                if model.barExpanded {
                    ReviewDetails(model: model, candidate: candidate)
                        .frame(maxHeight: detailsHeight)
                        .transition(.opacity)
                }
                ReviewButtons(model: model, candidate: candidate)
            }
            .padding(.horizontal, Space.m)
            .padding(.bottom, Space.xs)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
            .background(.bar)
            // The bar's text stops growing at AX2, as the system's bars do;
            // a long press shows a label in the large content viewer.
            .dynamicTypeSize(...DynamicTypeSize.accessibility2)
            .animation(Motion.move(reduced: reduceMotion), value: model.barExpanded)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("review-bar")
            .background { keyboardShortcuts(candidate) }
            .task(id: candidate.id) { await nudgeOnce() }
        }
    }

    /// A hardware keyboard gets desktop's keys (ADR 0098): Return Replace,
    /// K Keep, ← → step, ⌘. (or Esc) ends review. ⌘Z and ⇧⌘Z come from the undo
    /// manager.
    private func keyboardShortcuts(_ candidate: Candidate) -> some View {
        Group {
            Button("Replace") { model.decideCurrent(.replace) }
                .keyboardShortcut(.return, modifiers: [])
            Button("Keep") { model.decideCurrent(.keep) }
                .keyboardShortcut("k", modifiers: [])
            Button("Previous") { step(-1) }
                .keyboardShortcut(.leftArrow, modifiers: [])
            Button("Next") { step(1) }
                .keyboardShortcut(.rightArrow, modifiers: [])
            Button("End review") { model.endReview() }
                .keyboardShortcut(".", modifiers: .command)
            // Esc too, where iOS passes it to the app.
            Button("End review ") { model.endReview() }
                .keyboardShortcut(.cancelAction)
        }
        .opacity(0)
        .frame(width: 0, height: 0)
        .accessibilityHidden(true)
    }

    /// Once: the card slides a little and back, so the swipe is seen; with
    /// Reduce Motion the tip's caption says it instead.
    private func nudgeOnce() async {
        guard !nudged, !reduceMotion, model.reviewOrder.count > 1 else { return }
        nudged = true
        try? await Task.sleep(for: .milliseconds(600))
        withAnimation(.easeInOut(duration: 0.25)) { drag = -28 }
        try? await Task.sleep(for: .milliseconds(260))
        withAnimation(.spring(response: 0.35, dampingFraction: 0.6)) { drag = 0 }
    }

    // MARK: Grabber

    private var grabber: some View {
        Capsule()
            .fill(.tertiary)
            .frame(width: 36, height: 5)
            .padding(.top, 6)
            .frame(maxWidth: .infinity, minHeight: 14)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 12)
                    .onEnded { value in
                        let dy = value.translation.height
                        if dy < -30 {
                            model.barExpanded = true
                        } else if dy > 30 {
                            if model.barExpanded {
                                model.barExpanded = false
                            } else {
                                model.endReview()
                            }
                        }
                    })
            .onTapGesture { model.barExpanded.toggle() }
            .accessibilityElement()
            .accessibilityLabel(Text(model.barExpanded ? "Hide details" : "Show details"))
            .accessibilityAddTraits(.isButton)
            .accessibilityAction { model.barExpanded.toggle() }
            .accessibilityIdentifier("review-grabber")
            .popoverTip(DetailsTip(), arrowEdge: .bottom)
    }

    // MARK: Pager

    private func pager(_ candidate: Candidate) -> some View {
        let order = model.reviewOrder
        let index = order.index(of: candidate.id) ?? 0
        let hasPrevious = index > 0
        let hasNext = index < order.count - 1
        // The neighbors peek out at the edges, as tall as the card and no
        // taller: overlays, so they never stretch the bar.
        return ReviewCard(model: model, candidate: candidate)
            .popoverTip(SwipeTip(), arrowEdge: .bottom)
            .id(candidate.id)
            .transition(reduceMotion
                ? .opacity
                : .asymmetric(
                    insertion: .offset(x: CGFloat(direction) * 60).combined(with: .opacity),
                    removal: .offset(x: CGFloat(-direction) * 60).combined(with: .opacity)))
            .offset(x: drag)
            .padding(.horizontal, 14)
            .background(alignment: .leading) {
                Peek(visible: hasPrevious) { step(-1) }
                    .accessibilityHidden(true)
            }
            .background(alignment: .trailing) {
                Peek(visible: hasNext) { step(1) }
                    .accessibilityHidden(true)
            }
            .fixedSize(horizontal: false, vertical: true)
            .contentShape(Rectangle())
        .simultaneousGesture(
            DragGesture(minimumDistance: 16)
                .onChanged { value in
                    let dx = value.translation.width
                    guard abs(dx) > abs(value.translation.height) else { return }
                    // Rubber-band resistance where there is nothing to swipe to.
                    let blocked = (dx > 0 && !hasPrevious) || (dx < 0 && !hasNext)
                    drag = blocked ? dx / 4 : dx
                }
                .onEnded { value in
                    let dx = value.translation.width
                    let threshold: CGFloat = 60
                    if dx < -threshold, hasNext {
                        drag = 0
                        step(1)
                    } else if dx > threshold, hasPrevious {
                        drag = 0
                        step(-1)
                    } else {
                        if abs(dx) > threshold { Haptics.edge() }
                        withAnimation(.spring(response: 0.3, dampingFraction: 0.7)) { drag = 0 }
                    }
                })
        .accessibilityElement(children: .contain)
        .accessibilityAction(named: Text("Previous")) { step(-1) }
        .accessibilityAction(named: Text("Next")) { step(1) }
    }

    private func step(_ delta: Int) {
        direction = delta
        withAnimation(Motion.move(reduced: reduceMotion)) {
            if model.step(delta) {
                DetailsTip.stepped = true
                Haptics.selection()
            } else {
                Haptics.edge()
            }
        }
    }
}

/// The sliver of the previous or next card at the bar's edge: it shows
/// there is more to swipe to, and a tap steps.
private struct Peek: View {
    let visible: Bool
    let action: () -> Void

    var body: some View {
        RoundedRectangle(cornerRadius: Radius.control, style: .continuous)
            .fill(Palette.surface)
            .opacity(visible ? 0.7 : 0)
            .frame(width: 22)
            .padding(.vertical, Space.s)
            .contentShape(Rectangle().inset(by: -8))
            .onTapGesture { if visible { action() } }
    }
}

/// "3 of 15" beside a thin track filled with the decided share, or the
/// focus ("ORG_1 · 1 of 3 ×"). Tapping it opens Identities; × ends review.
private struct ProgressLine: View {
    @Bindable var model: DocumentViewModel

    var body: some View {
        let order = model.reviewOrder
        let index = (order.index(of: model.reviewing) ?? 0) + 1
        HStack(spacing: Space.xs) {
            Button {
                model.showsIdentities = true
            } label: {
                HStack(spacing: Space.xs) {
                    Track(fraction: order.count == 0 ? 0 : Double(order.decided) / Double(order.count))
                    Text(label(index: index, count: order.count))
                        .font(Typography.progress)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .frame(minHeight: 24)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text(spoken(index: index, count: order.count, decided: order.decided)))
            .accessibilityHint(Text("Opens Identities."))
            .accessibilityIdentifier("review-progress")
            if model.focus != nil {
                Button {
                    model.clearFocus()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .accessibilityLabel(Text("Review the whole document"))
                .accessibilityIdentifier("review-clear-focus")
            }
            Spacer(minLength: 0)
            Button {
                model.endReview()
            } label: {
                Image(systemName: "xmark")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 28, height: 28)
                    .background(Palette.chipFill, in: Circle())
            }
            .frame(minWidth: Size.target, minHeight: 28)
            .accessibilityLabel(Text("End review"))
            .accessibilityIdentifier("review-end")
        }
    }

    private func label(index: Int, count: Int) -> String {
        switch model.focus {
        case .identity(let id):
            let alias = model.snapshot.identities.first { $0.id == id }?.alias ?? ""
            return String(localized: "\(alias) · \(index) of \(count)")
        case .category(let category):
            return String(localized: "\(category.label) · \(index) of \(count)")
        case nil:
            return String(localized: "\(index) of \(count)")
        }
    }

    private func spoken(index: Int, count: Int, decided: Int) -> String {
        String(localized: "\(index) of \(count), \(decided) decided")
    }
}

private struct Track: View {
    let fraction: Double

    var body: some View {
        Capsule()
            .fill(Palette.chipFill)
            .frame(width: 56, height: 4)
            .overlay(alignment: .leading) {
                Capsule()
                    .fill(Palette.accent)
                    .frame(width: 56 * max(0, min(1, fraction)), height: 4)
            }
            .accessibilityHidden(true)
    }
}

/// Undo (Redo on a long press), Keep and Replace, 40 pt high. A decided
/// card offers the other choice instead ("Keep instead").
private struct ReviewButtons: View {
    @Bindable var model: DocumentViewModel
    let candidate: Candidate

    var body: some View {
        HStack(spacing: Space.s) {
            Button {
                model.undoTapped()
            } label: {
                Label("Undo", systemImage: "arrow.uturn.backward")
                    .labelStyle(.iconOnly)
                    .frame(width: Size.barButton, height: Size.barButton)
            }
            .buttonStyle(.bordered)
            .disabled(!model.canUndo)
            .contextMenu {
                Button {
                    model.redoTapped()
                } label: {
                    Label("Redo", systemImage: "arrow.uturn.forward")
                }
                .disabled(!model.canRedo)
            }
            .accessibilityLabel(Text("Undo"))
            .accessibilityAction(named: Text("Redo")) { model.redoTapped() }
            .accessibilityIdentifier("review-undo")
            .popoverTip(UndoTip(), arrowEdge: .bottom)

            Button {
                model.decideCurrent(.keep)
            } label: {
                Text(candidate.state == .replaced ? "Keep instead" : candidate.state == .kept ? "Kept" : "Keep")
                    .frame(maxWidth: .infinity, minHeight: Size.barButton)
            }
            .buttonStyle(.bordered)
            .disabled(candidate.state == .kept)
            .accessibilityIdentifier("review-keep")

            Button {
                model.decideCurrent(.replace)
            } label: {
                Text(candidate.state == .kept ? "Replace instead" : candidate.state == .replaced ? "Replaced" : "Replace")
                    .fontWeight(.semibold)
                    .frame(maxWidth: .infinity, minHeight: Size.barButton)
            }
            .buttonStyle(.borderedProminent)
            .disabled(candidate.state == .replaced)
            .accessibilityIdentifier("review-replace")
        }
    }
}
