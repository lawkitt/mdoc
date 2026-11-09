import MdocFFI
import UIKit

/// What edit mode does with the text view's changes (ADR 0109): the model's
/// typing path, its undo and the core's text.
@MainActor
struct EditActions {
    /// One committed edit; nil when the core refused it.
    var typed: (SourceRange, String) -> Typed?
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
}

extension EditActions {
    init(_ model: DocumentViewModel) {
        self.init(
            typed: { [weak model] in model?.typed($0, $1) },
            caretMoved: { [weak model] in model?.caretMoved() },
            coreText: { [weak model] in model?.coreText() ?? "" },
            undo: { [weak model] in model?.undoTapped() },
            redo: { [weak model] in model?.redoTapped() },
            canUndo: { [weak model] in model?.canUndo ?? false },
            canRedo: { [weak model] in model?.canRedo ?? false },
            revision: { [weak model] in model?.typing.revision })
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
        bridge.endTyping = { actions?.caretMoved() }
        bridge.onTyped = { [weak self] typed in self?.shownTextChanged(to: typed.textRevision) }
        bridge.onResync = { [weak self, weak view] in
            guard let self, let view, let actions else { return }
            self.resync(view, core: actions.coreText())
        }
    }

    /// Shows `styled` while editing: when the view's string is the same,
    /// only the attributes that differ change; when the core changed the
    /// text (undo, redo, a case rejoined), the stretch that differs is
    /// replaced and the caret kept beside it. Waits while text is marked.
    func showEditing(_ styled: NSAttributedString, in view: CandidateTextView) {
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
        let whole = NSRange(location: 0, length: styled.length)
        styled.enumerateAttributes(in: whole) { attributes, run, _ in
            var current = NSRange()
            let old = storage.attributes(at: run.location, longestEffectiveRange: &current, in: run)
            if current != run || !NSDictionary(dictionary: old).isEqual(to: attributes) {
                storage.setAttributes(attributes, range: run)
            }
        }
        storage.endEditing()
        let location = min(selection.location, storage.length)
        view.selectedRange = NSRange(location: location, length: min(selection.length, storage.length - location))
        bridge.reset(to: styled.string)
        updateTypingAttributes(view)
    }

    /// Typed text takes the look of the text before the caret, without a
    /// candidate's: until the pause restyles it, a new word is no name.
    func updateTypingAttributes(_ view: UITextView) {
        let storage = view.textStorage
        guard storage.length > 0 else { return }
        let location = max(0, min(view.selectedRange.location, storage.length) - 1)
        var attributes = storage.attributes(at: location, effectiveRange: nil)
        for key in MarkdownStyler.candidateKeys { attributes[key] = nil }
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
    }

    // MARK: Text view delegate, while editing

    func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
        guard textView.isEditable, !(textView is CellTextView) else { return false }
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

    func textViewDidChangeSelection(_ textView: UITextView) {
        guard textView.isEditable else { return }
        bridge.selectionChanged(textView)
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
        if selectedRange == NSRange(location: 0, length: 0), contentOffset.y > -adjustedContentInset.top + 1,
           let top = closestPosition(to: CGPoint(x: textContainerInset.left, y: contentOffset.y + adjustedContentInset.top + 8)) {
            // The caret starts where the reader is, not at the top.
            let location = offset(from: beginningOfDocument, to: top)
            selectedRange = NSRange(location: location, length: 0)
        }
        becomeFirstResponder()
    }

    func copySelection() {
        guard let range = selectedTextRange, !range.isEmpty, let text = text(in: range) else { return }
        InAppClipboard.shared.write(text)
    }

    func cutSelection() {
        guard let range = selectedTextRange, !range.isEmpty, let text = text(in: range) else { return }
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
