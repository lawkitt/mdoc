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
    /// The mentions' scroll, back at the top each time they open.
    @State private var detailsScroll = ScrollPosition(edge: .top)
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
                VStack(spacing: 0) {
                    // Always built, at no height while the bar is compact, so
                    // the bar knows where full is before it opens; spaced
                    // like the rest while open or while a finger drags it.
                    details(candidate)
                        .padding(.bottom, detailsShown ? 6 : 0)
                    ReviewButtons(model: model, candidate: candidate)
                }
            }
            .padding(.horizontal, Space.m)
            .padding(.bottom, Space.xs)
            .frame(maxWidth: ReviewBarMetrics.maxWidth)
            .frame(maxWidth: .infinity)
            .background(.bar)
            .offset(y: sag)
            // The bar's text stops growing at AX2, as the system's bars do;
            // a long press shows a label in the large content viewer.
            .dynamicTypeSize(...DynamicTypeSize.accessibility2)
            .animation(Motion.fade, value: model.hint)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("review-bar")
            .background { keyboardShortcuts(candidate) }
            .task(id: candidate.id) { await nudgeOnce() }
            .onChange(of: model.review.cardsSeen) { _, seen in
                if seen == 3 { Task { await bounceGrabberOnce() } }
            }
        }
    }

    /// A hardware keyboard gets desktop's keys (ADR 0098): Return Replace,
    /// K Keep, ← → step, ⌘. (or Esc) ends review. ⌘Z and ⇧⌘Z come from the undo
    /// manager.
    private func keyboardShortcuts(_ candidate: Candidate) -> some View {
        Group {
            Button("Replace") { key(.return, candidate) }
                .keyboardShortcut(.return, modifiers: [])
            Button("Keep") { key(.keep, candidate) }
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

    /// A key's action on the card, as the model gives it for its state.
    private func key(_ key: ReviewKey, _ candidate: Candidate) {
        if let action = model.keyAction(key, for: candidate) { model.perform(action) }
    }

    /// Once: the card slides a little and back, so the swipe is seen.
    private func nudgeOnce() async {
        guard ReviewHint.enabled, !preferences.nudged, !reduceMotion, model.reviewOrder.count > 1 else { return }
        preferences.nudged = true
        // timing: the nudge's beats, a pause after the card shows, then out and back (ADR 0098)
        try? await Task.sleep(for: .milliseconds(600))
        withAnimation(.easeInOut(duration: 0.25)) { drag = -28 }
        // timing: the nudge's beats (ADR 0098)
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
            // timing: the grabber's hop, up and down (ADR 0101)
            try? await Task.sleep(for: .milliseconds(170))
            withAnimation(.spring(response: 0.3, dampingFraction: 0.5)) { grabberLift = 0 }
            // timing: the grabber's hop (ADR 0101)
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
        // Opening it, the text keeps the name in view above the taller bar.
        withAnimation(reduceMotion ? Motion.fade : Self.settle) {
            model.setBarExpanded(expand)
        }
        if expand != was {
            Haptics.selection()
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
        withAnimation(Motion.move(reduced: reduceMotion)) { model.setBarExpanded(!model.barExpanded) }
        Haptics.selection()
    }

    // MARK: Mentions

    /// The mentions show, or a finger drags them: kept while it drags them
    /// shut, so the drag isn't cut off.
    private var detailsShown: Bool { shownHeight > 0 || detailsPull != 0 }

    private func details(_ candidate: Candidate) -> some View {
        ScrollView {
            ReviewDetails(model: model, candidate: candidate)
        }
        .scrollPosition($detailsScroll)
        .scrollBounceBehavior(.basedOnSize)
        .scrollDisabled(detailsPull != 0)
        .onScrollGeometryChange(for: Bool.self) { $0.contentOffset.y <= 0.5 } action: { _, top in detailsAtTop = top }
        // Their own height, measured while compact too.
        .onScrollGeometryChange(for: CGFloat.self) { $0.contentSize.height } action: { _, height in contentHeight = height }
        // Shut, they open at the top again.
        .onChange(of: detailsShown) { _, shown in
            if !shown {
                detailsScroll.scrollTo(edge: .top)
                detailsAtTop = true
            }
        }
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
        .opacity(detailsShown ? 1 : 0)
        .allowsHitTesting(detailsShown)
        .disabled(!detailsShown)
        .accessibilityHidden(!detailsShown)
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
                    insertion: .offset(x: CGFloat(direction) * ReviewBarMetrics.cardSlide).combined(with: .opacity),
                    removal: .offset(x: CGFloat(-direction) * ReviewBarMetrics.cardSlide).combined(with: .opacity)))
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
                    let threshold = ReviewBarMetrics.swipeThreshold
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
            .frame(width: ReviewBarMetrics.trackWidth, height: 4)
            .overlay(alignment: .leading) {
                Capsule()
                    .fill(Palette.accent)
                    .frame(width: ReviewBarMetrics.trackWidth * max(0, min(1, fraction)), height: 4)
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

    private var all: Bool { model.reviewScope == .all && count > 1 }

    var body: some View {
        let actions = model.cardActions(for: candidate)
        HStack(spacing: Space.s) {
            undo
            button(actions.other, filled: false)
            button(actions.filled, filled: true)
        }
        .lineLimit(1)
        .minimumScaleFactor(0.8)
    }

    /// One of the card's buttons, its words by the action.
    @ViewBuilder
    private func button(_ action: CandidateAction, filled: Bool) -> some View {
        let label = Group {
            switch action {
            case .keep: Text(all ? "Keep all \(count)" : "Keep")
            case .replace: Text(all ? "Replace all \(count)" : "Replace")
            case .keepInstead: Text("Keep instead")
            case .replaceInstead: Text("Replace instead")
            case .next:
                HStack(spacing: Space.xxs) {
                    Text(model.documentIsDone ? "Done" : "Next")
                    if !model.documentIsDone {
                        Image(systemName: "arrow.right")
                            .accessibilityHidden(true)
                    }
                }
            case .replaceThisOne, .showOriginal, .sameAs, .changeCategory:
                EmptyView()
            }
        }
        .fontWeight(filled ? .semibold : nil)
        .frame(maxWidth: .infinity, minHeight: Size.barButton)
        let tap = Button { model.perform(action) } label: { label }
        if filled {
            tap.buttonStyle(.borderedProminent)
                .modifier(ButtonAccessibility(action: action, documentIsDone: model.documentIsDone))
        } else {
            tap.buttonStyle(.bordered)
                .modifier(ButtonAccessibility(action: action, documentIsDone: model.documentIsDone))
        }
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

/// A card button's hint and test identifier.
private struct ButtonAccessibility: ViewModifier {
    let action: CandidateAction
    let documentIsDone: Bool

    func body(content: Content) -> some View {
        switch action {
        case .next:
            content
                .accessibilityHint(Text(documentIsDone ? "Ends review." : "Goes to the next name to decide."))
                .accessibilityIdentifier("review-next")
        case .keep, .keepInstead:
            content.accessibilityIdentifier("review-keep")
        case .replace, .replaceInstead:
            content.accessibilityIdentifier("review-replace")
        case .replaceThisOne, .showOriginal, .sameAs, .changeCategory:
            content
        }
    }
}
