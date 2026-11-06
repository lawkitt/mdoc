import SwiftUI
import UIKit

/// The document's Markdown source in a read-only, selectable TextKit 2
/// `UITextView` (ADR 0085), rendered as desktop draws it unless Show
/// Markdown is on (ADR 0096). Candidates are links: a tap opens the
/// candidate sheet, in a table cell too; VoiceOver reaches them through the
/// Candidates rotor, and the edit menu adds Replace… for a selection.
struct MarkdownTextView: UIViewRepresentable {
    let snapshot: DocumentSnapshot
    let selected: CandidateID?
    /// The mentions the bar's scope reaches besides `selected`.
    var scoped: Set<CandidateID> = []
    /// Bumped by the review bar to scroll back to `selected`.
    var revealRequest = 0
    /// A Find match to scroll to when `revealRequest` changes.
    var revealRange: SourceRange?
    /// Markers hidden and blocks drawn; false shows the source.
    var rendered = true
    /// Document font › Serif; the styler reads it, the key restyles on it.
    var serif = false
    let onCandidate: (CandidateID) -> Void
    let onReplace: (SourceRange) -> Void
    let onReviewNext: () -> Void
    /// A candidate's long-press menu (ADR 0098); nil leaves none.
    var menu: (CandidateID) -> UIMenu? = { _ in nil }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeUIView(context: Context) -> CandidateTextView {
        let view = CandidateTextView()
        view.isEditable = false
        view.isSelectable = true
        view.dataDetectorTypes = []
        view.textDragInteraction?.isEnabled = false
        view.writingToolsBehavior = .none
        view.linkTextAttributes = [:]
        view.textLayoutManager?.usesHyphenation = false
        view.backgroundColor = .systemBackground
        view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 32, right: 12)
        view.delegate = context.coordinator
        view.accessibilityIdentifier = "document-text"
        view.accessibilityHint = String(localized: "Underlined words are names mdoc found. Use the Candidates rotor to move between them.")
        view.installAccessibility()
        let coordinator = context.coordinator
        view.configureCell = { [weak coordinator] cell in
            cell.delegate = coordinator
            cell.allowsCopy = false
        }
        view.onWidth = { [weak coordinator, weak view] width in
            guard let coordinator, let view else { return }
            coordinator.render(view, width: width)
        }
        return view
    }

    func updateUIView(_ view: CandidateTextView, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self
        view.onReviewNext = onReviewNext
        let key = StyleKey(
            snapshot: snapshot, selected: selected, scoped: scoped, rendered: rendered, serif: serif,
            dynamicTypeSize: context.environment.dynamicTypeSize,
            colorScheme: context.environment.colorScheme)
        if revealRequest != coordinator.revealRequest {
            coordinator.revealRequest = revealRequest
            if let range = revealRange, selected.flatMap({ snapshot.candidate($0) }).map({ $0.range.start > range.start || $0.range.end < range.end }) ?? true {
                coordinator.reveal(range: range, in: view)
                // The match selected, so the reader sees which words.
                let found = coordinator.offsets.nsRange(range)
                if found.location + found.length <= view.attributedText.length { view.selectedRange = found }
            } else if let selected, key == coordinator.styled {
                coordinator.reveal(selected, in: view, always: true)
            }
        }
        guard key != coordinator.styled else {
            coordinator.pending = nil
            return
        }
        // iOS draws a backgrounded app again in light and dark for the app
        // switcher. Restyling then rebuilt every table's cells twice, and
        // with Find names using the CPU it ran past the 10 s iOS allows, so
        // iOS stopped mdoc. The text catches up on return.
        if view.window?.windowScene?.activationState == .background {
            coordinator.restyleOnReturn(key, view)
            return
        }
        coordinator.restyle(key, view)
    }

    struct StyleKey: Equatable {
        var snapshot: DocumentSnapshot
        var selected: CandidateID?
        var scoped: Set<CandidateID>
        var rendered: Bool
        var serif: Bool
        var dynamicTypeSize: DynamicTypeSize
        var colorScheme: ColorScheme
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownTextView
        var styled: StyleKey?
        var revealRequest = 0
        var offsets = TextOffsets("")
        /// False until the first text is shown, so a new document starts at the top.
        var keepsScroll = false
        /// The style the view waits to show until mdoc is back on screen.
        var pending: StyleKey?
        nonisolated(unsafe) private var foreground: NSObjectProtocol?

        init(parent: MarkdownTextView) {
            self.parent = parent
        }

        deinit {
            if let foreground {
                NotificationCenter.default.removeObserver(foreground)
            }
        }

        func restyle(_ key: StyleKey, _ view: CandidateTextView) {
            pending = nil
            let textChanged = styled?.snapshot.markdown != key.snapshot.markdown
            let newlySelected = key.selected != styled?.selected ? key.selected : nil
            styled = key
            offsets = TextOffsets(key.snapshot.markdown)
            render(view, width: view.textWidth, keepsScroll: !textChanged || keepsScroll)
            keepsScroll = true
            if let newlySelected {
                // After the tables are placed, which follows the layout.
                DispatchQueue.main.async { [weak self, weak view] in
                    guard let self, let view else { return }
                    self.reveal(newlySelected, in: view)
                }
            } else if key.selected == nil {
                dropRevealRoom(in: view)
            }
        }

        /// Scrolls the candidate under review into the top third of the
        /// text, the part the candidate sheet leaves in view, so Next
        /// proposal never opens one hidden under the sheet. In a table the
        /// grid scrolls sideways to it too. Near the end of the document the
        /// text gets room below its end while the sheet is up.
        func reveal(_ id: CandidateID, in view: CandidateTextView, always: Bool = false) {
            guard let candidate = parent.snapshot.candidate(id) else { return }
            reveal(range: candidate.range, in: view, always: always)
        }

        /// Scrolls a source range into the text's top third, briefly
        /// selecting it when it is no candidate (a Find match).
        func reveal(range source: SourceRange, in view: CandidateTextView, always: Bool = true) {
            let range = offsets.nsRange(source)
            let cell = view.tableViews.lazy.flatMap { $0.cells.joined() }.first { cell in
                NSLocationInRange(range.location, NSRange(location: cell.location, length: cell.attributedText.length))
            }
            let rect: CGRect
            if let cell {
                guard let local = Self.firstRect(in: cell, location: range.location - cell.location, length: range.length) else { return }
                if let table = cell.superview as? MarkdownTableView {
                    table.scrollRectToVisible(cell.convert(local, to: table).insetBy(dx: -12, dy: 0), animated: false)
                }
                rect = cell.convert(local, to: view)
            } else {
                guard let found = Self.firstRect(in: view, location: range.location, length: range.length) else { return }
                rect = found
            }
            let insets = view.adjustedContentInset
            let top = view.contentOffset.y + insets.top
            let band = (view.bounds.height - insets.top - insets.bottom) / 3
            guard always || rect.minY < top + 8 || rect.maxY > top + band else { return }
            let highest = -insets.top
            let target = max(rect.minY - insets.top - band / 3, highest)
            // Room below the end, so a candidate there can rise above the sheet.
            let lowest = view.contentSize.height + insets.bottom - view.bounds.height
            if target > lowest {
                let more = target - lowest
                view.contentInset.bottom += more
                revealRoom += more
            }
            view.setContentOffset(CGPoint(x: view.contentOffset.x, y: target), animated: !UIAccessibility.isReduceMotionEnabled)
        }

        /// The room `reveal` added below the text, taken away once no
        /// candidate is under review.
        var revealRoom: CGFloat = 0

        func dropRevealRoom(in view: CandidateTextView) {
            guard revealRoom > 0 else { return }
            view.contentInset.bottom -= revealRoom
            revealRoom = 0
            let insets = view.adjustedContentInset
            let lowest = max(-insets.top, view.contentSize.height + insets.bottom - view.bounds.height)
            if view.contentOffset.y > lowest {
                view.setContentOffset(CGPoint(x: view.contentOffset.x, y: lowest), animated: !UIAccessibility.isReduceMotionEnabled)
            }
        }

        private static func firstRect(in view: UITextView, location: Int, length: Int) -> CGRect? {
            guard
                let start = view.position(from: view.beginningOfDocument, offset: location),
                let end = view.position(from: start, offset: length),
                let textRange = view.textRange(from: start, to: end)
            else { return nil }
            let rect = view.firstRect(for: textRange)
            return rect.isNull || rect.isInfinite ? nil : rect
        }

        func restyleOnReturn(_ key: StyleKey, _ view: CandidateTextView) {
            pending = key
            guard foreground == nil else { return }
            foreground = NotificationCenter.default.addObserver(
                forName: UIScene.willEnterForegroundNotification, object: nil, queue: .main
            ) { [weak self, weak view] _ in
                MainActor.assumeIsolated {
                    guard let self, let view, let key = self.pending, key != self.styled else { return }
                    self.restyle(key, view)
                }
            }
        }

        /// Styles the snapshot for a text column `width` wide (tables need
        /// one) and points the Candidates rotor at the text and the cells.
        func render(_ view: CandidateTextView, width: CGFloat, keepsScroll: Bool = true) {
            let snapshot = parent.snapshot
            let offset = view.contentOffset
            let styler = MarkdownStyler(
                traits: view.traitCollection, selected: parent.selected, scoped: parent.scoped, rendered: parent.rendered,
                width: width > 0 ? width : nil)
            view.show(styler.document(for: snapshot), width: width)
            var items: [CandidateTextView.RotorItem] = []
            for (order, candidate) in snapshot.candidates.enumerated() {
                let range = offsets.nsRange(candidate.range)
                let cell = view.tableViews.lazy.flatMap { $0.cells.joined() }.first { cell in
                    NSLocationInRange(range.location, NSRange(location: cell.location, length: cell.attributedText.length))
                }
                if let cell {
                    items.append(.init(view: cell, range: NSRange(location: range.location - cell.location, length: range.length), order: order))
                } else {
                    items.append(.init(view: view, range: range, order: order))
                }
            }
            view.rotorItems = items.sorted { $0.order < $1.order }
            if keepsScroll {
                view.layoutIfNeeded()
                view.setContentOffset(offset, animated: false)
            }
        }

        func textView(_ textView: UITextView, primaryActionFor textItem: UITextItem, defaultAction: UIAction) -> UIAction? {
            guard case .link(let url) = textItem.content, let id = MarkdownStyler.candidateID(from: url) else {
                return nil
            }
            return UIAction { [weak self] _ in
                self?.parent.onCandidate(id)
            }
        }

        func textView(_ textView: UITextView, menuConfigurationFor textItem: UITextItem, defaultMenu: UIMenu) -> UITextItem.MenuConfiguration? {
            // A candidate's tap opens the bar; its long press, its menu.
            if case .link(let url) = textItem.content, let id = MarkdownStyler.candidateID(from: url) {
                return parent.menu(id).map { UITextItem.MenuConfiguration(menu: $0) }
            }
            return UITextItem.MenuConfiguration(menu: defaultMenu)
        }

        /// Only Replace…: text leaves mdoc through Copy for AI and Share,
        /// never a plain Copy, Share, Look Up or Translate of the selection
        /// (ADR 0085's export invariant).
        func textView(_ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]) -> UIMenu? {
            guard range.length > 0 else { return UIMenu(children: []) }
            // A selection in the main text that runs into a folded table
            // holds hidden source: no Replace… (ADR 0096). Cells are their
            // own text views.
            if let main = textView as? CandidateTextView,
               main.tableViews.contains(where: { NSIntersectionRange($0.table.range, range).length > 0 }) {
                return UIMenu(children: [])
            }
            // A table cell's selection, in document offsets.
            let base = (textView as? CellTextView)?.location ?? 0
            let source = offsets.sourceRange(NSRange(location: base + range.location, length: range.length))
            let replace = UIAction(title: String(localized: "Hide name"), image: UIImage(systemName: "eye.slash")) { [weak self, weak textView] _ in
                textView?.selectedRange = NSRange(location: range.location, length: 0)
                self?.parent.onReplace(source)
            }
            return UIMenu(children: [replace])
        }
    }
}

