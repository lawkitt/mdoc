import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass
import os

/// Which side of the screen a `SideDrawer`'s panel lies on.
enum DrawerSide { case leading, trailing }

/// Where a sideways swipe opens a closed `SideDrawer` (ADR 0114).
enum DrawerOpening {
    /// Anywhere on the screen, but on what keeps its own sideways swipes.
    case anywhere
    /// Only at the near edge of the screen (the sidebar while editing).
    case edge
    /// Never: only `isOpen` opens it (Identities while editing, ADR 0110).
    case never
}

/// A panel under the screen that the screen slides aside to show, as the
/// Claude app's sidebar does (docs/mobile-ui.md, Sidebar): the content
/// moves toward the far edge, keeps a dimmed sliver with rounded corners,
/// and a tap on the sliver or a swipe back closes the panel. It opens from a
/// sideways swipe anywhere on the screen (`opens`) or from a button that
/// sets `isOpen`, and follows the finger both ways. Open, only a swipe that
/// starts on the sliver or at the panel's inner edge moves it: a swipe over
/// the panel's content is the content's, such as an Identities row's (ADR
/// 0106). While a drawer inside its content shows its panel, the swipe is
/// that drawer's: with Identities open, a swipe right from the left edge
/// closes Identities and never opens the sidebar over it.
///
/// The drag runs in UIKit (ADR 0114): the panel and the screen each sit in
/// a hosting controller, one pan recognizer moves their layers by
/// transform, and SwiftUI hears only open or shut at the end. Nothing
/// changes a shadow or a mask while the finger moves, and the panel stays
/// built while closed, so the first frames of a drag build nothing.
struct SideDrawer<Panel: View, Content: View>: View {
    typealias Side = DrawerSide

    var side: Side = .leading
    @Binding var isOpen: Bool
    /// What VoiceOver's close action and the sliver are called.
    var closeLabel: LocalizedStringResource = "Close sidebar"
    /// Where a swipe opens the closed panel.
    var opens: DrawerOpening = .anywhere
    @ViewBuilder var panel: () -> Panel
    @ViewBuilder var content: () -> Content

    var body: some View {
        DrawerHost(
            side: side, isOpen: $isOpen, closeLabel: String(localized: closeLabel), opens: opens,
            panel: panel(), content: content())
            // The hosted screen keeps its own safe areas; the drawer is the
            // whole window, so nothing the screen draws in them is cut off.
            .ignoresSafeArea()
    }
}

/// The drawer's controller in SwiftUI: hands it the panel and the screen,
/// with the environment around the drawer, and whether it is open.
private struct DrawerHost<Panel: View, Content: View>: UIViewControllerRepresentable {
    let side: DrawerSide
    @Binding var isOpen: Bool
    let closeLabel: String
    let opens: DrawerOpening
    let panel: Panel
    let content: Content

    func makeUIViewController(context: Context) -> DrawerController {
        DrawerController()
    }

    func updateUIViewController(_ controller: DrawerController, context: Context) {
        let environment = context.environment
        controller.side = side
        controller.layoutDirection = environment.layoutDirection
        controller.reduceMotion = environment.accessibilityReduceMotion
        controller.closeLabel = closeLabel
        controller.opens = opens
        let binding = $isOpen
        controller.setOpenBinding = { binding.wrappedValue = $0 }
        // A hosting controller starts from its window's environment: the
        // values set around the drawer (the app model, the layout, the
        // preferences) go in with the views.
        controller.setViews(
            panel: AnyView(panel
                .environment(\.self, environment)
                // The panel's own close button slides it shut.
                .environment(\.closeDrawer) { [weak controller] in
                    MainActor.assumeIsolated { controller?.slide(open: false) }
                }),
            content: AnyView(content.environment(\.self, environment)))
        let transaction = context.transaction
        controller.show(open: isOpen, animation: transaction.disablesAnimations ? nil : transaction.animation)
    }
}

