import SwiftUI

/// Buttons of equal width in a row when the widest label fits its share.
/// Otherwise the first button (the primary one) takes a row of its own and
/// the rest share the next when they fit, else every button takes a row: a
/// label never wraps or truncates (ADR 0087).
struct ButtonRow: Layout {
    var spacing: CGFloat = 8
    /// One button a row, always.
    var forceColumn = false

    /// The rows: how many buttons each holds, first row first.
    private func rows(_ width: CGFloat?, _ subviews: Subviews) -> [Int] {
        let count = subviews.count
        guard count > 1 else { return count == 1 ? [1] : [] }
        if forceColumn { return Array(repeating: 1, count: count) }
        let widths = subviews.map { $0.sizeThatFits(.unspecified).width }
        guard let width else { return [count] }
        if fits(widths, in: width) { return [count] }
        if count > 2, fits(Array(widths.dropFirst()), in: width) { return [1, count - 1] }
        return Array(repeating: 1, count: count)
    }

    private func fits(_ widths: [CGFloat], in width: CGFloat) -> Bool {
        let share = (width - spacing * CGFloat(widths.count - 1)) / CGFloat(widths.count)
        return (widths.max() ?? 0) <= share
    }

    /// Each row's buttons with their frames' width.
    private func arranged(_ width: CGFloat, _ subviews: Subviews) -> [(Subviews, CGFloat)] {
        var start = subviews.startIndex
        return rows(width, subviews).map { count in
            let row = subviews[start..<start + count]
            start += count
            return (row, (width - spacing * CGFloat(count - 1)) / CGFloat(count))
        }
    }

    private func height(of row: Subviews, share: CGFloat) -> CGFloat {
        row.map { $0.sizeThatFits(ProposedViewSize(width: share, height: nil)).height }.max() ?? 0
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let widest: CGFloat = subviews.map { $0.sizeThatFits(.unspecified).width }.max() ?? 0
        let gaps = CGFloat(max(subviews.count - 1, 0))
        let width: CGFloat = proposal.width ?? widest * CGFloat(subviews.count) + spacing * gaps
        let rows = arranged(width, subviews)
        var total: CGFloat = spacing * CGFloat(max(rows.count - 1, 0))
        for (row, share) in rows {
            total += height(of: row, share: share)
        }
        return CGSize(width: width, height: total)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for (row, share) in arranged(bounds.width, subviews) {
            let rowHeight = height(of: row, share: share)
            var x = bounds.minX
            for subview in row {
                subview.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(width: share, height: rowHeight))
                x += share + spacing
            }
            y += rowHeight + spacing
        }
    }
}
