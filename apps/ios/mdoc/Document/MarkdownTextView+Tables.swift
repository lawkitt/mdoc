import MdocFFI
import UIKit

/// Table cells in live-preview edit mode (ADR 0109). The text view keeps
/// the caret in the cell's source, where typing goes through the typing
/// path as ever; the grid draws the cell, and the caret, selection and
/// marked-text rects are the cell's. The editor core keeps the caret inside
/// one cell (`editor_table_selection`), escapes what would leave it
/// (`editor_type_in_table`), moves between cells (`editor_table_move`) and
/// runs the table controls (`editor_table_command`).
extension CandidateTextView {
    /// Edit mode in live preview: tables are grids whose cells are edited.
    var editsCells: Bool { isEditMode && hidesMarkers }

    /// The grid cell whose span holds the text's UTF-16 `offset`, and where
    /// in the cell's text that is. Nil outside a drawn table's cells, and
    /// outside live-preview edit mode.
    func cell(at offset: Int) -> (table: MarkdownTableView, view: CellTextView, offset: Int)? {
        guard editsCells else { return nil }
        for table in tableViews where table.table.range.location <= offset && offset <= NSMaxRange(table.table.range) {
            if let hit = table.cell(at: offset) { return (table, hit.view, hit.offset) }
        }
        return nil
    }

    /// The cell holding the caret (the selection's end).
    var caretCell: (table: MarkdownTableView, view: CellTextView, offset: Int)? {
        cell(at: NSMaxRange(selectedRange))
    }

    /// Whether `offset` lies in a drawn table's lines, where the caret is
    /// kept in a cell; the end of a table that ends the text is not, so a
    /// caret there types a paragraph below it.
    func inDrawnTable(_ offset: Int) -> Bool {
        guard editsCells else { return false }
        let length = textStorage.length
        return tableViews.contains { table in
            let range = table.table.range
            return range.location <= offset && (offset < NSMaxRange(range) || offset == NSMaxRange(range) && offset < length)
        }
    }

    private func position(in view: CellTextView, at offset: Int) -> UITextPosition? {
        view.position(from: view.beginningOfDocument, offset: offset)
    }

    /// The caret's rect in a cell, in this view's coordinates.
    func cellCaretRect(for position: UITextPosition) -> CGRect? {
        guard let hit = cell(at: offset(from: beginningOfDocument, to: position)),
              let inCell = self.position(in: hit.view, at: hit.offset) else { return nil }
        return hit.view.convert(hit.view.caretRect(for: inCell), to: self)
    }

    /// The range's text in one cell: that cell's view and range.
    private func cellRange(_ range: UITextRange) -> (CellTextView, UITextRange)? {
        guard let start = cell(at: offset(from: beginningOfDocument, to: range.start)),
              let end = cell(at: offset(from: beginningOfDocument, to: range.end)),
              start.view === end.view,
              let from = position(in: start.view, at: start.offset),
              let to = position(in: end.view, at: end.offset),
              let inCell = start.view.textRange(from: from, to: to) else { return nil }
        return (start.view, inCell)
    }

    /// A selection inside one cell, drawn over the cell's text.
    func cellSelectionRects(for range: UITextRange) -> [UITextSelectionRect]? {
        guard let (view, inCell) = cellRange(range) else { return nil }
        return view.selectionRects(for: inCell).map { CellSelectionRect($0, from: view, to: self) }
    }

    /// Where marked text and autocorrect show for a range in one cell.
    func cellFirstRect(for range: UITextRange) -> CGRect? {
        guard let (view, inCell) = cellRange(range) else { return nil }
        return view.convert(view.firstRect(for: inCell), to: self)
    }

    /// The text position under `point` when it falls on a grid: the
    /// nearest cell's character there (drags, the loupe, the space bar's
    /// trackpad).
    func cellPosition(at point: CGPoint) -> UITextPosition? {
        guard editsCells else { return nil }
        for table in tableViews where table.frame.contains(point) {
            guard let offset = table.offset(at: convert(point, to: table)) else { continue }
            return position(from: beginningOfDocument, offset: offset)
        }
        return nil
    }

    /// Shows the caret's cell: the grid scrolls sideways to it, the text up
    /// or down.
    func revealCaretCell() {
        guard let hit = caretCell, let inCell = position(in: hit.view, at: hit.offset) else { return }
        let caret = hit.view.convert(hit.view.caretRect(for: inCell), to: hit.table)
        hit.table.reveal(caret)
        let shown = hit.table.convert(caret, to: self)
        let visible = bounds.inset(by: adjustedContentInset)
        if !visible.contains(shown) {
            scrollRectToVisible(shown.insetBy(dx: 0, dy: -Space.xs), animated: false)
        }
    }
}