/// The panel under the screen and the screen over it, moved by one pan.
final class DrawerController: UIViewController, UIGestureRecognizerDelegate {
    /// Every drawer, so one can tell whether a drawer inside its screen
    /// shows its panel.
    private static let all = NSHashTable<DrawerController>.weakObjects()
    /// A drag and the slide after it, as an animation interval whose
    /// hitches a device test measures (DeviceTapTimingUITests).
    private static let signposter = OSSignposter(subsystem: "io.github.lawkitt.mdoc", category: "Drawer")

    var side: DrawerSide = .leading { didSet { if side != oldValue { view.setNeedsLayout() } } }
    var layoutDirection: LayoutDirection = .leftToRight { didSet { if layoutDirection != oldValue { view.setNeedsLayout() } } }
    var reduceMotion = false
    var closeLabel = "" { didSet { sliver.accessibilityLabel = closeLabel } }
    var opens: DrawerOpening = .anywhere
    /// Tells SwiftUI the drawer came to rest open or shut.
    var setOpenBinding: (Bool) -> Void = { _ in }

    private let panelHost = UIHostingController(rootView: AnyView(EmptyView()))
    private let contentHost = UIHostingController(rootView: AnyView(EmptyView()))
    /// Rounds the screen's corners while the panel shows.
    private let clip = UIView()
    /// The screen's shadow on the panel: a fixed path, never redrawn.
    private let shadow = UIView()
    private let sliver = DrawerSliverView()
    /// Over the panel's inner edge while open: a drag there is the drawer's
    /// and never reaches a row below.
    private let innerEdge = UIView()
    private let pan = DrawerPan(target: nil, action: nil)

    /// Where the drawer rests or is sliding to.
    private(set) var isOpen = false
    /// What SwiftUI last said `isOpen` is.
    private var boundOpen = false
    /// How far the screen has moved aside, from 0 to the panel's width.
    private var shown: CGFloat = 0
    private var dragStart: CGFloat = 0
    private var dragging = false
    /// The drawer is sliding to rest: the panel stays until it gets there.
    private var settling = false
    /// Counts the slides, so an earlier slide's end leaves a later one be.
    private var slides = 0
    /// The pan is following a touch that started where it may move the
    /// drawer.
    private var tracking = false
    private weak var trackedTouch: UITouch?
    /// The panel as SwiftUI last gave it, and how many times it was made
    /// anew: closed, it starts over (no search or Select left from before).
    private var panelView = AnyView(EmptyView())
    private var panelGeneration = 0
    private var dragInterval: OSSignpostIntervalState?

    /// The panel is there: open, dragged or sliding to rest.
    var showsPanel: Bool { isOpen || dragging || settling }

    /// +1 when the screen moves right to open, -1 when it moves left.
    private var sign: CGFloat {
        let leading: CGFloat = side == .leading ? 1 : -1
        return layoutDirection == .rightToLeft ? -leading : leading
    }

    private var panelOnLeft: Bool { sign > 0 }
    private var panelWidth: CGFloat { DrawerDrag.panelWidth(view.bounds.width) }

