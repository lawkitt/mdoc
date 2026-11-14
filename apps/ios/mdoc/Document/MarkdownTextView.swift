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
    /// Hide name's callout (ADR 0114): what a selection hides and Hide
    /// as's categories, from the core; nil leaves plain Hide name.
    var hideTarget: (SourceRange) -> WordsTarget? = { _ in nil }
    /// Hide "…" (nil: the core's guess) or a Hide as › category.
    var onHideAs: (SourceRange, Category?) -> Void = { _, _ in }
    /// Fix the words (ADR 0114): handles on a name's words in the draft
    /// `snapshot` shows; nil otherwise.
    var wordsFix: WordsFixRequest?
    var wordsFixActions: WordsFixActions?
    let onReviewNext: () -> Void
    /// A candidate's long press opens its menu.
    var onMenu: (CandidateID) -> Void = { _ in }
    /// A candidate's long-press menu and its header (ADR 0098); nil leaves
    /// none.
    var menu: (CandidateID) -> UITextItem.MenuConfiguration? = { _ in nil }
    /// Edit mode (ADR 0109), in live preview or with Show Markdown on: the
    /// text is editable and typing reaches the core through these. Nil in
    /// review.
    var edit: EditActions?
    /// Bumped to show the system find bar (Replace in edit mode).
    var findBar = 0
    /// False while the Original or the reply covers the text on iPhone: the
    /// view is kept, and the keyboard goes until the text shows again
    /// (ADR 0110).
    var shown = true

    /// The text is edited now.
    var editing: Bool { edit != nil }

    /// The name whose words are fixed (ADR 0115): the draft shows as plain
    /// text, no candidate a link and this name undecorated.
    var fixing: CandidateID? { editing ? nil : wordsFix?.candidate }

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
            colorScheme: context.environment.colorScheme, inViewUpdate: true)
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
        var fixing: CandidateID?
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
            fixing = view.fixing
            serif = view.serif
            self.dynamicTypeSize = dynamicTypeSize
            self.colorScheme = colorScheme
        }

        static func == (lhs: Self, rhs: Self) -> Bool {
            guard lhs.candidates == rhs.candidates, lhs.rendered == rhs.rendered, lhs.editing == rhs.editing,
                  lhs.fixing == rhs.fixing, lhs.serif == rhs.serif,
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

    /// A reveal the model asked for, with the name then under review.
    struct RevealOrigin {
        let selected: CandidateID?
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
        /// The scroll owed once the next layout is done: the model's
        /// request, or a newly selected name.
        var pendingReveal: RevealRequest.Target?
        /// The pending reveal is the model's request, and the name that
        /// was under review when it was made: a name selected later is a
        /// newer wish, and takes its place.
        var pendingRequest: RevealOrigin?
        /// Where each candidate is in the shown text (UTF-16), as last
        /// built: a rebuild finds the same place in the new text by it.
        private var shownRanges: [CandidateID: NSRange] = [:]
        /// The room the text had at the last layout pass: the view's
        /// height and the bars inside it.
        var shownRoom: (height: CGFloat, insets: UIEdgeInsets)?
        /// Each candidate's stretches, as built, for recolouring in place.
        private(set) var runs: [CandidateID: [DecoratedRun]] = [:]
        /// Inside `render`, whose own layout pass shows a scroll it will
        /// undo: no reveal runs there.
        private(set) var rendering = false
        /// A reveal's scroll on its way: once it lands, the name is measured
        /// again and the scroll corrected once, if the layout the scroll
        /// brought moved it out of view.
        var landing: (range: SourceRange, offset: CGFloat)?
        /// An animated reveal's scroll is running: it lands at its end.
        var scrolling = false
        /// A reveal waits for the end of this turn of the run loop.
        var revealQueued = false
        nonisolated(unsafe) private var foreground: NSObjectProtocol?
        /// Edit mode's typing path to the core (ADR 0109).
        let bridge = TypingBridge()
        /// A restyle that waits for marked (IME) text to be committed.
        var restyleAfterComposing = false
        /// The find bar request last shown.
        var shownFindBar = 0
        /// Fix the words as the view shows it (ADR 0114): the request
        /// taken, the words the handles last held, the text view whose
        /// selection holds them, the table held still around it, and a
        /// selection set here (not by the reader). The presenter shows
        /// mdoc's own selection menus.
        var shownFix: WordsFixRequest?
        var fixWords: SourceRange?
        weak var fixHolder: UITextView?
        weak var fixTable: MarkdownTableView?
        var fixSettling = false
        var menuPresenter: SelectionMenuPresenter?
        /// A `followResponder` waits for SwiftUI's update to end.
        private var responderWaits = false
        /// Whether the text last showed in edit mode; nil before it first
        /// showed. A change keeps the place and the selection (ADR 0110).
        var shownEditing: Bool?
        /// The keyboard went when the Original covered the text in edit
        /// mode: it comes back with the text.
        var focusOnReturn = false
        /// Where the reader was when the mode last changed: on iPad
        /// Identities come or go with it and the text's width changes a
        /// moment later, and that layout and the next change return here
        /// too, until the reader scrolls, types or a reveal moves the text.
        var modeAnchor: CandidateTextView.ScrollAnchor?
        /// A word selected in a cell as the pencil was tapped, noted before
        /// the edit-mode render restyles the cell (which drops it) and
        /// selected in the text once the grids switch (`connectTables`).
        var enteringCellSelection: NSRange?
        /// The width the text was last styled for.
        var renderedWidth: CGFloat?
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
        nonisolated(unsafe) var keyboardMoved: NSObjectProtocol?
        /// The room added below the text in edit mode so that no line
        /// rests under the floating toolbar (ADR 0110).
        var toolbarRoom: CGFloat = 0

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
            for observer in [keyboardObserver, keyboardMoved].compactMap({ $0 }) {
                NotificationCenter.default.removeObserver(observer)
            }
        }

        func attach(to view: CandidateTextView) {
            view.configureCell = { [weak self] cell in
                cell.delegate = self
                cell.allowsCopy = false
                cell.tintColor = Self.selectionTint
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

        /// `inViewUpdate` is true from SwiftUI's own update of the view:
        /// what moves the first responder then waits for the update to
        /// end (`followResponder`).
        func update(
            _ view: CandidateTextView, with parent: MarkdownTextView, dynamicTypeSize: DynamicTypeSize,
            colorScheme: ColorScheme, inViewUpdate: Bool = false
        ) {
            self.parent = parent
            view.onReviewNext = parent.onReviewNext
            connectEditing(view)
            let content = ContentKey(parent, dynamicTypeSize: dynamicTypeSize, colorScheme: colorScheme)
            let decoration = DecorationKey(selected: parent.selected, scoped: parent.scoped)
            if let reveal = parent.reveal, reveal.id != takenReveal {
                takenReveal = reveal.id
                pendingReveal = reveal.target
                pendingRequest = RevealOrigin(selected: parent.selected)
            }
            if parent.editing, shownEditing == false {
                // Read only: nothing here moves the first responder.
                enteringCellSelection = view.cellSelection
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
            // Outside edit mode a selection is tinted, not the ink's grey
            // (ADR 0114); the caret keeps the ink while editing.
            if fixHolder !== view { view.tintColor = parent.editing ? nil : Self.selectionTint }
            if parent.wordsFix != shownFix {
                // After this update: the draft's text is shown by then.
                DispatchQueue.main.async { [weak self, weak view] in
                    guard let self, let view else { return }
                    self.followWordsFix(view)
                }
            }
            if inViewUpdate {
                // Noted now: by the time the update has ended the Original
                // may cover the text, and the keyboard is gone with it.
                if parent.editing, !parent.shown, view.isFirstResponder { focusOnReturn = true }
                guard !responderWaits else { return }
                responderWaits = true
                DispatchQueue.main.async { [weak self, weak view] in
                    guard let self else { return }
                    self.responderWaits = false
                    if let view { self.followResponder(view) }
                }
            } else {
                followResponder(view)
            }
        }

        /// Edit mode, the keyboard and the find bar follow the model.
        /// Never inside SwiftUI's update of the view: making the text
        /// editable or not resigns the first responder, UIKit then asks
        /// each view above whether it can take over, and a hosting view
        /// answers from its view graph, which is in the middle of that
        /// update. On iOS 27 that never returned and iOS stopped mdoc
        /// (build 882, twice on 2026-11-12). The same for the tables' cells,
        /// which a cell holding a selection resigns (rc.9, build 899).
        ///
        /// The switch keeps the place (`holdsPlace`): the render put the line
        /// at the top of the view back already (ADR 0110).
        func followResponder(_ view: CandidateTextView) {
            view.holdsPlace = true
            connectTables(view)
            view.setEditMode(parent.editing, shown: parent.shown)
            view.holdsPlace = false
            followShown(view)
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
                    self.followWordsFix(view)
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
            // Edit and Done keep the line at the top of the view and the
            // selection where they were (ADR 0110).
            let switching = shownEditing.map { $0 != parent.editing } ?? false
            shownEditing = parent.editing
            let widthChanged = renderedWidth.map { abs($0 - width) > 0.5 } ?? false
            renderedWidth = width
            // Kept from the last change while the reader has not moved:
            // a wider text that fits whole sits at the top, and Done still
            // goes back to where they were.
            if switching { modeAnchor = modeAnchor ?? view.scrollAnchor() }
            let anchor = switching || widthChanged ? modeAnchor : nil
            // A decision changes the text: a name under review that shows
            // whole keeps its place on screen, and otherwise the line at
            // the top of the view does (ADR 0098, amended 2026-11-11).
            let held = keepsScroll && anchor == nil && !parent.editing ? heldPlace(in: view) : nil
            let selection = view.selectedRange
            // A selection survives a change of mode or width (Identities
            // coming back on iPad) when the text is the same.
            let shownText = switching || selection.length > 0 ? view.textStorage.string : nil
            // Built undecorated, its candidates' stretches noted, then
            // decorated as a selection change recolours them, so the two
            // never differ; the view gets the text decorated already.
            var styler = MarkdownStyler(
                traits: view.traitCollection, selected: nil, scoped: [], rendered: parent.rendered,
                width: width > 0 ? width : nil)
            styler.editing = parent.editing
            // Fix the words (ADR 0115): no link a press could lift, and the
            // name's own look gone under its selection.
            styler.links = parent.fixing == nil
            var styled = snapshot
            if let fixing = parent.fixing { styled.candidates.removeAll { $0.id == fixing } }
            var document = styler.document(for: styled)
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
                if let anchor { view.scroll(to: anchor) }
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
            // A review scrolls to names all over the text: laid out whole,
            // no place in it is an estimate that a later pass corrects.
            if keepsScroll { view.layOutAll() }
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
            if shownFix != nil {
                // Built again while a name's words are fixed: the handles
                // come back once this update is over (ADR 0115).
                DispatchQueue.main.async { [weak self, weak view] in
                    guard let self, let view else { return }
                    self.restoreWordsFix(view)
                }
            }
            let ranges = shownRanges
            shownRanges = Dictionary(
                snapshot.candidates.map { ($0.id, offsets.nsRange($0.range)) }, uniquingKeysWith: { first, _ in first })
            if let shownText, shownText == view.textStorage.string, NSMaxRange(selection) <= view.textStorage.length {
                // The same text, so the same selection.
                view.selectedRange = selection
            }
            if let anchor {
                view.scroll(to: anchor)
            } else if keepsScroll {
                view.layoutIfNeeded()
                // Replacing can shorten the text below the old place; left
                // there, a text that now fits is stuck, as it doesn't pan.
                let insets = view.adjustedContentInset
                let lowest = max(-insets.top, view.laidOutContentHeight + insets.bottom - view.bounds.height)
                view.setContentOffset(CGPoint(x: offset.x, y: min(max(offset.y, -insets.top), lowest)), animated: false)
                switch held {
                case .name(let id, let above):
                    guard let candidate = snapshot.candidate(id) else { break }
                    let range = offsets.nsRange(candidate.range)
                    hold(above, in: view) { self.place(of: range, in: view, onlyShown: $0)?.rect }
                    // A name that grew may run past its table's edge.
                    if let place = place(of: range, in: view, onlyShown: true), let cell = place.cell,
                       let table = cell.superview as? MarkdownTableView {
                        table.reveal(cell.convert(place.local, to: table))
                    }
                case .line(let line):
                    let location = Self.moved(line.location, from: ranges, to: shownRanges)
                    guard location <= view.textStorage.length else { break }
                    hold(line.delta + insets.top, in: view) {
                        view.laidOutRect(of: NSRange(location: location, length: 0), onlyShown: $0)
                    }
                case nil:
                    break
                }
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
            // The recolouring is an edit: what it turned back into
            // estimates is laid out again, so nothing moves.
            if edited[.text] != nil { view.layOutAll() }
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

        /// What a press on a text item gets: a candidate's own tap and
        /// menu in review, and nothing from the system for a candidate's
        /// link in any other state (ADR 0115), so its `mdoc-candidate` URL
        /// never shows with Open, Copy and Share (ADR 0085's export
        /// invariant). Any other item keeps UIKit's own.
        enum TextItemUse: Equatable {
            case candidate(CandidateID)
            case nothing
            case system
        }

        static func use(of content: UITextItem.Content, editable: Bool, fixing: Bool) -> TextItemUse {
            guard case .link(let url) = content, url.scheme == MarkdownStyler.candidateScheme else { return .system }
            guard !editable, !fixing, let id = MarkdownStyler.candidateID(from: url) else { return .nothing }
            return .candidate(id)
        }

        func textView(_ textView: UITextView, primaryActionFor textItem: UITextItem, defaultAction: UIAction) -> UIAction? {
            // While a name's words are fixed, a tap ends the selection.
            guard case .candidate(let id) = Self.use(
                of: textItem.content, editable: textView.isEditable, fixing: parent.wordsFix != nil) else { return nil }
            return UIAction { [weak self] _ in
                self?.parent.onCandidate(id)
            }
        }

        func textView(_ textView: UITextView, menuConfigurationFor textItem: UITextItem, defaultMenu: UIMenu) -> UITextItem.MenuConfiguration? {
            // A candidate's tap opens the bar; its long press, its menu.
            switch Self.use(of: textItem.content, editable: textView.isEditable, fixing: parent.wordsFix != nil) {
            case .candidate(let id):
                parent.onMenu(id)
                return parent.menu(id)
            case .nothing:
                return nil
            case .system:
                return UITextItem.MenuConfiguration(menu: defaultMenu)
            }
        }

        /// Under the menu's own header (not the system's lifted text) UIKit
        /// leaves the pressed name selected when the menu closes: no
        /// selection stays behind.
        func textView(_ textView: UITextView, textItemMenuWillEndFor textItem: UITextItem, animator: any UIContextMenuInteractionAnimating) {
            guard !textView.isEditable else { return }
            animator.addCompletion { [weak self, weak textView] in
                // Fix the words, chosen in this menu, holds the name's
                // words selected (ADR 0114).
                guard let textView, !textView.isEditable, self?.parent.wordsFix == nil else { return }
                textView.selectedTextRange = nil
            }
        }

        /// Only Hide name, and nothing while a name's words are fixed (its
        /// bar holds Hide «…», ADR 0115): text leaves mdoc through Copy for
        /// AI and Share, never a plain Copy, Share, Look Up or Translate of
        /// the selection (ADR 0085's export invariant).
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
            if parent.wordsFix != nil { return UIMenu(children: []) }
            // A table cell's selection, in document offsets.
            let base = (textView as? CellTextView)?.location ?? 0
            let source = offsets.sourceRange(NSRange(location: base + range.location, length: range.length))
            return hideMenu(for: source, in: textView, at: range)
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
    /// Paste with the caret in a table cell (ADR 0109): true when made
    /// there, as the cell's own bytes.
    var onCellPaste: ((String) -> Bool)?

    /// The edit menu for a selection made in a cell by a double tap or a
    /// long press on its grid: its actions are the text view's.
    lazy var cellMenu: UIEditMenuInteraction = {
        let menu = UIEditMenuInteraction(delegate: self)
        addInteraction(menu)
        return menu
    }()

    /// Drags the handles of a selection in a cell (`CellHandleDragger`).
    lazy var cellHandles = CellHandleDragger(view: self)

    /// What `cellMenu` is shown for: the selection's lines, which the
    /// menu keeps clear of.
    var cellMenuTarget = CGRect.null

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

    /// In a table cell Tab and ⇧Tab move between cells (ADR 0109).
    @MainActor static let cellKeyCommands: [UIKeyCommand] = {
        func key(_ title: String, _ flags: UIKeyModifierFlags, _ action: Selector) -> UIKeyCommand {
            let command = UIKeyCommand(
                title: title, action: action, input: "\t", modifierFlags: flags, discoverabilityTitle: title)
            command.wantsPriorityOverSystemBehavior = true
            return command
        }
        return [
            key(String(localized: "Next cell"), [], #selector(indentKey)),
            key(String(localized: "Previous cell"), .shift, #selector(outdentKey)),
        ]
    }()

    override var keyCommands: [UIKeyCommand]? {
        guard isEditMode, isFirstResponder else { return super.keyCommands }
        let edit = caretCell == nil ? Self.editKeyCommands : Array(Self.editKeyCommands.prefix(3)) + Self.cellKeyCommands
        return (super.keyCommands ?? []) + edit
    }

    /// The move a hardware key makes in a table cell: Tab, ⇧Tab and the
    /// arrows (no other modifier); nil for any other key or outside a cell.
    private func cellMove(_ presses: Set<UIPress>) -> EditorTableMove? {
        guard editsCells, presses.count == 1, let key = presses.first?.key,
              key.modifierFlags.subtracting([.shift, .numericPad]).isEmpty, caretCell != nil else { return nil }
        let shift = key.modifierFlags.contains(.shift)
        switch key.keyCode {
        case .keyboardTab: return shift ? .previous : .next
        case .keyboardLeftArrow where !shift: return .left
        case .keyboardRightArrow where !shift: return .right
        case .keyboardUpArrow where !shift: return .up
        case .keyboardDownArrow where !shift: return .down
        default: return nil
        }
    }

    /// ⇧Return in a table cell: `<br>`, a line break in the cell.
    private func isCellLineBreak(_ presses: Set<UIPress>) -> Bool {
        guard editsCells, presses.count == 1, let key = presses.first?.key, caretCell != nil else { return false }
        return key.keyCode == .keyboardReturnOrEnter
            && key.modifierFlags.subtracting([.numericPad]) == .shift
    }

    /// Tab and ⇧Tab from a hardware keyboard in edit mode: indent and
    /// outdent, or in a table cell the next and previous cell, as the
    /// arrows move between cells there. The text view would type a tab, and
    /// the key command above does not reach it before the text input does.
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        if let move = cellMove(presses) {
            onCommand?(.moveInTable(move))
            return
        }
        if isCellLineBreak(presses) {
            onCommand?(.lineBreak)
            return
        }
        if isEditMode, presses.count == 1, let key = presses.first?.key, key.keyCode == .keyboardTab,
           key.modifierFlags.subtracting([.shift, .numericPad]).isEmpty {
            if key.modifierFlags.contains(.shift) { outdentKey() } else { indentKey() }
            return
        }
        super.pressesBegan(presses, with: event)
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        if cellMove(presses) != nil || isCellLineBreak(presses) { return }
        if isEditMode, presses.count == 1, presses.first?.key?.keyCode == .keyboardTab { return }
        super.pressesEnded(presses, with: event)
    }

    // MARK: The caret in a table cell (ADR 0109)

    /// While edit mode switches on or off, nothing UIKit does scrolls the
    /// text: the pencil and Done keep the line at the top of the view
    /// (ADR 0110). A cell's selection ending hands this view the focus, and
    /// UIKit, told of the keyboard as the mode changed, scrolled to its
    /// caret, which in review is wherever UIKit left it: the pencil at row
    /// 17 of a large contract's table showed the contract's end (rc.9).
    var holdsPlace = false

    /// With the caret in a cell, UIKit scrolls to its selection where the
    /// table's hidden source lies (the end of the grid), so a tap on a
    /// cell of a long table threw it out of view as the keyboard came up.
    /// The caret is drawn in the cell: that is what is kept in view, and
    /// a cell in view already moves nothing.
    override func scrollRectToVisible(_ rect: CGRect, animated: Bool) {
        // A handle dragged in a cell scrolls the text itself, under the
        // finger: the selection's end may be the one left behind.
        if cellHandles.isDragging || holdsPlace { return }
        guard let end = selectedTextRange?.end, let cell = cellCaretRect(for: end) else {
            super.scrollRectToVisible(rect, animated: animated)
            return
        }
        let line = CGRect(x: bounds.minX, y: cell.minY - Space.xs, width: 1, height: cell.height + 2 * Space.xs)
        let visible = bounds.inset(by: adjustedContentInset)
        if line.minY >= visible.minY, line.maxY <= visible.maxY { return }
        super.scrollRectToVisible(line, animated: animated)
    }

    override func caretRect(for position: UITextPosition) -> CGRect {
        if let cell = cellCaretRect(for: position) { return cell }
        return emptyLineCaret(super.caretRect(for: position), at: position)
    }

    /// On an empty line in edit mode the caret is as tall as on a body
    /// line: TextKit makes it the empty line's whole height, its paragraph
    /// spacing too (ADR 0110).
    private func emptyLineCaret(_ rect: CGRect, at position: UITextPosition) -> CGRect {
        guard isEditMode, !rect.isNull, !rect.isInfinite else { return rect }
        let text = textStorage.string as NSString
        let location = offset(from: beginningOfDocument, to: position)
        guard location >= 0, location <= text.length else { return rect }
        let line = text.lineRange(for: NSRange(location: location, length: 0))
        guard text.substring(with: line).trimmingCharacters(in: .newlines).isEmpty else { return rect }
        let height = ceil(MarkdownStyler.font(.body, traits: traitCollection).lineHeight)
        guard rect.height > height else { return rect }
        var caret = rect
        caret.size.height = height
        return caret
    }

    override func selectionRects(for range: UITextRange) -> [UITextSelectionRect] {
        cellSelectionRects(for: range) ?? super.selectionRects(for: range)
    }

    override func firstRect(for range: UITextRange) -> CGRect {
        cellFirstRect(for: range) ?? super.firstRect(for: range)
    }

    override func closestPosition(to point: CGPoint) -> UITextPosition? {
        cellPosition(at: point) ?? super.closestPosition(to: point)
    }

    override func selectAll(_ sender: Any?) {
        if !selectCellText() { super.selectAll(sender) }
    }

    /// UIKit's view for one of the selection's handles (the loupe rises
    /// from it).
    func handleView(for end: SelectionEnd) -> UIView? {
        for case let display as UITextSelectionDisplayInteraction in interactions {
            let views = display.handleViews
            return end == .start ? views.first : views.last
        }
        return nil
    }

    /// The caret is drawn where UIKit last asked for it: after its cell is
    /// laid out again or scrolled, it asks again.
    func redrawCaret() {
        guard caretCell != nil else { return }
        for case let display as UITextSelectionDisplayInteraction in interactions {
            display.setNeedsSelectionUpdate()
        }
    }

    /// The system's own ⌘B and ⌘I (its Format menu), which UIKit may match
    /// before the responder's: the same commands in edit mode.
    override func toggleBoldface(_ sender: Any?) {
        if isEditMode { boldKey() }
    }

    override func toggleItalics(_ sender: Any?) {
        if isEditMode { italicKey() }
    }

    @objc func boldKey() { onCommand?(.format(.bold)) }
    @objc func italicKey() { onCommand?(.format(.italic)) }
    @objc func linkKey() { onLinkKey?() }
    @objc func indentKey() { onCommand?(caretCell == nil ? .indent : .moveInTable(.next)) }
    @objc func outdentKey() { onCommand?(caretCell == nil ? .outdent : .moveInTable(.previous)) }

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

extension CandidateTextView: @preconcurrency UIEditMenuInteractionDelegate {
    func editMenuInteraction(
        _ interaction: UIEditMenuInteraction, targetRectFor configuration: UIEditMenuConfiguration
    ) -> CGRect {
        cellMenuTarget
    }
}
