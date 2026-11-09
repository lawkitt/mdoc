import MdocFFI
import SwiftUI
import UIKit

/// The document's Markdown source in a read-only, selectable TextKit 2
/// `UITextView` (ADR 0085), rendered as desktop draws it unless Show
/// Markdown is on (ADR 0096). Candidates are links: a tap opens the
/// candidate sheet, in a table cell too; VoiceOver reaches them through the
/// Candidates rotor, and the edit menu adds Replace… for a selection.
///
/// Two kinds of change reach it (ADR 0104). A content change (the text, the
/// candidates' states, the look) builds the styled document again, keyed by
/// the core's `text_revision`. A decoration change (the name under review
/// and the mentions its scope reaches) recolours those names in place, in
/// table cells too, so a card change never builds the document again.
struct MarkdownTextView: UIViewRepresentable {
    let snapshot: DocumentSnapshot
    let selected: CandidateID?
    /// The mentions the bar's scope reaches besides `selected`.
    var scoped: Set<CandidateID> = []
    /// The scroll the model asks for, run once after the next layout.
    var reveal: RevealRequest?
    /// Markers hidden and blocks drawn; false shows the source.
    var rendered = true
    /// Document font › Serif; the styler reads it, the key restyles on it.
    var serif = false
    let onCandidate: (CandidateID) -> Void
    let onReplace: (SourceRange) -> Void
    let onReviewNext: () -> Void
    /// A candidate's long press opens its menu.
    var onMenu: (CandidateID) -> Void = { _ in }
    /// A candidate's long-press menu (ADR 0098); nil leaves none.
    var menu: (CandidateID) -> UIMenu? = { _ in nil }
    /// Edit mode (ADR 0109), in live preview or with Show Markdown on: the
    /// text is editable and typing reaches the core through these. Nil in
    /// review.
    var edit: EditActions?
    /// Bumped to show the system find bar (Replace in edit mode).
    var findBar = 0

