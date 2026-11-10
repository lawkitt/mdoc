import MdocFFI
import UIKit

/// What edit mode does with the text view's changes (ADR 0109): the model's
/// typing path, its undo and the core's text.
@MainActor
struct EditActions {
    /// One committed edit; nil when the core refused it.
    var typed: (SourceRange, String) -> Typed?
    /// One editor command's edits, one "Formatting" step; nil when the core
    /// refused them.
    var formatted: ([EditorEdit]) -> Typed? = { _ in nil }
    /// The link sheet, for the Link key, ⌘K or the link chip's Edit.
    var editLink: (LinkDraft) -> Void = { _ in }
    /// The caret moved away from where typing left it.
    var caretMoved: () -> Void
    /// The core's text now, when the view's has drifted from it.
    var coreText: () -> String
    var undo: () -> Void
    var redo: () -> Void
    var canUndo: () -> Bool
    var canRedo: () -> Bool
    /// The text revision the latest typed edit left: a snapshot of another
    /// revision is older than the view's text.
    var revision: () -> UInt64?
    /// Hands the model what commits marked text and syncs the view, for
    /// Done to run before it reads the core's text.
    var setFlush: (@escaping @MainActor () -> Void) -> Void
}

extension EditActions {
    init(_ model: DocumentViewModel) {
        self.init(
            typed: { [weak model] in model?.typed($0, $1) },
            formatted: { [weak model] in model?.formatted($0) },
            editLink: { [weak model] in model?.linkDraft = $0 },
            caretMoved: { [weak model] in model?.caretMoved() },
            coreText: { [weak model] in model?.coreText() ?? "" },
            undo: { [weak model] in model?.undoTapped() },
            redo: { [weak model] in model?.redoTapped() },
            canUndo: { [weak model] in model?.canUndo ?? false },
            canRedo: { [weak model] in model?.canRedo ?? false },
            revision: { [weak model] in model?.typing.revision },
            setFlush: { [weak model] in model?.typing.flush = $0 })
    }
}

/// The text view's undo manager in edit mode: ⌘Z, ⇧⌘Z, shake and the
/// three-finger swipe reach the core's one undo line (ADR 0109), and
/// UIKit's own typing undo is never recorded, since the core holds typing.
final class CoreUndoManager: UndoManager {
    var actions: EditActions?

    override init() {
        super.init()
        disableUndoRegistration()
    }

    override var canUndo: Bool { MainActor.assumeIsolated { actions?.canUndo() ?? false } }
    override var canRedo: Bool { MainActor.assumeIsolated { actions?.canRedo() ?? false } }

    override func undo() {
        MainActor.assumeIsolated { actions?.undo() }
    }

    override func redo() {
        MainActor.assumeIsolated { actions?.redo() }
    }
}

extension MarkdownTextView.Coordinator {
    /// Points the typing path at this render's edit actions.
    func connectEditing(_ view: CandidateTextView) {
        let actions = parent.edit
        view.editActions = actions
        view.coreUndo.actions = actions
        view.onEdited = { [weak self, weak view] in
            guard let self, let view else { return }
            self.bridge.didChange(view)
        }
        view.onStorageEdit = { [weak self, weak view] storage in
            guard let self, let view else { return }
            self.bridge.storageEdited(storage, in: view)
        }
        bridge.send = { range, text in actions?.typed(range, text) }
        bridge.sendCommand = { edits in actions?.formatted(edits) }
        connectToolbar(view)
        bridge.endTyping = { actions?.caretMoved() }
        actions?.setFlush { [weak self, weak view] in
            guard let self, let view else { return }
            // Marked text is committed as typed, then sent.
            if view.markedTextRange != nil { view.unmarkText() }
            self.bridge.didChange(view)
        }
        bridge.onTyped = { [weak self] typed in self?.shownTextChanged(to: typed.textRevision) }
        bridge.onResync = { [weak self, weak view] in
            guard let self, let view, let actions else { return }
            self.resync(view, core: actions.coreText())
        }
        bridge.onApplied = { [weak self, weak view] edits in
            guard let self, let view else { return }
            self.restyle(after: edits, in: view)
        }
        let rendered = parent.rendered
        bridge.structural = { [weak view] key, range, selection in
            guard let view else { return .native }
            return StructuralKeys.outcome(
                key, text: view.textStorage.string as NSString, selection: selection, native: range, rendered: rendered,
                // A table and the line breaks around it: joining a line to
                // a row would change the table.
                protected: rendered ? view.tableViews.map { Self.protected($0.table.range, in: view.textStorage.length) } : [])
        }
        view.hidesMarkers = parent.rendered
        view.onTableTap = actions == nil ? nil : { [weak view] table in
            // Tables are not edited in this slice: the caret goes after one.
            guard let view else { return }
            let text = view.textStorage.string as NSString
            var after = min(NSMaxRange(table.table.range), text.length)
            if after < text.length { after = NSMaxRange(text.lineRange(for: NSRange(location: after, length: 0))) }
            if !view.isFirstResponder { view.becomeFirstResponder() }
            view.selectedRange = NSRange(location: after, length: 0)
        }
    }

