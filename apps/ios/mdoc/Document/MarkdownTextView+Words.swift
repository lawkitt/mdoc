import MdocFFI
import UIKit

/// Fix the words' request to the text view (ADR 0114): the name whose words
/// are fixed, where its handles start and how far they reach, in the
/// draft's source bytes, and its category for the selection's colour.
struct WordsFixRequest: Equatable {
    let candidate: CandidateID
    let wording: SourceRange
    let bounds: SourceRange
    let category: Category?

    init(_ fix: WordsFix) {
        candidate = fix.candidate
        wording = fix.draft.wording
        bounds = fix.draft.bounds
        category = fix.draft.snapshot.candidate(fix.candidate)?.category
    }
}

/// What the text view tells the model while a name's words are fixed:
/// Hide «…» and Cancel are on `FixWordsBar` (ADR 0115).
struct WordsFixActions {
    /// The handles moved; the words they hold, in the draft's bytes.
    var selected: (SourceRange) -> Void
    var cancel: () -> Void
    /// Adjust Words' one-time demo is on: a ghost handle glides along the
    /// words once the handles show.
    var demo: () -> Bool = { false }
}

extension WordsFixActions {
    @MainActor
    init(_ model: DocumentViewModel) {
        self.init(
            selected: { [weak model] in model?.fixSelectionChanged($0) },
            cancel: { [weak model] in model?.cancelFixWords() },
            demo: { [weak model] in model?.adjustWordsDemo ?? false })
    }
}

/// A name's words in the text (ADR 0114). Hide name's selection is tinted
/// and its edit menu reads Hide "…" beside Hide as ›. Fix the words selects
/// the name in the draft, plain text without links (ADR 0115), in a table
/// cell too, whose table then holds still; the handles stay inside the
/// paragraph or cell the core allows, no menu shows over them (the bar
/// holds Hide «…»), and it ends when the selection goes.
extension MarkdownTextView.Coordinator {
    /// A selection's colour outside edit mode: tinted, not the ink's grey.
    static let selectionTint = UIColor.systemBlue

    /// Hide name's edit menu for a selection outside any name: Hide "…"
    /// with the words the core would hide, and Hide as › its categories,
    /// the guess first. Plain Hide name when the core sees nothing to hide
    /// in its own terms: a tap then shows its reason.
    func hideMenu(for source: SourceRange, in textView: UITextView, at range: NSRange) -> UIMenu {
        let clear = { [weak textView] in textView?.selectedRange = NSRange(location: range.location, length: 0) }
        guard let target = parent.hideTarget(source) else {
            let hide = UIAction(title: String(localized: "Hide name"), image: UIImage(systemName: "eye.slash")) { [weak self] _ in
                clear()
                self?.parent.onReplace(source)
            }
            hide.accessibilityIdentifier = "hide-name"
            return UIMenu(children: [hide])
        }
        snapSelection(to: SourceRange(target.range), in: textView)
        let hide = UIAction(
            title: String(localized: "Hide \"\(target.wording.truncatedForMenu)\""), image: UIImage(systemName: "eye.slash")
        ) { [weak self] _ in
            clear()
            self?.parent.onHideAs(source, nil)
        }
        hide.accessibilityIdentifier = "hide-name"
        guard target.categories.count > 1 else { return UIMenu(children: [hide]) }
        let categories = target.categories.map { category in
            UIAction(title: category.label, image: category.menuIcon) { [weak self] _ in
                clear()
                self?.parent.onHideAs(source, category)
            }
        }
        let hideAs = UIMenu(title: String(localized: "Hide as"), image: UIImage(systemName: "tag"), children: categories)
        return UIMenu(children: [hide, hideAs])
    }

    /// The menu's words are the selection's: a word it reached into shows
    /// whole, and the punctuation at its ends goes. Moving the selection
    /// closes UIKit's menu, so the same menu shows again over the words.
    private func snapSelection(to target: SourceRange, in textView: UITextView) {
        let base = (textView as? CellTextView)?.location ?? 0
        let shown = offsets.nsRange(target)
        let local = NSRange(location: shown.location - base, length: shown.length)
        guard local != textView.selectedRange, local.location >= 0,
              NSMaxRange(local) <= textView.textStorage.length else { return }
        DispatchQueue.main.async { [weak self, weak textView] in
            guard let self, let textView, textView.selectedRange.length > 0 else { return }
            textView.selectedRange = local
            self.presentMenu(on: textView) { [weak self] view, source in
                self?.hideMenu(for: source, in: view, at: view.selectedRange)
            }
        }
    }

