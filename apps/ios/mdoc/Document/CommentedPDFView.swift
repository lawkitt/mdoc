import MdocFFI
import PDFKit
import SwiftUI
import UIKit

/// A rendered DOCX original in PDFKit with its comments: each passage
/// tinted, and either margin markers (a phone, a narrow pane) or the rail
/// of cards beside the page at their passages' height (an iPad's wide
/// pane). Marks are never links: the core's `mdoc-comment:` links come off
/// the shown page, and a real link under a comment keeps its target.
struct CommentedPDFView: UIViewRepresentable {
    let url: URL
    let comments: DocxComments
    let showsComments: Bool
    let rail: Bool
    /// The comment whose passage is strengthened: the open sheet's or the
    /// expanded card's.
    let active: String?
    let onMarker: (DocxComments.Marker) -> Void
    let onCard: (String) -> Void
    /// The ⤡ / ↑ control's state and actions.
    let viewport: PageViewport

    func makeUIView(context: Context) -> CommentedPDFContainer {
        let view = CommentedPDFContainer()
        view.load(url: url, comments: comments)
        view.pdfView.onControl = { [weak viewport] in viewport?.report($0) }
        viewport.attach { [weak view] in view?.pdfView.perform($0) }
        return view
    }

    func updateUIView(_ view: CommentedPDFContainer, context: Context) {
        if view.url != url {
            view.load(url: url, comments: comments)
        }
        view.onMarker = onMarker
        view.onCard = onCard
        view.update(showsComments: showsComments, rail: rail, active: active)
    }
}

/// The PDF view, the markers over it and the rail beside it, in UIKit so
/// they follow every scroll and zoom frame. For VoiceOver it reads the page
/// line by line, each marker or card right after its line, and offers a
/// Comments rotor.
final class CommentedPDFContainer: UIView {
    static let railWidth: CGFloat = 260
    private static let markerSize: CGFloat = 24
    /// The touch target around a marker on each side: 44 pt in all.
    static let markerTouchSlop: CGFloat = 10

    /// Indexes of markers that share one, in order: a marker joins the
    /// group whose first marker's touch target its own would overlap, on
    /// the same page (#962). A marker in `visible` never joins a first
    /// marker scrolled out of it, which would hide it behind one nobody
    /// sees; their targets overlap only in a strip at the edge, where the
    /// later marker, on top, takes the tap.
    static func markerGroups(_ markers: [(page: Int, frame: CGRect)], visible: CGRect? = nil) -> [[Int]] {
        func out(_ index: Int) -> Bool { visible.map { !$0.intersects(markers[index].frame) } ?? false }
        var groups: [[Int]] = []
        for (index, marker) in markers.enumerated() {
            if let first = groups.last?.first, markers[first].page == marker.page,
                marker.frame.minY - markerTouchSlop < markers[first].frame.maxY + markerTouchSlop,
                !out(first) || out(index)
            {
                groups[groups.count - 1].append(index)
            } else {
                groups.append([index])
            }
        }
        return groups
    }

    let pdfView = FittingPDFView()
    private let overlay = PassthroughView()
    private let railView = PassthroughView()
    private(set) var url: URL?
    private var comments = DocxComments(comments: [], anchors: [])
    private var showsComments = true
    private var rail = false
    private var active: String?
    private var markerButtons: [CommentMarkerButton] = []
    private var cards: [CommentRailCard] = []
    private var tints: [String: [PDFAnnotation]] = [:]
    /// Each page's text right edge, 0..1 of its width, read once.
    private var textRights: [Int: CGFloat] = [:]
    private var underlines: [PDFAnnotation] = []
    private var scrollObservation: NSKeyValueObservation?
    private var lineElements: [PageLineElement]?
    /// The lines, markers and cards in reading order, built when VoiceOver
    /// first asks.
    private var readingOrder: [Any]?
    var onMarker: (DocxComments.Marker) -> Void = { _ in }
    var onCard: (String) -> Void = { _ in }

