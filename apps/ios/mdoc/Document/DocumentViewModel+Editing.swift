import Foundation
import MdocFFI

/// Typing between the core's snapshots (ADR 0109): what the last typed edit
/// answered, until a snapshot is newer, and the pending discovery.
struct TypingState {
    /// The text revision the last typed edit left; nil before any typing.
    var revision: UInt64?
    /// Undo and Redo as the last typed edit left them; nil when the
    /// snapshot is newer.
    var canUndo: Bool?
    var canRedo: Bool?
    /// The one-line hint about an edited alias has shown in this edit
    /// session.
    var hintedAlias = false
    /// Discovery, once typing pauses.
    var discovery: Task<Void, Never>?
    /// Commits marked text in the view and sends what it has not sent yet:
    /// Done runs it first (the edit view sets it).
    var flush: (@MainActor () -> Void)?
    /// Leaves edit mode in the view, keyboard and all, before the screen
    /// updates: hiding the keyboard inside SwiftUI's update looped there
    /// (Done runs it after `flush`; the view sets it).
    var leave: (@MainActor () -> Void)?

    /// A snapshot newer than any typed edit came: it says what Undo does.
    mutating func settled(at snapshot: DocumentSnapshot) {
        revision = snapshot.textRevision
        canUndo = nil
        canRedo = nil
    }
}

/// Edit mode (ADR 0109): Edit on the document screen (⌘E), typing into the
/// core as "Typing" steps in the one undo line, discovery and the save once
/// typing pauses, and Done. The text is edited in live preview (markers
/// hidden but the caret's) or with Show Markdown on.
extension DocumentViewModel {
    /// Why Edit waits: OCR or Find names runs for this document, and an
    /// edit would make the core throw its result away.
    var editBlockedReason: String? {
        guard let job = runningJob else { return nil }
        return job.kind == .scan
            ? String(localized: "Finding names. Edit when it finishes.")
            : String(localized: "Recognizing. Edit when it finishes.")
    }

    /// The document has been read, so it has text to edit, even none: a
    /// document emptied by editing can be typed into again.
    var hasEditableText: Bool {
        snapshot.phase != nil
    }

    /// Edit: the review bar closes and Identities hide (ADR 0110). The
    /// text view keeps its place; the keyboard comes up on a selection, or
    /// waits for a tap in the text.
    func enterEditMode() {
        guard hasEditableText, !isEditing else { return }
        if let reason = editBlockedReason {
            say(reason, symbol: "hourglass")
            return
        }
        if isReviewing { endReview() }
        toast = nil
        typing = TypingState(revision: snapshot.textRevision)
        if pane != .text && !layout.isRegular { show(.text) }
        hideIdentitiesForEditing()
        isEditing = true
    }

    /// Done: back to review. Discovery runs now rather than after the
    /// pause, and the typing is saved then rather than after it.
    func doneEditing() {
        guard isEditing else { return }
        linkDraft = nil
        // Marked (IME) text is committed and sent while typing still counts.
        typing.flush?()
        typing.leave?()
        // The text as typed, before review shows it: the snapshot is older.
        applyReview(session.snapshot())
        isEditing = false
        restoreIdentitiesAfterEditing()
        session.endTyping()
        typing.discovery?.cancel()
        typing.discovery = nil
        Task {
            await discoverNow()
            do {
                try session.saveChanges()
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    /// One committed edit from the text view: the core's answer, or nil
    /// when it refused (the view then syncs again).
    func typed(_ range: SourceRange, _ text: String) -> Typed? {
        edited(label: String(localized: "Typing")) { try session.type(range, text) }
    }

    /// One editor command's edits (the toolbar, the formatting keyboard, a
    /// shortcut, the link sheet), as the view made them: one "Formatting"
    /// step. Nil when the core refused them.
    func formatted(_ edits: [EditorEdit]) -> Typed? {
        edited(label: String(localized: "Formatting")) { try session.format(edits) }
    }

    /// What follows each edit in edit mode: the undo step mirrored, the
    /// alias hint, discovery once typing pauses.
    private func edited(label: String, _ edit: () throws -> Typed) -> Typed? {
        guard isEditing else { return nil }
        let typed: Typed
        do {
            typed = try edit()
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
        typing.revision = typed.textRevision
        typing.canUndo = typed.canUndo
        typing.canRedo = typed.canRedo
        // Each new step mirrors on the window's undo stack, as a review
        // step does (ADR 0098).
        if typed.newStep { registerSystemUndo(label: label) }
        if typed.brokeAlias && !typing.hintedAlias {
            typing.hintedAlias = true
            say(String(localized: "You edited an alias: that mention no longer restores."), symbol: "info.circle")
        }
        scheduleDiscovery()
        return typed
    }

    /// The caret moved away: the next keystroke starts a new undo step.
    func caretMoved() {
        session.endTyping()
    }

    /// The core's text, for the view to sync to when the two differ. Not
    /// shown as a snapshot: the view is about to change the core's text.
    func coreText() -> String {
        session.snapshot().markdown
    }

    /// Discovery once typing pauses (ADR 0109); a keystroke before then
    /// starts the wait again.
    private func scheduleDiscovery() {
        typing.discovery?.cancel()
        typing.discovery = Task { [weak self] in
            // timing: typing pauses this long before discovery (ADR 0109)
            try? await Task.sleep(for: .seconds(1))
            guard !Task.isCancelled else { return }
            await self?.discoverNow()
        }
    }

    /// Runs discovery off the main thread and shows its snapshot, unless
    /// typing changed the text meanwhile: the next pause runs it again. A
    /// new document's title follows its first line first (ADR 0109).
    func discoverNow() async {
        let session = session
        let revision = typing.revision
        let (retitled, found) = await Task.detached(priority: .userInitiated) {
            let retitled = (try? session.followFirstLine()) != nil
            return (retitled, try? session.discover())
        }.value
        // The lists show the new title now, not at the save.
        if retitled { onChange() }
        guard let found, revision == typing.revision, found.textRevision == revision else { return }
        applyReview(found)
    }

    /// Undo and Redo in edit mode name the step they reached (ADR 0109).
    func announceEditUndo(_ undid: Bool) {
        guard isEditing else { return }
        let step = undid ? snapshot.redoStep : snapshot.undoStep
        say(
            undid
                ? step.map { String(localized: "Undid \($0.label)") } ?? String(localized: "Undone")
                : step.map { String(localized: "Redid \($0.label)") } ?? String(localized: "Redone"),
            symbol: undid ? "arrow.uturn.backward" : "arrow.uturn.forward")
    }
}
