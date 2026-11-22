import MdocFFI
import UIKit

/// What the reveal measures in: the text the coordinator shows now.
struct RevealText {
    var offsets: TextOffsets
    var snapshot: DocumentSnapshot
    /// The name under review, as the text decorates it.
    var selected: CandidateID?
    var editing: Bool
    /// False while the text is built (its own layout pass shows a scroll
    /// it will undo) or held back: no reveal runs then.
    var settled: Bool
}

/// How the text scrolls to the name under review (ADR 0098, amended
/// 2026-11-11): a name that shows whole between the bars leaves the text
/// where it is; any other scrolls just into view. This owns the reveal
/// owed, the scroll on its way to it, and where the reader was when the
/// mode last changed; the coordinator says what happened (a request, a
/// newly selected name, a layout pass, the reader's drag).
///
/// TextKit 2 lays out lazily. What is outside the viewport is an estimate,
/// a change to the text storage (a recolouring too) makes it one again, and
/// UIKit corrects the estimates, and moves the content offset with them, in
/// the viewport's own layout, after the text view's. So a name is measured
/// where it is drawn, outside the text view's layout pass, and measured
/// again once a scroll has landed.
@MainActor
final class TextReveal {
    /// The text now; the coordinator sets it once.
    var text: () -> RevealText? = { nil }
    /// The last reveal request taken, so each runs once.
    private var taken: Int?
    /// The scroll owed once the next layout is done: the model's
    /// request, or a newly selected name.
    private(set) var pending: RevealRequest.Target?
    /// The pending reveal is the model's request, and the name that was
    /// under review when it was made: a name selected later is a newer
    /// wish, and takes its place.
    private var origin: Origin?
    /// The room the text had at the last layout pass: the view's height
    /// and the bars inside it.
    private var room: (height: CGFloat, insets: UIEdgeInsets)?
    /// A reveal's scroll on its way: once it lands, the name is measured
    /// again and the scroll corrected once, if the layout the scroll
    /// brought moved it out of view.
    private(set) var landing: (range: SourceRange, offset: CGFloat)?
    /// An animated reveal's scroll is running: it lands at its end.
    private(set) var scrolling = false
    /// A reveal waits for the end of this turn of the run loop.
    private(set) var queued = false
    /// Where the reader was when the mode last changed: on iPad
    /// Identities come or go with it and the text's width changes a
    /// moment later, and that layout and the next change return here
    /// too, until the reader scrolls, types or a reveal moves the text.
    private var modeAnchor: CandidateTextView.ScrollAnchor?

    /// A reveal the model asked for, with the name then under review.
    private struct Origin {
        let selected: CandidateID?
    }

    /// `taken`: a request made before this view existed (another pane
    /// showed, or the document was closed) is not this view's to run.
    init(taken: Int?) {
        self.taken = taken
    }

    /// Whether a scroll is owed once the next layout is done.
    var isOwed: Bool { pending != nil }

    /// Takes the model's request, once, with the name `selected` then.
    func take(_ request: RevealRequest?, selected: CandidateID?) {
        guard let request, request.id != taken else { return }
        taken = request.id
        pending = request.target
        origin = Origin(selected: selected)
    }

    /// A newly selected name scrolls into view once laid out, unless
    /// the model asked for another place with this name under review;
    /// with none selected, no correction is owed. The name before is
    /// let go: a scroll still on its way to it stops where it is, and a
    /// reveal still owed to it (two quick taps, or swipes) is this
    /// name's now.
    func selectionChanged(from previous: CandidateID?, to selected: CandidateID?, in view: CandidateTextView) {
        if let selected, selected != previous {
            stop(in: view)
            if origin?.selected != selected {
                pending = .candidate(selected)
                origin = nil
            }
            view.setNeedsLayout()
        } else if selected == nil {
            end()
        }
    }

    /// Stops a reveal's scroll where it is, with no correction owed. Left
    /// running after the bar moved on, it went on to the name before and
    /// could carry the new one out of view.
    private func stop(in view: CandidateTextView) {
        if scrolling { view.setContentOffset(view.contentOffset, animated: false) }
        scrolling = false
        landing = nil
    }

    /// The room the text has changed (the review bar grew a line or
    /// was pulled up, the device turned): a name under review that
    /// showed whole and no longer does scrolls back into view. One the
    /// reader had scrolled away from stays away. The review bar is
    /// outside the text view, which gets shorter as the bar grows; a
    /// bar inside it shows as an inset.
    private func roomChanged(_ view: CandidateTextView) {
        let old = room
        let new = (height: view.bounds.height, insets: view.adjustedContentInset)
        room = new
        guard let old, old.height != new.height || old.insets != new.insets, let text = text(), text.settled,
              pending == nil, !text.editing, !view.isTracking,
              let selected = text.selected, let candidate = text.snapshot.candidate(selected),
              let place = view.place(of: text.offsets.nsRange(candidate.range), onlyShown: true) else { return }
        if Self.shows(place.rect, in: view, height: old.height, insets: old.insets),
           !Self.shows(place.rect, in: view, height: new.height, insets: new.insets) {
            pending = .candidate(selected)
        }
    }

