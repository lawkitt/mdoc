import MdocFFI
import UIKit

/// Edit mode's restyling (ADR 0109): each keystroke restyles only the block
/// it edited and the lines the caret left and entered, in place, never the
/// whole text. A block is the lines between blank lines (the whole fenced
/// code block in code), which the core styles on its own exactly as it
/// styles them in the whole text (`markdown_block_styling`).
///
/// In live preview it also keeps the phone's reveal rule: the markers of
/// the construct the caret touches and its line's prefix show dimmed, and a
/// fence or rule on the caret's line shows its source (`editor_visible_markers`).
@MainActor
final class LivePreview {
    /// What the styler needs from the view.
    struct Look {
        var traits: UITraitCollection
        /// Markers hidden and blocks drawn (Show Markdown off).
        var rendered: Bool
        /// The text column's width, for tables; nil before layout.
        var width: CGFloat?
    }

    /// The candidates as the view shows them: the last snapshot's, moved by
    /// each edit since, and dropped where an edit ran into one, as the core
    /// does, until the pause's snapshot brings the core's.
    var candidates: [Candidate] = []
    /// The text's fenced code blocks at the last restyle, in UTF-8.
    private var fences: [SourceRange]?
    /// The markers shown and the lines the selection is on, in UTF-8.
    private(set) var visible: [SourceRange] = []
    private(set) var caretLines: SourceRange?
    /// The caret's line, in both modes, while nothing is selected: a blank
    /// one keeps the body's height (ADR 0110), in UTF-8.
    private(set) var caretLine: SourceRange?
    /// How many keystrokes or caret moves restyled blocks: a test hook.
    private(set) var restyles = 0
    /// The UTF-16 range the last restyle styled: a test hook.
    private(set) var lastStyled: [NSRange] = []

    /// The snapshot was shown in full.
    func reset(candidates: [Candidate]) {
        self.candidates = candidates
        fences = nil
    }

    /// The core took `edits`: candidates, grids and the reveal move with
    /// them, and the block around the edit restyles with the lines the
    /// caret left and entered; a table typed into keeps its grid and
    /// columns, and only what changed in it is laid out again. False when
    /// more than a block changed (a fence typed or removed, several edits
    /// at once, an edit across a table's edge): the owner restyles the
    /// whole text.
    func edited(_ edits: [AppliedEdit], in view: CandidateTextView, look: Look) -> Bool {
        // The whole keystroke's restyle, as ADR 0109's budget counts it.
        let interval = editingSignposter.beginInterval("Restyle", id: editingSignposter.makeSignpostID())
        defer { editingSignposter.endInterval("Restyle", interval) }
        var intact = edits.count == 1
        for edit in edits {
            candidates = candidates.compactMap { candidate in
                Self.moved(candidate.range, by: edit).map { range in
                    var moved = candidate
                    moved.range = range
                    return moved
                }
            }
            fences = fences.map { $0.compactMap { Self.grown($0, by: edit) } }
            visible = visible.compactMap { Self.moved($0, by: edit) }
            caretLines = caretLines.flatMap { Self.grown($0, by: edit) }
            caretLine = caretLine.flatMap { Self.grown($0, by: edit) }
            for table in view.tableViews {
                let range = table.table.range
                if NSMaxRange(edit.range) <= range.location {
                    table.shift(by: edit.length - edit.range.length)
                } else if edit.range.location < NSMaxRange(range),
                          edit.range.location < range.location || NSMaxRange(edit.range) > NSMaxRange(range) {
                    // Across a table's edge (a selection deleted into it,
                    // the table deleted): more than its block may change.
                    intact = false
                }
            }
        }
        guard intact, let edit = edits.first else { return false }
        let text = Self.text(of: view)
        let (selection, lines) = Self.selection(in: view)
        let edited = SourceRange(start: edit.source.start, end: edit.source.start + edit.sourceLength)
        var ranges = [edited, lines]
        if let caretLines { ranges.append(caretLines) }
        if let caretLine { ranges.append(caretLine) }
        visible = look.rendered ? editorVisibleMarkers(text: text, selection: selection.textRange).map(SourceRange.init) : []
        caretLines = look.rendered ? lines : nil
        caretLine = selection.start == selection.end ? lines : nil
        return restyle(ranges, text: text, in: view, look: look, checkFences: true)
    }