    // MARK: Fix the words

    /// Begins or ends Fix the words as the model asks, once the draft
    /// shows.
    func followWordsFix(_ view: CandidateTextView) {
        guard parent.wordsFix != shownFix else { return }
        guard let request = parent.wordsFix else {
            shownFix = nil
            endFixSelection()
            return
        }
        // The draft's text must show first: its offsets map the request.
        guard pending == nil, content?.textRevision == parent.snapshot.textRevision else { return }
        fixWords = nil
        select(request.wording, for: request, in: view)
        guard let holder = fixHolder else { return }
        keepClear(holder, in: view)
        if parent.wordsFixActions?.demo() == true, !UIAccessibility.isReduceMotionEnabled {
            playHandleDemo(on: holder, tint: holder.tintColor)
        }
    }

    /// The words and their handles sit clear of the bars: the text scrolls
    /// so the selection has room above Adjust Words' bar (and the demo's
    /// caption over it) and below the top bar.
    private func keepClear(_ holder: UITextView, in view: CandidateTextView) {
        guard let range = holder.selectedTextRange else { return }
        let words = holder.selectionRects(for: range).reduce(CGRect.null) { $0.union($1.rect) }
        guard !words.isNull, !words.isInfinite else { return }
        // Room for the end knob and the demo's caption, which shows over
        // the bar after this layout.
        let room: CGFloat = 88
        let rect = holder.convert(words, to: view)
        let visible = view.bounds.inset(by: view.adjustedContentInset)
        var y = view.contentOffset.y
        if rect.maxY + room > visible.maxY {
            y += rect.maxY + room - visible.maxY
        } else if rect.minY - Space.l < visible.minY {
            y -= visible.minY - (rect.minY - Space.l)
        }
        let insets = view.adjustedContentInset
        let lowest = max(-insets.top, view.laidOutContentHeight + insets.bottom - view.bounds.height)
        y = min(max(y, -insets.top), lowest)
        guard abs(y - view.contentOffset.y) > 0.5 else { return }
        view.setContentOffset(CGPoint(x: view.contentOffset.x, y: y), animated: true)
    }

    /// The text was built again while a name's words are fixed (a text
    /// size, width or appearance change): its cells may be new views, so
    /// the handles go back on the words they last held, and a table holds
    /// still again.
    func restoreWordsFix(_ view: CandidateTextView) {
        guard let request = shownFix, parent.wordsFix == request, !parent.editing else { return }
        fixTable?.isScrollEnabled = true
        fixTable = nil
        select(fixWords ?? request.wording, for: request, in: view)
    }

    /// Selects `words` of the draft for `request`, in the cell or text
    /// that holds them.
    private func select(_ words: SourceRange, for request: WordsFixRequest, in view: CandidateTextView) {
        let range = offsets.nsRange(words)
        guard let (holder, local) = holder(of: range, in: view) else { return }
        shownFix = request
        if let cell = holder as? CellTextView, let table = cell.superview as? MarkdownTableView {
            hold(table, around: cell, words: local)
        }
        fixSettling = true
        holder.tintColor = request.category?.uiColor ?? Self.selectionTint
        holder.becomeFirstResponder()
        holder.selectedRange = local
        fixSettling = false
        fixHolder = holder
    }

    /// In a table the cell shows whole and the table holds still, so a
    /// drag of the handles never scrolls it sideways (ADR 0115). A cell
    /// wider than the screen (a long identifier widens its column) can't
    /// show whole: the table shows the name's words and still scrolls, so
    /// the rest of the cell stays in reach.
    private func hold(_ table: MarkdownTableView, around cell: CellTextView, words: NSRange) {
        guard cell.frame.width <= table.bounds.width else {
            if let start = cell.position(from: cell.beginningOfDocument, offset: words.location),
               let end = cell.position(from: start, offset: words.length),
               let range = cell.textRange(from: start, to: end) {
                let rect = cell.selectionRects(for: range).reduce(CGRect.null) { $0.union($1.rect) }
                if !rect.isNull, !rect.isInfinite { table.reveal(cell.convert(rect, to: table)) }
            }
            return
        }
        table.reveal(cell.frame)
        table.isScrollEnabled = false
        fixTable = table
    }