    /// The end of a layout pass with a reveal owed, or a landed scroll
    /// to check: it runs once this turn of the run loop is over, when
    /// the view is on screen with a size to measure in. Not here: this
    /// is the text view's own layout, and UIKit lays out the text's
    /// viewport after it, where it corrects the heights TextKit 2
    /// estimated above the viewport and moves the content offset by as
    /// much. A name measured in between read as far from where it is
    /// drawn, and the scroll that followed threw a name in view out of
    /// it.
    func layoutDone(_ view: CandidateTextView) {
        roomChanged(view)
        guard !queued, isDue(in: view) else { return }
        queued = true
        DispatchQueue.main.async { [weak self, weak view] in
            guard let self, let view else { return }
            self.queued = false
            self.run(in: view)
        }
    }

    private func isDue(in view: CandidateTextView) -> Bool {
        guard text()?.settled == true, view.window != nil, view.textWidth > 0, view.bounds.height > 0 else { return false }
        return pending != nil || (landing != nil && !scrolling)
    }

    /// Runs the pending reveal, once, or checks the scroll that landed.
    private func run(in view: CandidateTextView) {
        guard isDue(in: view) else { return }
        // The whole pass, the viewport's too: rects and offset agree.
        view.layoutIfNeeded()
        guard let reveal = pending else {
            // The scroll has landed: measured again, corrected once.
            if let landing, !scrolling {
                self.landing = nil
                // Unless something else scrolled the text meanwhile.
                guard abs(view.contentOffset.y - landing.offset) < view.bounds.height / 2 else { return }
                self.reveal(range: landing.range, in: view, correcting: true)
            }
            return
        }
        pending = nil
        origin = nil
        // A scroll on its way elsewhere ends here.
        stop(in: view)
        guard let text = text() else { return }
        switch reveal {
        case .candidate(let id):
            guard let candidate = text.snapshot.candidate(id) else { return }
            self.reveal(range: candidate.range, in: view)
        case .range(let range):
            self.reveal(range: range, in: view)
            // The match selected, so the reader sees which words.
            let found = text.offsets.nsRange(range)
            if found.location + found.length <= view.attributedText.length { view.selectedRange = found }
        }
    }

    /// Brings a source range into view (ADR 0098, amended 2026-11-11).
    /// A name that shows whole between the bars above the text and the
    /// review bar (the view's own insets) leaves the text where it is;
    /// any other scrolls just far enough, at the nearer edge with a
    /// little air. In a table the grid scrolls sideways to it, which
    /// never moves the text. It is measured where the laid-out text
    /// draws it (`laidOutRect`), not at TextKit 2's estimate; once the
    /// scroll lands, it is measured again and, if still out of view,
    /// corrected once (`correcting`).
    private func reveal(range source: SourceRange, in view: CandidateTextView, correcting: Bool = false) {
        modeAnchor = nil
        guard let offsets = text()?.offsets else { return }
        let range = offsets.nsRange(source)
        guard var place = settledPlace(of: range, in: view) else { return }
        if let cell = place.cell, let table = cell.superview as? MarkdownTableView {
            let before = table.contentOffset.x
            table.reveal(cell.convert(place.local, to: table))
            place.rect.origin.x -= table.contentOffset.x - before
        }
        let rect = place.rect
        let insets = view.adjustedContentInset
        guard !Self.shows(rect, in: view, height: view.bounds.height, insets: insets) else { return }
        let top = view.contentOffset.y + insets.top
        let bottom = view.contentOffset.y + view.bounds.height - insets.bottom
        // Above the view, or taller than it: its top shows. Below: its
        // last line rests above the bar.
        var target = rect.minY < top || rect.height + 2 * Self.margin > bottom - top
            ? rect.minY - Self.margin - insets.top
            : rect.maxY + Self.margin + insets.bottom - view.bounds.height
        let highest = -insets.top
        let lowest = max(highest, view.laidOutContentHeight + insets.bottom - view.bounds.height)
        target = min(max(target, highest), lowest)
        guard abs(target - view.contentOffset.y) > 0.5 else { return }
        let animated = !UIAccessibility.isReduceMotionEnabled
        view.setContentOffset(CGPoint(x: view.contentOffset.x, y: target), animated: animated)
        guard !correcting else { return }
        landing = (source, target)
        scrolling = animated
        // Without an animation it has landed: the next pass measures.
        if !animated { view.setNeedsLayout() }
    }

