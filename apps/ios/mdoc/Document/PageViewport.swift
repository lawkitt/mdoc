import PDFKit
import SwiftUI
import UIKit

/// The Original's one control: back to fit once zoomed, or back to the
/// first page once past it at fit.
enum PageControl: Equatable {
    case fit
    case top

    var symbol: String {
        switch self {
        case .fit: "arrow.down.right.and.arrow.up.left"
        case .top: "arrow.up"
        }
    }

    var label: LocalizedStringKey {
        switch self {
        case .fit: "Fit page"
        case .top: "Back to top"
        }
    }
}

/// What a page view shows its control for, and the view that acts on it.
@MainActor @Observable
final class PageViewport {
    private(set) var control: PageControl?
    @ObservationIgnored private var act: (PageControl) -> Void = { _ in }

    /// A page view reports here; `act` does what the control asks.
    func attach(act: @escaping (PageControl) -> Void) {
        self.act = act
    }

    func report(_ control: PageControl?) {
        if control != self.control { self.control = control }
    }

    func perform() {
        guard let control else { return }
        act(control)
    }
}

/// The control: a 44 pt glass circle at the pane's bottom trailing corner,
/// fading in and out.
struct PageControlButton: View {
    let viewport: PageViewport
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.pageControlInset) private var inset

    var body: some View {
        ZStack {
            if let control = viewport.control {
                Button(action: viewport.perform) {
                    Image(systemName: control.symbol)
                        .font(.body.weight(.semibold))
                        .foregroundStyle(.primary)
                        .frame(width: Size.target, height: Size.target)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .glassEffect(.regular.interactive(), in: .circle)
                .accessibilityLabel(Text(control.label))
                .accessibilityIdentifier("page-control")
                .transition(.opacity.combined(with: .scale(scale: 0.8)))
            }
        }
        .animation(reduceMotion ? Motion.fade : Motion.snappy, value: viewport.control)
        .padding(Space.m)
        .padding(.bottom, inset)
        // Never larger than a bar's text: it is an icon.
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
    }
}

extension EnvironmentValues {
    /// The home indicator's height when the page runs under it, so the
    /// control stays above it.
    @Entry var pageControlInset: CGFloat = 0
}

/// A PDF view that fits the page to its width again whenever that width
/// changes (rotation, iPad's columns); `autoScales` alone keeps the scale it
/// first computed. Zoom stays between fit and 5× fit, a double-tap toggles
/// fit and 2× at the tapped point, and `onControl` hears which control the
/// pane shows.
final class FittingPDFView: PDFView {
    /// Past fit by more than this, the page counts as zoomed.
    static let fitTolerance: CGFloat = 0.02
    static let maxZoom: CGFloat = 5
    static let doubleTapZoom: CGFloat = 2

