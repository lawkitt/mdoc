import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass
import os

/// Which side of the screen a drawer's panel lies on.
enum DrawerSide { case leading, trailing }

/// Where a sideways swipe opens a closed drawer (ADR 0114).
enum DrawerOpening {
    /// Anywhere on the screen, but on what keeps its own sideways swipes.
    case anywhere
    /// Only at the near edge of the screen (the sidebar while editing).
    case edge
    /// Never: only `isOpen` opens it (Identities while editing, ADR 0110).
    case never
}

/// One side's panel in a `SideDrawer`: whether it is open, what its sliver
/// is called for VoiceOver, and where a swipe opens it.
struct DrawerPanel<Panel: View> {
    var isOpen: Binding<Bool>
    var closeLabel: LocalizedStringResource
    var opens: DrawerOpening = .anywhere
    var panel: Panel

    init(
        isOpen: Binding<Bool>, closeLabel: LocalizedStringResource, opens: DrawerOpening = .anywhere,
        @ViewBuilder panel: () -> Panel
    ) {
        self.isOpen = isOpen
        self.closeLabel = closeLabel
        self.opens = opens
        self.panel = panel()
    }
}

/// Panels under the screen that the screen slides aside to show, as the
/// Claude app's sidebar does (docs/mobile-ui.md, Sidebar): the sidebar on
/// the leading side, Identities on the trailing one. The screen moves toward
/// the far edge, keeps a dimmed sliver with rounded corners, and a tap on
/// the sliver or a swipe back closes the panel. A panel opens from a
/// sideways swipe anywhere on the screen (each panel's `opens`) or from a
/// button that sets its `isOpen`, and follows the finger both ways. One
/// panel shows at a time. Open, it closes as it opened (ADR 0115): a swipe
/// toward the panel's side anywhere over it, or either way from the sliver
/// or the panel's inner edge. A swipe the other way over the panel is the
/// content's, such as an Identities row's (its actions are on a left
/// swipe), and so is one that starts on what keeps its own sideways
/// swipes; with Identities open, a swipe right from the left edge closes
/// Identities and never opens the sidebar.
///
/// The drag runs in UIKit (ADR 0114): the panels and the screen each sit in
/// a hosting controller of one view controller, one pan recognizer moves
/// their layers by transform, and SwiftUI hears only open or shut at the
/// end. The panels stay built and drawn under the screen while closed, and
/// the screen's corners and its shadow are one bitmap drawn once (no mask,
/// no layer shadow), so a drag's first frame has nothing new to draw: on
/// the iPhone 12 Pro each of them cost that frame. Both panels are in the one
/// controller: a drawer's hosting controller inside another's updated its
/// traits without end.
struct SideDrawer<Leading: View, Trailing: View, Content: View>: View {
    var leading: DrawerPanel<Leading>
    var trailing: DrawerPanel<Trailing>?
    @ViewBuilder var content: () -> Content

    var body: some View {
        // Read here, so a change to either redraws the drawer.
        DrawerHost(
            leading: leading, trailing: trailing, leadingOpen: leading.isOpen.wrappedValue,
            trailingOpen: trailing?.isOpen.wrappedValue ?? false, content: content())
            // The hosted screen keeps its own safe areas; the drawer is the
            // whole window, so nothing the screen draws in them is cut off.
            .ignoresSafeArea()
    }
}

extension SideDrawer where Trailing == EmptyView {
    /// A drawer with a leading panel only.
    init(leading: DrawerPanel<Leading>, @ViewBuilder content: @escaping () -> Content) {
        self.init(leading: leading, trailing: nil, content: content)
    }
}

/// The drawer's controller in SwiftUI: hands it the panels and the screen,
/// with the app's environment around the drawer, and which panel is open.
private struct DrawerHost<Leading: View, Trailing: View, Content: View>: UIViewControllerRepresentable {
    let leading: DrawerPanel<Leading>
    let trailing: DrawerPanel<Trailing>?
    let leadingOpen: Bool
    let trailingOpen: Bool
    let content: Content

