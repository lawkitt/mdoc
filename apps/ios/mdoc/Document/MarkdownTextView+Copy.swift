import MdocFFI
import UIKit

/// The core's answers for a selection's copy items (#1209), asked once per
/// selection: by the text's revision and the selection's source range.
struct SelectionChecks {
    struct Key: Equatable {
        var revision: UInt64?
        var range: SourceRange
    }

    /// The latest answer; a nil check is a check that failed.
    private(set) var answered: (key: Key, check: SelectionCheck?)?
    /// The selection asked about now, and that request's number.
    private(set) var asking: (key: Key, request: Int)?
    private var requests = 0

    /// The answer for `key`, if the core gave it; `.some(nil)` is a check
    /// that failed.
    func answer(for key: Key) -> SelectionCheck?? {
        guard let answered, answered.key == key else { return nil }
        return .some(answered.check)
    }

    /// Starts asking about `key`: the request's number, nil when that is
    /// asked or answered already.
    mutating func ask(_ key: Key) -> Int? {
        guard asking?.key != key, answer(for: key) == nil else { return nil }
        requests += 1
        asking = (key, requests)
        return requests
    }

    /// The core answered `request`: kept only while it is the one asked
    /// now. Returns whether it was kept.
    @discardableResult
    mutating func answer(_ request: Int, _ check: SelectionCheck?) -> Bool {
        guard let asking, asking.request == request else { return false }
        self.asking = nil
        answered = (asking.key, check)
        return true
    }

    /// The selection went: a later one is asked about afresh, since the
    /// Project's names may have changed meanwhile, and an answer still on
    /// its way is dropped.
    mutating func forget() {
        answered = nil
        asking = nil
    }
}

/// A selection Hide name snapped to whole words, its ends' punctuation
/// trimmed (ADR 0114): the reader's own, which Copy takes, and the snapped
/// one shown, while it stays.
struct UnsnappedSelection {
    weak var view: UITextView?
    var original: NSRange
    var snapped: NSRange

    /// The reader's own selection when `view` shows the snapped one.
    func original(of view: UITextView) -> NSRange? {
        self.view === view && view.selectedRange == snapped ? original : nil
    }
}

/// A selection's menu outside edit mode (#1209, ADR 0109 as amended): the
/// copy items first, then Hide name, then Select All. The copy items wait
/// for the core's check, which reads the Vault off the main thread: until
/// it answers, Hide name and Select All show alone, and the menu fills in
/// when it does. A copy is the selection as written.
extension MarkdownTextView.Coordinator {
    func selectionMenu(in textView: UITextView, at range: NSRange, suggested: [UIMenuElement]) -> UIMenu {
        let base = (textView as? CellTextView)?.location ?? 0
        let source = SelectionCopy.sourceRange(range, base: base, in: offsets)
        // The copy items are for the reader's own selection, not the one
        // Hide name snapped.
        let own = unsnapped?.original(of: textView) ?? range
        let copy = copyElements(for: SelectionCopy.sourceRange(own, base: base, in: offsets), in: textView, at: own)
        // A selection in the main text that runs into a folded table holds
        // hidden source: no Hide name (ADR 0096). Cells are their own text
        // views.
        let crossesTable = (textView as? CandidateTextView)?.tableViews
            .contains { NSIntersectionRange($0.table.range, range).length > 0 } ?? false
        let hide = crossesTable ? [] : hideMenu(for: source, in: textView, at: range).children
        let groups: [[UIMenuElement]] = [copy, hide, [selectAll(from: suggested, in: textView)]]
        return UIMenu(children: groups.filter { !$0.isEmpty }.map { UIMenu(options: .displayInline, children: $0) })
    }

    /// The system's Select All, or one in its words when UIKit suggested
    /// none (a menu mdoc shows itself), which shows the whole text's menu.
    private func selectAll(from suggested: [UIMenuElement], in textView: UITextView) -> UIMenuElement {
        func find(_ element: UIMenuElement) -> UIMenuElement? {
            if let command = element as? UICommand,
               command.action == #selector(UIResponderStandardEditActions.selectAll(_:)) { return command }
            return (element as? UIMenu)?.children.lazy.compactMap(find).first
        }
        if let command = suggested.lazy.compactMap(find).first { return command }
        let action = UIAction(title: String(localized: "Select All")) { [weak self, weak textView] _ in
            guard let self, let textView else { return }
            textView.selectAll(nil)
            // Once UIKit closed the menu Select All was in.
            DispatchQueue.main.async { [weak self, weak textView] in
                guard let self, let textView, textView.selectedRange.length > 0 else { return }
                self.presentMenu(on: textView) { [weak self] view, _ in
                    self?.selectionMenu(in: view, at: view.selectedRange, suggested: [])
                }
            }
        }
        action.accessibilityIdentifier = "select-all"
        return action
    }

