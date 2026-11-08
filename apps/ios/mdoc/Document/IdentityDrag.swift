import MdocFFI
import SwiftUI
import UIKit

/// Drag-to-merge in Identities (ADR 0106): in Select, a row's ≡ handle lifts
/// a copy of the row, and letting go over another name makes them one.
/// SwiftUI's drag and drop never delivers a drop inside the List that the
/// drag started in, so the drag is a gesture of our own: the rows report
/// their frames, and the row under the finger is found from those.
enum IdentityDrag {
    /// How far past a row's frame, above and below, still counts as over it:
    /// half the gap between two rows, so the target never flickers off
    /// between them.
    static let slop: CGFloat = Space.xxs

    /// Within this of the visible list's top or bottom, the list scrolls.
    static let edge: CGFloat = 56

    /// The fastest the list scrolls, in points a second, with the finger at
    /// or past an edge.
    static let maxSpeed: CGFloat = 700

    /// The row under `point`: one of `rows` whose frame (with `slop`) holds
    /// it, inside `visible`, the part of the list on screen. Never `dragged`'s
    /// own row. All in one coordinate space.
    static func target(
        at point: CGPoint, rows: [IdentityID: CGRect], visible: CGRect, dragged: IdentityID
    ) -> IdentityID? {
        guard visible.contains(point) else { return nil }
        return rows
            .filter { $0.key != dragged && $0.value.insetBy(dx: 0, dy: -slop).contains(point) }
            // Two frames that both reach the point (the slop): the nearer middle.
            .min { abs($0.value.midY - point.y) < abs($1.value.midY - point.y) }?
            .key
    }

    /// How fast the list scrolls with the finger at `y`, in points a second,
    /// up negative: nothing in the middle, faster the nearer an edge of
    /// `visible`, and `maxSpeed` at or past it.
    static func scrollSpeed(at y: CGFloat, visible: CGRect) -> CGFloat {
        guard visible.height > 2 * edge else { return 0 }
        let fromTop = y - visible.minY
        let fromBottom = visible.maxY - y
        if fromTop < edge { return -maxSpeed * min(1, (edge - fromTop) / edge) }
        if fromBottom < edge { return maxSpeed * min(1, (edge - fromBottom) / edge) }
        return 0
    }

    /// A content offset moved by `delta`, kept between `lowest` and `highest`.
    static func offset(_ y: CGFloat, by delta: CGFloat, lowest: CGFloat, highest: CGFloat) -> CGFloat {
        min(max(lowest, highest), max(lowest, y + delta))
    }
}

/// A drag in progress: whose row, where it was picked up, where the finger
/// is and the row under it, all in global points.
struct IdentityDragState: Equatable {
    let dragged: IdentityID
    /// The row's frame when it was lifted.
    let frame: CGRect
    /// Where the finger went down.
    let start: CGPoint
    var location: CGPoint
    var target: IdentityID?
    /// Let go away from a name: the copy slides back to its row.
    var returning = false

    /// The lifted copy's frame: the row's, moved up or down with the finger.
    var copyFrame: CGRect {
        frame.offsetBy(dx: 0, dy: location.y - start.y)
    }
}

/// The rows' and handles' frames on screen, and the list's, in global
/// points. Written as the list lays out and scrolls, read by the drag: not
/// observed, so a scroll redraws nothing.
@MainActor
final class IdentityDragTracking {
    var rows: [IdentityID: CGRect] = [:]
    var handles: [IdentityID: CGRect] = [:]
    var list: CGRect = .zero
    /// The top of the selection bar over the list's bottom, while it shows.
    var barTop: CGFloat?
    let autoScroll = IdentityAutoScroll()
    private(set) lazy var recognizer = HandleDragRecognizer(tracking: self)

    /// The part of the list on screen, above the selection bar.
    var visible: CGRect {
        guard let barTop, barTop < list.maxY else { return list }
        return CGRect(x: list.minX, y: list.minY, width: list.width, height: max(0, barTop - list.minY))
    }

    /// The name whose ≡ handle is at `point`.
    func handle(at point: CGPoint) -> IdentityID? {
        guard visible.contains(point) else { return nil }
        return handles.first { $0.value.contains(point) }?.key
    }
}