    func makeUIViewController(context: Context) -> DrawerController {
        DrawerController()
    }

    func updateUIViewController(_ controller: DrawerController, context: Context) {
        let environment = context.environment
        controller.layoutDirection = environment.layoutDirection
        controller.reduceMotion = environment.accessibilityReduceMotion
        func panelView(_ panel: some View, _ side: DrawerSide) -> AnyView {
            AnyView(panel
                .modifier(ForwardedEnvironment(values: environment))
                // The panel's own close button slides it shut.
                .environment(\.closeDrawer) { [weak controller] in
                    MainActor.assumeIsolated { controller?.close(side) }
                })
        }
        let leadingBinding = leading.isOpen
        controller.configure(
            .leading, present: true, closeLabel: String(localized: leading.closeLabel), opens: leading.opens,
            view: panelView(leading.panel, .leading)) { leadingBinding.wrappedValue = $0 }
        if let trailing {
            let trailingBinding = trailing.isOpen
            controller.configure(
                .trailing, present: true, closeLabel: String(localized: trailing.closeLabel), opens: trailing.opens,
                view: panelView(trailing.panel, .trailing)) { trailingBinding.wrappedValue = $0 }
        } else {
            controller.configure(.trailing, present: false, closeLabel: "", opens: .never, view: AnyView(EmptyView())) { _ in }
        }
        controller.setContent(AnyView(content.modifier(ForwardedEnvironment(values: environment))))
        let transaction = context.transaction
        controller.show(
            leading: leadingOpen, trailing: trailingOpen,
            animation: transaction.disablesAnimations ? nil : transaction.animation)
    }
}

/// The app's environment values set around the drawer, for the hosting
/// controllers inside it. Only these: the whole environment (`\.self`)
/// carries the outer host's own values and traits, which fed back into the
/// hosts without end. The system's values (colour scheme, text size,
/// Reduce Motion) reach the hosts as traits.
private struct ForwardedEnvironment: ViewModifier {
    let values: EnvironmentValues

    func body(content: Content) -> some View {
        content
            .environment(\.appModel, values.appModel)
            .environment(\.preferences, values.preferences)
            .environment(\.documentLayout, values.documentLayout)
            .environment(\.showSidebar, values.showSidebar)
    }
}

/// The panels under the screen and the screen over them, moved by one pan.
final class DrawerController: UIViewController, UIGestureRecognizerDelegate {
    /// A drag and the slide after it, as an animation interval whose
    /// hitches a device test measures (DeviceTapTimingUITests).
    private static let signposter = OSSignposter(subsystem: "io.github.lawkitt.mdoc", category: "Drawer")

    /// One side's panel and what SwiftUI said about it.
    @MainActor private final class Panel {
        let host = UIHostingController(rootView: AnyView(EmptyView()))
        /// There is a panel on this side (Identities: a document is open,
        /// in a narrow window).
        var present = false
        var closeLabel = ""
        var opens: DrawerOpening = .never
        /// Tells SwiftUI the panel came to rest open or shut.
        var setOpen: (Bool) -> Void = { _ in }
        /// What SwiftUI last said its `isOpen` is.
        var bound = false
        /// The panel as SwiftUI last gave it, and how many times it was
        /// made anew: closed, it starts over (no search or Select left).
        var view = AnyView(EmptyView())
        var generation = 0
    }

    var layoutDirection: LayoutDirection = .leftToRight { didSet { if layoutDirection != oldValue { view.setNeedsLayout() } } }
    var reduceMotion = false { didSet { if reduceMotion != oldValue { stack() } } }