    /// The copy items for `source`, once the core answered for it; none
    /// while it is asked. A view showing text the core no longer holds
    /// offers Copy with names alone.
    private func copyElements(for source: SourceRange, in textView: UITextView, at range: NSRange) -> [UIMenuElement] {
        guard let actions = parent.selectionCopy, source.length > 0 else { return [] }
        guard showsCurrentText else { return elements(SelectionCopy.items(for: nil), source, textView, range, revision: nil) }
        let key = SelectionChecks.Key(revision: parent.snapshot.textRevision, range: source)
        guard let check = selectionChecks.answer(for: key) else {
            if let request = selectionChecks.ask(key) { ask(key, request, actions, for: textView) }
            return []
        }
        return elements(SelectionCopy.items(for: check), source, textView, range, revision: key.revision)
    }

    private func elements(
        _ items: [SelectionCopy.Item], _ source: SourceRange, _ textView: UITextView, _ range: NSRange, revision: UInt64?
    ) -> [UIMenuElement] {
        items.map { item in
            let action = UIAction(title: item.title, image: item.image) { [weak self, weak textView] _ in
                guard let self, let textView else { return }
                switch item {
                case .copy, .copyWithNames:
                    self.copyAsWritten(textView)
                case .replaceAndCopy:
                    textView.selectedRange = NSRange(location: range.location, length: 0)
                    self.parent.selectionCopy?.replaceAndCopy(source, revision)
                }
            }
            action.accessibilityIdentifier = item.identifier
            return action
        }
    }

    /// Asks the core about `key`; once it answers, a menu still over that
    /// selection shows the copy items.
    private func ask(
        _ key: SelectionChecks.Key, _ request: Int, _ actions: SelectionCopyActions, for textView: UITextView
    ) {
        Task { @MainActor [weak self, weak textView] in
            let check = await actions.check(key.range)
            guard let self, self.selectionChecks.answer(request, check) else { return }
            guard let textView, textView.selectedRange.length > 0, self.showsCurrentText,
                  self.parent.snapshot.textRevision == key.revision,
                  self.sourceRange(of: textView) == key.range else { return }
            self.reloadSelectionMenu(on: textView)
        }
    }

    /// The reader's selection of `view` in the shown text's source offsets.
    private func sourceRange(of view: UITextView) -> SourceRange {
        let own = unsnapped?.original(of: view) ?? view.selectedRange
        return SelectionCopy.sourceRange(own, base: (view as? CellTextView)?.location ?? 0, in: offsets)
    }

    /// Builds the menu showing over `view`'s selection again: UIKit's own,
    /// or the one mdoc showed after snapping it (`SelectionMenuPresenter`).
    /// Edit mode's cell menu is not a selection menu of the review.
    private func reloadSelectionMenu(on view: UITextView) {
        for case let interaction as UIEditMenuInteraction in view.interactions
        where !(interaction.delegate is CandidateTextView) {
            interaction.reloadVisibleMenu()
        }
    }

    /// ⌘C, VoiceOver's Copy, edit mode's Copy and the menu's Copy and Copy
    /// with names: the selection as written, marked unless the core finds
    /// it clean now.
    func copyAsWritten(_ view: UITextView) {
        let range = unsnapped?.original(of: view) ?? view.selectedRange
        guard range.length > 0, NSMaxRange(range) <= view.textStorage.length else { return }
        let text = (view as? CellTextView)?.source(in: range) ?? (view.textStorage.string as NSString).substring(with: range)
        guard let actions = parent.selectionCopy else { return SelectionCopy.write(text, marked: true) }
        actions.copy(text, coreRange(of: view, range))
    }

    /// `range` of `view` in the text the core holds; nil when the view
    /// shows other text: a name's words being fixed (the draft shows),
    /// marked text being composed, or newer text waiting to show.
    private func coreRange(of view: UITextView, _ range: NSRange) -> SourceRange? {
        guard parent.wordsFix == nil, view.markedTextRange == nil else { return nil }
        let base = (view as? CellTextView)?.location ?? 0
        // Typing moves edit mode's text on without a rebuild.
        if parent.editing {
            let text = shownView?.textStorage.string ?? view.textStorage.string
            return SelectionCopy.sourceRange(range, base: base, in: TextOffsets(text))
        }
        return showsCurrentText ? SelectionCopy.sourceRange(range, base: base, in: offsets) : nil
    }
}
