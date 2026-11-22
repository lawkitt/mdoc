import MdocFFI
import UIKit

/// A selection in a table cell in edit mode (ADR 0110): its handles are
/// dragged by this view's own gesture, and it never leaves its cell.
///
/// UIKit's range adjustment cannot be used on a grid. It asks
/// `closestPosition(to:)`, which answers with the cell's character, and
/// then takes the range from TextKit's own hit test of the same point
/// (`NSTextSelectionNavigation`), which finds the table's hidden source
/// under the grid: while the finger moved, the selection shown ran from
/// the table's first line, the whole grid grey, and the handles stood
/// apart from the highlight. So a touch on a handle of a selection in a
/// cell is taken here, before UIKit's gesture: each move sets the selection
/// once, from the cell's own geometry, and UIKit draws the highlight, the
/// handles and the loupe for it. A selection UIKit stretches some other
/// way is kept in its cell as the caret is (`textViewDidChangeSelection`).
extension CandidateTextView {
    /// One end of a selection.
    enum SelectionEnd {
        case start, end
    }

    /// The span of the one cell that holds all of `range`, in the text's
    /// UTF-16 offsets: the cell's text, without the spaces beside its pipes.
    func cellText(holding range: NSRange) -> (table: MarkdownTableView, view: CellTextView, span: NSRange)? {
        guard let start = cell(at: range.location), let end = cell(at: NSMaxRange(range)), start.view === end.view else { return nil }
        return (start.table, start.view, NSRange(location: start.view.location, length: start.view.textStorage.length))
    }

    /// The selection just made, when it stretched `previous`, a selection
    /// in a cell, from one of its ends (⇧ with an arrow, a range UIKit
    /// adjusted itself): only the part in that cell's text, and `previous`
    /// when no part is. Nil for anything else: a caret, a selection made
    /// anew, one that is in the cell already.
    func keptInSelectedCell(from previous: NSRange?) -> NSRange? {
        let made = selectedRange
        guard editsCells, made.length > 0, let previous, previous.length > 0, made != previous,
              NSMaxRange(previous) <= textStorage.length, let cell = cellText(holding: previous) else { return nil }
        let ends = [previous.location, NSMaxRange(previous)]
        guard ends.contains(made.location) || ends.contains(NSMaxRange(made)) else { return nil }
        let kept = NSIntersectionRange(made, cell.span)
        if kept == made { return nil }
        return kept.length > 0 ? kept : previous
    }

    /// Where the two handles of the selection in a cell stand, as UIKit
    /// draws them from `selectionRects(for:)`: the start's on the leading
    /// edge of the rect that holds the selection's start, the end's on
    /// the trailing edge of the one that holds its end. In right-to-left
    /// text the leading edge is the right one, so a Hebrew word's start
    /// handle is at its right. TextKit gives a rect its paragraph's
    /// direction, not its run's, and UIKit draws by that: the handles are
    /// taken where they are drawn. (An end at a soft line break has its
    /// caret on the next line; its handle is on this one.)
    var cellHandleLines: (start: CGRect, end: CGRect)? {
        guard editsCells, selectedRange.length > 0, let range = selectedTextRange,
              let rects = cellSelectionRects(for: range)?.filter({ !$0.rect.isNull && !$0.rect.isInfinite }),
              let first = rects.first(where: \.containsStart) ?? rects.first(where: { $0.rect.width > 0 }) ?? rects.first,
              let last = rects.last(where: \.containsEnd) ?? rects.last(where: { $0.rect.width > 0 }) ?? rects.last
        else { return nil }
        let leading = first.writingDirection == .rightToLeft ? first.rect.maxX : first.rect.minX
        let trailing = last.writingDirection == .rightToLeft ? last.rect.minX : last.rect.maxX
        return (
            CGRect(x: leading - 1, y: first.rect.minY, width: 2, height: first.rect.height),
            CGRect(x: trailing - 1, y: last.rect.minY, width: 2, height: last.rect.height))
    }