    /// The selection moved: when what the caret reveals changed, the lines
    /// it left and entered restyle. Show Markdown reveals nothing, but a
    /// blank line the caret enters or leaves restyles there too.
    func selectionChanged(in view: CandidateTextView, look: Look, force: Bool = false) {
        let interval = editingSignposter.beginInterval("Restyle", id: editingSignposter.makeSignpostID())
        defer { editingSignposter.endInterval("Restyle", interval) }
        let text = Self.text(of: view)
        let (selection, lines) = Self.selection(in: view)
        let caret = selection.start == selection.end ? lines : nil
        var ranges = [lines]
        if let caretLine, caretLine != lines { ranges.append(caretLine) }
        if look.rendered {
            let shown = editorVisibleMarkers(text: text, selection: selection.textRange).map(SourceRange.init)
            guard force || shown != visible || lines != caretLines || caret != caretLine else { return }
            if let caretLines, caretLines != lines { ranges.append(caretLines) }
            visible = shown
            caretLines = lines
        } else {
            visible = []
            caretLines = nil
            guard caret != caretLine, [caret, caretLine].contains(where: { $0.map { Self.blank($0, in: text) } ?? false }) else {
                caretLine = caret
                return
            }
        }
        caretLine = caret
        restyle(ranges, text: text, in: view, look: look)
    }

    /// Whether `line` holds nothing but its line break.
    private static func blank(_ line: SourceRange, in text: String) -> Bool {
        let bytes = text.utf8
        guard line.start <= bytes.count, line.end <= bytes.count else { return false }
        let from = bytes.index(bytes.startIndex, offsetBy: line.start)
        let to = bytes.index(bytes.startIndex, offsetBy: line.end)
        return bytes[from..<to].allSatisfy { $0 == 0x0A || $0 == 0x0D }
    }

    /// The view's text, in UTF-8 once: each core call copies it.
    private static func text(of view: UITextView) -> String {
        var text = view.textStorage.string
        text.makeContiguousUTF8()
        return text
    }

    /// The selection and the whole lines it is on, in UTF-8.
    private static func selection(in view: UITextView) -> (SourceRange, SourceRange) {
        let text = view.textStorage.string as NSString
        let location = min(view.selectedRange.location, text.length)
        let selection = NSRange(location: location, length: min(view.selectedRange.length, text.length - location))
        return (TypingBridge.sourceRange(selection, in: text), TypingBridge.sourceRange(text.lineRange(for: selection), in: text))
    }

    /// Restyles the blocks holding `ranges`; false, restyling nothing, when
    /// the text's fenced code changed beyond them.
    @discardableResult
    private func restyle(_ ranges: [SourceRange], text: String, in view: CandidateTextView, look: Look, checkFences: Bool = false) -> Bool {
        var blocks: [MarkdownBlockStyling] = []
        for range in ranges.sorted(by: { $0.start < $1.start }) {
            if blocks.contains(where: { Int($0.range.start) <= range.start && range.end <= Int($0.range.end) }) { continue }
            let block = markdownBlockStyling(text: text, range: range.textRange)
            blocks.removeAll { block.range.start <= $0.range.start && $0.range.end <= block.range.end }
            blocks.append(block)
        }
        if let now = blocks.first?.fences.map(SourceRange.init) {
            if checkFences, let fences, fences != now {
                self.fences = now
                return false
            }
            fences = now
        }
        restyles += 1
        lastStyled = []
        for block in blocks { style(block, in: view, look: look) }
        return true
    }

    /// Styles one block as the whole text would be, with the reveal, and
    /// sets the attributes that differ; its grids are made again.
    private func style(_ block: MarkdownBlockStyling, in view: CandidateTextView, look: Look) {
        let storage = view.textStorage
        let range = NSRange(location: Int(block.utf16Start), length: Int(block.utf16Len))
        guard NSMaxRange(range) <= storage.length else { return }
        let window = (storage.string as NSString).substring(with: range)
        let base = Int(block.range.start), end = Int(block.range.end)
        func local(_ source: SourceRange) -> SourceRange? {
            guard source.start >= base, source.end <= end else { return nil }
            return SourceRange(start: source.start - base, end: source.end - base)
        }
        var styler = MarkdownStyler(traits: look.traits, selected: nil, scoped: [], rendered: look.rendered, width: look.width)
        styler.editing = true
        styler.caretLine = caretLine.flatMap(local)
        // Its tables keep their columns while their cells are typed into.
        styler.keptTables = Dictionary(
            view.tableViews.filter { NSLocationInRange($0.table.anchor, range) }
                .map { ($0.table.anchor - range.location, $0.table.shifted(by: -range.location)) },
            uniquingKeysWith: { first, _ in first })
        if look.rendered {
            styler.revealed = visible.compactMap(local)
                + (caretLines.flatMap(local).map { Self.shownLines(in: block.styling, caret: $0) } ?? [])
        }
        let candidates = candidates.compactMap { candidate -> Candidate? in
            guard let range = local(candidate.range) else { return nil }
            var moved = candidate
            moved.range = range
            return moved
        }
        let document = styler.document(markdown: window, styling: block.styling, candidates: candidates)
        Self.apply(document.text, at: range.location, in: storage)
        if look.rendered {
            view.replaceTables(in: range, with: document.tables.map { $0.shifted(by: range.location) })
        }
        lastStyled.append(range)
    }