    /// Where `range` is drawn, with the content offset that goes with
    /// it. A name the viewport has not laid out lays out the whole
    /// text, which moves what is above the viewport; the pass that
    /// follows lets UIKit move the content offset with it.
    private func settledPlace(of range: NSRange, in view: CandidateTextView) -> CandidateTextView.Place? {
        if let shown = view.place(of: range, onlyShown: true) { return shown }
        guard view.place(of: range) != nil else { return nil }
        view.setNeedsLayout()
        view.layoutIfNeeded()
        return view.place(of: range)
    }

    /// An animated reveal's scroll ended: it has landed.
    func scrollEnded(_ scrollView: UIScrollView) {
        guard scrolling else { return }
        scrolling = false
        scrollView.setNeedsLayout()
    }

    /// The reader took the text: no correction follows, and Done no
    /// longer goes back to where the mode changed.
    func readerScrolled() {
        landing = nil
        scrolling = false
        modeAnchor = nil
    }

    /// Review ended: no correction is owed.
    private func end() {
        landing = nil
        scrolling = false
    }

    // MARK: The place across a change of mode

    /// Where a rebuild puts the text back: the place kept from the last
    /// change of mode while the reader has not moved, so a wider text
    /// that fits whole sits at the top, and Done still goes back to where
    /// they were. Noted as the mode changes (`switching`); nil when
    /// neither the mode nor the width changed.
    func anchor(switching: Bool, widthChanged: Bool, in view: CandidateTextView) -> CandidateTextView.ScrollAnchor? {
        if switching { modeAnchor = modeAnchor ?? view.scrollAnchor() }
        return switching || widthChanged ? modeAnchor : nil
    }

    /// The reader typed or moved the caret: Done keeps the place they are
    /// at now, not where the mode changed.
    func placeMoved() {
        modeAnchor = nil
    }

    // MARK: Measuring

    /// The air between a name scrolled into view and the edge it came
    /// from.
    private static let margin = Space.m

    /// Whether `rect` (the text view's coordinates) shows whole in a
    /// view `height` high, between the bars `insets` stand for.
    static func shows(_ rect: CGRect, in view: UIScrollView, height: CGFloat, insets: UIEdgeInsets) -> Bool {
        let top = view.contentOffset.y + insets.top
        let bottom = view.contentOffset.y + height - insets.bottom
        return rect.minY >= top - 0.5 && rect.maxY <= bottom + 0.5
    }
}

extension CandidateTextView {
    /// Where a range is drawn: in the text view's coordinates, and the
    /// table cell that draws it with the rect in the cell's own.
    typealias Place = (rect: CGRect, cell: CellTextView?, local: CGRect)

    /// Where `range` (UTF-16, in the shown text) is drawn, in the text
    /// view's coordinates, from its first line to its last (a name
    /// wraps, most of all in a narrow table cell): in the text, or in
    /// the table cell that draws it (`cell`, with the rect in the cell's
    /// own coordinates, where the grid scrolls sideways to). `onlyShown` answers only for a name the viewport has laid
    /// out, and lays nothing out.
    func place(of range: NSRange, onlyShown: Bool = false) -> Place? {
        let cell = tableViews.lazy.flatMap { $0.cells.joined() }.first { cell in
            NSLocationInRange(range.location, NSRange(location: cell.location, length: cell.attributedText.length))
        }
        // The name's last character, on its last line.
        let end = NSRange(location: max(range.location, NSMaxRange(range) - 1), length: min(range.length, 1))
        guard let cell else {
            guard let first = laidOutRect(of: range, onlyShown: onlyShown) else { return nil }
            let last = range.length > 1 ? laidOutRect(of: end, onlyShown: onlyShown) : nil
            return (first.union(last ?? first), nil, first)
        }
        // The grid's place is its table's first line's, laid out
        // through it: the rest of the table's source lies hidden at
        // the grid's end, in view or not whatever the cell is.
        let anchor = (cell.superview as? MarkdownTableView)?.table.anchor ?? cell.location
        guard laidOutRect(of: NSRange(location: anchor, length: 0), onlyShown: onlyShown) != nil,
              let local = Self.firstRect(in: cell, location: range.location - cell.location, length: range.length)
        else { return nil }
        // To its last line: a later line can be the wider one, and the
        // grid scrolls sideways to all of the name.
        let last = range.length > 1 ? Self.firstRect(in: cell, location: end.location - cell.location, length: end.length) : nil
        let whole = local.union(last ?? local)
        return (cell.convert(whole, to: self), cell, whole)
    }