    /// The handle of the selection in a cell that a touch at `point` takes
    /// hold of, and the point on its line the handle stands at. UIKit draws
    /// the start's knob above its line and the end's below.
    func cellHandle(at point: CGPoint) -> (end: SelectionEnd, line: CGPoint)? {
        guard let lines = cellHandleLines else { return nil }
        let reach: CGFloat = 22
        let knob: CGFloat = 8
        // A knob's own square, and a narrower strip along its line.
        func grabs(_ line: CGRect, knobAt y: CGFloat) -> Bool {
            CGRect(x: line.midX - reach, y: y - reach, width: 2 * reach, height: 2 * reach).contains(point)
                || CGRect(x: line.midX - reach / 2, y: line.minY, width: reach, height: line.height).contains(point)
        }
        let startKnob = CGPoint(x: lines.start.midX, y: lines.start.minY - knob)
        let endKnob = CGPoint(x: lines.end.midX, y: lines.end.maxY + knob)
        let atStart = grabs(lines.start, knobAt: startKnob.y)
        let atEnd = grabs(lines.end, knobAt: endKnob.y)
        func distance(_ to: CGPoint) -> CGFloat { hypot(point.x - to.x, point.y - to.y) }
        let start = (SelectionEnd.start, CGPoint(x: lines.start.midX, y: lines.start.midY))
        let end = (SelectionEnd.end, CGPoint(x: lines.end.midX, y: lines.end.midY))
        switch (atStart, atEnd) {
        case (false, false): return nil
        case (true, false): return start
        case (false, true): return end
        case (true, true): return distance(startKnob) <= distance(endKnob) ? start : end
        }
    }

    /// The caret stops of the text in table cells, as the editor core
    /// has them (`editor_table_selection`): an offset inside an escape
    /// (`\\|`) or a `<br>` is no stop, and goes to the one after it.
    /// Built from the whole text, so a handle's drag builds it once.
    struct CellStops {
        private let text: String
        private let string: NSString
        private let map: UTF8Map

        #if DEBUG
        /// How many were built: for tests.
        nonisolated(unsafe) static var built = 0
        #endif

        init(_ source: String) {
            var text = source
            text.makeContiguousUTF8()
            self.text = text
            string = text as NSString
            map = UTF8Map(text)
            #if DEBUG
            Self.built += 1
            #endif
        }

        /// The UTF-16 length of the text they were built from.
        var length: Int { string.length }

        /// The stop at `offset` (UTF-16), or the next one.
        func at(_ offset: Int) -> Int {
            let caret = TypingBridge.sourceRange(NSRange(location: offset, length: 0).clamped(to: string.length), in: string)
            guard let kept = editorTableSelection(text: text, selection: caret.textRange) else { return offset }
            return map.range(of: SourceRange(kept)).location
        }

        /// The first stop after `offset`, a stop itself.
        func after(_ offset: Int) -> Int {
            guard offset < string.length else { return offset }
            return at(NSMaxRange(string.rangeOfComposedCharacterSequence(at: offset)))
        }

        /// The last stop before `offset`, a stop itself, not before `floor`.
        func before(_ offset: Int, floor: Int) -> Int {
            var candidate = offset
            while candidate > floor {
                candidate = string.rangeOfComposedCharacterSequence(at: candidate - 1).location
                if at(candidate) < offset { return at(candidate) }
            }
            return floor
        }
    }