    /// The lines the caret shows whole (ADR 0109), in `styling`'s offsets:
    /// a rule on the caret's lines, and both fences of a fenced code block
    /// the caret is in, so its fences can be reached and edited.
    static func shownLines(in styling: MarkdownStyling, caret: SourceRange) -> [SourceRange] {
        func touches(_ range: SourceRange) -> Bool { range.start < caret.end && caret.start <= range.end }
        var shown = styling.lines.filter { $0.kind == .rule }.map { SourceRange($0.range) }.filter(touches)
        let fences = styling.blocks.filter { $0.kind == .codeFence }.map { SourceRange($0.range) }
        for index in stride(from: 0, to: fences.count, by: 2) {
            let open = fences[index]
            let close = index + 1 < fences.count ? fences[index + 1] : nil
            let end = close?.end ?? (styling.lines.last.map { Int($0.range.end) } ?? open.end)
            if touches(SourceRange(start: open.start, end: max(end, open.end))) {
                shown.append(open)
                if let close { shown.append(close) }
            }
        }
        return shown
    }

    /// Sets `styled`'s attributes on `storage` from `location`, only where
    /// they differ, so TextKit lays out again only what changed. The text
    /// is the same.
    static func apply(_ styled: NSAttributedString, at location: Int, in storage: NSTextStorage) {
        storage.beginEditing()
        styled.enumerateAttributes(in: NSRange(location: 0, length: styled.length)) { attributes, run, _ in
            let target = NSRange(location: location + run.location, length: run.length)
            var current = NSRange()
            let old = storage.attributes(at: target.location, longestEffectiveRange: &current, in: target)
            if current != target || !NSDictionary(dictionary: old).isEqual(to: attributes) {
                storage.setAttributes(attributes, range: target)
            }
        }
        storage.endEditing()
    }

    /// `range` after `edit`: moved when the edit is before it, kept when
    /// after, nil when the edit runs into it (a candidate it changed).
    static func moved(_ range: SourceRange, by edit: AppliedEdit) -> SourceRange? {
        if range.end <= edit.source.start { return range }
        let delta = edit.sourceLength - edit.source.length
        if range.start >= edit.source.end { return SourceRange(start: range.start + delta, end: range.end + delta) }
        return nil
    }

    /// As `moved`, but an edit inside `range` grows or shrinks it; one
    /// across its edge drops it.
    static func grown(_ range: SourceRange, by edit: AppliedEdit) -> SourceRange? {
        if let moved = moved(range, by: edit) { return moved }
        guard edit.source.start >= range.start, edit.source.end <= range.end else { return nil }
        return SourceRange(start: range.start, end: range.end + edit.sourceLength - edit.source.length)
    }
}

/// Where a caret may rest in live preview (ADR 0109): never in a hidden
/// marker run, judged on the text as drawn when it moved.
enum CaretSnap {
    /// `selection` moved off `previous`, snapped out of a hidden run: to the
    /// run's end, where desktop's click maps a hidden run (before the next
    /// character shown: after an opening marker, after a closing one, after
    /// a line's prefix). A run of whole hidden lines (a table, a folded
    /// line) is stepped over in the direction the caret travelled, so arrow
    /// keys skip a table and a tap on one lands after it. A selection is
    /// left as it is: its lines show every marker.
    static func snap(_ selection: NSRange, from previous: NSRange?, in storage: NSAttributedString) -> NSRange {
        guard selection.length == 0, selection.location < storage.length,
              storage.attribute(.mdocHidden, at: selection.location, effectiveRange: nil) != nil else { return selection }
        var run = NSRange()
        _ = storage.attribute(
            .mdocHidden, at: selection.location, longestEffectiveRange: &run,
            in: NSRange(location: 0, length: storage.length))
        let string = storage.string as NSString
        let wholeLines = string.lineRange(for: NSRange(location: run.location, length: 0)).location == run.location
            && (NSMaxRange(run) == string.length || string.character(at: NSMaxRange(run) - 1) == 0x0A)
        if wholeLines, run.location > 0, let previous, selection.location < previous.location {
            return NSRange(location: run.location - 1, length: 0)
        }
        return NSRange(location: NSMaxRange(run), length: 0)
    }
}