    /// A table's range and the line breaks either side of it, within the
    /// text: a deletion there would join a line to a row.
    static func protected(_ table: NSRange, in length: Int) -> NSRange {
        let start = max(table.location - 1, 0)
        let end = min(NSMaxRange(table) + 1, length)
        return NSRange(location: start, length: max(end - start, 0))
    }

    /// What the styler needs from the view now.
    func look(_ view: CandidateTextView) -> LivePreview.Look {
        LivePreview.Look(traits: view.traitCollection, rendered: parent.rendered, width: view.textWidth > 0 ? view.textWidth : nil)
    }

    /// The core took typed edits: the block around them restyles in place,
    /// or the whole text when more than a block changed.
    func restyle(after edits: [AppliedEdit], in view: CandidateTextView) {
        // Marked text reaches the core only once committed, and restyles then.
        guard view.markedTextRange == nil else { return }
        if !livePreview.edited(edits, in: view, look: look(view)) {
            restyleWhole(view)
        }
        lastSelection = view.selectedRange
        view.invalidateSpoken()
        updateTypingAttributes(view)
        toolbarChanged(view)
    }

    /// Restyles the whole text as the view has it, with the candidates as
    /// edits left them: a fence typed or removed, a Replace All.
    func restyleWhole(_ view: CandidateTextView) {
        let text = view.textStorage.string
        var styler = MarkdownStyler(
            traits: view.traitCollection, selected: nil, scoped: [], rendered: parent.rendered,
            width: view.textWidth > 0 ? view.textWidth : nil)
        styler.editing = true
        let document = styler.document(markdown: text, styling: MarkdownStyling(markdown: text), candidates: livePreview.candidates)
        showEditing(document.text, tables: parent.rendered ? document.tables : [], in: view)
        livePreview.selectionChanged(in: view, look: look(view), force: true)
    }

    /// Shows `styled` while editing: when the view's string is the same,
    /// only the attributes that differ change; when the core changed the
    /// text (undo, redo, a case rejoined), the stretch that differs is
    /// replaced and the caret kept beside it. Waits while text is marked.
    func showEditing(_ styled: NSAttributedString, tables: [RenderedTable]? = nil, in view: CandidateTextView) {
        guard view.markedTextRange == nil else {
            restyleAfterComposing = true
            return
        }
        restyleAfterComposing = false
        let storage = view.textStorage
        var selection = view.selectedRange
        storage.beginEditing()
        if let change = TypingBridge.difference(from: storage.string as NSString, to: styled.string as NSString) {
            storage.replaceCharacters(in: change.range, with: change.text)
            let delta = (change.text as NSString).length - change.range.length
            if selection.location >= NSMaxRange(change.range) {
                selection.location += delta
            } else if NSMaxRange(selection) > change.range.location {
                // The change ran into the selection: the caret goes after it.
                selection = NSRange(location: change.range.location + (change.text as NSString).length, length: 0)
            }
        }
        storage.endEditing()
        LivePreview.apply(styled, at: 0, in: storage)
        if let tables { view.setTables(tables) }
        let location = min(selection.location, storage.length)
        view.selectedRange = NSRange(location: location, length: min(selection.length, storage.length - location))
        lastSelection = view.selectedRange
        bridge.reset(to: styled.string)
        view.invalidateSpoken()
        updateTypingAttributes(view)
    }

    /// Typed text takes the look of the text before the caret, without a
    /// candidate's: until the pause restyles it, a new word is no name. A
    /// hidden marker's look is never taken: the shown character nearest
    /// the caret on its line gives it.
    func updateTypingAttributes(_ view: UITextView) {
        let storage = view.textStorage
        guard storage.length > 0 else { return }
        let caret = min(view.selectedRange.location, storage.length)
        let line = (storage.string as NSString).lineRange(for: NSRange(location: caret, length: 0))
        func shown(_ index: Int) -> Bool {
            index >= 0 && index < storage.length && storage.attribute(.mdocHidden, at: index, effectiveRange: nil) == nil
        }
        var location = max(0, caret - 1)
        if !shown(location) {
            location = (line.location..<caret).reversed().first(where: shown)
                ?? (caret..<NSMaxRange(line)).first(where: shown)
                ?? location
        }
        var attributes = storage.attributes(at: location, effectiveRange: nil)
        for key in MarkdownStyler.candidateKeys { attributes[key] = nil }
        if attributes[.mdocHidden] != nil {
            attributes = [.font: MarkdownStyler.font(.body, traits: view.traitCollection), .foregroundColor: UIColor.label]
        }
        view.typingAttributes = attributes
    }