    private let leadingPanel = Panel()
    private let trailingPanel = Panel()
    private let contentHost = UIHostingController(rootView: AnyView(EmptyView()))
    /// Holds the screen; its corners are drawn by `edge`, not masked.
    private let clip = UIView()
    /// The screen's edge beside the panel: its rounded corners, drawn over
    /// it in the panel's colour, and its shadow on the panel, following the
    /// corners' curve (ADR 0115). A corner mask on the whole screen
    /// rendered it offscreen, and a blurred layer shadow cost a frame when
    /// it appeared: this is one bitmap, drawn once.
    private let edge = DrawerEdge()
    private let sliver = DrawerSliverView()
    /// Over the open panel's inner edge: a drag there is the drawer's and
    /// never reaches a row below.
    private let innerEdge = UIView()
    private let pan = DrawerPan(target: nil, action: nil)

    /// The panel that shows or moves; `shown` is its.
    private var active: DrawerSide = .leading
    /// The panel open, or sliding open: `nil` when the screen rests shut or
    /// slides shut.
    private var openSide: DrawerSide?
    /// How far the screen has moved aside, from 0 to the panel's width.
    private var shown: CGFloat = 0
    private var dragStart: CGFloat = 0
    private var dragging = false
    /// The screen is sliding to rest: the panel stays until it gets there.
    private var settling = false
    /// Counts the slides, so an earlier slide's end leaves a later one be.
    private var slides = 0
    /// The panels a touch may open or move, found where it started.
    private var candidates: [DrawerSide] = []
    /// The tracked touch started at a candidate's near edge, where the
    /// drawer goes before anything that scrolls sideways.
    private var fromEdge = false
    /// Open, the tracked touch started over the panel's content, not on the
    /// sliver or the inner edge: only a swipe that closes it is the
    /// drawer's.
    private var overPanel = false
    /// The panel the pan chose once the touch went clearly sideways.
    private var chosen: DrawerSide?
    private weak var trackedTouch: UITouch?
    private var dragInterval: OSSignpostIntervalState?

    private var isOpen: Bool { openSide != nil }

    /// For tests: both panels lie under the screen (`clip`), as they must
    /// while they slide; with Reduce Motion the active one is over it.
    var panelsLieUnderScreen: Bool {
        let order = view.subviews
        guard let screen = order.firstIndex(of: clip) else { return false }
        return [leadingPanel, trailingPanel].allSatisfy { (order.firstIndex(of: $0.host.view) ?? .max) < screen }
    }
    /// The active panel is there: open, dragged or sliding to rest.
    private var showsPanel: Bool { isOpen || dragging || settling }

    private func panel(_ side: DrawerSide) -> Panel {
        side == .leading ? leadingPanel : trailingPanel
    }

    /// +1 when the screen moves right to open `side`, -1 when it moves left.
    private func sign(_ side: DrawerSide) -> CGFloat {
        let leading: CGFloat = side == .leading ? 1 : -1
        return layoutDirection == .rightToLeft ? -leading : leading
    }

    private var sign: CGFloat { sign(active) }
    private func onLeft(_ side: DrawerSide) -> Bool { sign(side) > 0 }
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

        for side in [DrawerSide.leading, .trailing] {
            let host = panel(side).host
            addChild(host)
            host.view.backgroundColor = .clear
            host.view.accessibilityElementsHidden = true
            view.addSubview(host.view)
            host.didMove(toParent: self)
        }

        innerEdge.accessibilityElementsHidden = true
        view.addSubview(innerEdge)

        clip.clipsToBounds = true
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
        edge.isUserInteractionEnabled = false
        edge.accessibilityElementsHidden = true
        view.addSubview(edge)

        pan.delegate = self
        pan.decide = { [weak self] delta in
            guard let self else { return false }
            return decide(delta)
        }
        pan.onReset = { [weak self] in
            self?.candidates = []
            self?.fromEdge = false
            self?.overPanel = false
            self?.chosen = nil
            self?.trackedTouch = nil
        }
        pan.addTarget(self, action: #selector(panned(_:)))
        view.addGestureRecognizer(pan)

        stack()
        apply()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        let bounds = view.bounds
        let middle = CGPoint(x: bounds.midX, y: bounds.midY)
        // Bounds and centre, not frames: these views move by transform.
        clip.bounds = bounds
        clip.center = middle
        contentHost.view.frame = clip.bounds
        let width = panelWidth
        for side in [DrawerSide.leading, .trailing] {
            let host = panel(side).host.view!
            host.bounds = CGRect(x: 0, y: 0, width: width, height: bounds.height)
            host.center = CGPoint(x: onLeft(side) ? width / 2 : bounds.width - width / 2, y: middle.y)
        }
        arrange()
        if !dragging { shown = isOpen ? width : 0 }
        apply()
    }