    /// The selection of a name being fixed changed: kept inside the
    /// bounds, reported to the model, and gone means Cancel.
    func fixSelectionChanged(_ textView: UITextView) {
        // A rebuild (rotation, text size, appearance) resets the selection
        // on its own; only the reader's change counts.
        guard let request = shownFix, !fixSettling, !rendering, parent.wordsFix != nil,
              textView === fixHolder else { return }
        let base = (textView as? CellTextView)?.location ?? 0
        let selected = textView.selectedRange
        guard textView.selectedTextRange != nil, selected.length > 0 else {
            parent.wordsFixActions?.cancel()
            return
        }
        let bounds = offsets.nsRange(request.bounds)
        let shown = NSRange(location: selected.location + base, length: selected.length)
        let kept = NSIntersectionRange(shown, bounds)
        if kept != shown, kept.length > 0 {
            // A handle stops where a drag would break the name.
            fixSettling = true
            textView.selectedRange = NSRange(location: kept.location - base, length: kept.length)
            fixSettling = false
        }
        let words = offsets.sourceRange(kept.length > 0 ? kept : shown)
        fixWords = words
        parent.wordsFixActions?.selected(words)
    }

    /// The view holding a fixed name's words lost the focus to a touch
    /// elsewhere, in a cell or in the text (ADR 0116): its selection no
    /// longer shows, so Adjust Words ends as Cancel ends it. A rebuild or
    /// the handles moving to another view resign it too: those are not
    /// the reader's. Checked once this turn of the run loop is over, as a
    /// resign may come inside SwiftUI's update of the view.
    func fixHolderResigned(_ view: UITextView) {
        guard view === fixHolder, shownFix != nil, !fixSettling, !rendering, parent.wordsFix != nil else { return }
        DispatchQueue.main.async { [weak self, weak view] in
            guard let self, let view, view === self.fixHolder, !view.isFirstResponder, !self.rendering,
                  let request = self.shownFix, self.parent.wordsFix == request else { return }
            self.parent.wordsFixActions?.cancel()
        }
    }

    /// The view and its own range that hold `range` of the text: a table
    /// cell for a name in one, else the text itself.
    private func holder(of range: NSRange, in view: CandidateTextView) -> (UITextView, NSRange)? {
        for table in view.tableViews where NSLocationInRange(range.location, table.table.range) {
            guard let cell = table.cells.joined().first(where: {
                $0.location <= range.location && NSMaxRange(range) <= $0.location + $0.textStorage.length
            }) else { return nil }
            return (cell, NSRange(location: range.location - cell.location, length: range.length))
        }
        return (view, range)
    }

    /// `menu` over the selection of `view`, shown by this view's own
    /// interaction (`SelectionMenuPresenter`).
    private func presentMenu(on view: UITextView, menu: @escaping (UITextView, SourceRange) -> UIMenu?) {
        guard let shown = view.selectedTextRange else { return }
        let rect = view.selectionRects(for: shown).reduce(CGRect.null) { $0.union($1.rect) }
        guard !rect.isNull, !rect.isInfinite else { return }
        let presenter = menuPresenter ?? SelectionMenuPresenter(offsets: { [weak self] in self?.offsets })
        menuPresenter = presenter
        presenter.present(on: view, over: rect, menu: menu)
    }

    // MARK: The demo

