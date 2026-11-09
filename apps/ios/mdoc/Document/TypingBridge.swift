import MdocFFI
import UIKit

/// Edit mode's typing path (ADR 0109): the text view applies typing
/// natively, and each committed change reaches the core as edits in its
/// UTF-8 offsets (`DocumentSession.type`), where they join the undo line as
/// "Typing" steps. Marked text (IME) never reaches the core: a change made
/// while text is marked waits until it is committed.
///
/// It keeps `mirror`, the text as the core has it, in the view's UTF-16
/// units. The edits UIKit announces (`willChange`) are checked against it:
/// applied in order they must give the view's text, or the change is found
/// by comparing the two instead (a committed IME word, a find replacement,
/// a drop). Whatever the core answers, its length is checked too; on any
/// doubt the bridge asks to sync again (`onResync`). So the core's text,
/// which is what saves, stays the view's string (ADR 0030, ADR 0060).
/// One edit the core took: its range in the text before it, in UTF-16 and
/// UTF-8, and the text that replaced it.
struct AppliedEdit: Equatable {
    var range: NSRange
    var source: SourceRange
    var text: String

    /// What it replaced with, in UTF-16 and UTF-8 units.
    var length: Int { (text as NSString).length }
    var sourceLength: Int { text.utf8.count }
}

@MainActor
final class TypingBridge {
    /// Sends one edit to the core; nil when the core refused it.
    var send: (SourceRange, String) -> Typed? = { _, _ in nil }
    /// The caret moved away from where typing left it: the core starts the
    /// next typing step afresh.
    var endTyping: () -> Void = {}
    /// The core's text and the view's differ: the owner fetches the core's
    /// text and calls `resync(core:view:)`.
    var onResync: () -> Void = {}
    /// The core took the view's text as `Typed.text_revision`.
    var onTyped: (Typed) -> Void = { _ in }
    /// After a sync sent edits: each, in order, in the offsets of the text
    /// before it (live preview restyles around them).
    var onApplied: ([AppliedEdit]) -> Void = { _ in }
    /// Return, ⌫ and ⌦ as the editor core decides them (ADR 0109): what the
    /// key does to `mirror` with the view's selection, where UIKit would
    /// change `range`.
    var structural: (StructuralKey, _ range: NSRange, _ selection: NSRange) -> KeyOutcome = { _, _, _ in .native }

    /// The text as the core has it.
    private(set) var mirror = NSMutableString()
    /// What UIKit announced for the change in progress, oldest first.
    private var announced: [(range: NSRange, text: String)] = []
    /// Text was marked in a change since the last sync: the announced edits
    /// are composition steps, not the committed text.
    private var composing = false
    /// Between `willChange` and `didChange`: a selection change then is the
    /// edit's own.
    private var changing = false
    /// The selection the last synced edit left.
    private var caretAfterEdit: NSRange?
    /// How many edits reached the core: a test hook.
    private(set) var sent = 0
    /// Applying a structural key's edit: its own change is plain.
    private var applying = false
    /// Between `willChange` and `didChange`: a selection change now is the
    /// edit's own, not the reader's.
    var isChanging: Bool { changing || applying }

    /// The view shows `text`, which is the core's. A restyle that kept the
    /// text keeps where typing left the caret, so a move after it still
    /// ends the typing step.
    func reset(to text: String) {
        let kept = mirror.isEqual(to: text)
        announced.removeAll()
        composing = false
        changing = false
        if kept { return }
        mirror = NSMutableString(string: text)
        caretAfterEdit = nil
    }

    // MARK: From the text view's delegate

    /// `textView(_:shouldChangeTextIn:replacementText:)`: notes the edit,
    /// and says whether UIKit applies it.
    func willChange(_ view: UITextView, range: NSRange, text: String) -> Bool {
        if !applying, view.markedTextRange == nil,
           let key = StructuralKey(range: range, text: text, selection: view.selectedRange, in: mirror) {
            let outcome = structural(key, range, view.selectedRange)
            if outcome != .native {
                apply(outcome, in: view)
                return false
            }
        }
        changing = true
        if view.markedTextRange != nil { composing = true }
        announced.append((range, text))
        return true
    }

    /// An edit of the text storage's characters, as UIKit processes it.
    /// Outside a change the delegate announced (the find bar's Replace and
    /// Replace All), it is noted as that change's edit: the range it had
    /// before and the text it holds now.
    func storageEdited(_ storage: NSTextStorage, in view: UITextView) {
        guard !changing else { return }
        if view.markedTextRange != nil { composing = true }
        let edited = storage.editedRange
        let before = NSRange(location: edited.location, length: edited.length - storage.changeInLength)
        guard edited.location != NSNotFound, before.length >= 0 else { return }
        announced.append((before, (storage.string as NSString).substring(with: edited)))
    }

    /// A structural key's edit, made through the text view's own
    /// `replace(_:withText:)` (which reaches `willChange` as an ordinary
    /// edit) so it is one core edit on the typing path and its undo line,
    /// then the core's selection.
    private func apply(_ outcome: KeyOutcome, in view: UITextView) {
        applying = true
        defer { applying = false }
        switch outcome {
        case .native, .ignored:
            return
        case .select(let selection):
            view.selectedRange = selection
        case .replace(let range, let text, let selection):
            guard let start = view.position(from: view.beginningOfDocument, offset: range.location),
                  let end = view.position(from: start, offset: range.length),
                  let textRange = view.textRange(from: start, to: end) else { return }
            view.replace(textRange, withText: text)
            if NSMaxRange(selection) <= view.textStorage.length { view.selectedRange = selection }
            didChange(view)
        }
    }

