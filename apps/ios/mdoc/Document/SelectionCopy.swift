import MdocFFI
import UIKit
import UniformTypeIdentifiers

/// A selection copied by hand (#1209, ADR 0109 as amended): the user's own
/// copy, never blocked. Its menu says what it holds, as the core's
/// `check_selection` finds it, and a copy not known to be clean is marked
/// as a restored reply's copy is (`RestoredPasteboard`). The menu, its
/// order and the marking are decided here; the text view shows them.
enum SelectionCopy {
    /// A copy item of a selection's menu.
    enum Item: Equatable {
        /// A clean selection: a plain copy.
        case copy
        /// Proposals in it: they are replaced, then it copies.
        case replaceAndCopy(Int)
        /// Names in it not replaced, or a document never scanned: it
        /// copies as written, marked.
        case copyWithNames

        var title: String {
            switch self {
            case .copy: String(localized: "Copy")
            case .replaceAndCopy(let count): String(localized: "Replace \(count) and copy")
            case .copyWithNames: String(localized: "Copy with names")
            }
        }

        var identifier: String {
            switch self {
            case .copy: "copy-selection"
            case .replaceAndCopy: "replace-and-copy"
            case .copyWithNames: "copy-with-names"
            }
        }

        var image: UIImage? {
            switch self {
            case .copy, .copyWithNames: UIImage(systemName: "doc.on.doc")
            case .replaceAndCopy: UIImage(systemName: "eye.slash")
            }
        }
    }

    /// The copy items, in the menu's order, for the core's answer; nil is
    /// a check that failed (the document changed, say), which still copies,
    /// with names and marked.
    static func items(for check: SelectionCheck?) -> [Item] {
        switch check {
        case .clean?: [.copy]
        case .notClean(let proposals, _, _)? where proposals > 0: [.replaceAndCopy(Int(proposals)), .copyWithNames]
        case .notClean?, .neverScanned?, nil: [.copyWithNames]
        }
    }

    /// Whether a copy is marked: unless a check made for it says Clean.
    static func marked(_ check: SelectionCheck?) -> Bool {
        if case .clean? = check { return false }
        return true
    }

    /// A selection of a text view in the document's UTF-8 source: `range`
    /// in the view's UTF-16 offsets, plus `base`, a table cell's place in
    /// the text. Each end that falls inside a character (half an emoji's
    /// surrogate pair) moves to its end.
    static func sourceRange(_ range: NSRange, base: Int = 0, in offsets: TextOffsets) -> SourceRange {
        offsets.sourceRange(NSRange(location: base + range.location, length: range.length))
    }

    /// The source's text in `range`, as written; nil for a range outside
    /// the text or inside a character.
    static func text(_ range: SourceRange, in markdown: String) -> String? {
        let utf8 = markdown.utf8
        guard range.start >= 0, range.start <= range.end, range.end <= utf8.count else { return nil }
        let start = utf8.index(utf8.startIndex, offsetBy: range.start)
        let end = utf8.index(start, offsetBy: range.length)
        guard start.samePosition(in: markdown.unicodeScalars) != nil,
              end.samePosition(in: markdown.unicodeScalars) != nil else { return nil }
        return String(markdown.unicodeScalars[start..<end])
    }

    /// Puts a hand copy on `pasteboard`: an ordinary copy when clean, else
    /// marked, on this device only and for ten minutes.
    @MainActor
    static func write(_ text: String, marked: Bool, to pasteboard: UIPasteboard = .general) {
        if marked {
            RestoredPasteboard.copy(text, to: pasteboard)
        } else {
            pasteboard.setItems([[UTType.utf8PlainText.identifier: text]], options: [:])
        }
    }
}