    /// The view's text drifted from the core's: the one edit between them
    /// goes to the core, or, refused, the view shows the core's text.
    func resync(_ view: CandidateTextView, core: String) {
        if !bridge.resync(core: core, view: view.textStorage.string) {
            let restored = NSMutableAttributedString(attributedString: view.textStorage)
            restored.replaceCharacters(in: NSRange(location: 0, length: restored.length), with: core)
            showEditing(restored, in: view)
        }
        // Where the text changed is not known: all of it restyles.
        restyleWhole(view)
    }

    // MARK: Text view delegate, while editing

    func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
        guard textView.isEditable, !(textView is CellTextView) else { return false }
        // Typing right after a table (at the text's end, where a tap on it
        // puts the caret) starts a paragraph below it rather than adding to
        // its last row: tables are not edited in this slice.
        if parent.rendered, !bridge.isCommanding, range.length == 0, !text.isEmpty, text != "\n",
           textView.markedTextRange == nil,
           let view = textView as? CandidateTextView,
           view.tableViews.contains(where: { NSMaxRange($0.table.range) == range.location }) {
            let typed = "\n\n" + text
            bridge.replace(
                range, with: typed, selection: NSRange(location: range.location + (typed as NSString).length, length: 0),
                in: textView)
            return false
        }
        return bridge.willChange(textView, range: range, text: text)
    }

    func textViewDidChange(_ textView: UITextView) {
        guard textView.isEditable, let view = textView as? CandidateTextView else { return }
        bridge.didChange(view)
        if restyleAfterComposing, view.markedTextRange == nil, let content {
            // A restyle that waited for the marked text.
            restyleAfterComposing = false
            rebuild(content, view)
        }
    }

    /// A caret moved by the reader: out of a hidden marker run (live
    /// preview), the typing step ends, and the reveal follows it.
    func textViewDidChangeSelection(_ textView: UITextView) {
        guard textView.isEditable, let view = textView as? CandidateTextView else { return }
        let moved = !bridge.isChanging && view.markedTextRange == nil
        if moved, parent.rendered, !snapping {
            let snapped = CaretSnap.snap(view.selectedRange, from: lastSelection, in: view.textStorage)
            if snapped != view.selectedRange {
                snapping = true
                view.selectedRange = snapped
                snapping = false
            }
        }
        bridge.selectionChanged(textView)
        if moved {
            lastSelection = view.selectedRange
            livePreview.selectionChanged(in: view, look: look(view))
            view.invalidateSpoken()
            toolbarChanged(view)
        }
        updateTypingAttributes(textView)
    }
}

extension CandidateTextView {
    /// The find bar's own actions, in both modes.
    static let findActions: Set<Selector> = [
        #selector(UIResponder.find(_:)),
        #selector(UIResponder.findNext(_:)),
        #selector(UIResponder.findPrevious(_:)),
        #selector(UIResponder.useSelectionForFind(_:)),
    ]

    /// What edit mode's edit menu and keys offer (ADR 0109): Cut, Copy,
    /// Paste, Select All and find, Replace included. No Look Up, Translate,
    /// Search Web, Share, Writing Tools or text styles.
    func canPerformEditAction(_ action: Selector, withSender sender: Any?) -> Bool {
        switch action {
        case #selector(UIResponderStandardEditActions.cut(_:)), #selector(UIResponderStandardEditActions.copy(_:)):
            return selectedRange.length > 0
        case #selector(UIResponderStandardEditActions.paste(_:)):
            return InAppClipboard.shared.hasText
        case #selector(UIResponderStandardEditActions.selectAll(_:)), #selector(UIResponderStandardEditActions.select(_:)):
            return textStorage.length > 0
        case #selector(UIResponder.findAndReplace(_:)):
            return super.canPerformAction(action, withSender: sender)
        case #selector(UIResponderStandardEditActions.toggleBoldface(_:)),
             #selector(UIResponderStandardEditActions.toggleItalics(_:)):
            // ⌘B and ⌘I through the system's menu: the editor core's
            // toggles (`toggleBoldface`), never UIKit's text styles; the
            // edit menu leaves them out.
            return true
        case #selector(linkKey):
            // ⌘K (the app's menu sends it here first).
            return true
        default:
            return Self.findActions.contains(action) && super.canPerformAction(action, withSender: sender)
        }
    }