/// A cell's selection rect in the text view's coordinates.
final class CellSelectionRect: UITextSelectionRect {
    private let frame: CGRect
    private let direction: NSWritingDirection
    private let starts: Bool
    private let ends: Bool
    private let vertical: Bool

    init(_ rect: UITextSelectionRect, from cell: UIView, to view: UIView) {
        frame = cell.convert(rect.rect, to: view)
        direction = rect.writingDirection
        starts = rect.containsStart
        ends = rect.containsEnd
        vertical = rect.isVertical
        super.init()
    }

    override var rect: CGRect { frame }
    override var writingDirection: NSWritingDirection { direction }
    override var containsStart: Bool { starts }
    override var containsEnd: Bool { ends }
    override var isVertical: Bool { vertical }
}

extension MarkdownTextView.Coordinator {
    /// Points the grids' taps and scrolls at the caret (ADR 0109): a tap
    /// puts the caret in the cell's source at the character tapped.
    func connectTables(_ view: CandidateTextView) {
        guard parent.edit != nil else {
            view.onTableTap = nil
            view.onTableScroll = nil
            view.onCellPaste = nil
            return
        }
        view.onTableTap = { [weak view] table, point in
            guard let view, let offset = table.offset(at: point) else { return }
            if !view.isFirstResponder { view.becomeFirstResponder() }
            view.selectedRange = NSRange(location: offset, length: 0)
        }
        view.onTableScroll = { [weak view] table in
            // The caret is drawn by the text view: it follows the grid.
            guard let view, view.isFirstResponder, view.caretCell?.table === table else { return }
            view.redrawCaret()
        }
        view.onCellPaste = { [weak self, weak view] pasted in
            guard let self, let view, view.caretCell != nil else { return false }
            return self.typeInCell(view, range: view.selectedRange) { text, selection in
                editorPaste(text: text, selection: selection, pasted: pasted, rendered: true)
            }
        }
    }

    /// The selection kept in one cell (`editor_table_selection`) when its
    /// caret, or the whole of it, lies in a drawn table; nil elsewhere.
    func keptInCell(_ view: CandidateTextView) -> NSRange? {
        let selection = view.selectedRange
        let end = NSMaxRange(selection)
        guard view.inDrawnTable(end), selection.length == 0 || view.inDrawnTable(selection.location) else { return nil }
        var text = view.textStorage.string
        text.makeContiguousUTF8()
        let string = text as NSString
        let selected = TypingBridge.sourceRange(selection.clamped(to: string.length), in: string)
        guard let kept = editorTableSelection(text: text, selection: selected.textRange) else { return nil }
        return UTF8Map(text).range(of: SourceRange(kept))
    }

    /// Typing in a cell that the core makes the cell's own bytes: a pipe,
    /// a backslash or a line break (`editor_type_in_table`), and word
    /// deletes kept inside the cell. True when the change was made here.
    func typedInCell(_ view: CandidateTextView, range: NSRange, text: String) -> Bool {
        guard parent.rendered, !bridge.isChanging, view.markedTextRange == nil, text != "\n" else { return false }
        if text.isEmpty {
            // ⌫ held (word deletes): never past the caret's cell's pipes.
            guard range.length > 0, let hit = view.cell(at: NSMaxRange(range)),
                  let span = hit.table.span(of: hit.view) else { return false }
            let kept = NSIntersectionRange(range, span)
            guard kept != range else { return false }
            if kept.length > 0 {
                bridge.replace(kept, with: "", selection: NSRange(location: kept.location, length: 0), in: view)
            }
            return true
        }
        guard text.contains(where: { "|\\\n\r".contains($0) }), let hit = view.cell(at: range.location) else { return false }
        guard view.cell(at: NSMaxRange(range))?.view === hit.view else { return true }
        return typeInCell(view, range: range) { source, selection in
            editorTypeInTable(text: source, selection: selection, typed: text, rendered: true)
        }
    }

    /// The core's change for `range` in a cell, made as one edit on the
    /// typing path (one "Typing" step).
    func typeInCell(_ view: CandidateTextView, range: NSRange, _ make: (String, TextRange) -> EditorChange?) -> Bool {
        var source = view.textStorage.string
        source.makeContiguousUTF8()
        let string = source as NSString
        let selection = TypingBridge.sourceRange(range.clamped(to: string.length), in: string)
        guard let change = make(source, selection.textRange) else { return false }
        switch StructuralKeys.replacement(for: change, in: source) {
        case .replace(let range, let text, let selection):
            bridge.replace(range, with: text, selection: selection, in: view)
        case .select(let selection):
            view.selectedRange = selection
        case .native, .ignored:
            break
        }
        return true
    }
}