    var onControl: (PageControl?) -> Void = { _ in }
    private var fittedWidth: CGFloat = 0
    private var fit: CGFloat = 0
    private var shownControl: PageControl??
    private var scrollObservation: NSKeyValueObservation?
    private lazy var doubleTap: UITapGestureRecognizer = {
        let tap = UITapGestureRecognizer(target: self, action: #selector(doubleTapped(_:)))
        tap.numberOfTapsRequired = 2
        return tap
    }()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addGestureRecognizer(doubleTap)
        NotificationCenter.default.addObserver(
            self, selector: #selector(viewportChanged), name: .PDFViewScaleChanged, object: self)
        NotificationCenter.default.addObserver(
            self, selector: #selector(viewportChanged), name: .PDFViewPageChanged, object: self)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    override func layoutSubviews() {
        super.layoutSubviews()
        takeOverDoubleTap()
        observeScrolling()
        guard bounds.width > 0, bounds.width != fittedWidth, document != nil else { return }
        fittedWidth = bounds.width
        // The page the reader was on, and the point of it at the view's top:
        // PDFKit's destinations land mid-page after a new scale, so the
        // scroll offset puts that point back at the top (hiding the iPad's
        // sidebar refits the page; #963).
        let page = currentPage
        let anchor = page.map { convert(CGPoint(x: bounds.midX, y: bounds.minY), to: $0) }
        // The fit is only right once PDFKit has laid out its own views.
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            let fit = self.scaleFactorForSizeToFit
            self.fit = fit
            // Below fit a continuous column is a thin strip.
            self.minScaleFactor = fit
            self.maxScaleFactor = fit * Self.maxZoom
            self.scaleFactor = fit
            self.viewportChanged()
            guard let page else { return }
            self.go(to: page)
            guard let anchor, let scroll = self.scrollView else { return }
            let shift = self.convert(anchor, from: page).y - self.bounds.minY
            let top = -scroll.adjustedContentInset.top
            let bottom = max(
                scroll.contentSize.height - scroll.bounds.height + scroll.adjustedContentInset.bottom, top)
            scroll.contentOffset.y = min(max(scroll.contentOffset.y + shift, top), bottom)
        }
    }

    var scrollView: UIScrollView? { firstSubview(of: UIScrollView.self) }

    /// The double-tap that zooms, for a single tap to wait on.
    var doubleTapGesture: UITapGestureRecognizer { doubleTap }

    /// More than 2% off fit.
    var isZoomed: Bool {
        fit > 0 && abs(scaleFactor - fit) / fit > Self.fitTolerance
    }

    /// At fit, the first page's end is above the view's top.
    var isPastFirstPage: Bool {
        guard let first = document?.page(at: 0) else { return false }
        let pageBottom = convert(first.bounds(for: displayBox), from: first).maxY
        return pageBottom < bounds.minY + safeAreaInsets.top
    }

    func perform(_ control: PageControl) {
        switch control {
        case .fit: zoomToFit()
        case .top: scrollToTop()
        }
    }

    /// Back to fit, keeping the spot at the view's top where it is.
    func zoomToFit() {
        guard fit > 0, let scroll = scrollView, let zoomView = scroll.delegate?.viewForZooming?(in: scroll) else {
            scaleFactor = fit
            return
        }
        let target = scroll.zoomScale * fit / scaleFactor
        let top = zoomView.convert(CGPoint(x: 0, y: scroll.bounds.minY + scroll.adjustedContentInset.top), from: scroll)
        let size = CGSize(width: scroll.bounds.width / target, height: scroll.bounds.height / target)
        scroll.zoom(to: CGRect(origin: CGPoint(x: zoomView.bounds.minX, y: top.y), size: size), animated: true)
    }

    /// Back to the first page's top.
    func scrollToTop() {
        guard let scroll = scrollView else { return }
        scroll.setContentOffset(
            CGPoint(x: scroll.contentOffset.x, y: -scroll.adjustedContentInset.top), animated: true)
    }

    /// Fit and 2× at the tapped point, as a scan page does.
    @objc private func doubleTapped(_ gesture: UITapGestureRecognizer) {
        guard fit > 0 else { return }
        if isZoomed {
            zoomToFit()
            return
        }
        guard let scroll = scrollView, let zoomView = scroll.delegate?.viewForZooming?(in: scroll) else {
            scaleFactor = fit * Self.doubleTapZoom
            return
        }
        let target = scroll.zoomScale * Self.doubleTapZoom
        let point = gesture.location(in: zoomView)
        let size = CGSize(width: scroll.bounds.width / target, height: scroll.bounds.height / target)
        scroll.zoom(
            to: CGRect(x: point.x - size.width / 2, y: point.y - size.height / 2, width: size.width, height: size.height),
            animated: true)
    }

    /// PDFKit's own double-tap zoom gives way to `doubleTapped`, so fit
    /// means one thing.
    private func takeOverDoubleTap() {
        var queue: [UIView] = [self]
        while !queue.isEmpty {
            let view = queue.removeFirst()
            for case let tap as UITapGestureRecognizer in view.gestureRecognizers ?? []
            where tap !== doubleTap && tap.numberOfTapsRequired == 2 && tap.isEnabled {
                tap.isEnabled = false
            }
            queue += view.subviews
        }
    }

    private func observeScrolling() {
        guard scrollObservation == nil, let scroll = scrollView else { return }
        scrollObservation = scroll.observe(\.contentOffset, options: []) { [weak self] _, _ in
            MainActor.assumeIsolated { self?.viewportChanged() }
        }
    }

    @objc private func viewportChanged() {
        let control: PageControl? = isZoomed ? .fit : isPastFirstPage ? .top : nil
        guard shownControl != .some(control) else { return }
        shownControl = .some(control)
        onControl(control)
    }
}
