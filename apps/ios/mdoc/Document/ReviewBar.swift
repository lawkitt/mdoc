import SwiftUI

/// The docked review bar (ADR 0098): it takes the bottom bar's place while
/// reviewing, leaves the text scrollable and tappable above it, and never
/// closes to step. Top to bottom: grabber, progress line, a hint line now
/// and then, the swipeable review card with peeks of its neighbors, the
/// mentions when pulled up, then Undo, Keep and Replace.
///
/// The bar follows the finger: drag the grabber, the progress line or the
/// card up for the identity's mentions and down again (docs/mobile-ui.md).
/// It settles compact or full; dragging down on the compact bar springs
/// back, and only × ends review.
struct ReviewBar: View {
    @Bindable var model: DocumentViewModel
    /// The most the mentions may take, so the active name stays in view
    /// (about 60% of the screen with the compact bar).
    var detailsHeight: CGFloat = 260

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The card's one-time nudge.
    @State private var drag: CGFloat = 0
    /// The finger on the header, the mentions and the card. Gesture state, so
    /// a cancelled drag (a notification, leaving the app) resets it, with a
    /// spring.
    @GestureState(resetTransaction: Transaction(animation: ReviewBar.settle)) private var headerPull: CGFloat = 0
    @GestureState(resetTransaction: Transaction(animation: ReviewBar.settle)) private var detailsPull: CGFloat = 0
    @GestureState(resetTransaction: Transaction(animation: ReviewBar.settle)) private var cardDrag = CardDrag()
    /// The mentions' own height, when less than the cap.
    @State private var contentHeight: CGFloat = 0
    @State private var detailsAtTop = true
    /// +1 stepping forward, -1 back: where the next card comes from.
    @State private var direction = 1
    @State private var grabberLift: CGFloat = 0
    @Environment(\.preferences) private var preferences

    /// A drag on the card: its axis, fixed by the first movement, and how far.
    struct CardDrag: Equatable {
        var axis: Axis?
        var dx: CGFloat = 0
        var dy: CGFloat = 0
    }

    /// How the bar settles when a finger lets go.
    static let settle = Animation.spring(response: 0.35, dampingFraction: 0.86)

    /// The bar's vertical drag, up negative, while a finger is on it.
    private var pull: CGFloat {
        headerPull + detailsPull + (cardDrag.axis == .vertical ? cardDrag.dy : 0)
    }

    /// The full height of the mentions: their own height up to the cap.
    private var fullHeight: CGFloat {
        contentHeight > 0 ? min(detailsHeight, contentHeight) : detailsHeight
    }

    /// How far open the bar is, following the finger.
    private var openHeight: CGFloat {
        (model.barExpanded ? fullHeight : 0) - pull
    }

    /// The mentions' visible height: rubber-banded past full.
    private var shownHeight: CGFloat {
        let open = openHeight
        if open <= 0 { return 0 }
        return open > fullHeight ? fullHeight + (open - fullHeight) / 4 : open
    }

    /// Below compact the bar gives a little, then springs back.
    private var sag: CGFloat {
        openHeight < 0 ? -openHeight / 5 : 0
    }

