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

/// What the text view tells the model while a name's words are fixed.
struct WordsFixActions {
    /// The handles moved; the words they hold, in the draft's bytes.
    var selected: (SourceRange) -> Void
    /// Hide «…»'s title for the words now; nil while they can't be hidden.
    var hideTitle: () -> String?
    /// The core's reason they can't, for the callout.
    var problem: () -> String?
    var confirm: () -> Void
    var cancel: () -> Void
}

extension WordsFixActions {
    @MainActor
    init(_ model: DocumentViewModel) {
        self.init(
            selected: { [weak model] in model?.fixSelectionChanged($0) },
            hideTitle: { [weak model] in
                guard let fix = model?.wordsFix, case .success = fix.target else { return nil }
                return FixWordsBar.hideTitle(for: fix)
            },
            problem: { [weak model] in
                guard let fix = model?.wordsFix, case .failure(let error) = fix.target else { return nil }
                return error.localizedDescription
            },
            confirm: { [weak model] in model?.confirmFixWords() },
            cancel: { [weak model] in model?.cancelFixWords() })
    }
}

/// A name's words in the text (ADR 0114). Hide name's selection is tinted
/// and its edit menu reads Hide "…" beside Hide as ›. Fix the words selects
/// the name in the draft, in a table cell too, shows Hide «…» over it, keeps
/// the handles inside the paragraph or cell the core allows, and ends when
/// the selection goes.
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
        shownFix = request
        let range = offsets.nsRange(request.wording)
        guard let (holder, local) = holder(of: range, in: view) else { return }
        fixSettling = true
        holder.tintColor = request.category?.uiColor ?? Self.selectionTint
        holder.becomeFirstResponder()
        holder.selectedRange = local
        fixSettling = false
        fixHolder = holder
        presentFixMenu(on: holder)
    }

    /// The selection of a name being fixed changed: kept inside the
    /// bounds, reported to the model, and gone means Cancel.
    func fixSelectionChanged(_ textView: UITextView) {
        guard let request = shownFix, !fixSettling, textView === fixHolder else { return }
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
        parent.wordsFixActions?.selected(offsets.sourceRange(kept.length > 0 ? kept : shown))
    }

    /// The edit menu over a name being fixed: Hide «…», or why not.
    func fixMenu(for source: SourceRange) -> UIMenu {
        guard let actions = parent.wordsFixActions else { return UIMenu(children: []) }
        actions.selected(source)
        guard let title = actions.hideTitle() else {
            let problem = actions.problem() ?? String(localized: "Select the name's words.")
            return UIMenu(children: [UIAction(title: problem, attributes: .disabled) { _ in }])
        }
        let hide = UIAction(title: title, image: UIImage(systemName: "eye.slash")) { _ in actions.confirm() }
        hide.accessibilityIdentifier = "fix-words-callout"
        return UIMenu(children: [hide])
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

    /// Hide «…» shows over the name at once, as the edit menu does after a
    /// drag.
    private func presentFixMenu(on holder: UITextView) {
        presentMenu(on: holder) { [weak self] _, source in self?.fixMenu(for: source) }
    }

    /// `menu` over the selection of `view`, shown by this view's own
    /// interaction (`SelectionMenuPresenter`).
    private func presentMenu(on view: UITextView, menu: @escaping (UITextView, SourceRange) -> UIMenu?) {
        guard let shown = view.selectedTextRange else { return }
        let rect = view.selectionRects(for: shown).reduce(CGRect.null) { $0.union($1.rect) }
        guard !rect.isNull, !rect.isInfinite else { return }
        let presenter = fixPresenter ?? SelectionMenuPresenter(offsets: { [weak self] in self?.offsets })
        fixPresenter = presenter
        presenter.present(on: view, over: rect, menu: menu)
    }

    private func endFixSelection() {
        guard let holder = fixHolder else { return }
        fixHolder = nil
        fixSettling = true
        fixPresenter?.dismiss()
        if holder.selectedRange.length > 0 { holder.selectedTextRange = nil }
        holder.tintColor = Self.selectionTint
        fixSettling = false
    }
}

/// Shows a selection's menu from mdoc's own interaction: Fix the words'
/// Hide «…» when the handles first appear, and Hide name's menu again once
/// the selection snapped to whole words. UIKit's own edit menu
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

    func dismiss() {
        interaction?.dismissMenu()
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