    /// Stacks the active panel above the other one: under the screen, or
    /// with Reduce Motion over it, since nothing slides and the panel fades
    /// in. Only when the panel or the setting changes: moving a view in the
    /// hierarchy sends traits down again, and the screen's host lays out
    /// anew.
    private func stack() {
        let host = panel(active).host.view!
        if reduceMotion {
            view.insertSubview(host, aboveSubview: clip)
        } else {
            // Under the screen, wherever Reduce Motion put it.
            view.insertSubview(host, belowSubview: clip)
        }
        view.insertSubview(innerEdge, aboveSubview: host)
        view.setNeedsLayout()
    }

    /// Places the sliver and the inner edge for the active panel; with
    /// Reduce Motion the sliver dims only the rest of the screen.
    private func arrange() {
        let bounds = view.bounds
        let width = panelWidth
        if reduceMotion {
            let rest = bounds.width - width
            sliver.bounds = CGRect(x: 0, y: 0, width: rest, height: bounds.height)
            sliver.center = CGPoint(x: onLeft(active) ? width + rest / 2 : rest / 2, y: bounds.midY)
        } else {
            sliver.bounds = bounds
            sliver.center = CGPoint(x: bounds.midX, y: bounds.midY)
        }
        innerEdge.frame = CGRect(
            x: onLeft(active) ? width - DrawerDrag.innerEdge : bounds.width - width, y: 0,
            width: DrawerDrag.innerEdge, height: bounds.height)
        // The screen's edge beside the panel, from the shadow's reach
        // outside it to its corners inside; it moves with the screen.
        let reach = DrawerEdge.reach, inside = DrawerEdge.cornerSide
        edge.bounds = CGRect(x: 0, y: 0, width: reach + inside, height: bounds.height)
        edge.center = CGPoint(
            x: onLeft(active) ? (inside - reach) / 2 : bounds.width + (reach - inside) / 2, y: bounds.midY)
        edge.onLeft = onLeft(active)
    }

    // MARK: SwiftUI

    func configure(
        _ side: DrawerSide, present: Bool, closeLabel: String, opens: DrawerOpening, view panelView: AnyView,
        setOpen: @escaping (Bool) -> Void
    ) {
        let panel = panel(side)
        let wasPresent = panel.present
        panel.present = present
        panel.closeLabel = closeLabel
        panel.opens = opens
        panel.setOpen = setOpen
        panel.view = panelView
        panel.host.rootView = AnyView(panelView.id(panel.generation))
        // A panel that goes (the document closed) takes nothing with it.
        if wasPresent, !present, active == side, showsPanel {
            cancelDrag()
            rest(open: false)
        }
    }

    func setContent(_ content: AnyView) {
        contentHost.rootView = content
    }

    /// SwiftUI's `isOpen`s: slides with the change's animation, or moves at
    /// once without one (a row that opens something: the screen covers the
    /// panel in the same update).
    func show(leading: Bool, trailing: Bool, animation: Animation?) {
        // Only a change from SwiftUI: while a slide of the drawer's own is on
        // its way, SwiftUI still holds the old values until it ends.
        guard leading != leadingPanel.bound || trailing != trailingPanel.bound else { return }
        let wanted = DrawerDrag.wanted(
            leading: leading, trailing: trailing && trailingPanel.present,
            wasLeading: leadingPanel.bound, wasTrailing: trailingPanel.bound, open: openSide)
        leadingPanel.bound = leading
        trailingPanel.bound = trailing
        // One panel at a time: the other's `isOpen` goes back to false, after
        // this update (⌘K's search over Identities, say).
        let other: DrawerSide? = wanted == .leading && trailing ? .trailing : wanted == .trailing && leading ? .leading : nil
        if let other {
            DispatchQueue.main.async { [weak self] in self?.tellSwiftUI(other, open: false) }
        }
        guard wanted != openSide else { return }
        cancelDrag()
        if let wanted, wanted != active {
            // One panel at a time: the other goes at once.
            if showsPanel { rest(open: false) }
            setActive(wanted)
        }
        if let animation {
            slide(open: wanted != nil, animation: animation, tell: false)
        } else {
            rest(open: wanted != nil)
        }
    }

