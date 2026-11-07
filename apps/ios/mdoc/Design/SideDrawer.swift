import SwiftUI

/// A panel under the screen that the screen slides aside to show, as the
/// Claude app's sidebar does (docs/mobile-ui.md, Sidebar): the content
/// moves toward the far edge, keeps a dimmed sliver with rounded corners,
/// and a tap on the sliver or a swipe back closes the panel. It opens from
/// a swipe at the near edge of the screen or from a button that sets
/// `isOpen`, and follows the finger both ways.
struct SideDrawer<Panel: View, Content: View>: View {
    enum Side { case leading, trailing }

    var side: Side = .leading
    @Binding var isOpen: Bool
    /// What VoiceOver's close action and the sliver are called.
    var closeLabel: LocalizedStringKey = "Close sidebar"
    @ViewBuilder var panel: () -> Panel
    @ViewBuilder var content: () -> Content

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.layoutDirection) private var layoutDirection
    /// The finger's travel while dragging, toward the open side positive.
    @State private var drag: CGFloat = 0
    @State private var dragging = false
    /// The panel is drawn only while it shows, or is about to: closed, it
    /// isn't there for VoiceOver, taps or tests (its UIKit views would be,
    /// hidden or not), and it is made fresh for each opening.
    @State private var showsPanel = false
    /// Follows `isOpen` with the drawer's own motion, so the panel goes
    /// once the screen has slid back over it (the animation's completion).
    @State private var settled = false

    /// The sliver of the screen left showing while the panel is open.
    static var sliver: CGFloat { 72 }
    /// The width of the edge that opens the panel with a swipe.
    static var edgeWidth: CGFloat { 22 }

    private func panelWidth(_ total: CGFloat) -> CGFloat {
        min(360, max(260, total - Self.sliver))
    }

    /// +1 when the content moves right to open, -1 when it moves left.
    private var sign: CGFloat {
        let leading: CGFloat = side == .leading ? 1 : -1
        return layoutDirection == .rightToLeft ? -leading : leading
    }

    var body: some View {
        GeometryReader { geometry in
            let width = panelWidth(geometry.size.width)
            let open = (isOpen ? width : 0) + drag
            let shown = min(max(open, 0), width)
            let progress = width > 0 ? shown / width : 0
            ZStack(alignment: side == .leading ? .leading : .trailing) {
                if showsPanel || isOpen || dragging {
                    panel()
                        .frame(width: width)
                        .frame(maxHeight: .infinity)
                        // A little parallax: the panel comes from under the content.
                        .offset(x: sign * -(1 - progress) * 24)
                        .opacity(reduceMotion ? Double(progress) : 1)
                        .accessibilityAddTraits(isOpen ? .isModal : [])
                        .accessibilityAction(.escape) { setOpen(false) }
                        // Closed, it is not there for VoiceOver (or tests); last,
                        // so nothing after it brings it back.
                        .accessibilityHidden(!isOpen)
                        .allowsHitTesting(isOpen || dragging)
                }
                content()
                    .frame(width: geometry.size.width, height: geometry.size.height)
                    .background(Palette.background)
                    .overlay {
                        sliverOverlay(progress: progress)
                    }
                    .clipShape(RoundedRectangle(cornerRadius: progress > 0 ? Radius.screen : 0, style: .continuous))
                    .shadow(color: .black.opacity(0.18 * progress), radius: 24 * progress)
                    .offset(x: reduceMotion ? 0 : sign * shown)
                    .opacity(reduceMotion ? Double(1 - 0.85 * progress) : 1)
                    .accessibilityHidden(isOpen)
                if !isOpen {
                    // The near edge opens the panel with a swipe.
                    Color.clear
                        .frame(width: Self.edgeWidth)
                        .frame(maxHeight: .infinity)
                        .contentShape(Rectangle())
                        .gesture(dragGesture(width: width))
                        .accessibilityHidden(true)
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height, alignment: side == .leading ? .leading : .trailing)
            .background(Palette.background.ignoresSafeArea())
            // Nothing to see: what the settling animation moves, so its
            // completion comes when the drawer's motion ends.
            .background(Color.clear.frame(width: settled ? 1 : 0, height: 1).accessibilityHidden(true))
            .simultaneousGesture(isOpen ? dragGesture(width: width) : nil)
        }
        .onChange(of: isOpen) { _, open in
            if open { showsPanel = true }
            withAnimation(Motion.move(reduced: reduceMotion), completionCriteria: .removed) {
                settled = open
            } completion: {
                // Gone once the screen has slid back over it.
                if !isOpen && !dragging { showsPanel = false }
            }
        }
        .onChange(of: dragging) { _, now in
            if now {
                showsPanel = true
            } else if !isOpen {
                // A drag that ends shut: gone once the screen is back.
                withAnimation(Motion.move(reduced: reduceMotion), completionCriteria: .removed) {
                    settled.toggle()
                } completion: {
                    if !isOpen && !dragging { showsPanel = false }
                }
            }
        }
    }

    /// The dimmed sliver: a tap closes the panel.
    @ViewBuilder
    private func sliverOverlay(progress: CGFloat) -> some View {
        if progress > 0 {
            Color.black.opacity(0.22 * progress)
                .contentShape(Rectangle())
                .onTapGesture { setOpen(false) }
                .accessibilityElement()
                .accessibilityLabel(Text(closeLabel))
                .accessibilityAddTraits(.isButton)
                .accessibilityAction { setOpen(false) }
        }
    }

    private func dragGesture(width: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 10, coordinateSpace: .global)
            .onChanged { value in
                let dx = value.translation.width * sign
                // Mostly sideways, or it belongs to a scroll.
                guard dragging || abs(value.translation.width) > abs(value.translation.height) * 1.2 else { return }
                dragging = true
                drag = isOpen ? min(0, dx) : max(0, dx)
            }
            .onEnded { value in
                guard dragging else { return }
                dragging = false
                let predicted = value.predictedEndTranslation.width * sign
                let end = (isOpen ? width : 0) + predicted
                let target = end > width / 2
                withAnimation(Motion.move(reduced: reduceMotion)) {
                    drag = 0
                    isOpen = target
                }
                Haptics.selection()
            }
    }

    private func setOpen(_ open: Bool) {
        withAnimation(Motion.move(reduced: reduceMotion)) { isOpen = open }
    }
}