    var body: some View {
        if let candidate = model.reviewedCandidate {
            VStack(spacing: 6) {
                header
                if let hint = model.hint {
                    ReviewHintLine(hint: hint) { model.learn(hint) }
                        .transition(.opacity)
                }
                pager(candidate)
                // Kept while a finger drags them shut, so the drag isn't cut off.
                if shownHeight > 0 || detailsPull != 0 {
                    details(candidate)
                }
                ReviewButtons(model: model, candidate: candidate)
            }
            .padding(.horizontal, Space.m)
            .padding(.bottom, Space.xs)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
            .background(.bar)
            .offset(y: sag)
            // The bar's text stops growing at AX2, as the system's bars do;
            // a long press shows a label in the large content viewer.
            .dynamicTypeSize(...DynamicTypeSize.accessibility2)
            .animation(Motion.fade, value: model.hint)
            // The mentions' height, measured whether or not they show, so the
            // bar knows where full is before it opens.
            .background(alignment: .top) {
                ReviewDetails(model: model, candidate: candidate)
                    .padding(.horizontal, Space.m)
                    .frame(maxWidth: 560)
                    .fixedSize(horizontal: false, vertical: true)
                    .hidden()
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { contentHeight = $0 }
                    .accessibilityHidden(true)
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("review-bar")
            .background { keyboardShortcuts(candidate) }
            .task(id: candidate.id) { await nudgeOnce() }
            .onChange(of: model.cardsSeen) { _, seen in
                if seen == 3 { Task { await bounceGrabberOnce() } }
            }
        }
    }

    /// A hardware keyboard gets desktop's keys (ADR 0098): Return Replace,
    /// K Keep, ← → step, ⌘. (or Esc) ends review. ⌘Z and ⇧⌘Z come from the undo
    /// manager.
    private func keyboardShortcuts(_ candidate: Candidate) -> some View {
        Group {
            Button("Replace") { model.returnKey() }
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

    /// Once: the card slides a little and back, so the swipe is seen.
    private func nudgeOnce() async {
        guard ReviewHint.enabled, !preferences.nudged, !reduceMotion, model.reviewOrder.count > 1 else { return }
        preferences.nudged = true
        try? await Task.sleep(for: .milliseconds(600))
        withAnimation(.easeInOut(duration: 0.25)) { drag = -28 }
        try? await Task.sleep(for: .milliseconds(260))
        withAnimation(.spring(response: 0.35, dampingFraction: 0.6)) { drag = 0 }
    }

    /// Once, on the third card: the grabber hops, so the pull is seen. No
    /// words; Reduce Motion skips it.
    private func bounceGrabberOnce() async {
        guard ReviewHint.enabled, !preferences.grabberBounced, !reduceMotion, !model.barExpanded else { return }
        preferences.grabberBounced = true
        for _ in 0..<2 {
            withAnimation(.easeOut(duration: 0.16)) { grabberLift = -6 }
            try? await Task.sleep(for: .milliseconds(170))
            withAnimation(.spring(response: 0.3, dampingFraction: 0.5)) { grabberLift = 0 }
            try? await Task.sleep(for: .milliseconds(320))
        }
    }

    // MARK: Pulling

    /// Where the finger let go, with its speed: compact or full, whichever
    /// the fling points nearer to. Dragging down on the compact bar only
    /// springs back.
    private func pullEnded(predicted dy: CGFloat) {
        let was = model.barExpanded
        let target = (was ? fullHeight : 0) - dy
        let expand = target > fullHeight / 2
        withAnimation(reduceMotion ? Motion.fade : Self.settle) {
            model.barExpanded = expand
        }
        if expand != was {
            Haptics.selection()
            // The text keeps the name in view above the taller bar.
            if expand { model.revealReviewed() }
        }
    }

    private var pullGesture: some Gesture {
        // In screen points: the bar grows under the finger, so its own
        // coordinates would move with it.
        DragGesture(minimumDistance: 6, coordinateSpace: .global)
            .updating($headerPull) { value, pull, _ in pull = value.translation.height }
            .onEnded { pullEnded(predicted: $0.predictedEndTranslation.height) }
    }

    // MARK: Header

    /// The grabber and the progress line: both take a drag.
    private var header: some View {
        VStack(spacing: 2) {
            grabber
            ProgressLine(model: model)
        }
        .contentShape(Rectangle())
        .gesture(pullGesture)
    }

    private var grabber: some View {
        Capsule()
            .fill(.tertiary)
            .frame(width: 36, height: 5)
            .offset(y: grabberLift)
            .padding(.top, Space.xs)
            .padding(.bottom, Space.xxs)
            .frame(maxWidth: .infinity, minHeight: 22)
            .contentShape(Rectangle())
            .onTapGesture { toggle() }
            .accessibilityElement()
            .accessibilityLabel(Text(model.barExpanded ? "Hide mentions" : "Show mentions"))
            .accessibilityAddTraits(.isButton)
            .accessibilityAction { toggle() }
            .accessibilityIdentifier("review-grabber")
    }

    private func toggle() {
        withAnimation(Motion.move(reduced: reduceMotion)) { model.barExpanded.toggle() }
        Haptics.selection()
        if model.barExpanded { model.revealReviewed() }
    }

    // MARK: Mentions

    private func details(_ candidate: Candidate) -> some View {
        ScrollView {
            ReviewDetails(model: model, candidate: candidate)
        }
        .scrollBounceBehavior(.basedOnSize)
        .scrollDisabled(detailsPull != 0)
        .onScrollGeometryChange(for: Bool.self) { $0.contentOffset.y <= 0.5 } action: { _, top in detailsAtTop = top }
        // A new scroll view starts at the top.
        .onAppear { detailsAtTop = true }
        // At the top, a drag down closes the bar as the grabber's does.
        .simultaneousGesture(
            DragGesture(minimumDistance: 10, coordinateSpace: .global)
                .updating($detailsPull) { value, pull, _ in
                    guard detailsAtTop || pull != 0 else { return }
                    pull = max(0, value.translation.height)
                }
                .onEnded { value in
                    guard detailsAtTop, value.translation.height > 0 else { return }
                    pullEnded(predicted: value.predictedEndTranslation.height)
                })
        .frame(height: shownHeight, alignment: .top)
        .clipped()
        .transition(.opacity)
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
            .id(candidate.id)
            .transition(reduceMotion
                ? .opacity
                : .asymmetric(
                    insertion: .offset(x: CGFloat(direction) * 60).combined(with: .opacity),
                    removal: .offset(x: CGFloat(-direction) * 60).combined(with: .opacity)))
            .offset(x: drag + (cardDrag.axis == .horizontal ? cardDrag.dx : 0))
            .padding(.horizontal, 14)
            .background(alignment: .leading) {
                Peek(visible: hasPrevious) { swiped(-1) }
                    .accessibilityHidden(true)
            }
            .background(alignment: .trailing) {
                Peek(visible: hasNext) { swiped(1) }
                    .accessibilityHidden(true)
            }
            .fixedSize(horizontal: false, vertical: true)
            .contentShape(Rectangle())
        .simultaneousGesture(
            DragGesture(minimumDistance: 12, coordinateSpace: .global)
                .updating($cardDrag) { value, card, _ in
                    let dx = value.translation.width, dy = value.translation.height
                    if card.axis == nil { card.axis = abs(dx) >= abs(dy) ? .horizontal : .vertical }
                    card.dy = dy
                    // Rubber-band resistance where there is nothing to swipe to.
                    let blocked = (dx > 0 && !hasPrevious) || (dx < 0 && !hasNext)
                    card.dx = blocked ? dx / 4 : dx
                }
                .onEnded { value in
                    let dx = value.translation.width, dy = value.translation.height
                    if abs(dy) > abs(dx) {
                        pullEnded(predicted: value.predictedEndTranslation.height)
                        return
                    }
                    let threshold: CGFloat = 60
                    if dx < -threshold, hasNext {
                        swiped(1)
                    } else if dx > threshold, hasPrevious {
                        swiped(-1)
                    } else if abs(dx) > threshold {
                        Haptics.edge()
                    }
                })
        .accessibilityElement(children: .contain)
        .accessibilityAction(named: Text("Previous")) { step(-1) }
        .accessibilityAction(named: Text("Next")) { step(1) }
    }

    /// A swipe or a tap on a peek: the gesture is found, so its hint goes.
    private func swiped(_ delta: Int) {
        model.learn(.swipe)
        step(delta)
    }

    private func step(_ delta: Int) {
        direction = delta
        withAnimation(Motion.move(reduced: reduceMotion)) {
            if model.step(delta) {
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
                model.showIdentities(true)
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

/// Undo (Redo on a long press), then two buttons. A proposal gets Keep and
/// Replace, saying "all 3" while the scope is All; a decided card gets the
/// other choice ("Keep instead") and Next, the filled one.
private struct ReviewButtons: View {
    @Bindable var model: DocumentViewModel
    let candidate: Candidate

    private var count: Int {
        model.snapshot.identities.first { $0.id == candidate.identityID }?.mentionCount ?? 1
    }

    private var all: Bool { model.reviewScope == .allOccurrences && count > 1 }

    var body: some View {
        HStack(spacing: Space.s) {
            undo
            switch candidate.state {
            case .proposed:
                Button {
                    model.decideCurrent(.keep)
                } label: {
                    Text(all ? "Keep all \(count)" : "Keep")
                        .frame(maxWidth: .infinity, minHeight: Size.barButton)
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("review-keep")
                Button {
                    model.decideCurrent(.replace)
                } label: {
                    Text(all ? "Replace all \(count)" : "Replace")
                        .fontWeight(.semibold)
                        .frame(maxWidth: .infinity, minHeight: Size.barButton)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("review-replace")
            case .replaced, .kept:
                let keep = candidate.state == .replaced
                Button {
                    model.changeCurrent(keep ? .keep : .replace)
                } label: {
                    Text(keep ? "Keep instead" : "Replace instead")
                        .frame(maxWidth: .infinity, minHeight: Size.barButton)
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier(keep ? "review-keep" : "review-replace")
                Button {
                    model.goToNextUndecided()
                } label: {
                    HStack(spacing: Space.xxs) {
                        Text(model.documentIsDone ? "Done" : "Next")
                        if !model.documentIsDone {
                            Image(systemName: "arrow.right")
                                .accessibilityHidden(true)
                        }
                    }
                    .fontWeight(.semibold)
                    .frame(maxWidth: .infinity, minHeight: Size.barButton)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityHint(Text(model.documentIsDone ? "Ends review." : "Goes to the next name to decide."))
                .accessibilityIdentifier("review-next")
            }
        }
        .lineLimit(1)
        .minimumScaleFactor(0.8)
    }

    private var undo: some View {
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
    }
}