    override func loadView() {
        let container = DrawerContainerView()
        container.escape = { [weak self] in
            guard let self, isOpen else { return false }
            slide(open: false)
            return true
        }
        view = container
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemGroupedBackground

        addChild(panelHost)
        panelHost.view.backgroundColor = .clear
        view.addSubview(panelHost.view)
        panelHost.didMove(toParent: self)

        innerEdge.accessibilityElementsHidden = true
        view.addSubview(innerEdge)

        shadow.isUserInteractionEnabled = false
        shadow.layer.shadowColor = UIColor.black.cgColor
        shadow.layer.shadowOpacity = 0.18
        shadow.layer.shadowRadius = 24
        shadow.layer.shadowOffset = .zero
        view.addSubview(shadow)

        clip.clipsToBounds = true
        clip.layer.cornerCurve = .continuous
        clip.backgroundColor = .systemGroupedBackground
        view.addSubview(clip)
        addChild(contentHost)
        contentHost.view.backgroundColor = .clear
        clip.addSubview(contentHost.view)
        contentHost.didMove(toParent: self)

        sliver.backgroundColor = .black
        sliver.close = { [weak self] in self?.slide(open: false) }
        sliver.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(sliverTapped)))
        view.addSubview(sliver)

        pan.delegate = self
        pan.decide = { [weak self] delta in
            guard let self else { return false }
            return decide(delta)
        }
        pan.onReset = { [weak self] in
            self?.tracking = false
            self?.trackedTouch = nil
        }
        pan.addTarget(self, action: #selector(panned(_:)))
        view.addGestureRecognizer(pan)

        Self.all.add(self)
        apply()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        let bounds = view.bounds
        let middle = CGPoint(x: bounds.midX, y: bounds.midY)
        // Bounds and centre, not frames: these views move by transform.
        for moved in [clip, shadow, sliver] {
            moved.bounds = bounds
            moved.center = middle
        }
        contentHost.view.frame = clip.bounds
        shadow.layer.shadowPath = UIBezierPath(roundedRect: bounds, cornerRadius: Radius.screen).cgPath
        let width = panelWidth
        panelHost.view.bounds = CGRect(x: 0, y: 0, width: width, height: bounds.height)
        panelHost.view.center = CGPoint(x: panelOnLeft ? width / 2 : bounds.width - width / 2, y: middle.y)
        innerEdge.frame = CGRect(
            x: panelOnLeft ? width - DrawerDrag.innerEdge : bounds.width - width, y: 0,
            width: DrawerDrag.innerEdge, height: bounds.height)
        if !dragging { shown = isOpen ? width : 0 }
        apply()
    }

    // MARK: SwiftUI

    func setViews(panel: AnyView, content: AnyView) {
        panelView = panel
        panelHost.rootView = AnyView(panel.id(panelGeneration))
        contentHost.rootView = content
    }

    /// SwiftUI's `isOpen`: slides with the change's animation, or moves at
    /// once without one (a row that opens something: the screen covers the
    /// panel in the same update).
    func show(open: Bool, animation: Animation?) {
        boundOpen = open
        guard open != isOpen else { return }
        if dragging {
            // The finger lets go of the drawer.
            pan.isEnabled = false
            pan.isEnabled = true
            dragging = false
        }
        if let animation {
            slide(open: open, animation: animation)
        } else {
            rest(open: open)
        }
    }

    // MARK: Motion

    /// Slides the screen to rest, open or shut, with the drawer's motion
    /// (a fade with Reduce Motion), and tells SwiftUI.
    func slide(open: Bool, animation: Animation? = nil) {
        slides += 1
        let slide = slides
        let from = currentShown()
        stopMotion()
        isOpen = open
        settling = true
        shown = from
        apply()
        let target = open ? panelWidth : 0
        let finish = { [weak self] in
            guard let self, slide == slides else { return }
            settling = false
            apply()
            endDragInterval()
            if !isOpen { renewPanel() }
        }
        if reduceMotion {
            UIView.transition(with: view, duration: 0.2, options: [.transitionCrossDissolve, .allowUserInteraction]) {
                self.shown = target
                self.apply()
            } completion: { _ in finish() }
        } else {
            UIView.animate(animation ?? Motion.move(reduced: false)) {
                self.shown = target
                self.apply()
            } completion: {
                finish()
            }
        }
        if open != boundOpen {
            boundOpen = open
            setOpenBinding(open)
        }
    }

    /// Moves to rest at once.
    private func rest(open: Bool) {
        slides += 1
        stopMotion()
        isOpen = open
        settling = false
        shown = open ? panelWidth : 0
        apply()
        endDragInterval()
        if !open { renewPanel() }
    }

    private func endDragInterval() {
        guard let interval = dragInterval else { return }
        dragInterval = nil
        Self.signposter.endInterval("Drawer drag", interval)
    }

    /// Where the screen is on screen now, mid-slide included.
    private func currentShown() -> CGFloat {
        guard settling, let moved = clip.layer.presentation() else { return shown }
        return min(max(moved.affineTransform().tx * sign, 0), panelWidth)
    }

    private func stopMotion() {
        for moved in [clip, shadow, sliver, panelHost.view!] {
            moved.layer.removeAllAnimations()
        }
    }

    /// Closed, the panel is made anew under the screen, as if it had gone:
    /// the next drag finds it built.
    private func renewPanel() {
        panelGeneration += 1
        panelHost.rootView = AnyView(panelView.id(panelGeneration))
    }

    /// Places the layers for `shown`: transforms and the sliver's dimming,
    /// nothing that redraws.
    private func apply() {
        let width = panelWidth
        let progress = width > 0 ? min(max(shown / width, 0), 1) : 0
        let move = CGAffineTransform(translationX: sign * shown, y: 0)
        clip.transform = move
        shadow.transform = move
        sliver.transform = move
        // A little parallax: the panel comes from under the screen.
        panelHost.view.transform = CGAffineTransform(translationX: sign * -(1 - progress) * 24, y: 0)
        sliver.alpha = 0.22 * progress

        let shows = showsPanel
        panelHost.view.isHidden = !shows
        panelHost.view.isUserInteractionEnabled = isOpen || dragging
        // Closed, the panel is not there for VoiceOver (or tests); open,
        // the panel and the sliver, its close button, are all there is.
        panelHost.view.accessibilityElementsHidden = !isOpen
        contentHost.view.accessibilityElementsHidden = isOpen
        innerEdge.isHidden = !isOpen
        sliver.isHidden = !shows
        shadow.isHidden = !shows
        // Set when the panel starts to show and when it is gone, never
        // while the finger moves.
        let radius = shows ? Radius.screen : 0
        if clip.layer.cornerRadius != radius { clip.layer.cornerRadius = radius }
    }

    @objc private func sliverTapped() {
        slide(open: false)
    }

    // MARK: The pan

    @objc private func panned(_ pan: DrawerPan) {
        let width = panelWidth
        switch pan.state {
        case .began:
            if dragInterval == nil { dragInterval = Self.signposter.beginAnimationInterval("Drawer drag") }
            dragStart = currentShown()
            slides += 1
            stopMotion()
            dragging = true
            settling = false
            shown = dragStart
            pan.setTranslation(.zero, in: view)
            apply()
        case .changed:
            guard dragging else { return }
            shown = min(max(dragStart + pan.translation(in: view).x * sign, 0), width)
            apply()
        case .ended, .cancelled:
            guard dragging else { return }
            dragging = false
            let velocity = pan.velocity(in: view).x * sign
            // Where a flick would carry it.
            let end = shown + velocity * 0.2
            slide(open: pan.state == .ended ? end > width / 2 : isOpen)
            Haptics.selection()
        default:
            break
        }
    }

    /// Whether a touch that has moved by `delta` is the drawer's: `nil`
    /// until it is clear. A vertical scroll is never taken; closed, only a
    /// move toward the open side.
    private func decide(_ delta: CGVector) -> Bool? {
        let across = abs(delta.dx), down = abs(delta.dy)
        if down > across * 1.2 { return down >= 4 ? false : nil }
        let toward = delta.dx * sign > 0
        guard across >= 10 else {
            // Away from the open side: another drawer's, or a table's.
            if !isOpen, across >= 4, !toward { return false }
            return nil
        }
        return isOpen || toward
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard gestureRecognizer === pan else { return true }
        tracking = takesTouch(at: touch.location(in: view))
        trackedTouch = tracking ? touch : nil
        return tracking
    }

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard gestureRecognizer === pan else { return true }
        // A long press, a selection handle's drag or a row's swipe that
        // already began keeps the touch.
        let others = trackedTouch?.gestureRecognizers ?? []
        return !others.contains { $0 !== pan && ($0.state == .began || $0.state == .changed) }
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldBeRequiredToFailBy other: UIGestureRecognizer
    ) -> Bool {
        // The screen's vertical scrolls wait until the drawer's pan has
        // decided, so a sideways swipe never scrolls the text too.
        guard gestureRecognizer === pan, tracking, !isOpen,
              let scroll = other.view as? UIScrollView, other === scroll.panGestureRecognizer,
              scroll.isDescendant(of: contentHost.view) else { return false }
        return !DrawerSwipes.scrollsSideways(scroll)
    }

    /// Whether a touch starting at `point` may move the drawer.
    private func takesTouch(at point: CGPoint) -> Bool {
        if innerShows { return false }
        let width = view.bounds.width
        if isOpen {
            return DrawerDrag.movesOpenDrawer(
                startingAt: point.x, width: width, panelWidth: panelWidth, panelOnLeft: panelOnLeft)
        }
        let atEdge = panelOnLeft ? point.x <= DrawerDrag.edgeWidth : point.x >= width - DrawerDrag.edgeWidth
        switch opens {
        case .never: return false
        case .edge: return atEdge
        case .anywhere:
            return atEdge || !DrawerSwipes.keepsSideSwipes(at: point, in: view, within: contentHost.view)
        }
    }

    /// A drawer inside the screen shows its panel.
    private var innerShows: Bool {
        Self.all.allObjects.contains {
            $0 !== self && $0.showsPanel && $0.isViewLoaded && $0.view.isDescendant(of: contentHost.view)
        }
    }
}

