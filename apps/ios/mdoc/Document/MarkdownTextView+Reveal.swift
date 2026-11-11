import MdocFFI
import UIKit

/// How the text scrolls to the name under review (ADR 0098, amended
/// 2026-11-11), and what a rebuild of the text keeps where it is on
/// screen. A name that shows whole between the bars leaves the text where
/// it is; any other scrolls just into view.
///
/// TextKit 2 lays out lazily. What is outside the viewport is an estimate,
/// a change to the text storage (a recolouring too) makes it one again, and
/// UIKit corrects the estimates, and moves the content offset with them, in
/// the viewport's own layout, after the text view's. So a name is measured
/// where it is drawn, outside the text view's layout pass, and measured
/// again once a scroll has landed.
extension MarkdownTextView.Coordinator {
    /// A newly selected name scrolls into view once laid out, unless
    /// the model asked for another place with this name under review;
    /// with none selected, no correction is owed. The name before is
    /// let go: a scroll still on its way to it stops where it is, and a
    /// reveal still owed to it (two quick taps, or swipes) is this
    /// name's now.
    func afterSelection(from previous: CandidateID?, in view: CandidateTextView) {
        if let selected = decoration.selected, selected != previous {
            stopReveal(in: view)
            if pendingRequest?.selected != selected {
                pendingReveal = .candidate(selected)
                pendingRequest = nil
            }
            view.setNeedsLayout()
        } else if decoration.selected == nil {
            endReveal()
        }
    }

