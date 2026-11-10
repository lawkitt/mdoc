import MdocFFI
import UIKit
import os

/// Times edit mode's work per keystroke (ADR 0109's budgets): "Restyle"
/// (under 8 ms) and "Structural", a structural key's core call (under 16 ms).
let editingSignposter = OSSignposter(subsystem: "io.github.lawkitt.mdoc", category: "Editing")

/// A key the editor core decides (ADR 0109): Return, and ⌫ or ⌦ of one
/// character with nothing selected. Any other change is plain typing.
enum StructuralKey: Equatable {
    case enter
    case backspace
    case deleteForward

    /// The key UIKit's change of `range` to `text` is, with `selection` in
    /// `text`; nil for plain typing. Holding ⌫ deletes words, which stays
    /// UIKit's.
    init?(range: NSRange, text: String, selection: NSRange, in mirror: NSString) {
        if text == "\n" {
            self = .enter
            return
        }
        guard text.isEmpty, selection.length == 0, range.length > 0, NSMaxRange(range) <= mirror.length else { return nil }
        if NSMaxRange(range) == selection.location,
           mirror.rangeOfComposedCharacterSequence(at: range.location) == range {
            self = .backspace
        } else if range.location == selection.location,
                  mirror.rangeOfComposedCharacterSequence(at: range.location) == range {
            self = .deleteForward
        } else {
            return nil
        }
    }
}

/// What a structural key does to the text view.
enum KeyOutcome: Equatable {
    /// UIKit's own change is the core's: let it happen.
    case native
    /// Nothing: the key would join a line to a table's row.
    case ignored
    /// The caret moves; the text stays.
    case select(NSRange)
    /// One replacement in the view's UTF-16 offsets, then the selection.
    case replace(range: NSRange, text: String, selection: NSRange)
}

/// Asks the editor core (`mdoc-ffi`'s `editor_*` calls) what Return, ⌫ and
/// ⌦ do, and turns its UTF-8 edits into one replacement in the view's
/// UTF-16 offsets, so the key is one core edit on the typing path.
enum StructuralKeys {
    /// `key` in `text` (the core's text) with `selection`; `native` is the
    /// range UIKit would change. `rendered` is live preview. A deletion that
    /// would reach into `protected` (the drawn tables) does nothing.
    static func outcome(
        _ key: StructuralKey, text: NSString, selection: NSRange, native: NSRange, rendered: Bool,
        protected: [NSRange] = []
    ) -> KeyOutcome {
        var source = text as String
        source.makeContiguousUTF8()
        let selected = TypingBridge.sourceRange(selection, in: text).textRange
        // The core call and turning its edits into one replacement.
        let interval = editingSignposter.beginInterval("Structural", id: editingSignposter.makeSignpostID())
        defer { editingSignposter.endInterval("Structural", interval) }
        let change: EditorChange? = switch key {
        case .enter: editorEnter(text: source, selection: selected, rendered: rendered)
        case .backspace: editorBackspace(text: source, selection: selected, rendered: rendered)
        case .deleteForward: editorDeleteForward(text: source, selection: selected, rendered: rendered)
        }
        guard let change else { return .native }
        let outcome = replacement(for: change, in: source)
        guard key != .enter, case .replace(let range, let text, _) = outcome else { return outcome }
        if protected.contains(where: { NSIntersectionRange($0, range).length > 0 || NSIntersectionRange($0, native).length > 0 }) {
            return .ignored
        }
        if text.isEmpty && range == native { return .native }
        return outcome
    }

    /// The core's `change` to `source` as one replacement in UTF-16
    /// offsets, then the selection; a change without edits moves the caret.
    static func replacement(for change: EditorChange, in source: String) -> KeyOutcome {
        let map = UTF8Map(source)
        guard let first = change.edits.first, let last = change.edits.last else {
            return .select(map.range(of: SourceRange(change.selection)))
        }
        // The edits as one replacement: what lies between them stays.
        let bytes = Array(source.utf8)
        let start = Int(first.range.start), end = Int(last.range.end)
        var replacement: [UInt8] = []
        var at = start
        for edit in change.edits {
            replacement += bytes[at..<Int(edit.range.start)]
            replacement += Array(edit.text.utf8)
            at = Int(edit.range.end)
        }
        let text = String(decoding: replacement, as: UTF8.self)
        let range = map.range(of: SourceRange(start: start, end: end))
        let edited = String(decoding: Array(bytes[..<start]) + replacement + Array(bytes[end...]), as: UTF8.self)
        return .replace(range: range, text: text, selection: UTF8Map(edited).range(of: SourceRange(change.selection)))
    }
}

/// UTF-8 offsets of one text as UTF-16 offsets, walked once.
struct UTF8Map {
    private let text: String

    init(_ text: String) {
        self.text = text
    }

    /// The UTF-16 offset of a UTF-8 offset, pulled back onto a character.
    func utf16(_ offset: Int) -> Int {
        let utf8 = text.utf8
        var index = utf8.index(utf8.startIndex, offsetBy: min(max(offset, 0), utf8.count))
        while index.samePosition(in: text.unicodeScalars) == nil {
            index = utf8.index(before: index)
        }
        return text.utf16.distance(from: text.utf16.startIndex, to: index)
    }

    func range(of source: SourceRange) -> NSRange {
        let start = utf16(source.start)
        return NSRange(location: start, length: utf16(source.end) - start)
    }
}