/// Scrolls the list while the finger stays near its top or bottom edge,
/// once a frame, at `speed` points a second; `onScroll` follows each step.
@MainActor
final class IdentityAutoScroll: NSObject {
    weak var scrollView: UIScrollView?
    var onScroll: () -> Void = {}
    var speed: CGFloat = 0 {
        didSet { speed == 0 ? stop() : start() }
    }
    private var link: CADisplayLink?
    private var last: CFTimeInterval?

    private func start() {
        guard link == nil, scrollView != nil else { return }
        let link = CADisplayLink(target: self, selector: #selector(step))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
        last = nil
    }

    @objc private func step(_ link: CADisplayLink) {
        defer { last = link.timestamp }
        guard let scrollView, let last else { return }
        let inset = scrollView.adjustedContentInset
        let y = IdentityDrag.offset(
            scrollView.contentOffset.y, by: speed * CGFloat(link.timestamp - last),
            lowest: -inset.top,
            highest: scrollView.contentSize.height + inset.bottom - scrollView.bounds.height)
        guard y != scrollView.contentOffset.y else { return }
        scrollView.contentOffset.y = y
        onScroll()
    }
}

/// What the handle's finger does.
enum HandleDragEvent {
    /// Down on `id`'s handle at the point, now lifted.
    case began(IdentityID, CGPoint)
    case moved(CGPoint)
    case ended(CGPoint)
    case cancelled
}

/// The handles' drag, a UIKit gesture on the list's own scroll view: a touch
/// that starts on a ≡ handle takes the finger (no press to wait out), so the
/// list neither scrolls nor swipes the row, and the drag lives on
/// while the lifted row scrolls out of view; any other touch is the list's.
/// SwiftUI's gestures can't do it: one on the List never begins for a touch
/// that starts in a row, and one on the handle ends when its row scrolls
/// away.
@MainActor
final class HandleDragRecognizer: NSObject, UIGestureRecognizerDelegate {
    var onEvent: (HandleDragEvent) -> Void = { _ in }
    private let tracking: IdentityDragTracking
    private let recognizer = UILongPressGestureRecognizer()
    /// The handle the finger went down on, and where.
    private var started: (id: IdentityID, at: CGPoint)?

    init(tracking: IdentityDragTracking) {
        self.tracking = tracking
        super.init()
        recognizer.addTarget(self, action: #selector(changed))
        recognizer.minimumPressDuration = 0
        recognizer.allowableMovement = .greatestFiniteMagnitude
        recognizer.delegate = self
    }

    /// Puts the gesture on `scrollView`, the List's, once.
    func install(on scrollView: UIScrollView) {
        guard recognizer.view !== scrollView else { return }
        recognizer.view?.removeGestureRecognizer(recognizer)
        scrollView.addGestureRecognizer(recognizer)
        tracking.autoScroll.scrollView = scrollView
    }

    func gestureRecognizer(_ recognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        // SwiftUI's global points are the window's.
        let point = touch.location(in: nil)
        started = tracking.handle(at: point).map { ($0, point) }
        return started != nil
    }

    @objc private func changed(_ recognizer: UILongPressGestureRecognizer) {
        let point = recognizer.location(in: nil)
        switch recognizer.state {
        case .began:
            // From where the finger went down: iOS begins the press once
            // the finger has moved a little.
            guard let started else { return }
            onEvent(.began(started.id, started.at))
            onEvent(.moved(point))
        case .changed:
            onEvent(.moved(point))
        case .ended:
            onEvent(.ended(point))
        default:
            onEvent(.cancelled)
        }
    }
}

/// Finds the List's scroll view from inside a row and puts the handles'
/// drag on it.
struct HandleDragInstaller: UIViewRepresentable {
    let recognizer: HandleDragRecognizer
    let onEvent: (HandleDragEvent) -> Void

    final class Probe: UIView {
        var found: ((UIScrollView) -> Void)?

        override func didMoveToWindow() {
            super.didMoveToWindow()
            var view = superview
            while let current = view, !(current is UIScrollView) { view = current.superview }
            if let scrollView = view as? UIScrollView { found?(scrollView) }
        }
    }

    func makeUIView(context: Context) -> Probe {
        let probe = Probe()
        probe.isUserInteractionEnabled = false
        probe.found = { [recognizer] in recognizer.install(on: $0) }
        return probe
    }

    func updateUIView(_ probe: Probe, context: Context) {
        recognizer.onEvent = onEvent
    }
}