/// The text view with the Candidates rotor and a Review next action. The
/// selection exists only for Replace…: copying, sharing and dragging it out
/// are off, keyboard shortcuts included, in table cells too.
final class CandidateTextView: RenderedTextView {
    /// Where the Candidates rotor goes: a candidate's text, in the main
    /// text or in a table cell, in document order.
    struct RotorItem {
        let view: UITextView
        let range: NSRange
        let order: Int
    }

    var rotorItems: [RotorItem] = []
    var onReviewNext: (() -> Void)?

    /// VoiceOver reads what the rendered view shows (ADR 0096): hidden
    /// markers and folded table source come back as word joiners, so
    /// offsets (the Candidates rotor, Replace…) still line up, a word split
    /// by markers stays one word, and the table is read from its grid. Copy
    /// is off here, so nothing else takes text from this.
    override func text(in range: UITextRange) -> String? {
        guard let text = super.text(in: range) else { return nil }
        return attributedText.spoken(text, at: offset(from: beginningOfDocument, to: range.start))
    }

    override var accessibilityValue: String? {
        get { attributedText.spoken(attributedText.string, at: 0) }
        set { super.accessibilityValue = newValue }
    }

    /// Selecting is the only standard action left.
    static let allowedActions: Set<Selector> = [
        #selector(UIResponderStandardEditActions.select(_:)),
        #selector(UIResponderStandardEditActions.selectAll(_:)),
    ]

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        Self.allowedActions.contains(action)
    }

    override func copy(_ sender: Any?) {}

    func installAccessibility() {
        let rotor = UIAccessibilityCustomRotor(name: String(localized: "Candidates")) { [weak self] predicate in
            self?.rotorItem(after: predicate)
        }
        accessibilityCustomRotors = [rotor]
        accessibilityCustomActions = [
            UIAccessibilityCustomAction(name: String(localized: "Review next proposal")) { [weak self] _ in
                self?.onReviewNext?()
                return true
            },
        ]
    }

    private func rotorItem(after predicate: UIAccessibilityCustomRotorSearchPredicate) -> UIAccessibilityCustomRotorItemResult? {
        // The current item, as an index into the document-ordered list.
        let current: Int? = predicate.currentItem.targetRange.flatMap { target in
            let element = predicate.currentItem.targetElement as? UITextView
            let location = element.map { $0.offset(from: $0.beginningOfDocument, to: target.start) } ?? -1
            return rotorItems.firstIndex { $0.view === element && $0.range.location == location }
        }
        let next: Int? = switch predicate.searchDirection {
        case .next: current.map { $0 + 1 } ?? 0
        case .previous: current.map { $0 - 1 } ?? rotorItems.count - 1
        @unknown default: nil
        }
        guard let next, rotorItems.indices.contains(next) else { return nil }
        let item = rotorItems[next]
        let view = item.view
        guard let start = view.position(from: view.beginningOfDocument, offset: item.range.location),
              let end = view.position(from: start, offset: item.range.length),
              let range = view.textRange(from: start, to: end) else { return nil }
        return UIAccessibilityCustomRotorItemResult(targetElement: view, targetRange: range)
    }
}
