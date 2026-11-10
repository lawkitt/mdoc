import MdocFFI
import UIKit

/// Edit mode's toolbar, formatting keyboard, links and shortcuts (ADR
/// 0109) on the text view: each button or key asks the editor core for its
/// change and applies it through the typing path (`TypingBridge.command`),
/// one edit and one "Formatting" undo step.
extension MarkdownTextView.Coordinator {
    /// Puts the toolbar above the keyboard in edit mode and points its
    /// buttons and the shortcuts at this view; takes them away in review.
    func connectToolbar(_ view: CandidateTextView) {
        guard let actions = parent.edit else {
            view.onCommand = nil
            view.onLinkKey = nil
            if view.inputAccessoryView != nil || view.inputView != nil {
                view.inputAccessoryView = nil
                view.inputView = nil
                toolbar.formatting = false
                view.reloadInputViews()
            }
            setToolbarRoom(0, in: view)
            return
        }
        view.onCommand = { [weak self, weak view] command in
            guard let self, let view else { return }
            self.perform(command, in: view)
        }
        view.onLinkKey = { [weak self, weak view] in
            guard let self, let view else { return }
            self.editLink(in: view)
        }
        toolbar.run = view.onCommand ?? { _ in }
        toolbar.editLink = view.onLinkKey ?? {}
        toolbar.undo = actions.undo
        toolbar.redo = actions.redo
        toolbar.setFormatting = { [weak self, weak view] on in
            guard let self, let view else { return }
            self.setFormatting(on, in: view)
        }
        toolbar.hideKeyboard = { [weak self, weak view] in
            guard let self, let view else { return }
            self.setFormatting(false, in: view, reload: false)
            view.resignFirstResponder()
        }
        if keyboardObserver == nil {
            // The system keyboard's height as it shows, for the formatting
            // keyboard to take its place at its size.
            keyboardObserver = NotificationCenter.default.addObserver(
                forName: UIResponder.keyboardWillShowNotification, object: nil, queue: .main
            ) { [weak self] note in
                guard let frame = note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect else { return }
                MainActor.assumeIsolated {
                    guard let self, !self.toolbar.formatting else { return }
                    self.keyboardHeight = frame.height
                }
            }
            keyboardMoved = NotificationCenter.default.addObserver(
                forName: UIResponder.keyboardDidChangeFrameNotification, object: nil, queue: .main
            ) { [weak self, weak view] _ in
                MainActor.assumeIsolated {
                    guard let self, let view else { return }
                    self.keepCaretInView(view, settled: false)
                }
            }
        }
        if view.inputAccessoryView == nil {
            view.inputAccessoryView = EditToolbarHost(model: toolbar)
            if view.isFirstResponder { view.reloadInputViews() }
        }
        toolbar.canUndo = actions.canUndo()
        toolbar.canRedo = actions.canRedo()
        toolbarChanged(view)
    }

    /// Runs `command` at the selection: the core's change, made in the view
    /// and sent as one step.
    func perform(_ command: EditCommand, in view: CandidateTextView) {
        guard view.isEditMode else { return }
        let rendered = parent.rendered
        bridge.command(in: view) { text, selection in
            command.change(text: text, selection: selection, rendered: rendered)
        }
        // A table control's cell stays in view, and the caret's line
        // above the toolbar (a heading or a list grows it).
        view.revealCaretCell()
        view.redrawCaret()
        refreshToolbar(view)
        keepCaretInView(view, settled: false)
    }

    /// Swaps the formatting keyboard in for the system one, or back.
    func setFormatting(_ on: Bool, in view: CandidateTextView, reload: Bool = true) {
        guard toolbar.formatting != on || (on && view.inputView == nil) else { return }
        toolbar.formatting = on
        // As tall as the system keyboard below the toolbar, when known.
        let accessory = view.inputAccessoryView?.bounds.height ?? 0
        let height = keyboardHeight.map { max($0 - accessory, 216) } ?? 291
        view.inputView = on ? FormattingKeyboardHost(model: toolbar, height: height) : nil
        guard reload else { return }
        if view.isFirstResponder {
            view.reloadInputViews()
        } else {
            view.becomeFirstResponder()
        }
        keepCaretInView(view, settled: false)
    }

    /// The keys below the text changed (the system keyboard, the
    /// formatting keyboard, a hardware keyboard's bare toolbar, the link
    /// chip) or a command changed the caret's line: the room below the
    /// text follows the toolbar, and the caret's whole line is scrolled
    /// back above it, once the text has its new size. Unsettled, it waits
    /// for this turn's layout to finish first.
    func keepCaretInView(_ view: CandidateTextView, settled: Bool = true) {
        guard settled else {
            DispatchQueue.main.async { [weak self, weak view] in
                guard let self, let view else { return }
                view.window?.layoutIfNeeded()
                self.keepCaretInView(view)
            }
            return
        }
        updateToolbarRoom(view)
        scrollCaretLineIntoView(view)
    }