    /// The selection in a cell with one end moved to the character at
    /// `point` (this view's coordinates), kept in that cell: a point
    /// outside it gives the nearest character of the cell, above or below
    /// its text the text's start or end. Both ends stand on the core's
    /// caret stops, and they never cross: one stop's worth stays selected
    /// (a letter, a whole `\\|` or `<br>`), so nothing corrects the
    /// selection after it is set. Nil with no selection in one cell.
    /// `stops` are the text's, when a drag has them already.
    func cellSelection(moving end: SelectionEnd, to point: CGPoint, stops: CellStops? = nil) -> NSRange? {
        let selected = selectedRange
        guard editsCells, selected.length > 0, let cell = cellText(holding: selected) else { return nil }
        let stops = stops.flatMap { $0.length == textStorage.length ? $0 : nil } ?? CellStops(textStorage.string)
        let offset = stops.at(cell.table.offset(at: convert(point, to: cell.table), in: cell.view))
        let range: NSRange
        switch end {
        case .start:
            let last = stops.before(NSMaxRange(selected), floor: cell.span.location)
            let start = max(min(offset, last), cell.span.location)
            range = NSRange(location: start, length: NSMaxRange(selected) - start)
        case .end:
            let first = stops.after(selected.location)
            let finish = min(max(offset, first), NSMaxRange(cell.span))
            range = NSRange(location: selected.location, length: finish - selected.location)
        }
        // A cell with one stop only cannot give less than what it has.
        return range.length > 0 ? range : selected
    }

    /// Moves one end of the selection in a cell to `point`: one selection
    /// change, or none when the end stays on its character. Where the
    /// moved handle stands then, in this view's coordinates.
    @discardableResult
    func dragCellHandle(_ end: SelectionEnd, to point: CGPoint, stops: CellStops? = nil) -> CGRect? {
        guard let range = cellSelection(moving: end, to: point, stops: stops) else { return nil }
        if range != selectedRange {
            selectedRange = range
            // The highlight and the handles move with the finger, not a
            // frame or two behind it.
            for case let display as UITextSelectionDisplayInteraction in interactions {
                display.layoutManagedSubviews()
            }
        }
        return cellHandleLines.map { end == .start ? $0.start : $0.end }
    }

    /// Select All with the caret in a cell selects that cell's text, as it
    /// selects one field of a form: the table's source around it is not
    /// text the reader sees, and a selection does not span cells.
    func selectCellText() -> Bool {
        guard let hit = caretCell else { return false }
        let length = hit.view.textStorage.length
        if length > 0 { selectedRange = NSRange(location: hit.view.location, length: length) }
        return true
    }
}

/// The gesture that drags a handle of a selection in a table cell.
@MainActor
final class CellHandleDragger: NSObject, UIGestureRecognizerDelegate {
    private weak var view: CandidateTextView?
    let press = UILongPressGestureRecognizer()
    /// The handle held, and from the finger to the point on its line.
    private var held: (end: CandidateTextView.SelectionEnd, reach: CGVector)?
    /// The finger, in the window's coordinates: it stays put there while
    /// the text scrolls under it.
    private var finger: CGPoint = .zero
    /// The text's caret stops, taken when the drag begins: the text does
    /// not change while a handle moves.
    private var stops: CandidateTextView.CellStops?
    private var loupe: UITextLoupeSession?
    private var scroller: CADisplayLink?
    private var lastFrame: CFTimeInterval?
    /// Where the scroll has reached, unrounded: the scroll view rounds its
    /// offset to whole pixels, which would round away a 120 Hz frame's
    /// step or double it.
    private var scrolledTo: CGFloat?

    /// A handle is being dragged: the text scrolls only under the finger.
    var isDragging: Bool { held != nil }

    init(view: CandidateTextView) {
        self.view = view
        super.init()
        press.minimumPressDuration = 0
        press.allowableMovement = .greatestFiniteMagnitude
        // On in edit mode only (`connectTables`).
        press.isEnabled = false
        press.delegate = self
        press.addTarget(self, action: #selector(pressed(_:)))
        view.addGestureRecognizer(press)
    }

    /// Only a touch on a handle of a selection in a cell is taken.
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard let view else { return false }
        return view.cellHandle(at: touch.location(in: view)) != nil
    }