    /// Stops a reveal's scroll where it is, with no correction owed. Left
    /// running after the bar moved on, it went on to the name before and
    /// could carry the new one out of view.
    private func stopReveal(in view: CandidateTextView) {
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
        let old = shownRoom
        let new = (height: view.bounds.height, insets: view.adjustedContentInset)
        shownRoom = new
        guard let old, old.height != new.height || old.insets != new.insets, !rendering, pending == nil,
              pendingReveal == nil, !parent.editing, !view.isTracking,
              let selected = decoration.selected, let candidate = parent.snapshot.candidate(selected),
              let place = place(of: offsets.nsRange(candidate.range), in: view, onlyShown: true) else { return }
        if Self.shows(place.rect, in: view, height: old.height, insets: old.insets),
           !Self.shows(place.rect, in: view, height: new.height, insets: new.insets) {
            pendingReveal = .candidate(selected)
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
        guard !revealQueued, revealIsDue(in: view) else { return }
        revealQueued = true
        DispatchQueue.main.async { [weak self, weak view] in
            guard let self, let view else { return }
            self.revealQueued = false
            self.runReveal(in: view)
        }
    }

    private func revealIsDue(in view: CandidateTextView) -> Bool {
        guard !rendering, pending == nil, view.window != nil, view.textWidth > 0, view.bounds.height > 0 else { return false }
        return pendingReveal != nil || (landing != nil && !scrolling)
    }

    /// Runs the pending reveal, once, or checks the scroll that landed.
    func runReveal(in view: CandidateTextView) {
        guard revealIsDue(in: view) else { return }
        // The whole pass, the viewport's too: rects and offset agree.
        view.layoutIfNeeded()
        guard let reveal = pendingReveal else {
            // The scroll has landed: measured again, corrected once.
            if let landing, !scrolling {
                self.landing = nil
                // Unless something else scrolled the text meanwhile.
                guard abs(view.contentOffset.y - landing.offset) < view.bounds.height / 2 else { return }
                self.reveal(range: landing.range, in: view, correcting: true)
            }
            return
        }
        pendingReveal = nil
        pendingRequest = nil
        // A scroll on its way elsewhere ends here.
        stopReveal(in: view)
        switch reveal {
        case .candidate(let id):
            self.reveal(id, in: view)
        case .range(let range):
            self.reveal(range: range, in: view)
            // The match selected, so the reader sees which words.
            let found = offsets.nsRange(range)
            if found.location + found.length <= view.attributedText.length { view.selectedRange = found }
        }
    }

    /// Brings the candidate under review into view (`reveal(range:)`).
    func reveal(_ id: CandidateID, in view: CandidateTextView) {
        guard let candidate = parent.snapshot.candidate(id) else { return }
        reveal(range: candidate.range, in: view)
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
    func reveal(range source: SourceRange, in view: CandidateTextView, correcting: Bool = false) {
        modeAnchor = nil
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
        var target = rect.minY < top || rect.height + 2 * Self.revealMargin > bottom - top
            ? rect.minY - Self.revealMargin - insets.top
            : rect.maxY + Self.revealMargin + insets.bottom - view.bounds.height
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
    private func settledPlace(of range: NSRange, in view: CandidateTextView) -> (rect: CGRect, cell: CellTextView?, local: CGRect)? {
        if let shown = place(of: range, in: view, onlyShown: true) { return shown }
        guard place(of: range, in: view) != nil else { return nil }
        view.setNeedsLayout()
        view.layoutIfNeeded()
        return place(of: range, in: view)
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

    /// The air between a name scrolled into view and the edge it came
    /// from.
    static let revealMargin = Space.m

    /// Whether `rect` (the text view's coordinates) shows whole in a
    /// view `height` high, between the bars `insets` stand for.
    static func shows(_ rect: CGRect, in view: UIScrollView, height: CGFloat, insets: UIEdgeInsets) -> Bool {
        let top = view.contentOffset.y + insets.top
        let bottom = view.contentOffset.y + height - insets.bottom
        return rect.minY >= top - 0.5 && rect.maxY <= bottom + 0.5
    }

    /// Where `range` (UTF-16, in the shown text) is drawn, in the text
    /// view's coordinates, from its first line to its last (a name
    /// wraps, most of all in a narrow table cell): in the text, or in
    /// the table cell that draws it (`cell`, with the rect in the cell's
    /// own coordinates, where the grid scrolls sideways to). `onlyShown` answers only for a name the viewport has laid
    /// out, and lays nothing out.
    func place(
        of range: NSRange, in view: CandidateTextView, onlyShown: Bool = false
    ) -> (rect: CGRect, cell: CellTextView?, local: CGRect)? {
        let cell = view.tableViews.lazy.flatMap { $0.cells.joined() }.first { cell in
            NSLocationInRange(range.location, NSRange(location: cell.location, length: cell.attributedText.length))
        }
        // The name's last character, on its last line.
        let end = NSRange(location: max(range.location, NSMaxRange(range) - 1), length: min(range.length, 1))
        guard let cell else {
            guard let first = view.laidOutRect(of: range, onlyShown: onlyShown) else { return nil }
            let last = range.length > 1 ? view.laidOutRect(of: end, onlyShown: onlyShown) : nil
            return (first.union(last ?? first), nil, first)
        }
        // The grid's place is its table's first line's, laid out
        // through it: the rest of the table's source lies hidden at
        // the grid's end, in view or not whatever the cell is.
        let anchor = (cell.superview as? MarkdownTableView)?.table.anchor ?? cell.location
        guard view.laidOutRect(of: NSRange(location: anchor, length: 0), onlyShown: onlyShown) != nil,
              let local = Self.firstRect(in: cell, location: range.location - cell.location, length: range.length)
        else { return nil }
        // To its last line: a later line can be the wider one, and the
        // grid scrolls sideways to all of the name.
        let last = range.length > 1 ? Self.firstRect(in: cell, location: end.location - cell.location, length: end.length) : nil
        let whole = local.union(last ?? local)
        return (cell.convert(whole, to: view), cell, whole)
    }

    func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) {
        guard scrolling else { return }
        scrolling = false
        scrollView.setNeedsLayout()
    }

    /// The reader took the text: no correction follows.
    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        landing = nil
        scrolling = false
        modeAnchor = nil
    }

    /// Review ended: no correction is owed.
    func endReveal() {
        landing = nil
        scrolling = false
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
           Self.shows(rect, in: view, height: view.bounds.height, insets: view.adjustedContentInset) {
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
            return place(of: NSRange(location: start, length: end - start), in: view, onlyShown: true)?.rect
        case .cell(let table, let row, let column):
            guard view.tableViews.indices.contains(table), view.tableViews[table].cells.indices.contains(row),
                  view.tableViews[table].cells[row].indices.contains(column) else { return nil }
            // A cell's stretches count from the cell's start.
            let cell = view.tableViews[table].cells[row][column]
            return place(of: NSRange(location: cell.location + start, length: end - start), in: view, onlyShown: true)?.rect
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