    /// Scrolls the caret's whole line into the text left in view above the
    /// room below it.
    func scrollCaretLineIntoView(_ view: CandidateTextView, animated: Bool = !UIAccessibility.isReduceMotionEnabled) {
        guard view.isFirstResponder, view.isEditMode, let end = view.selectedTextRange?.end else { return }
        let caret = view.caretRect(for: end)
        guard !caret.isNull, !caret.isInfinite else { return }
        // The line, not only the caret: its spacing and a little air.
        let line = caret.insetBy(dx: 0, dy: -Space.xs)
        let insets = view.adjustedContentInset
        let top = view.contentOffset.y + insets.top
        let bottom = view.contentOffset.y + view.bounds.height - insets.bottom
        var offset = view.contentOffset.y
        if line.maxY > bottom {
            offset += line.maxY - bottom
        } else if line.minY < top {
            offset -= top - line.minY
        }
        offset = min(max(offset, -insets.top), max(-insets.top, view.contentSize.height + insets.bottom - view.bounds.height))
        guard abs(offset - view.contentOffset.y) > 0.5 else { return }
        view.setContentOffset(CGPoint(x: view.contentOffset.x, y: offset), animated: animated)
    }

    /// The room below the text: as much of the text view as the floating
    /// toolbar covers, plus air for its glass edge, while the keyboard or
    /// a hardware keyboard's toolbar shows in edit mode; none otherwise.
    /// UIKit keeps a typed caret above it too.
    func updateToolbarRoom(_ view: CandidateTextView) {
        guard view.isEditMode, view.isFirstResponder, let host = view.inputAccessoryView, let window = view.window,
              let hostWindow = host.window, !host.isHidden else {
            setToolbarRoom(0, in: view)
            return
        }
        let toolbar = host.convert(host.bounds, to: hostWindow.screen.coordinateSpace)
        let text = view.convert(view.bounds, to: window.screen.coordinateSpace)
        let covered = max(0, text.maxY - toolbar.minY)
        setToolbarRoom(covered + Self.toolbarAir, in: view)
    }

    /// Air between the last line in view and the toolbar's capsule, past
    /// its soft glass edge.
    static let toolbarAir: CGFloat = Space.s

    func setToolbarRoom(_ room: CGFloat, in view: UIScrollView) {
        guard abs(room - toolbarRoom) > 0.5 else { return }
        view.contentInset.bottom += room - toolbarRoom
        view.verticalScrollIndicatorInsets.bottom += room - toolbarRoom
        toolbarRoom = room
    }

    /// The link sheet for the link at the caret, or for the selection: Done
    /// makes it one edit at the selection the sheet opened with.
    func editLink(in view: CandidateTextView) {
        guard view.isEditMode, let actions = parent.edit else { return }
        refreshToolbar(view)
        let selection = view.selectedRange
        let text = view.textStorage.string as NSString
        let link = toolbar.state?.link
        let selected = selection.length > 0 && NSMaxRange(selection) <= text.length ? text.substring(with: selection) : ""
        let draft = LinkDraft(
            text: link?.text ?? selected, url: link?.url ?? "", editing: link != nil,
            finish: { [weak self, weak view] linkText, url in
                guard let self, let view, view.isEditMode else { return }
                let length = view.textStorage.length
                let location = min(selection.location, length)
                view.selectedRange = NSRange(location: location, length: min(selection.length, length - location))
                self.bridge.command(in: view) { text, range in
                    editorSetLink(text: text, selection: range, linkText: linkText, url: url)
                }
                self.refreshToolbar(view)
            },
            closed: { [weak view] in
                guard let view, view.isEditMode, !view.isFirstResponder else { return }
                view.becomeFirstResponder()
            })
        actions.editLink(draft)
    }

    /// The selection moved or the text changed: the toolbar asks the core
    /// once this turn of the run loop is over.
    func toolbarChanged(_ view: CandidateTextView) {
        guard view.isEditMode, !toolbarPending else { return }
        toolbarPending = true
        DispatchQueue.main.async { [weak self, weak view] in
            guard let self, let view else { return }
            self.toolbarPending = false
            self.refreshToolbar(view)
        }
    }

    /// The toolbar's state for the selection now (`editor_caret_state`),
    /// and Undo and Redo as the core has them.
    func refreshToolbar(_ view: CandidateTextView) {
        guard view.isEditMode, let actions = parent.edit else { return }
        var text = view.textStorage.string
        text.makeContiguousUTF8()
        let string = text as NSString
        let selected = view.selectedRange.clamped(to: string.length)
        let state = editorCaretState(
            text: text, selection: TypingBridge.sourceRange(selected, in: string).textRange, rendered: parent.rendered)
        if toolbar.state != state { toolbar.state = state }
        let canUndo = actions.canUndo(), canRedo = actions.canRedo()
        if toolbar.canUndo != canUndo { toolbar.canUndo = canUndo }
        if toolbar.canRedo != canRedo { toolbar.canRedo = canRedo }
    }
}