    /// `textViewDidChange`, and after any change made through the view's own
    /// API: sends what changed since the last sync, once nothing is marked.
    /// Calling it again with nothing new sends nothing.
    func didChange(_ view: UITextView) {
        changing = false
        guard view.markedTextRange == nil else {
            composing = true
            announced.removeAll()
            return
        }
        let current = view.textStorage.string as NSString
        let planned = composing ? nil : plan(against: current)
        announced.removeAll()
        composing = false
        let edits: [AppliedEdit]
        if let planned {
            edits = planned
        } else {
            guard let change = Self.difference(from: mirror, to: current) else { return }
            edits = [AppliedEdit(range: change.range, source: Self.sourceRange(change.range, in: mirror), text: change.text)]
            mirror.replaceCharacters(in: change.range, with: change.text)
        }
        var last: Typed?
        for edit in edits {
            guard let typed = send(edit.source, edit.text) else {
                onResync()
                return
            }
            sent += 1
            last = typed
            onTyped(typed)
        }
        caretAfterEdit = view.selectedRange
        if let last, Int(last.utf16Len) != mirror.length || mirror.length != current.length {
            onResync()
            return
        }
        if !edits.isEmpty { onApplied(edits) }
    }

    /// `textViewDidChangeSelection`: a caret moved by the reader, not by an
    /// edit, ends the typing step.
    func selectionChanged(_ view: UITextView) {
        guard !changing, let caret = caretAfterEdit else { return }
        if view.selectedRange != caret {
            caretAfterEdit = nil
            endTyping()
        }
    }

    /// After `onResync`: the core has `core`, the view shows `view`. Sends
    /// the one edit between them; false when the core refuses it too, and
    /// the view should show the core's text.
    func resync(core: String, view: String) -> Bool {
        mirror = NSMutableString(string: core)
        announced.removeAll()
        composing = false
        guard let change = Self.difference(from: mirror, to: view as NSString) else { return true }
        let range = Self.sourceRange(change.range, in: mirror)
        guard let typed = send(range, change.text) else { return false }
        onTyped(typed)
        mirror.replaceCharacters(in: change.range, with: change.text)
        return Int(typed.utf16Len) == mirror.length
    }

    // MARK: Edits

    /// The announced edits applied to the mirror, as UTF-8 edits, when
    /// they give `current`; nil when they don't, and the mirror is as it was.
    private func plan(against current: NSString) -> [AppliedEdit]? {
        guard !announced.isEmpty, let trial = mirror.mutableCopy() as? NSMutableString else { return nil }
        var edits: [AppliedEdit] = []
        for (range, text) in announced {
            guard NSMaxRange(range) <= trial.length else { return nil }
            edits.append(AppliedEdit(range: range, source: Self.sourceRange(range, in: trial), text: text))
            trial.replaceCharacters(in: range, with: text)
        }
        guard trial.isEqual(to: current as String) else { return nil }
        mirror = trial
        return edits.filter { !($0.range.length == 0 && $0.text.isEmpty) }
    }

    /// The one stretch where `old` and `new` differ: its range in `old` and
    /// what `new` has there; nil when they are equal. Never splits a
    /// surrogate pair.
    nonisolated static func difference(from old: NSString, to new: NSString) -> (range: NSRange, text: String)? {
        let oldLength = old.length, newLength = new.length
        var prefix = 0
        let shorter = min(oldLength, newLength)
        while prefix < shorter, old.character(at: prefix) == new.character(at: prefix) { prefix += 1 }
        if prefix == oldLength && prefix == newLength { return nil }
        if prefix > 0, UTF16.isLeadSurrogate(old.character(at: prefix - 1)) { prefix -= 1 }
        var suffix = 0
        while suffix < shorter - prefix,
              old.character(at: oldLength - 1 - suffix) == new.character(at: newLength - 1 - suffix) { suffix += 1 }
        if suffix > 0, UTF16.isTrailSurrogate(old.character(at: oldLength - suffix)) { suffix -= 1 }
        let range = NSRange(location: prefix, length: oldLength - prefix - suffix)
        let text = new.substring(with: NSRange(location: prefix, length: newLength - prefix - suffix))
        return (range, text)
    }

    /// `range` of `text` in UTF-8 offsets, counted without copying.
    nonisolated static func sourceRange(_ range: NSRange, in text: NSString) -> SourceRange {
        let start = utf8Count(text, NSRange(location: 0, length: range.location))
        return SourceRange(start: start, end: start + utf8Count(text, range))
    }

    nonisolated private static func utf8Count(_ text: NSString, _ range: NSRange) -> Int {
        guard range.length > 0 else { return 0 }
        var used: CFIndex = 0
        CFStringGetBytes(
            text as CFString, CFRange(location: range.location, length: range.length),
            CFStringBuiltInEncodings.UTF8.rawValue, 0, false, nil, 0, &used)
        return used
    }
}
