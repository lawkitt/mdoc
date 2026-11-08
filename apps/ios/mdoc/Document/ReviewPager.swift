import SwiftUI

/// The review bar's cards as a carousel (docs/mobile-ui.md, Review bar):
/// the previous, current and next cards side by side in one strip, the
/// neighbors peeking in at the bar's edges. The finger moves the whole strip;
/// let go past a third of a card, or with a fling, and it settles on the
/// neighbor with one spring, otherwise it springs back. At either end it
/// gives a little and bumps. Next and Previous (keys, VoiceOver) and a tap
/// on a peek move the same strip. Reduce Motion cross-fades in place.
///
/// Each card is placed by its place in the bar's order less the current
/// card's, so stepping moves every card by one pitch in the same animation,
/// whichever way: the card coming in is fully drawn as it slides.
struct ReviewPager: View {
    @Bindable var model: DocumentViewModel
    let candidate: Candidate
    /// The card's one-time nudge, which moves the strip a little and back.
    var nudge: CGFloat = 0
    /// The bar's vertical drag from the card, which the bar reads.
    let pull: GestureState<ReviewBar.CardDrag>
    /// A vertical drag let go: the bar settles compact or full.
    let onPullEnded: (CGFloat) -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The finger's sideways travel, resisted at the ends.
    @State private var swipe: CGFloat = 0
    /// The drag's axis, fixed by its first movement.
    @State private var axis: Axis?
    /// The strip's width: the bar's, out to its edges.
    @State private var width: CGFloat = 0

    /// A card's width: the strip less a peek's room at each side.
    private var cardWidth: CGFloat { max(0, width - 2 * (Space.m + ReviewBarMetrics.cardInset)) }
    /// From one card to the next.
    private var pitch: CGFloat { cardWidth + ReviewBarMetrics.cardGap }

    /// One card in the strip, keyed by its candidate; under Reduce Motion by
    /// its place, so the strip stays put and the card in it cross-fades.
    private struct Item: Identifiable {
        enum Key: Hashable {
            case card(CandidateID)
            case place(Int)
        }

        let id: Key
        let card: Candidate
        /// Places from the current card.
        let place: Int
    }

    private var items: [Item] {
        let order = model.reviewOrder
        guard let index = order.index(of: candidate.id) else {
            return [Item(id: .card(candidate.id), card: candidate, place: 0)]
        }
        // Two each side, so the card that comes in as a neighbor slides in
        // from off the bar's edge.
        let reach = reduceMotion ? 1 : 2
        let range = max(0, index - reach)...min(order.count - 1, index + reach)
        return range.map { at in
            let card = order.candidates[at]
            return Item(id: reduceMotion ? .place(at - index) : .card(card.id), card: card, place: at - index)
        }
    }

    var body: some View {
        let order = model.reviewOrder
        let index = order.index(of: candidate.id) ?? 0
        let hasPrevious = index > 0
        let hasNext = index < order.count - 1
        ZStack(alignment: .top) {
            ForEach(items) { item in
                card(item)
            }
        }
        .frame(maxWidth: .infinity, alignment: .top)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .contentShape(Rectangle())
        .clipped()
        // Out to the bar's edges, so a card slides off them.
        .padding(.horizontal, -Space.m)
        .simultaneousGesture(drag(hasPrevious: hasPrevious, hasNext: hasNext))
        // A cancelled drag (a notification, leaving the app) puts it back.
        .onChange(of: pull.wrappedValue.axis == nil) { _, ended in
            if ended, swipe != 0 { settle(by: 0) }
            if ended { axis = nil }
        }
        .accessibilityElement(children: .contain)
        .accessibilityAction(named: Text("Previous")) { Self.step(model, -1, reduceMotion: reduceMotion) }
        .accessibilityAction(named: Text("Next")) { Self.step(model, 1, reduceMotion: reduceMotion) }
    }