    override init(frame: CGRect) {
        super.init(frame: frame)
        pdfView.autoScales = true
        pdfView.displayMode = .singlePageContinuous
        pdfView.backgroundColor = .secondarySystemBackground
        overlay.clipsToBounds = true
        railView.clipsToBounds = true
        railView.backgroundColor = .secondarySystemBackground
        addSubview(pdfView)
        addSubview(overlay)
        addSubview(railView)
        let tap = UITapGestureRecognizer(target: self, action: #selector(tapped(_:)))
        tap.cancelsTouchesInView = false
        tap.delegate = self
        // A double-tap zooms; only a single tap opens a passage's thread.
        tap.require(toFail: pdfView.doubleTapGesture)
        pdfView.addGestureRecognizer(tap)
        NotificationCenter.default.addObserver(
            self, selector: #selector(placeComments), name: .PDFViewScaleChanged, object: pdfView)
        NotificationCenter.default.addObserver(
            self, selector: #selector(voiceOverChanged), name: UIAccessibility.voiceOverStatusDidChangeNotification,
            object: nil)
        voiceOverChanged()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    func load(url: URL, comments: DocxComments) {
        self.url = url
        self.comments = comments
        tints = [:]
        underlines = []
        textRights = [:]
        lineElements = nil
        readingOrder = nil
        let document = PDFDocument(url: url)
        if let document {
            Self.removeMarks(from: document)
        }
        pdfView.document = document
        rebuildComments()
    }

    func update(showsComments: Bool, rail: Bool, active: String?) {
        let changed = showsComments != self.showsComments || rail != self.rail
        let activeChanged = active != self.active
        self.showsComments = showsComments
        self.rail = rail
        self.active = active
        if changed {
            rebuildComments()
            setNeedsLayout()
        } else if activeChanged {
            restyleTints()
            for card in cards { card.expanded = card.commentID == active }
            placeComments()
        }
    }

    // MARK: Marks

    /// The core's marks come off the shown page, so nothing the user taps
    /// is an `mdoc-comment:` link; a mark over a real link keeps its target.
    private static func removeMarks(from document: PDFDocument) {
        for index in 0..<document.pageCount {
            guard let page = document.page(at: index) else { continue }
            for annotation in page.annotations {
                guard let target = CommentMarks.target(of: annotation) else { continue }
                if let separator = target.firstIndex(of: ";"),
                    let link = URL(string: String(target[target.index(after: separator)...])),
                    ["http", "https"].contains(link.scheme?.lowercased() ?? "")
                {
                    annotation.url = link
                } else {
                    page.removeAnnotation(annotation)
                }
            }
        }
    }

    private func rebuildComments() {
        for tint in tints.values.joined() + underlines { tint.page?.removeAnnotation(tint) }
        tints = [:]
        underlines = []
        markerButtons.forEach { $0.removeFromSuperview() }
        cards.forEach { $0.removeFromSuperview() }
        markerButtons = []
        cards = []
        readingOrder = nil
        voiceOverChanged()
        guard showsComments, let document = pdfView.document else {
            placeComments()
            return
        }
        for (id, passage) in comments.passages {
            tints[id] = passage.compactMap { place in
                guard let page = document.page(at: place.page) else { return nil }
                let tint = Self.annotation(.highlight, over: Self.pageRect(place.rect, on: page))
                page.addAnnotation(tint)
                return tint
            }
        }
        restyleTints()
        if rail {
            for marker in comments.markers {
                for thread in marker.threads {
                    let card = CommentRailCard(thread: thread)
                    card.expanded = thread.id == active
                    card.addAction(UIAction { [weak self] _ in self?.onCard(thread.id) }, for: .touchUpInside)
                    card.onFocus = { [weak self] in self?.reveal(marker) }
                    railView.addSubview(card)
                    cards.append(card)
                }
            }
        } else {
            for marker in comments.markers {
                let button = CommentMarkerButton(marker: marker)
                button.addAction(
                    UIAction { [weak self, weak button] _ in
                        if let button { self?.onMarker(button.shown) }
                    }, for: .primaryActionTriggered)
                button.onFocus = { [weak self] in self?.reveal(marker) }
                overlay.addSubview(button)
                markerButtons.append(button)
            }
        }
        placeComments()
    }

    /// The comments' yellow, as on a white page in light mode: the page is
    /// white in both themes.
    static let tint = UIColor.systemYellow.resolvedColor(with: UITraitCollection(userInterfaceStyle: .light))

    /// Every passage softly tinted; the active one stronger and underlined,
    /// so colour is never the only cue. The page is white in both themes.
    private func restyleTints() {
        for underline in underlines { underline.page?.removeAnnotation(underline) }
        underlines = []
        for (id, list) in tints {
            let isActive = id == active
            for tint in list {
                tint.color = Self.tint.withAlphaComponent(isActive ? 0.4 : 0.14)
                guard isActive, let page = tint.page else { continue }
                let underline = Self.annotation(.underline, over: tint.bounds)
                underline.color = .black
                page.addAnnotation(underline)
                underlines.append(underline)
            }
        }
    }

    /// A markup annotation over `bounds`, its one quadrilateral the bounds
    /// (PDFKit draws markup only where its quadrilaterals are).
    private static func annotation(_ type: PDFAnnotationSubtype, over bounds: CGRect) -> PDFAnnotation {
        let annotation = PDFAnnotation(bounds: bounds, forType: type, withProperties: nil)
        annotation.quadrilateralPoints = [
            NSValue(cgPoint: CGPoint(x: 0, y: bounds.height)), NSValue(cgPoint: CGPoint(x: bounds.width, y: bounds.height)),
            NSValue(cgPoint: .zero), NSValue(cgPoint: CGPoint(x: bounds.width, y: 0)),
        ]
        return annotation
    }

    /// A normalized rectangle (0..1, top left) in the page's own space.
    private static func pageRect(_ rect: PageRect, on page: PDFPage) -> CGRect {
        let box = page.bounds(for: .cropBox)
        return CGRect(
            x: box.minX + CGFloat(rect.x) * box.width,
            y: box.maxY - CGFloat(rect.y + rect.h) * box.height,
            width: max(CGFloat(rect.w) * box.width, 1),
            height: CGFloat(rect.h) * box.height)
    }

    /// A marker's line in this view's coordinates.
    fileprivate func lineFrame(_ marker: DocxComments.Marker) -> (line: CGRect, page: CGRect)? {
        guard let page = pdfView.document?.page(at: marker.page) else { return nil }
        let line = pdfView.convert(Self.pageRect(marker.line, on: page), from: page)
        let shown = pdfView.convert(page.bounds(for: .cropBox), from: page)
        return (convert(line, from: pdfView), convert(shown, from: pdfView))
    }

    // MARK: Layout

    override func layoutSubviews() {
        super.layoutSubviews()
        var page = bounds
        if rail && showsComments {
            page.size.width = max(bounds.width - Self.railWidth, 0)
            railView.frame = CGRect(x: page.maxX, y: 0, width: bounds.width - page.width, height: bounds.height)
        } else {
            railView.frame = .zero
        }
        pdfView.frame = page
        overlay.frame = page
        observeScrolling()
        placeComments()
    }

    private func observeScrolling() {
        guard scrollObservation == nil, let scroll = pdfView.firstSubview(of: UIScrollView.self) else { return }
        scrollObservation = scroll.observe(\.contentOffset, options: []) { [weak self] _, _ in
            MainActor.assumeIsolated { self?.placeComments() }
        }
    }

    /// Markers in the page's right margin on their line, clear of the text,
    /// or at the page's edge where the margin is too narrow; cards at their
    /// passage's height, pushed down when they would overlap.
    @objc private func placeComments() {
        // Markers whose touch targets would overlap share the first one's
        // marker, with the count, so a tap never lands on a neighbour
        // (#962); zoom changes which.
        var placed: [(button: CommentMarkerButton, page: Int, frame: CGRect)] = []
        for button in markerButtons {
            guard let frames = lineFrame(button.marker) else { continue }
            let size = Self.markerSize
            let margin = frames.page.width * (1 - textRight(ofPage: button.marker.page))
            let centre = margin >= size + 8 ? frames.page.maxX - margin / 2 : frames.page.maxX - size / 2 - 4
            let origin = CGPoint(x: centre - size / 2, y: frames.line.midY - size / 2)
            button.frame = convert(CGRect(origin: origin, size: CGSize(width: size, height: size)), to: overlay)
            placed.append((button, button.marker.page, button.frame))
        }
        var regrouped = false
        for group in Self.markerGroups(placed.map { ($0.page, $0.frame) }, visible: overlay.bounds) {
            let leader = placed[group[0]].button
            let merged = DocxComments.Marker(
                id: leader.marker.id, page: leader.marker.page, line: leader.marker.line,
                threads: group.flatMap { placed[$0].button.marker.threads })
            regrouped = leader.show(merged) || regrouped
            for (offset, index) in group.enumerated() {
                let button = placed[index].button
                let hidden = offset > 0
                regrouped = regrouped || button.isHidden != hidden
                button.isHidden = hidden
            }
        }
        if regrouped { readingOrder = nil }
        var bottom = -CGFloat.infinity
        let width = railView.bounds.width - 2 * Space.xs
        guard width > 0 else { return }
        for card in cards {
            guard let marker = comments.marker(of: card.commentID), let frames = lineFrame(marker) else { continue }
            let height = card.systemLayoutSizeFitting(
                CGSize(width: width, height: UIView.layoutFittingCompressedSize.height),
                withHorizontalFittingPriority: .required, verticalFittingPriority: .fittingSizeLevel).height
            let top = max(convert(frames.line, to: railView).minY - Space.xs, bottom + Space.xs)
            card.frame = CGRect(x: Space.xs, y: top, width: width, height: height)
            bottom = top + height
        }
        // At the end of the document the page can't scroll the last cards
        // up to the rail, so they stack up from its bottom instead.
        guard let scroll = pdfView.firstSubview(of: UIScrollView.self),
            scroll.contentOffset.y >= scroll.contentSize.height - scroll.bounds.height + scroll.adjustedContentInset.bottom - 1
        else { return }
        var limit = railView.bounds.height - Space.xs
        for card in cards.reversed() where card.frame.maxY > limit {
            card.frame.origin.y = max(limit - card.frame.height, 0)
            limit = card.frame.minY - Space.xs
        }
    }

    /// How far right the page's own text reaches, 0..1 of its width: the
    /// margin beyond it is where a marker sits clear of the text. A page
    /// with no text reaches its edge.
    private func textRight(ofPage index: Int) -> CGFloat {
        if let known = textRights[index] { return known }
        var right: CGFloat = 1
        if let page = pdfView.document?.page(at: index) {
            let box = page.bounds(for: pdfView.displayBox)
            if box.width > 0, let text = page.selection(for: box), !(text.string ?? "").isEmpty {
                right = min(max((text.bounds(for: page).maxX - box.minX) / box.width, 0), 1)
            }
        }
        textRights[index] = right
        return right
    }

    /// Scrolls a marker's line into view, when it isn't, without animation.
    func reveal(_ marker: DocxComments.Marker) {
        guard let page = pdfView.document?.page(at: marker.page), let frames = lineFrame(marker) else { return }
        guard !pdfView.frame.insetBy(dx: 0, dy: 40).contains(frames.line) else { return }
        pdfView.go(to: Self.pageRect(marker.line, on: page).insetBy(dx: 0, dy: -80), on: page)
    }

    // MARK: Taps

    /// A tap on a passage opens its comments, as its marker or card does.
    @objc private func tapped(_ gesture: UITapGestureRecognizer) {
        guard showsComments, let page = pdfView.page(for: gesture.location(in: pdfView), nearest: false) else { return }
        let point = pdfView.convert(gesture.location(in: pdfView), to: page)
        guard let index = pdfView.document?.index(for: page) else { return }
        let hit = comments.passages.first { _, places in
            places.contains { $0.page == index && Self.pageRect($0.rect, on: page).insetBy(dx: -2, dy: -2).contains(point) }
        }
        guard let id = hit?.key else { return }
        if rail {
            onCard(id)
        } else if let marker = comments.marker(of: id) {
            onMarker(marker)
        }
    }

    // MARK: Accessibility

    /// The markers that show: one per group of close lines.
    private var visibleMarkers: [CommentMarkerButton] {
        markerButtons.filter { !$0.isHidden }
    }

    /// The thread ids each shown marker opens, top to bottom: for tests.
    var shownMarkerThreads: [[String]] {
        visibleMarkers.map { $0.shown.threads.map(\.id) }
    }

    /// While VoiceOver runs with comments on the page, the page's lines and
    /// the comments in reading order replace PDFKit's own elements.
    private var readsLines: Bool {
        UIAccessibility.isVoiceOverRunning && showsComments && !comments.markers.isEmpty
    }

    @objc private func voiceOverChanged() {
        pdfView.accessibilityElementsHidden = readsLines
        readingOrder = nil
    }

    override var accessibilityElements: [Any]? {
        get {
            // Without VoiceOver the markers and cards follow the page, for
            // Voice Control, Switch Control and UI tests.
            guard readsLines else { return [pdfView] + visibleMarkers + cards }
            if let readingOrder { return readingOrder }
            let lines = lineElements ?? buildLines()
            lineElements = lines
            // Each marker or card right after its line.
            let elements: [(page: Int, y: CGFloat, element: Any)] =
                lines.map { ($0.pageIndex, $0.top, $0 as Any) }
                + visibleMarkers.map { ($0.marker.page, CGFloat($0.marker.line.y) + 0.0001, $0 as Any) }
                + cards.compactMap { card in
                    comments.marker(of: card.commentID).map { ($0.page, CGFloat($0.line.y) + 0.0001, card as Any) }
                }
            let ordered = elements.enumerated().sorted { a, b in
                (a.element.page, a.element.y, a.offset) < (b.element.page, b.element.y, b.offset)
            }.map(\.element.element)
            readingOrder = ordered
            return ordered
        }
        set {}
    }

    override var accessibilityCustomRotors: [UIAccessibilityCustomRotor]? {
        get {
            guard rail ? !cards.isEmpty : !visibleMarkers.isEmpty else { return [] }
            return [UIAccessibilityCustomRotor(name: String(localized: "Comments")) { [weak self] predicate in
                // Read on every search: a zoom since may have merged markers.
                guard let self else { return nil }
                let items: [UIView] = self.rail ? self.cards.map { $0 as UIView } : self.visibleMarkers.map { $0 as UIView }
                let target = predicate.currentItem.targetElement as AnyObject
                // A marker merged since into the one above it: go on from
                // that one.
                let current =
                    items.firstIndex { $0 === target }
                    ?? (target as? CommentMarkerButton)?.marker.threads.first.flatMap { thread in
                        items.firstIndex { item in
                            (item as? CommentMarkerButton)?.shown.threads.contains { $0.id == thread.id } ?? false
                        }
                    }
                let next: Int
                switch (predicate.searchDirection, current) {
                case (.next, let index?): next = index + 1
                case (.previous, let index?): next = index - 1
                case (.next, nil): next = 0
                case (.previous, nil): next = items.count - 1
                @unknown default: return nil
                }
                guard items.indices.contains(next) else { return nil }
                return UIAccessibilityCustomRotorItemResult(targetElement: items[next], targetRange: nil)
            }]
        }
        set {}
    }

    /// One element per line of text, top to bottom, page by page.
    private func buildLines() -> [PageLineElement] {
        guard let document = pdfView.document else { return [] }
        var elements: [PageLineElement] = []
        for index in 0..<document.pageCount {
            guard let page = document.page(at: index), let all = page.selection(for: page.bounds(for: .cropBox)) else {
                continue
            }
            let box = page.bounds(for: .cropBox)
            for line in all.selectionsByLine() {
                let text = line.string?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                guard !text.isEmpty else { continue }
                let bounds = line.bounds(for: page)
                let element = PageLineElement(accessibilityContainer: self)
                element.accessibilityLabel = text
                element.accessibilityTraits = .staticText
                element.container = self
                element.page = page
                element.pageIndex = index
                element.bounds = bounds
                element.top = (box.maxY - bounds.maxY) / max(box.height, 1)
                elements.append(element)
            }
        }
        return elements
    }

    /// Scrolls a line into view when VoiceOver moves to one off screen.
    fileprivate func reveal(line: PageLineElement) {
        guard let page = line.page else { return }
        let shown = convert(pdfView.convert(line.bounds, from: page), from: pdfView)
        guard !pdfView.frame.contains(shown) else { return }
        pdfView.go(to: line.bounds.insetBy(dx: 0, dy: -80), on: page)
    }

}

extension CommentedPDFContainer: UIGestureRecognizerDelegate {
    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        true
    }
}

/// A line of the page for VoiceOver, framed wherever the page is now.
final class PageLineElement: UIAccessibilityElement {
    weak var container: CommentedPDFContainer?
    weak var page: PDFPage?
    var pageIndex = 0
    var bounds: CGRect = .zero
    /// 0..1 of the page from its top, for reading order.
    var top: CGFloat = 0

    override var accessibilityFrame: CGRect {
        get {
            guard let container, let page else { return .zero }
            let pdfView = container.pdfView
            return UIAccessibility.convertToScreenCoordinates(pdfView.convert(bounds, from: page), in: pdfView)
        }
        set {}
    }

    override func accessibilityElementDidBecomeFocused() {
        container?.reveal(line: self)
    }
}

/// A margin marker: a labelled button with the line's comment count.
final class CommentMarkerButton: UIButton {
    /// The marker of this button's own line.
    let marker: DocxComments.Marker
    /// What it shows and opens: its own threads, plus those of the next
    /// lines when their markers would sit on top of it (#962).
    private(set) var shown: DocxComments.Marker
    var onFocus: () -> Void = {}

    init(marker: DocxComments.Marker) {
        self.marker = marker
        shown = marker
        super.init(frame: .zero)
        configure()
        accessibilityHint = String(localized: "Opens the comments.")
        accessibilityIdentifier = "comment-marker"
    }

    /// Shows `marker`'s threads; true when that changed what it shows.
    @discardableResult
    func show(_ marker: DocxComments.Marker) -> Bool {
        guard marker.threads.map(\.id) != shown.threads.map(\.id) else { return false }
        shown = marker
        configure()
        return true
    }

    private func configure() {
        // A soft yellow disc with an ink glyph on the white page, no
        // shadow; a resolved thread a quiet grey check.
        var configuration = UIButton.Configuration.filled()
        configuration.baseBackgroundColor = shown.resolved
            ? UIColor(white: 0.9, alpha: 1) : CommentedPDFContainer.tint.withAlphaComponent(0.55)
        configuration.baseForegroundColor = shown.resolved ? UIColor(white: 0.35, alpha: 1) : UIColor(white: 0.1, alpha: 1)
        configuration.cornerStyle = .capsule
        configuration.contentInsets = .zero
        if shown.threads.count > 1 {
            configuration.title = String(shown.threads.count)
            configuration.titleTextAttributesTransformer = UIConfigurationTextAttributesTransformer {
                var attributes = $0
                attributes.font = .systemFont(ofSize: 12, weight: .semibold)
                return attributes
            }
        } else {
            configuration.image = UIImage(systemName: shown.resolved ? "checkmark" : "text.bubble.fill")?
                .applyingSymbolConfiguration(.init(pointSize: 11, weight: .semibold))
        }
        self.configuration = configuration
        accessibilityLabel = shown.spokenLabel
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    /// A 44 pt target around the 30 pt marker.
    override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        bounds.insetBy(dx: -CommentedPDFContainer.markerTouchSlop, dy: -CommentedPDFContainer.markerTouchSlop).contains(point)
    }

    override func accessibilityElementDidBecomeFocused() {
        onFocus()
    }
}

/// A rail card: initials, name and date, two lines of the text and "N
/// replies"; expanded, all of the text and the replies under it. A resolved
/// thread's card is dimmed and says so.
final class CommentRailCard: UIControl {
    let commentID: String
    var onFocus: () -> Void = {}
    private let body = CardText()
    private let replies = CardText()
    private let replyCount: Int
    private let replyText: String

    var expanded = false {
        didSet { showExpanded() }
    }

    private func showExpanded() {
        body.textContainer.maximumNumberOfLines = expanded ? 0 : 2
        // Expanded, the text selects and copies like the page's.
        body.isUserInteractionEnabled = expanded
        replies.isUserInteractionEnabled = expanded
        let shown: String? = replyCount == 0 ? nil : expanded ? replyText
            : replyCount == 1 ? String(localized: "1 reply") : String(localized: "\(replyCount) replies")
        replies.text = shown ?? ""
        replies.isHidden = shown == nil
        // The text view keeps its old height until it lays out again.
        body.invalidateIntrinsicContentSize()
        replies.invalidateIntrinsicContentSize()
        layer.borderWidth = expanded ? 2 : 0
        accessibilityValue = expanded ? String(localized: "Expanded") : nil
    }

    init(thread: DocxComments.Thread) {
        let comment = thread.comment
        commentID = comment.id
        replyCount = thread.replies.count
        replyText = thread.replies.map { "\($0.shownAuthor), \($0.shownDate): \($0.text)" }.joined(separator: "\n")
        super.init(frame: .zero)
        alpha = comment.resolved ? 0.6 : 1
        backgroundColor = .systemBackground
        layer.cornerRadius = Radius.control
        layer.cornerCurve = .continuous
        layer.borderColor = UIColor.ink.cgColor
        let initials = UILabel()
        initials.text = comment.initials
        initials.font = .preferredFont(forTextStyle: .caption2)
        initials.textColor = .onAccent
        initials.backgroundColor = .ink
        initials.textAlignment = .center
        initials.layer.cornerRadius = 11
        initials.clipsToBounds = true
        let name = UILabel()
        name.text = comment.shownAuthor
        name.font = .preferredFont(forTextStyle: .subheadline).bold()
        name.adjustsFontForContentSizeCategory = true
        let date = UILabel()
        date.text = comment.shownDate
        date.font = .preferredFont(forTextStyle: .caption1)
        date.textColor = .secondaryLabel
        date.adjustsFontForContentSizeCategory = true
        body.text = comment.text
        body.font = .preferredFont(forTextStyle: .footnote)
        body.textColor = .label
        body.adjustsFontForContentSizeCategory = true
        replies.font = .preferredFont(forTextStyle: .caption1)
        replies.textColor = .secondaryLabel
        replies.adjustsFontForContentSizeCategory = true
        let header = UIStackView(arrangedSubviews: [initials, name, date])
        header.spacing = Space.xxs
        header.alignment = .center
        if comment.resolved {
            let check = UIImageView(image: UIImage(systemName: "checkmark"))
            check.tintColor = .secondaryLabel
            header.addArrangedSubview(check)
        }
        let stack = UIStackView(arrangedSubviews: [header, body, replies])
        stack.axis = .vertical
        stack.spacing = Space.xxs
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            initials.widthAnchor.constraint(equalToConstant: 22),
            initials.heightAnchor.constraint(equalToConstant: 22),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.xs),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.xs),
            stack.topAnchor.constraint(equalTo: topAnchor, constant: Space.xs),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Space.xs),
        ])
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        showExpanded()
        isAccessibilityElement = true
        accessibilityLabel = thread.spokenLabel
        accessibilityTraits = .button
        accessibilityIdentifier = "comment-card"
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    /// Touches on expanded text select it; anywhere else they tap the card.
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        guard self.point(inside: point, with: event) else { return nil }
        for text in [body, replies] where text.isUserInteractionEnabled && !text.isHidden {
            let inText = convert(point, to: text)
            if text.point(inside: inText, with: event) { return text.hitTest(inText, with: event) ?? self }
        }
        return self
    }

    override func accessibilityActivate() -> Bool {
        sendActions(for: .touchUpInside)
        return true
    }

    override func accessibilityElementDidBecomeFocused() {
        onFocus()
    }
}

/// A card's read-only text: selectable once its card is expanded, laid out
/// like a label.
private final class CardText: UITextView {
    init() {
        super.init(frame: .zero, textContainer: nil)
        isEditable = false
        isSelectable = true
        isScrollEnabled = false
        backgroundColor = .clear
        textContainerInset = .zero
        textContainer.lineFragmentPadding = 0
        textContainer.lineBreakMode = .byTruncatingTail
        isAccessibilityElement = false
        isUserInteractionEnabled = false
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }
}

/// A view that takes only touches on its subviews.
final class PassthroughView: UIView {
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hit = super.hitTest(point, with: event)
        return hit === self ? nil : hit
    }
}

extension UIView {
    /// The first view of `type` under this one, breadth first.
    func firstSubview<T: UIView>(of type: T.Type) -> T? {
        var queue = subviews
        while !queue.isEmpty {
            let view = queue.removeFirst()
            if let match = view as? T { return match }
            queue += view.subviews
        }
        return nil
    }
}

private extension UIFont {
    func bold() -> UIFont {
        fontDescriptor.withSymbolicTraits(.traitBold).map { UIFont(descriptor: $0, size: 0) } ?? self
    }
}