    /// A panel's own close button.
    func close(_ side: DrawerSide) {
        guard openSide == side else { return }
        slide(open: false)
    }

    private func setActive(_ side: DrawerSide) {
        guard side != active else { return }
        active = side
        stack()
    }

    private func cancelDrag() {
        guard dragging else { return }
        // The finger lets go of the drawer.
        pan.isEnabled = false
        pan.isEnabled = true
        dragging = false
    }

    // MARK: Motion

    /// Slides the screen to rest, the active panel open or shut, with the
    /// drawer's motion (with Reduce Motion, the panel fades), and tells
    /// SwiftUI at the end unless it asked: the screen's update then never
    /// runs while the screen moves.
    func slide(open: Bool, animation: Animation? = nil, tell: Bool = true) {
        slides += 1
        let slide = slides
        let from = currentShown()
        stopMotion()
        let side = active
        openSide = open ? side : nil
        settling = true
        shown = from
        apply()
        let target = open ? panelWidth : 0
        UIView.animate(reduceMotion ? Motion.fade : animation ?? Motion.move(reduced: false)) {
            self.shown = target
            self.apply()
        } completion: { [weak self] in
            guard let self, slide == slides else { return }
            settling = false
            apply()
            endDragInterval()
            if !isOpen { renew(side) }
            if tell { tellSwiftUI(side, open: open) }
        }
    }

    /// Moves to rest at once.
    private func rest(open: Bool) {
        slides += 1
        stopMotion()
        let side = active
        openSide = open ? side : nil
        settling = false
        shown = open ? panelWidth : 0
        apply()
        endDragInterval()
        if !open { renew(side) }
    }

    private func tellSwiftUI(_ side: DrawerSide, open: Bool) {
        let panel = panel(side)
        guard panel.bound != open else { return }
        panel.bound = open
        panel.setOpen(open)
    }

    private func endDragInterval() {
        guard let interval = dragInterval else { return }
        dragInterval = nil
        Self.signposter.endInterval("Drawer drag", interval)
    }

    /// Where the screen is on screen now, mid-slide included.
    private func currentShown() -> CGFloat {
        guard settling else { return shown }
        if reduceMotion {
            guard let faded = panel(active).host.view.layer.presentation() else { return shown }
            return CGFloat(faded.opacity) * panelWidth
        }
        guard let moved = clip.layer.presentation() else { return shown }
        return min(max(moved.affineTransform().tx * sign, 0), panelWidth)
    }

    private func stopMotion() {
        for moved in [clip, edge, sliver, leadingPanel.host.view!, trailingPanel.host.view!] {
            moved.layer.removeAllAnimations()
        }
    }

    /// Closed, a panel is made anew under the screen, as if it had gone:
    /// the next drag finds it built.
    private func renew(_ side: DrawerSide) {
        let panel = panel(side)
        panel.generation += 1
        panel.host.rootView = AnyView(panel.view.id(panel.generation))
    }