/// The drawer's own view: VoiceOver's escape closes an open drawer.
private final class DrawerContainerView: UIView {
    var escape: () -> Bool = { false }

    override func accessibilityPerformEscape() -> Bool {
        escape() || super.accessibilityPerformEscape()
    }
}

/// The dimmed sliver of the screen: a tap closes the panel; for VoiceOver
/// a button.
private final class DrawerSliverView: UIView {
    var close: () -> Void = {}

    override init(frame: CGRect) {
        super.init(frame: frame)
        isAccessibilityElement = true
        accessibilityTraits = .button
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is unused") }

    override func accessibilityActivate() -> Bool {
        close()
        return true
    }
}

/// A pan that waits until a touch has clearly gone sideways, or declines it
/// as soon as it goes up or down, so the text's scroll starts at once.
final class DrawerPan: UIPanGestureRecognizer {
    /// Whether a touch that has moved by a distance is the pan's, `nil`
    /// while it is not yet clear.
    var decide: (CGVector) -> Bool? = { _ in true }
    var onReset: () -> Void = {}
    private var start: CGPoint?

    override init(target: Any?, action: Selector?) {
        super.init(target: target, action: action)
        maximumNumberOfTouches = 1
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesBegan(touches, with: event)
        if start == nil, let touch = touches.first { start = touch.location(in: view) }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        if state == .possible, let start, let touch = touches.first {
            let point = touch.location(in: view)
            switch decide(CGVector(dx: point.x - start.x, dy: point.y - start.y)) {
            case nil: return
            case false?:
                state = .failed
                return
            case true?: break
            }
        }
        super.touchesMoved(touches, with: event)
    }