    /// The first line of a cell's range, in the cell's coordinates, from
    /// the cell's own text layout: a cell whose grid is hidden for a pass
    /// (`placeTables`) has no `firstRect(for:)` to give, and the two
    /// differ by a point, which read as the name moving.
    private static func firstRect(in view: UITextView, location: Int, length: Int) -> CGRect? {
        guard let layout = view.textLayoutManager, let content = layout.textContentManager,
              let from = content.location(content.documentRange.location, offsetBy: location),
              let to = content.location(from, offsetBy: length), let target = NSTextRange(location: from, end: to)
        else { return nil }
        layout.ensureLayout(for: content.documentRange)
        var first: CGRect?
        layout.enumerateTextSegments(in: target, type: .standard, options: []) { _, frame, _, _ in
            first = frame
            return false
        }
        return first.map { $0.offsetBy(dx: view.textContainerInset.left, dy: view.textContainerInset.top) }
    }
}

/// The reader's scrolls reach the reveal; what a rebuild keeps where it is
/// on screen (ADR 0098, amended 2026-11-11) is the coordinator's, as it
/// builds the text.
extension MarkdownTextView.Coordinator {
    func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) {
        reveal.scrollEnded(scrollView)
    }

    /// The reader took the text: no correction follows.
    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        reveal.readerScrolled()
    }

    /// Scrolls, without animation, until what `measure` finds rests
    /// `above` below the view's bounds' top: measured, moved and
    /// measured again where the viewport then draws it, since a text
    /// just built is laid out only around the viewport and the rest
    /// is an estimate.
    func hold(_ above: CGFloat, in view: CandidateTextView, measure: (_ onlyShown: Bool) -> CGRect?) {
        for _ in 0..<4 {
            view.setNeedsLayout()
            view.layoutIfNeeded()
            guard let rect = measure(true) ?? measure(false) else { return }
            let insets = view.adjustedContentInset
            // Replacing can shorten the text below the old place; left
            // there, a text that now fits is stuck, as it doesn't pan.
            let lowest = max(-insets.top, view.laidOutContentHeight + insets.bottom - view.bounds.height)
            let y = min(max(rect.minY - above, -insets.top), lowest)
            guard abs(y - view.contentOffset.y) > 0.5 else { return }
            view.setContentOffset(CGPoint(x: view.contentOffset.x, y: y), animated: false)
        }
    }

    /// What a rebuild keeps where it is on screen.
    enum Held {
        /// The name under review, its top `above` the view's own top.
        case name(CandidateID, above: CGFloat)
        /// The line at the top of the view.
        case line(CandidateTextView.ScrollAnchor)
    }

    /// The name the bar is on now, when the text shown before the
    /// rebuild draws it whole between the bars; else the top line,
    /// unless the text is at its top.
    func heldPlace(in view: CandidateTextView) -> Held? {
        if let id = parent.selected, let rect = shownRect(of: id, in: view),
           TextReveal.shows(rect, in: view, height: view.bounds.height, insets: view.adjustedContentInset) {
            return .name(id, above: rect.minY - view.contentOffset.y)
        }
        guard let line = view.scrollAnchor(), !line.atTop else { return nil }
        return .line(line)
    }

    /// Where a candidate is drawn now, from its first line to its last,
    /// if the viewport has laid it out: found by its stretches in the
    /// text shown, which a new snapshot's ranges no longer fit.
    private func shownRect(of id: CandidateID, in view: CandidateTextView) -> CGRect? {
        guard let runs = runs[id], let first = runs.first else { return nil }
        let start = runs.map(\.range.location).min() ?? first.range.location
        let end = runs.map { NSMaxRange($0.range) }.max() ?? NSMaxRange(first.range)
        switch first.place {
        case .text:
            return view.place(of: NSRange(location: start, length: end - start), onlyShown: true)?.rect
        case .cell(let table, let row, let column):
            guard view.tableViews.indices.contains(table), view.tableViews[table].cells.indices.contains(row),
                  view.tableViews[table].cells[row].indices.contains(column) else { return nil }
            // A cell's stretches count from the cell's start.
            let cell = view.tableViews[table].cells[row][column]
            return view.place(of: NSRange(location: cell.location + start, length: end - start), onlyShown: true)?.rect
        }
    }

    /// Where `location` of the text shown before is in the new text:
    /// moved by as much as the end of the last candidate before it
    /// moved, or at the start of the candidate it was in. A decision
    /// changes only candidates' wording.
    static func moved(_ location: Int, from old: [CandidateID: NSRange], to new: [CandidateID: NSRange]) -> Int {
        var nearest: (end: Int, delta: Int)?
        for (id, range) in old where range.location <= location {
            guard let now = new[id] else { continue }
            if location < NSMaxRange(range) { return now.location }
            if nearest.map({ NSMaxRange(range) > $0.end }) ?? true {
                nearest = (NSMaxRange(range), NSMaxRange(now) - NSMaxRange(range))
            }
        }
        return max(0, location + (nearest?.delta ?? 0))
    }
}