    /// Places the layers for `shown`: transforms and opacities, nothing
    /// that redraws.
    private func apply() {
        let width = panelWidth
        let progress = width > 0 ? min(max(shown / width, 0), 1) : 0
        let host = panel(active).host.view!
        if reduceMotion {
            // The finger fades the panel in and out; nothing moves.
            clip.transform = .identity
            edge.transform = .identity
            sliver.transform = .identity
            host.transform = .identity
            host.alpha = progress
        } else {
            let move = CGAffineTransform(translationX: sign * shown, y: 0)
            clip.transform = move
            edge.transform = move
            sliver.transform = move
            // A little parallax: the panel comes from under the screen.
            host.transform = CGAffineTransform(translationX: sign * -(1 - progress) * 24, y: 0)
            host.alpha = 1
        }
        sliver.alpha = 0.22 * progress

        let shows = showsPanel
        for side in [DrawerSide.leading, .trailing] {
            let view = panel(side).host.view!
            let this = side == active
            // Shown under the opaque screen even while closed, so its layers
            // are drawn before a drag: unhidden as the drag began, they were
            // drawn in its first frame.
            view.isHidden = !panel(side).present
            view.isUserInteractionEnabled = this && (isOpen || dragging)
            // Closed, a panel is not there for VoiceOver (or tests); open,
            // the panel and the sliver, its close button, are all there is.
            view.accessibilityElementsHidden = !(this && isOpen)
        }
        contentHost.view.accessibilityElementsHidden = isOpen
        innerEdge.isHidden = !isOpen
        // The sliver (clear at rest) stays shown, so a drag's first frame
        // shows nothing new but the edge, one bitmap drawn beforehand.
        edge.isHidden = !shows || reduceMotion
        // The sliver is a button for VoiceOver only while a panel is open.
        if sliver.isAccessibilityElement != isOpen { sliver.isAccessibilityElement = isOpen }
        sliver.accessibilityLabel = panel(active).closeLabel
    }

    @objc private func sliverTapped() {
        slide(open: false)
    }

    // MARK: The pan

    @objc private func panned(_ pan: DrawerPan) {
        switch pan.state {
        case .began:
                    if !isOpen, !settling || shown == 0, let chosen { setActive(chosen) }
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
            shown = min(max(dragStart + pan.translation(in: view).x * sign, 0), panelWidth)
            apply()
        case .ended, .cancelled:
            guard dragging else { return }
            dragging = false
            let velocity = pan.velocity(in: view).x * sign
            // Where a flick would carry it.
            let end = shown + velocity * 0.2
            slide(open: pan.state == .ended ? end > panelWidth / 2 : isOpen)
            Haptics.selection()
        default:
            break
        }
    }

    /// Whether the touch, moved by `delta`, is the drawer's: `nil` until it
    /// is clear. Open, the open panel's; closed, the first candidate it
    /// moves toward opening.
    private func decide(_ delta: CGVector) -> Bool? {
        var waiting = false
        for side in candidates {
            switch DrawerDrag.decide(delta, sign: sign(side), isOpen: isOpen, overPanel: overPanel) {
            case true?:
                chosen = side
                return true
            case nil: waiting = true
            case false?: break
            }
        }
        return waiting ? nil : false
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard gestureRecognizer === pan else { return true }
        let point = touch.location(in: view)
        if let openSide {
            // From the sliver or the inner edge either way; over the panel
            // only to close it, and never on what keeps its sideways swipes.
            overPanel = !DrawerDrag.movesOpenDrawer(
                startingAt: point.x, width: view.bounds.width, panelWidth: panelWidth, panelOnLeft: onLeft(openSide))
            // While a row shows its swipe actions, a swipe over the panel
            // is the row's: it puts them away.
            let host = panel(openSide).host.view!
            let keeps = overPanel
                && (DrawerSwipes.showsSwipeActions(in: host)
                    || DrawerSwipes.keepsSideSwipes(at: point, in: view, within: host))
            candidates = keeps ? [] : [openSide]
            fromEdge = false
        } else {
            // Hit-tested once, for both sides.
            var keeps: Bool?
            candidates = [DrawerSide.leading, .trailing].filter { side in
                let panel = panel(side)
                guard panel.present else { return false }
                switch panel.opens {
                case .never: return false
                case .edge: return atNearEdge(point, of: side)
                case .anywhere:
                    if atNearEdge(point, of: side) { return true }
                    if keeps == nil {
                        keeps = DrawerSwipes.keepsSideSwipes(at: point, in: view, within: contentHost.view)
                    }
                    return keeps == false
                }
            }
            // Sliding shut, only the panel on its way.
            if settling { candidates = candidates.filter { $0 == active } }
            fromEdge = candidates.contains { atNearEdge(point, of: $0) }
        }
        trackedTouch = candidates.isEmpty ? nil : touch
        return !candidates.isEmpty
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
        // decided, so a sideways swipe never scrolls the text too; from a
        // near edge, what scrolls sideways waits as well. Open, the panel's
        // lists wait the same way for a swipe over them.
        guard gestureRecognizer === pan, !candidates.isEmpty,
              let scroll = other.view as? UIScrollView, other === scroll.panGestureRecognizer else { return false }
        if let openSide {
            return overPanel && scroll.isDescendant(of: panel(openSide).host.view) && !DrawerSwipes.scrollsSideways(scroll)
        }
        guard scroll.isDescendant(of: contentHost.view) else { return false }
        return fromEdge || !DrawerSwipes.scrollsSideways(scroll)
    }