    override func reset() {
        super.reset()
        start = nil
        onReset()
    }
}

/// What keeps its own sideways swipes, so a drawer swipe never starts on
/// it (ADR 0114): content that scrolls sideways (a table wider than the
/// screen, an Original page zoomed past its width) even at its edge, views
/// marked with `keepsSideSwipes()` (the review bar and its cards), and a
/// selection's handles.
@MainActor
enum DrawerSwipes {
    /// How near a selection's end a touch is on its handle.
    static let handleReach: CGFloat = 24

    static func scrollsSideways(_ scroll: UIScrollView) -> Bool {
        guard scroll.isScrollEnabled else { return false }
        let inset = scroll.adjustedContentInset
        return scroll.contentSize.width + inset.left + inset.right > scroll.bounds.width + 1
    }

    /// Whether what lies at `point` in `container` (inside `content`, the
    /// screen) keeps a sideways swipe that starts there.
    @MainActor
    static func keepsSideSwipes(at point: CGPoint, in container: UIView, within content: UIView) -> Bool {
        for keeper in SideSwipeKeeperView.all.allObjects
        where keeper.window === container.window && keeper.window != nil && keeper.isDescendant(of: content) {
            if keeper.convert(keeper.bounds, to: container).contains(point) { return true }
        }
        var view = container.hitTest(point, with: nil)
        while let current = view, current !== container {
            if let scroll = current as? UIScrollView, scrollsSideways(scroll) { return true }
            if let text = current as? UITextView, onSelectionHandle(text, at: container.convert(point, to: text)) {
                return true
            }
            view = current.superview
        }
        return false
    }