    /// The text is edited now.
    var editing: Bool { edit != nil }

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
        // The system find bar, in both modes; Replace only while editable
        // (ADR 0109).
        view.isFindInteractionEnabled = true
        context.coordinator.attach(to: view)
        return view
    }

    func updateUIView(_ view: CandidateTextView, context: Context) {
        context.coordinator.update(
            view, with: self, dynamicTypeSize: context.environment.dynamicTypeSize,
            colorScheme: context.environment.colorScheme)
    }

    /// What the styled document is built from: a change here builds it
    /// again. The text is compared by the core's revision when it has one.
    struct ContentKey: Equatable {
        var textRevision: UInt64?
        /// Compared only without a revision.
        var markdown: String
        var styling: MarkdownStyling
        var candidates: [Candidate]
        var rendered: Bool
        var editing: Bool
        var serif: Bool
        var dynamicTypeSize: DynamicTypeSize
        var colorScheme: ColorScheme

        init(_ view: MarkdownTextView, dynamicTypeSize: DynamicTypeSize, colorScheme: ColorScheme) {
            textRevision = view.snapshot.textRevision
            markdown = view.snapshot.markdown
            styling = view.snapshot.styling
            candidates = view.snapshot.candidates
            rendered = view.rendered
            editing = view.editing
            serif = view.serif
            self.dynamicTypeSize = dynamicTypeSize
            self.colorScheme = colorScheme
        }

        static func == (lhs: Self, rhs: Self) -> Bool {
            guard lhs.candidates == rhs.candidates, lhs.rendered == rhs.rendered, lhs.editing == rhs.editing,
                  lhs.serif == rhs.serif,
                  lhs.dynamicTypeSize == rhs.dynamicTypeSize, lhs.colorScheme == rhs.colorScheme else { return false }
            if let left = lhs.textRevision, let right = rhs.textRevision { return left == right }
            return lhs.textRevision == rhs.textRevision && lhs.markdown == rhs.markdown && lhs.styling == rhs.styling
        }

        /// Whether the text itself changed, not only how it is drawn.
        func textDiffers(from other: ContentKey?) -> Bool {
            guard let other else { return true }
            if let revision = textRevision, let previous = other.textRevision { return revision != previous }
            return markdown != other.markdown
        }
    }

    /// What is recoloured in place.
    struct DecorationKey: Equatable {
        var selected: CandidateID?
        var scoped: Set<CandidateID>
    }

    /// A stretch of one candidate in the main text or a table cell, with
    /// the decoration keys it has undecorated.
    struct DecoratedRun {
        enum Place: Hashable {
            case text
            case cell(table: Int, row: Int, column: Int)
        }

        let place: Place
        let range: NSRange
        let base: [NSAttributedString.Key: Any]
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownTextView
        /// What the view shows.
        private(set) var content: ContentKey?
        private(set) var decoration = DecorationKey(selected: nil, scoped: [])
        /// How many times the styled document was built: a test hook.
        private(set) var rebuilds = 0
        var offsets = TextOffsets("")
        /// The text revision `offsets` maps: typing moves the shown text on
        /// without a rebuild, so the map follows the text, not the key.
        private var offsetsRevision: UInt64?
        /// False until the first text is shown, so a new document starts at the top.
        var keepsScroll = false
        /// The content the view waits to show until mdoc is back on screen.
        private(set) var pending: ContentKey?
        /// The last reveal request taken, so each runs once.
        private var takenReveal: Int?
        /// The scroll owed once the next layout is done. `always` scrolls
        /// even when the target is in view already: a request does, a newly
        /// selected name only when it is out of view.
        private(set) var pendingReveal: (target: RevealRequest.Target, always: Bool)?
        /// Each candidate's stretches, as built, for recolouring in place.
        private var runs: [CandidateID: [DecoratedRun]] = [:]
        /// Inside `render`, whose own layout pass shows a scroll it will
        /// undo: no reveal runs there.
        private var rendering = false
        /// A reveal's scroll on its way: once it lands, the name is measured
        /// again and the scroll corrected once, if the layout the scroll
        /// brought moved it out of the band.
        private(set) var landing: (range: SourceRange, offset: CGFloat)?
        /// An animated reveal's scroll is running: it lands at its end.
        private var scrolling = false
        nonisolated(unsafe) private var foreground: NSObjectProtocol?
        /// Edit mode's typing path to the core (ADR 0109).
        let bridge = TypingBridge()
        /// A restyle that waits for marked (IME) text to be committed.
        var restyleAfterComposing = false
        /// The find bar request last shown.
        var shownFindBar = 0
        /// Edit mode's restyling and reveal (ADR 0109).
        let livePreview = LivePreview()
        /// The selection before the latest move, for snapping the caret.
        var lastSelection: NSRange?
        /// Setting a snapped selection: its own change is not snapped again.
        var snapping = false
        /// Edit mode's toolbar and formatting keyboard (ADR 0109).
        let toolbar = EditToolbarModel()
        /// The toolbar's caret state waits for the end of this turn of the
        /// run loop, so a burst of caret moves asks the core once.
        var toolbarPending = false
        /// The system keyboard's height with the toolbar, as it last showed.
        var keyboardHeight: CGFloat?
        nonisolated(unsafe) var keyboardObserver: NSObjectProtocol?

        init(parent: MarkdownTextView) {
            self.parent = parent
            // A request made before this view existed (another pane showed,
            // or the document was closed) is not this view's to run.
            takenReveal = parent.reveal?.id
        }

        deinit {
            if let foreground {
                NotificationCenter.default.removeObserver(foreground)
            }
            if let keyboardObserver {
                NotificationCenter.default.removeObserver(keyboardObserver)
            }
        }

        func attach(to view: CandidateTextView) {
            view.configureCell = { [weak self] cell in
                cell.delegate = self
                cell.allowsCopy = false
            }
            view.onWidth = { [weak self, weak view] width in
                guard let self, let view else { return }
                self.render(view, width: width)
            }
            view.onLayout = { [weak self, weak view] in
                guard let self, let view else { return }
                self.layoutDone(view)
            }
        }

        func update(_ view: CandidateTextView, with parent: MarkdownTextView, dynamicTypeSize: DynamicTypeSize, colorScheme: ColorScheme) {
            self.parent = parent
            view.onReviewNext = parent.onReviewNext
            connectEditing(view)
            let content = ContentKey(parent, dynamicTypeSize: dynamicTypeSize, colorScheme: colorScheme)
            let decoration = DecorationKey(selected: parent.selected, scoped: parent.scoped)
            if let reveal = parent.reveal, reveal.id != takenReveal {
                takenReveal = reveal.id
                pendingReveal = (reveal.target, true)
            }
            if content != self.content {
                // iOS draws a backgrounded app again in light and dark for
                // the app switcher. Building then made every table's cells
                // twice, and with Find names using the CPU it ran past the
                // 10 s iOS allows, so iOS stopped mdoc. The text catches up
                // on return, and so does a reveal.
                if view.window?.windowScene?.activationState == .background {
                    restyleOnReturn(content, view)
                    return
                }
                rebuild(content, view)
            } else {
                pending = nil
                syncOffsets(content)
                if decoration != self.decoration {
                    let previous = self.decoration.selected
                    redecorate(to: decoration, in: view)
                    afterSelection(from: previous, in: view)
                }
            }
            if pendingReveal != nil { view.setNeedsLayout() }
            view.setEditMode(parent.editing)
            if parent.findBar != shownFindBar {
                shownFindBar = parent.findBar
                view.findInteraction?.presentFindNavigator(showingReplace: view.isEditable)
            }
        }

        /// Typing changed the shown text, to the core's `revision`: a
        /// snapshot of that text needs no rebuild, and one of any other,
        /// even the text before the typing (an undo), does.
        func shownTextChanged(to revision: UInt64) {
            content?.textRevision = revision
        }

        /// Maps `content`'s text when `offsets` maps another revision: after
        /// typing, a snapshot of the typed text keeps the content key but
        /// not the text the map was built from.
        private func syncOffsets(_ content: ContentKey) {
            guard let revision = content.textRevision, revision != offsetsRevision else { return }
            offsets = TextOffsets(content.markdown)
            offsetsRevision = revision
        }

        /// Builds the styled document for `content`.
        func rebuild(_ content: ContentKey, _ view: CandidateTextView) {
            pending = nil
            let textChanged = content.textDiffers(from: self.content)
            if textChanged {
                offsets = TextOffsets(content.markdown)
                offsetsRevision = content.textRevision
            } else {
                syncOffsets(content)
            }
            let previous = decoration.selected
            self.content = content
            render(view, width: view.textWidth, keepsScroll: !textChanged || keepsScroll)
            keepsScroll = true
            afterSelection(from: previous, in: view)
        }

        /// A newly selected name scrolls into view once laid out; with none
        /// selected, the room a reveal added below the text goes.
        private func afterSelection(from previous: CandidateID?, in view: CandidateTextView) {
            if let selected = decoration.selected, selected != previous {
                // A correction owed to the name before is no longer wanted.
                landing = nil
                if pendingReveal?.always != true { pendingReveal = (.candidate(selected), false) }
                view.setNeedsLayout()
            } else if decoration.selected == nil {
                dropRevealRoom(in: view)
            }
        }

        /// The end of a layout pass: the pending reveal runs, once, when the
        /// view is on screen with a size to measure in.
        func layoutDone(_ view: CandidateTextView) {
            guard !rendering, pending == nil, view.window != nil, view.textWidth > 0, view.bounds.height > 0 else { return }
            guard let reveal = pendingReveal else {
                // The scroll has landed: measured again, corrected once.
                if let landing, !scrolling {
                    self.landing = nil
                    // Unless something else scrolled the text meanwhile.
                    guard abs(view.contentOffset.y - landing.offset) < view.bounds.height / 2 else { return }
                    self.reveal(range: landing.range, in: view, always: false, correcting: true)
                }
                return
            }
            pendingReveal = nil
            switch reveal.target {
            case .candidate(let id):
                self.reveal(id, in: view, always: reveal.always)
            case .range(let range):
                self.reveal(range: range, in: view)
                // The match selected, so the reader sees which words.
                let found = offsets.nsRange(range)
                if found.location + found.length <= view.attributedText.length { view.selectedRange = found }
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

        /// Scrolls a source range into the top third of the text left in
        /// view above the bar, briefly selecting it when it is no candidate
        /// (a Find match). It is measured where the laid-out text draws it
        /// (`laidOutRect`), not at TextKit 2's estimate. Once the scroll
        /// lands, it is measured again and, if still out of the band,
        /// corrected once (`correcting`).
        func reveal(range source: SourceRange, in view: CandidateTextView, always: Bool = true, correcting: Bool = false) {
            let range = offsets.nsRange(source)
            let cell = view.tableViews.lazy.flatMap { $0.cells.joined() }.first { cell in
                NSLocationInRange(range.location, NSRange(location: cell.location, length: cell.attributedText.length))
            }
            let rect: CGRect
            if let cell {
                // The grid's place is the text's, laid out through it.
                _ = view.laidOutRect(of: NSRange(location: cell.location, length: 0))
                guard let local = Self.firstRect(in: cell, location: range.location - cell.location, length: range.length) else { return }
                if let table = cell.superview as? MarkdownTableView {
                    table.scrollRectToVisible(cell.convert(local, to: table).insetBy(dx: -12, dy: 0), animated: false)
                }
                rect = cell.convert(local, to: view)
            } else {
                guard let found = view.laidOutRect(of: range) else { return }
                rect = found
            }
            let insets = view.adjustedContentInset
            let top = view.contentOffset.y + insets.top
            // The text left in view above the bar, compact or full: the room
            // added below the end is no bar.
            let band = (view.bounds.height - insets.top - (insets.bottom - revealRoom)) / 3
            guard always || rect.minY < top + 8 || rect.maxY > top + band else { return }
            let highest = -insets.top
            let target = max(rect.minY - insets.top - band / 3, highest)
            // Room below the end, so a candidate there can rise above the sheet.
            let lowest = view.laidOutContentHeight + insets.bottom - view.bounds.height
            if target > lowest {
                let more = target - lowest
                view.contentInset.bottom += more
                revealRoom += more
            }
            let animated = !UIAccessibility.isReduceMotionEnabled && abs(target - view.contentOffset.y) > 0.5
            view.setContentOffset(CGPoint(x: view.contentOffset.x, y: target), animated: animated)
            guard !correcting else { return }
            landing = (source, target)
            scrolling = animated
            // Without an animation it has landed: the next pass measures.
            if !animated { view.setNeedsLayout() }
        }

        func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) {
            guard scrolling else { return }
            scrolling = false
            scrollView.setNeedsLayout()
        }

        /// The reader took the text: no correction follows.
        func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
            landing = nil
            scrolling = false
        }

        /// The room `reveal` added below the text, taken away once no
        /// candidate is under review.
        var revealRoom: CGFloat = 0

        func dropRevealRoom(in view: CandidateTextView) {
            // Review ended: no correction brings the room back.
            landing = nil
            scrolling = false
            guard revealRoom > 0 else { return }
            view.contentInset.bottom -= revealRoom
            revealRoom = 0
            let lowest = Self.lowestOffset(in: view)
            if view.contentOffset.y > lowest {
                view.setContentOffset(CGPoint(x: view.contentOffset.x, y: lowest), animated: !UIAccessibility.isReduceMotionEnabled)
            }
        }

        /// How far down the text can rest, its top when all of it fits.
        private static func lowestOffset(in view: UIScrollView) -> CGFloat {
            let insets = view.adjustedContentInset
            return max(-insets.top, view.contentSize.height + insets.bottom - view.bounds.height)
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

        func restyleOnReturn(_ content: ContentKey, _ view: CandidateTextView) {
            pending = content
            guard foreground == nil else { return }
            foreground = NotificationCenter.default.addObserver(
                forName: UIScene.willEnterForegroundNotification, object: nil, queue: .main
            ) { [weak self, weak view] _ in
                MainActor.assumeIsolated {
                    guard let self, let view, let content = self.pending, content != self.content else { return }
                    self.rebuild(content, view)
                    if self.pendingReveal != nil { view.setNeedsLayout() }
                }
            }
        }

        /// Styles the snapshot for a text column `width` wide (tables need
        /// one), decorates the name under review and its scope, and points
        /// the Candidates rotor at the text and the cells.
        func render(_ view: CandidateTextView, width: CGFloat, keepsScroll: Bool = true) {
            rendering = true
            defer { rendering = false }
            rebuilds += 1
            let snapshot = parent.snapshot
            let offset = view.contentOffset
            // Built undecorated, its candidates' stretches noted, then
            // decorated as a selection change recolours them, so the two
            // never differ; the view gets the text decorated already.
            var styler = MarkdownStyler(
                traits: view.traitCollection, selected: nil, scoped: [], rendered: parent.rendered,
                width: width > 0 ? width : nil)
            styler.editing = parent.editing
            var document = styler.document(for: snapshot)
            if parent.editing {
                // The view's string is the source while editing: restyled
                // in place, never replaced (ADR 0109). A snapshot older than
                // the typing waits for the pause's, and the shown text keeps
                // its own revision, so a later snapshot of the older text (an
                // undo) is not taken for what shows.
                if let latest = parent.edit?.revision(), snapshot.textRevision != latest,
                   view.textStorage.string != snapshot.markdown {
                    content?.textRevision = latest
                    // What changed may be the look (Show Markdown, the text
                    // size): the view's own text restyles with it.
                    restyleWhole(view)
                    return
                }
                livePreview.reset(candidates: snapshot.candidates)
                showEditing(document.text, tables: parent.rendered ? document.tables : [], in: view)
                livePreview.selectionChanged(in: view, look: look(view), force: true)
                return
            }
            let text = NSMutableAttributedString(attributedString: document.text)
            var cells: [DecoratedRun.Place: NSMutableAttributedString] = [:]
            for (table, rendered) in document.tables.enumerated() {
                for (row, rowCells) in rendered.rows.enumerated() {
                    for (column, cell) in rowCells.enumerated() {
                        cells[.cell(table: table, row: row, column: column)] = NSMutableAttributedString(attributedString: cell.text)
                    }
                }
            }
            func string(_ place: DecoratedRun.Place) -> NSMutableAttributedString? {
                place == .text ? text : cells[place]
            }
            collectRuns(text: text, cells: cells)
            decoration = DecorationKey(selected: nil, scoped: [])
            decorate(DecorationKey(selected: parent.selected, scoped: parent.scoped), dark: view.traitCollection.userInterfaceStyle == .dark, in: string)
            document.text = text
            for (place, cell) in cells {
                if case .cell(let table, let row, let column) = place { document.tables[table].rows[row][column].text = cell }
            }
            view.show(document, width: width)
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
                // Replacing can shorten the text below the old place; left
                // there, a text that now fits is stuck, as it doesn't pan.
                view.setContentOffset(CGPoint(x: offset.x, y: min(offset.y, Self.lowestOffset(in: view))), animated: false)
            }
        }

        /// Where each candidate's link runs are, in the text and in each
        /// table cell (a folded table line has none), with their colours.
        private func collectRuns(text: NSAttributedString, cells: [DecoratedRun.Place: NSMutableAttributedString]) {
            runs = [:]
            let keys = MarkdownStyler.decorationKeys
            func collect(_ string: NSAttributedString, _ place: DecoratedRun.Place) {
                string.enumerateAttribute(.link, in: NSRange(location: 0, length: string.length)) { value, range, _ in
                    guard let url = value as? URL, let id = MarkdownStyler.candidateID(from: url) else { return }
                    string.enumerateAttributes(in: range) { attributes, part, _ in
                        let base = attributes.filter { keys.contains($0.key) }
                        runs[id, default: []].append(DecoratedRun(place: place, range: part, base: base))
                    }
                }
            }
            collect(text, .text)
            for (place, cell) in cells { collect(cell, place) }
        }

        /// Recolours the names whose decoration changed, in place: the one
        /// under review and the mentions its scope reaches, the text kept
        /// as it is laid out.
        func redecorate(to new: DecorationKey, in view: CandidateTextView) {
            var edited: [DecoratedRun.Place: NSTextStorage] = [:]
            decorate(new, dark: view.traitCollection.userInterfaceStyle == .dark) { place in
                if let storage = edited[place] { return storage }
                let textView: UITextView? = switch place {
                case .text: view
                case .cell(let table, let row, let column):
                    view.tableViews.indices.contains(table) && view.tableViews[table].cells.indices.contains(row)
                        && view.tableViews[table].cells[row].indices.contains(column)
                        ? view.tableViews[table].cells[row][column] : nil
                }
                guard let storage = textView?.textStorage else { return nil }
                storage.beginEditing()
                edited[place] = storage
                return storage
            }
            for storage in edited.values { storage.endEditing() }
        }

        /// Sets the decoration keys of each candidate whose decoration
        /// changes from the current one to `new`, in the strings `string`
        /// gives for each place.
        private func decorate(
            _ new: DecorationKey, dark: Bool, in string: (DecoratedRun.Place) -> NSMutableAttributedString?
        ) {
            let old = decoration
            decoration = new
            let changed = Set([old.selected, new.selected].compactMap { $0 }).union(old.scoped.symmetricDifference(new.scoped))
            for id in changed {
                guard let candidate = parent.snapshot.candidate(id), let runs = runs[id] else { continue }
                let added = MarkdownStyler.decoration(
                    of: candidate, selected: id == new.selected, scoped: new.scoped.contains(id), dark: dark)
                for run in runs {
                    guard let text = string(run.place), NSMaxRange(run.range) <= text.length else { continue }
                    for key in MarkdownStyler.decorationKeys {
                        if let value = added[key] ?? run.base[key] {
                            text.addAttribute(key, value: value, range: run.range)
                        } else {
                            text.removeAttribute(key, range: run.range)
                        }
                    }
                }
            }
        }

        func textView(_ textView: UITextView, primaryActionFor textItem: UITextItem, defaultAction: UIAction) -> UIAction? {
            guard !textView.isEditable, case .link(let url) = textItem.content, let id = MarkdownStyler.candidateID(from: url) else {
                return nil
            }
            return UIAction { [weak self] _ in
                self?.parent.onCandidate(id)
            }
        }

        func textView(_ textView: UITextView, menuConfigurationFor textItem: UITextItem, defaultMenu: UIMenu) -> UITextItem.MenuConfiguration? {
            // A candidate's tap opens the bar; its long press, its menu.
            if !textView.isEditable, case .link(let url) = textItem.content, let id = MarkdownStyler.candidateID(from: url) {
                parent.onMenu(id)
                return parent.menu(id).map { UITextItem.MenuConfiguration(menu: $0) }
            }
            return UITextItem.MenuConfiguration(menu: defaultMenu)
        }

        /// Only Replace…: text leaves mdoc through Copy for AI and Share,
        /// never a plain Copy, Share, Look Up or Translate of the selection
        /// (ADR 0085's export invariant).
        func textView(_ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]) -> UIMenu? {
            // Edit mode: Cut, Copy, Paste and Select All only (ADR 0109).
            if textView.isEditable { return CandidateTextView.editMenu(from: suggestedActions) }
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
    /// Edit mode's actions (ADR 0109); nil in review.
    var editActions: EditActions?
    /// After a change made through the view's own API (Paste, Cut, a find
    /// replacement): the typing path syncs it.
    var onEdited: (() -> Void)?
    /// Edit mode's undo: the core's, never UIKit's own (`CoreUndoManager`).
    let coreUndo = CoreUndoManager()
    /// Edit mode's watch on the text storage (`watchStorage`).
    var storageObserver: NSObjectProtocol?
    /// Each edit of the storage's characters, as it is processed.
    var onStorageEdit: ((NSTextStorage) -> Void)?
    /// Edit mode's hardware shortcuts (⌘B, ⌘I, Tab, ⇧Tab) and ⌘K.
    var onCommand: ((EditCommand) -> Void)?
    var onLinkKey: (() -> Void)?

    /// Edit mode's shortcuts, listed when ⌘ is held (ADR 0109). ⌘E is the
    /// Edit and Done button's, and ⌘Z and ⇧⌘Z the system's Undo and Redo,
    /// which reach the core through `CoreUndoManager`.
    @MainActor static let editKeyCommands: [UIKeyCommand] = {
        func key(_ title: String, _ input: String, _ flags: UIKeyModifierFlags, _ action: Selector) -> UIKeyCommand {
            let command = UIKeyCommand(
                title: title, action: action, input: input, modifierFlags: flags, discoverabilityTitle: title)
            command.wantsPriorityOverSystemBehavior = true
            return command
        }
        return [
            key(String(localized: "Bold"), "b", .command, #selector(boldKey)),
            key(String(localized: "Italic"), "i", .command, #selector(italicKey)),
            key(String(localized: "Link…"), "k", .command, #selector(linkKey)),
            key(String(localized: "Indent"), "\t", [], #selector(indentKey)),
            key(String(localized: "Outdent"), "\t", .shift, #selector(outdentKey)),
        ]
    }()

    override var keyCommands: [UIKeyCommand]? {
        guard isEditMode, isFirstResponder else { return super.keyCommands }
        return (super.keyCommands ?? []) + Self.editKeyCommands
    }

    @objc func boldKey() { onCommand?(.format(.bold)) }
    @objc func italicKey() { onCommand?(.format(.italic)) }
    @objc func linkKey() { onLinkKey?() }
    @objc func indentKey() { onCommand?(.indent) }
    @objc func outdentKey() { onCommand?(.outdent) }

    /// Editable, with the core behind it.
    var isEditMode: Bool { isEditable && editActions != nil }
    /// Live preview hides markers (Show Markdown off), in edit mode too.
    var hidesMarkers = true

    override var undoManager: UndoManager? {
        isEditMode ? coreUndo : super.undoManager
    }

    /// VoiceOver reads what the rendered view shows (ADR 0096): hidden
    /// markers and folded table source come back as word joiners, so
    /// offsets (the Candidates rotor, Replace…) still line up, a word split
    /// by markers stays one word, and the table is read from its grid. Copy
    /// is off here, so nothing else takes text from this.
    override func text(in range: UITextRange) -> String? {
        guard let text = super.text(in: range) else { return nil }
        if isEditMode {
            // Show Markdown hides nothing. Live preview's hidden markers are
            // not spoken (ADR 0109); without VoiceOver the keyboard reads
            // the source here, for autocorrect and spelling.
            guard hidesMarkers, UIAccessibility.isVoiceOverRunning else { return text }
            return textStorage.spoken(text, at: offset(from: beginningOfDocument, to: range.start))
        }
        return attributedText.spoken(text, at: offset(from: beginningOfDocument, to: range.start))
    }

    /// The spoken text, built once per content: VoiceOver and UI tests ask
    /// for the value again and again, and a long document took seconds of
    /// CPU per minute rebuilding it. Only new content changes which units
    /// are hidden; recolouring a name keeps it.
    private var spokenValue: String?
    /// How many times the spoken text was built, for tests.
    private(set) var spokenBuilds = 0

    // Not an `attributedText` override: VoiceOver reads that property from
    // a background queue, where a main-actor override traps (crash on the
    // 12 Pro, 2026-11-08). New content always comes through `show`.
    override func show(_ document: RenderedDocument, width: CGFloat) {
        spokenValue = nil
        super.show(document, width: width)
    }

    /// Edit mode changed the text or what it hides: the spoken text is built
    /// again when next asked.
    func invalidateSpoken() {
        spokenValue = nil
    }

    override var accessibilityValue: String? {
        get {
            if isEditMode && !hidesMarkers { return super.accessibilityValue }
            if let spokenValue { return spokenValue }
            let value = textStorage.spoken(textStorage.string, at: 0)
            spokenValue = value
            spokenBuilds += 1
            return value
        }
        set { super.accessibilityValue = newValue }
    }

    /// Selecting is the only standard action left.
    static let allowedActions: Set<Selector> = [
        #selector(UIResponderStandardEditActions.select(_:)),
        #selector(UIResponderStandardEditActions.selectAll(_:)),
    ]

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if isEditMode { return canPerformEditAction(action, withSender: sender) }
        if Self.findActions.contains(action) { return super.canPerformAction(action, withSender: sender) }
        return Self.allowedActions.contains(action)
    }

    override func copy(_ sender: Any?) {
        if isEditMode { copySelection() }
    }

    override func cut(_ sender: Any?) {
        if isEditMode { cutSelection() }
    }

    override func paste(_ sender: Any?) {
        if isEditMode { pasteClipboard() }
    }

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