    private func atNearEdge(_ point: CGPoint, of side: DrawerSide) -> Bool {
        onLeft(side) ? point.x <= DrawerDrag.edgeWidth : point.x >= view.bounds.width - DrawerDrag.edgeWidth
    }
}

/// The screen's edge beside the open panel (ADR 0115): its two rounded
/// corners, the bits of a square outside the screen's continuous corner in
/// the panels' colour (`systemGroupedBackground`), and its shadow on the
/// panel, which follows the corners' curve. One bitmap per side, drawn
/// once per size and appearance and stretched along the straight edge
/// (`contentsCenter`); the drag only moves it. The screen's other corners
/// are past the window's edge.
private final class DrawerEdge: UIView {
    /// How far the shadow reaches over the panel.
    static let reach: CGFloat = 24
    /// The corners' square inside the screen: a continuous corner bends
    /// over about 1.53 times its radius.
    static let cornerSide = ceil(Radius.screen * 1.6)
    /// The shadow's blur and colour, about as dark at the screen's edge as
    /// ADR 0114's gradient strip.
    private static let blur: CGFloat = 18
    private static let shade = UIColor.black.withAlphaComponent(0.24)

    /// The panel is on the left: the edge is the screen's left one.
    var onLeft = true { didSet { if onLeft != oldValue { show() } } }
    /// Both sides' bitmaps, so a drag that opens the other panel draws
    /// nothing.
    private var drawn: [Bool: CGImage] = [:]