    /// Whether `point`, in `text`'s coordinates, is on one of its selection's
    /// handles.
    @MainActor
    static func onSelectionHandle(_ text: UITextView, at point: CGPoint) -> Bool {
        guard let range = text.selectedTextRange, !range.isEmpty else { return false }
        return [range.start, range.end].contains { end in
            text.caretRect(for: end).insetBy(dx: -handleReach, dy: -handleReach).contains(point)
        }
    }
}

/// Marks a view whose sideways swipes are its own, such as the review bar's
/// card carousel: a drawer swipe never starts on it.
private struct SideSwipeKeeper: UIViewRepresentable {
    func makeUIView(context: Context) -> SideSwipeKeeperView { SideSwipeKeeperView() }
    func updateUIView(_ view: SideSwipeKeeperView, context: Context) {}
}

final class SideSwipeKeeperView: UIView {
    static let all = NSHashTable<SideSwipeKeeperView>.weakObjects()

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        accessibilityElementsHidden = true
        Self.all.add(self)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is unused") }
}

extension View {
    /// Sideways swipes that start on this view are its own: no drawer
    /// opens from them (ADR 0114).
    func keepsSideSwipes() -> some View {
        background(SideSwipeKeeper())
    }
}

/// Which drags move an open drawer (ADR 0106): those that start on the
/// dimmed sliver or within `innerEdge` of the panel's edge beside it. One
/// that starts over the panel's content belongs to the content: an
/// Identities row's swipe, or putting its revealed actions away.
enum DrawerDrag {
    /// How far into the panel, from its edge beside the sliver, a drag
    /// still moves the drawer.
    static let innerEdge: CGFloat = 20
    /// The sliver of the screen left showing while the panel is open.
    static let sliver: CGFloat = 72
    /// The near edge that opens the panel while only the edge does (edit
    /// mode), and always.
    static let edgeWidth: CGFloat = 22

    static func panelWidth(_ total: CGFloat) -> CGFloat {
        min(360, max(260, total - sliver))
    }

    /// `x` is where the drag starts, in points from the drawer's left edge
    /// on screen; `width` is the drawer's, `panelWidth` the panel's, which
    /// sits at the left edge when `panelOnLeft`, else at the right.
    static func movesOpenDrawer(startingAt x: CGFloat, width: CGFloat, panelWidth: CGFloat, panelOnLeft: Bool) -> Bool {
        // How far into the panel the drag starts; the sliver is below zero.
        let depth = panelOnLeft ? panelWidth - x : x - (width - panelWidth)
        return depth <= innerEdge
    }
}

extension EnvironmentValues {
    /// In a `SideDrawer`'s panel: slides the drawer shut.
    @Entry var closeDrawer: (() -> Void)? = nil
}