    /// Such a touch is this gesture's alone: UIKit's range adjustment, the
    /// grid's taps and both scrolls wait for it, and it begins at once.
    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer, shouldBeRequiredToFailBy otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        true
    }

    @objc private func pressed(_ recognizer: UILongPressGestureRecognizer) {
        guard let view else { return }
        let point = recognizer.location(in: view)
        switch recognizer.state {
        case .began: begin(at: point)
        case .changed: move(to: point)
        case .ended, .cancelled, .failed: end()
        default: break
        }
    }

    /// Takes hold of the handle under a finger down at `point`, if any.
    func begin(at point: CGPoint) {
        guard let view, let handle = view.cellHandle(at: point) else { return }
        held = (handle.end, CGVector(dx: handle.line.x - point.x, dy: handle.line.y - point.y))
        finger = view.convert(point, to: nil)
        stops = CandidateTextView.CellStops(view.textStorage.string)
        view.cellMenu.dismissMenu()
        loupe = UITextLoupeSession.begin(at: handle.line, fromSelectionWidgetView: view.handleView(for: handle.end), in: view)
        let link = CADisplayLink(target: self, selector: #selector(nextFrame(_:)))
        link.add(to: .main, forMode: .common)
        scroller = link
    }

    /// The finger moved to `point`: the handle follows.
    func move(to point: CGPoint) {
        guard let view, let held else { return }
        finger = view.convert(point, to: nil)
        let target = CGPoint(x: point.x + held.reach.dx, y: point.y + held.reach.dy)
        guard let caret = view.dragCellHandle(held.end, to: target, stops: stops) else { return }
        // The loupe shows the handle, which may have stopped at its
        // cell's edge with the finger beyond it.
        loupe?.move(to: CGPoint(x: caret.midX, y: caret.midY), withCaretRect: caret, trackingCaret: true)
    }

    /// The finger lifted, or the touch was cancelled.
    func end() {
        guard held != nil else { return }
        held = nil
        stops = nil
        scroller?.invalidate()
        scroller = nil
        lastFrame = nil
        scrolledTo = nil
        loupe?.invalidate()
        loupe = nil
        view?.showCellMenu()
    }

    @objc private func nextFrame(_ link: CADisplayLink) {
        defer { lastFrame = link.timestamp }
        guard let lastFrame else { return }
        scroll(for: link.timestamp - lastFrame)
    }

    /// How fast the text scrolls with the finger at `y`, in points a
    /// second, up negative: nothing away from the edges of `visible`,
    /// faster the further into an edge's strip, up to 240.
    static func scrollSpeed(at y: CGFloat, visible: CGRect) -> CGFloat {
        let edge: CGFloat = 28
        if y < visible.minY + edge { return -min(240, (visible.minY + edge - y) * 10) }
        if y > visible.maxY - edge { return min(240, (y - visible.maxY + edge) * 10) }
        return 0
    }

    /// A finger held at the top or bottom edge of the text scrolls it for
    /// `elapsed` seconds, as far at 120 frames a second as at 60, and the
    /// handle follows.
    func scroll(for elapsed: CFTimeInterval) {
        guard let view, held != nil else { return }
        let visible = view.bounds.inset(by: view.adjustedContentInset)
        let step = Self.scrollSpeed(at: view.convert(finger, from: nil).y, visible: visible) * CGFloat(elapsed)
        let top = -view.adjustedContentInset.top
        let bottom = max(top, view.contentSize.height - view.bounds.height + view.adjustedContentInset.bottom)
        // Scrolled some other way since, the scroll starts from there.
        let from = scrolledTo.flatMap { abs($0 - view.contentOffset.y) < 1 ? $0 : nil } ?? view.contentOffset.y
        let y = min(max(from + step, top), bottom)
        guard y != from else { return }
        scrolledTo = y
        view.contentOffset.y = y
        move(to: view.convert(finger, from: nil))
    }
}