    override init(frame: CGRect) {
        super.init(frame: frame)
        layer.contentsGravity = .resize
        registerForTraitChanges([
            UITraitUserInterfaceStyle.self, UITraitAccessibilityContrast.self, UITraitDisplayScale.self,
        ]) { (view: Self, _) in
            view.drawn = [:]
            view.show()
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is unused") }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        show()
    }

    /// The bitmap's height: both corners and one stretched row between.
    private static var height: CGFloat { cornerSide * 2 + 1 }

    private func show() {
        guard window != nil else { return }
        if drawn.isEmpty {
            for side in [true, false] { drawn[side] = draw(onLeft: side) }
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        layer.contents = drawn[onLeft]
        layer.contentsScale = traitCollection.displayScale
        layer.contentsCenter = CGRect(x: 0, y: Self.cornerSide / Self.height, width: 1, height: 1 / Self.height)
        CATransaction.commit()
    }

    /// The corners in the panels' colour, then the screen's shadow over
    /// everything outside its outline.
    private func draw(onLeft: Bool) -> CGImage? {
        let reach = Self.reach, inside = Self.cornerSide
        let size = CGSize(width: reach + inside, height: Self.height)
        // The screen, reaching well past the bitmap on its far side.
        let far = inside * 4
        let screen = CGRect(x: onLeft ? reach : inside - far, y: 0, width: far, height: size.height)
        let outline = UIBezierPath(roundedRect: screen, cornerRadius: Radius.screen)
        let format = UIGraphicsImageRendererFormat(for: traitCollection)
        format.opaque = false
        let fill = UIColor.systemGroupedBackground.resolvedColor(with: traitCollection)
        return UIGraphicsImageRenderer(size: size, format: format).image { context in
            let outside = UIBezierPath(rect: CGRect(origin: .zero, size: size))
            outside.append(outline)
            outside.usesEvenOddFillRule = true
            outside.addClip()
            fill.setFill()
            let x = onLeft ? reach : 0
            UIBezierPath(rect: CGRect(x: x, y: 0, width: inside, height: inside)).fill()
            UIBezierPath(rect: CGRect(x: x, y: size.height - inside, width: inside, height: inside)).fill()
            // The screen's own fill is clipped away: only its shadow shows.
            context.cgContext.setShadow(offset: .zero, blur: Self.blur, color: Self.shade.cgColor)
            UIColor.black.setFill()
            outline.fill()
        }.cgImage
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

    /// Whether a list row in `view` shows its swipe actions: UIKit's
    /// (private) pull view or its buttons, laid out and visible. Waiting for the list's
    /// swipe recognizer instead let a swipe right over a row with a full
    /// swipe run the row's first action.
    static func showsSwipeActions(in view: UIView) -> Bool {
        let name = NSStringFromClass(type(of: view))
        if !view.isHidden, view.alpha > 0, view.bounds.width > 1,
           name.contains("SwipeAction"), name.contains("PullView") || name.contains("Button") {
            return true
        }
        return view.subviews.contains { showsSwipeActions(in: $0) }
    }

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

/// Which drags move a drawer. Closed, a sideways one toward opening
/// (ADR 0114). Open (ADR 0115): either way from the dimmed sliver or within
/// `innerEdge` of the panel's edge beside it; over the panel's content only
/// toward closing, since the other way is the content's (an Identities
/// row's actions are on a left swipe).
enum DrawerDrag {
    /// How far into the panel, from its edge beside the sliver, a drag
    /// still moves the drawer.
    static let innerEdge: CGFloat = 20
    /// The sliver of the screen left showing while the panel is open.
    static let sliver: CGFloat = 72
    /// The near edge that opens a panel while only the edge does (edit
    /// mode), and always.
    static let edgeWidth: CGFloat = 22

    /// Whether a touch that has moved by `delta` is the drawer's: `nil`
    /// until it is clear. `sign` is +1 when the screen moves right to open.
    /// A vertical scroll is never taken: up or down by 4 pt, or past 10 pt
    /// without being mostly sideways, declines. Closed, only a move toward
    /// opening is taken; open, either way, but `overPanel` (it started over
    /// the panel's content) only toward closing.
    static func decide(_ delta: CGVector, sign: CGFloat, isOpen: Bool, overPanel: Bool = false) -> Bool? {
        let across = abs(delta.dx), down = abs(delta.dy)
        if down > across * 1.2 { return down >= 4 ? false : nil }
        let toward = delta.dx * sign > 0
        // Closed, away from opening is another drawer's or a table's; open
        // over the panel, toward opening is the content's.
        let otherWay = isOpen ? overPanel && toward : !toward
        guard across >= 10 else {
            if across >= 4, otherWay { return false }
            return nil
        }
        // Mostly sideways, or it belongs to a scroll.
        guard across > down * 1.2 else { return false }
        return !otherWay
    }

    /// Which panel shows when SwiftUI's `isOpen`s change from `was…` to
    /// `leading` and `trailing` while `open` shows: the one just asked for
    /// wins, then the one already open, then Identities over the sidebar.
    static func wanted(
        leading: Bool, trailing: Bool, wasLeading: Bool, wasTrailing: Bool, open: DrawerSide?
    ) -> DrawerSide? {
        if leading && !wasLeading { return .leading }
        if trailing && !wasTrailing { return .trailing }
        if let open, open == .leading ? leading : trailing { return open }
        return trailing ? .trailing : leading ? .leading : nil
    }

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
