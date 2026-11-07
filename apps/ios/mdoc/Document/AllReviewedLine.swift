import MdocFFI
import SwiftUI

/// Above Share once review is done: "All 15 reviewed", with how many were
/// replaced and kept, so the moment reads as finished.
struct AllReviewedLine: View {
    let candidates: [Candidate]

    var body: some View {
        let replaced = candidates.filter { $0.state == .replaced }.count
        let kept = candidates.filter { $0.state == .kept }.count
        let pill = StatusPill(Text("All \(candidates.count) reviewed"), systemImage: "checkmark.seal.fill", kind: .ready)
        let counts = Text("\(replaced) replaced · \(kept) kept")
            .font(Typography.caption)
            .foregroundStyle(.secondary)
        // One line when it fits, else two: nothing truncates.
        ViewThatFits(in: .horizontal) {
            HStack(spacing: Space.xs) {
                pill
                counts
                Spacer(minLength: 0)
            }
            .lineLimit(1)
            VStack(alignment: .leading, spacing: Space.xxs) {
                pill
                counts
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("all-reviewed")
    }
}