    /// Adjust Words' demo (ADR 0115): a ghost of the end handle glides back
    /// along the words and returns, once, a moment after the handles show.
    /// It is drawn in the text's own coordinates, so it scrolls with it,
    /// and takes no touches.
    private func playHandleDemo(on holder: UITextView, tint: UIColor) {
        guard let range = holder.selectedTextRange, !range.isEmpty else { return }
        let end = holder.caretRect(for: range.end)
        let start = holder.caretRect(for: range.start)
        guard !end.isNull, !end.isInfinite, end.height > 0 else { return }
        // Back half the words on one line, else a word's width or so.
        let sameLine = abs(start.midY - end.midY) < 1
        let glide = min(96, max(28, sameLine ? (end.minX - start.minX) * 0.5 : 56))
        let ghost = HandleGhost(height: end.height, tint: tint)
        ghost.center = CGPoint(x: end.midX, y: end.minY + ghost.bounds.height / 2)
        ghost.alpha = 0
        handleGhost?.removeFromSuperview()
        holder.addSubview(ghost)
        handleGhost = ghost
        // timing: the handles settle first; the glide then takes about 2 s (ADR 0115)
        UIView.animateKeyframes(withDuration: 2.2, delay: 0.4, options: [.calculationModeCubic]) {
            UIView.addKeyframe(withRelativeStartTime: 0, relativeDuration: 0.12) { ghost.alpha = 0.7 }
            UIView.addKeyframe(withRelativeStartTime: 0.14, relativeDuration: 0.34) {
                ghost.transform = CGAffineTransform(translationX: -glide, y: 0)
            }
            UIView.addKeyframe(withRelativeStartTime: 0.56, relativeDuration: 0.3) { ghost.transform = .identity }
            UIView.addKeyframe(withRelativeStartTime: 0.88, relativeDuration: 0.12) { ghost.alpha = 0 }
        } completion: { _ in
            ghost.removeFromSuperview()
        }
    }

    private func endFixSelection() {
        handleGhost?.removeFromSuperview()
        handleGhost = nil
        fixWords = nil
        fixTable?.isScrollEnabled = true
        fixTable = nil
        guard let holder = fixHolder else { return }
        fixHolder = nil
        fixSettling = true
        if holder.selectedRange.length > 0 { holder.selectedTextRange = nil }
        holder.tintColor = Self.selectionTint
        fixSettling = false
    }
}

/// Shows a selection's menu from mdoc's own interaction: Hide name's menu
/// again once the selection snapped to whole words. UIKit's own edit menu
/// (`editMenuForTextIn`) shows the same menus otherwise.
@MainActor
final class SelectionMenuPresenter: NSObject, @preconcurrency UIEditMenuInteractionDelegate {
    private let offsets: () -> TextOffsets?
    private var interaction: UIEditMenuInteraction?
    private var target = CGRect.null
    private var menu: ((UITextView, SourceRange) -> UIMenu?)?

    init(offsets: @escaping () -> TextOffsets?) {
        self.offsets = offsets
    }

    func present(on view: UITextView, over rect: CGRect, menu: @escaping (UITextView, SourceRange) -> UIMenu?) {
        if let interaction, interaction.view !== view {
            interaction.view?.removeInteraction(interaction)
            self.interaction = nil
        }
        let interaction = self.interaction ?? UIEditMenuInteraction(delegate: self)
        if interaction.view == nil { view.addInteraction(interaction) }
        self.interaction = interaction
        self.menu = menu
        target = rect.insetBy(dx: 0, dy: -Space.s)
        interaction.presentEditMenu(with: UIEditMenuConfiguration(identifier: nil, sourcePoint: CGPoint(x: rect.midX, y: rect.minY)))
    }

    func editMenuInteraction(
        _ interaction: UIEditMenuInteraction, menuFor configuration: UIEditMenuConfiguration, suggestedActions: [UIMenuElement]
    ) -> UIMenu? {
        guard let view = interaction.view as? UITextView, let offsets = offsets(), let menu else { return nil }
        let base = (view as? CellTextView)?.location ?? 0
        let range = view.selectedRange
        return menu(view, offsets.sourceRange(NSRange(location: range.location + base, length: range.length)))
    }

    func editMenuInteraction(_ interaction: UIEditMenuInteraction, targetRectFor configuration: UIEditMenuConfiguration) -> CGRect {
        target
    }
}

/// The demo's ghost of a selection's end handle: a line as tall as the
/// text with a knob under it, in the selection's tint.
final class HandleGhost: UIView {
    init(height: CGFloat, tint: UIColor) {
        let knob: CGFloat = 11
        super.init(frame: CGRect(x: 0, y: 0, width: knob, height: height + knob - 2))
        isUserInteractionEnabled = false
        isAccessibilityElement = false
        let line = UIView(frame: CGRect(x: (knob - 2) / 2, y: 0, width: 2, height: height))
        line.backgroundColor = tint
        line.layer.cornerRadius = 1
        let dot = UIView(frame: CGRect(x: 0, y: height - 2, width: knob, height: knob))
        dot.backgroundColor = tint
        dot.layer.cornerRadius = knob / 2
        addSubview(line)
        addSubview(dot)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is unused") }
}