    /// Edit mode's edit menu: the suggested Cut, Copy, Paste and Select All,
    /// in the system's words and order, and nothing else.
    static func editMenu(from suggested: [UIMenuElement]) -> UIMenu {
        let allowed: Set<Selector> = [
            #selector(UIResponderStandardEditActions.cut(_:)),
            #selector(UIResponderStandardEditActions.copy(_:)),
            #selector(UIResponderStandardEditActions.paste(_:)),
            #selector(UIResponderStandardEditActions.selectAll(_:)),
        ]
        func kept(_ element: UIMenuElement) -> UIMenuElement? {
            if let command = element as? UICommand { return allowed.contains(command.action) ? command : nil }
            if let menu = element as? UIMenu {
                let children = menu.children.compactMap(kept)
                return children.isEmpty ? nil : menu.replacingChildren(children)
            }
            return nil
        }
        return UIMenu(children: suggested.compactMap(kept))
    }

    /// Switches edit mode on or off: editable, spellcheck and autocorrect
    /// with it (smart quotes and dashes stay off), and the keyboard.
    func setEditMode(_ on: Bool) {
        guard isEditable != on else { return }
        isEditable = on
        smartQuotesType = .no
        smartDashesType = .no
        smartInsertDeleteType = .no
        autocorrectionType = on ? .yes : .no
        spellCheckingType = on ? .yes : .no
        accessibilityHint = on
            ? String(localized: "Edits the text. Changes save as you type.")
            : String(localized: "Underlined words are names mdoc found. Use the Candidates rotor to move between them.")
        watchStorage(on)
        guard on else {
            resignFirstResponder()
            return
        }
        // The caret starts where the reader is: at the first line in view,
        // never where showing the text left the selection (its end). A
        // selection made in review is kept.
        if selectedRange.length == 0 {
            let top = contentOffset.y + adjustedContentInset.top
            let location = top <= 1 ? 0 : closestPosition(to: CGPoint(x: textContainerInset.left, y: top + 8))
                .map { offset(from: beginningOfDocument, to: $0) } ?? 0
            selectedRange = NSRange(location: location, length: 0)
        }
        becomeFirstResponder()
    }

    /// The selected source, as typed: never what VoiceOver hears.
    private var selectedSource: String? {
        let range = selectedRange
        guard range.length > 0, NSMaxRange(range) <= textStorage.length else { return nil }
        return (textStorage.string as NSString).substring(with: range)
    }

    func copySelection() {
        guard let text = selectedSource else { return }
        InAppClipboard.shared.write(text)
    }

    func cutSelection() {
        guard let range = selectedTextRange, !range.isEmpty, let text = selectedSource else { return }
        InAppClipboard.shared.write(text)
        replace(range, withText: "")
        onEdited?()
    }

    /// A change UIKit makes without telling the delegate (the find bar's
    /// Replace and Replace All) still reaches the core: each edit of the
    /// characters is noted as it happens, so separate replacements stay
    /// separate core edits, and once the change is over the typing path
    /// syncs it. A change the delegate saw is synced already, and nothing
    /// is sent again.
    func watchStorage(_ on: Bool) {
        if let storageObserver { NotificationCenter.default.removeObserver(storageObserver) }
        storageObserver = nil
        guard on else { return }
        storageObserver = NotificationCenter.default.addObserver(
            forName: NSTextStorage.didProcessEditingNotification, object: textStorage, queue: nil
        ) { [weak self] notification in
            guard (notification.object as? NSTextStorage)?.editedMask.contains(.editedCharacters) == true else { return }
            MainActor.assumeIsolated { self?.charactersEdited() }
        }
    }

    /// `watchStorage`'s step: the edit is noted now, while the storage
    /// still knows its range, and synced once the change is over.
    private func charactersEdited() {
        guard isEditMode else { return }
        onStorageEdit?(textStorage)
        Task { @MainActor [weak self] in
            guard let self, self.isEditMode else { return }
            self.onEdited?()
        }
    }

    func pasteClipboard() {
        guard let range = selectedTextRange, let text = InAppClipboard.shared.read(), !text.isEmpty else { return }
        replace(range, withText: text)
        onEdited?()
    }
}
