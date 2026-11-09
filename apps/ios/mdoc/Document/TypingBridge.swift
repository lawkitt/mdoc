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

    /// The view shows `text`, which is the core's.
    func reset(to text: String) {
        mirror = NSMutableString(string: text)
        announced.removeAll()
        composing = false
        changing = false
        caretAfterEdit = nil
    }

    // MARK: From the text view's delegate

    /// `textView(_:shouldChangeTextIn:replacementText:)`: notes the edit,
    /// and says whether UIKit applies it.
    func willChange(_ view: UITextView, range: NSRange, text: String) -> Bool {
        if text == "\n", range.length == 0, view.markedTextRange == nil, !returnKey(in: view, at: range) {
            return false
        }
        changing = true
        if view.markedTextRange != nil { composing = true }
        announced.append((range, text))
        return true
    }

    /// Return (ADR 0109): a plain newline in this slice. The editor core's
    /// list continuation (`wsl/editor-core-ffi`) takes over here: ask it for
    /// the edit Return makes at `range`, apply that through the text view's
    /// `replace(_:withText:)` (which reaches `willChange` as an ordinary
    /// edit), and return false so UIKit inserts nothing itself.
    func returnKey(in view: UITextView, at range: NSRange) -> Bool {
        true
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
        let edits: [(SourceRange, String)]
        if let planned {
            edits = planned
        } else {
            guard let change = Self.difference(from: mirror, to: current) else { return }
            edits = [(Self.sourceRange(change.range, in: mirror), change.text)]
            mirror.replaceCharacters(in: change.range, with: change.text)
        }
        var last: Typed?
        for (range, text) in edits {
            guard let typed = send(range, text) else {
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
        }
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
    private func plan(against current: NSString) -> [(SourceRange, String)]? {
        guard !announced.isEmpty, let trial = mirror.mutableCopy() as? NSMutableString else { return nil }
        var edits: [(SourceRange, String)] = []
        for (range, text) in announced {
            guard NSMaxRange(range) <= trial.length else { return nil }
            edits.append((Self.sourceRange(range, in: trial), text))
            trial.replaceCharacters(in: range, with: text)
        }
        guard trial.isEqual(to: current as String) else { return nil }
        mirror = trial
        return edits.filter { !($0.0.length == 0 && $0.1.isEmpty) }
    }

    /// The one stretch where `old` and `new` differ: its range in `old` and
    /// what `new` has there; nil when they are equal. Never splits a
    /// surrogate pair.
    static func difference(from old: NSString, to new: NSString) -> (range: NSRange, text: String)? {
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
    static func sourceRange(_ range: NSRange, in text: NSString) -> SourceRange {
        let start = utf8Count(text, NSRange(location: 0, length: range.location))
        return SourceRange(start: start, end: start + utf8Count(text, range))
    }

    private static func utf8Count(_ text: NSString, _ range: NSRange) -> Int {
        guard range.length > 0 else { return 0 }
        var used: CFIndex = 0
        CFStringGetBytes(
            text as CFString, CFRange(location: range.location, length: range.length),
            CFStringBuiltInEncodings.UTF8.rawValue, 0, false, nil, 0, &used)
        return used
    }
}