    @ViewBuilder
    private func card(_ item: Item) -> some View {
        let current = item.place == 0
        let x = CGFloat(item.place) * pitch + (reduceMotion ? 0 : swipe + nudge)
        // The neighbors dim with their distance from the middle.
        let dim: CGFloat = pitch > 0 ? min(1, abs(x) / pitch) : (current ? 0 : 1)
        ReviewCard(model: model, candidate: item.card)
            .id(item.card.id)
            .transition(.opacity)
            .frame(width: width > 0 ? cardWidth : nil)
            .fixedSize(horizontal: false, vertical: true)
            .allowsHitTesting(current)
            .overlay {
                if !current {
                    // A tap on a peek steps to it.
                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture { swiped(item.place > 0 ? 1 : -1) }
                }
            }
            .opacity(Double(1 - 0.45 * dim))
            // Only the current card sets the strip's height; a neighbor
            // hangs from its top.
            .frame(height: current ? nil : 0, alignment: .top)
            .offset(x: x)
            // Only the card under review is there for VoiceOver.
            .accessibilityHidden(!current)
    }

    // MARK: Swiping

    private func drag(hasPrevious: Bool, hasNext: Bool) -> some Gesture {
        DragGesture(minimumDistance: 12, coordinateSpace: .global)
            .updating(pull) { value, card, _ in
                let dx = value.translation.width, dy = value.translation.height
                if card.axis == nil { card.axis = abs(dx) >= abs(dy) ? .horizontal : .vertical }
                card.dy = dy
            }
            .onChanged { value in
                let dx = value.translation.width, dy = value.translation.height
                if axis == nil { axis = abs(dx) >= abs(dy) ? .horizontal : .vertical }
                guard axis == .horizontal, !reduceMotion else { return }
                // Close to the finger, merging with a settle still running.
                withAnimation(.interactiveSpring(response: 0.12, dampingFraction: 0.9)) {
                    swipe = resisted(dx, hasPrevious: hasPrevious, hasNext: hasNext)
                }
            }
            .onEnded { value in
                defer { axis = nil }
                let dx = value.translation.width, dy = value.translation.height
                if axis == .vertical || (axis == nil && abs(dy) > abs(dx)) {
                    onPullEnded(value.predictedEndTranslation.height)
                    return
                }
                let predicted = value.predictedEndTranslation.width
                let threshold = max(ReviewBarMetrics.swipeThreshold, cardWidth / 3)
                let delta = dx < -threshold || predicted < -pitch / 2 ? 1
                    : dx > threshold || predicted > pitch / 2 ? -1 : 0
                if delta == 1 && hasNext || delta == -1 && hasPrevious {
                    model.learn(.swipe)
                    settle(by: delta)
                } else {
                    let blocked = (dx > 0 && !hasPrevious) || (dx < 0 && !hasNext)
                    if blocked && abs(dx) > ReviewBarMetrics.swipeThreshold { Haptics.edge() }
                    settle(by: 0)
                }
            }
    }

    /// Where there is nothing to swipe to, the strip gives a quarter of the
    /// finger's travel; never more than one card either way.
    private func resisted(_ dx: CGFloat, hasPrevious: Bool, hasNext: Bool) -> CGFloat {
        let blocked = (dx > 0 && !hasPrevious) || (dx < 0 && !hasNext)
        let moved = blocked ? dx / 4 : dx
        return pitch > 0 ? min(pitch, max(-pitch, moved)) : moved
    }

    /// The strip comes to rest: `delta` cards on (one spring, from where the
    /// finger let go), or back where it was.
    private func settle(by delta: Int) {
        if delta == 0 {
            withAnimation(reduceMotion ? Motion.fade : ReviewBar.settle) { swipe = 0 }
            return
        }
        withAnimation(reduceMotion ? Motion.fade : ReviewBar.settle) {
            swipe = 0
            if model.step(delta) { Haptics.selection() } else { Haptics.edge() }
        }
    }

    /// A tap on a peek: the gesture is found, so its hint goes.
    private func swiped(_ delta: Int) {
        model.learn(.swipe)
        Self.step(model, delta, reduceMotion: reduceMotion)
    }

    /// Steps `delta` cards with the strip's motion: Next and Previous, the
    /// keys, VoiceOver and the peeks.
    static func step(_ model: DocumentViewModel, _ delta: Int, reduceMotion: Bool) {
        withAnimation(reduceMotion ? Motion.fade : ReviewBar.settle) {
            if model.step(delta) {
                Haptics.selection()
            } else {
                Haptics.edge()
            }
        }
    }
}
