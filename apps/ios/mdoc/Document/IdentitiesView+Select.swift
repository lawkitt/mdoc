import MdocFFI
import SwiftUI

/// Identities' Select (ADR 0114; docs/mobile-ui.md, Identities): the Select
/// capsule, Select's header as in Mail and Photos (Cancel, "3 selected",
/// Select All), and the one-time demo of what Select does: a ghost of the
/// first row lifts on its ≡ handle and settles on the row below, which shows
/// "Same as PERSON_1", under a caption. TipKit decides whether it is due
/// (`ReviewHint.dragMerge`); the drawing is the drag-merge's own.
extension IdentitiesView {
    /// Outside Select: "Select" with ☑︎ in a glass capsule.
    var selectButton: some View {
        Button {
            selection = []
            selecting = true
        } label: {
            Label("Select", systemImage: "checkmark.circle")
                .font(Typography.chip)
                .labelStyle(.titleAndIcon)
                // One line: the summary beside it gives way first.
                .lineLimit(1)
                .fixedSize()
        }
        .secondaryGlass()
        .buttonBorderShape(.capsule)
        .tint(.primary)
        .frame(minHeight: Size.target)
        .accessibilityLabel(Text("Select"))
        .accessibilityHint(Text("Tick names to replace, keep or join them."))
        .accessibilityIdentifier("identities-select")
    }

    /// The rows Select All ticks: those on screen, so not the folded Kept.
    var selectable: [Identity] {
        shown.filter { showsKept || !query.isEmpty || placed($0) != .kept }
    }

    private var allSelected: Bool {
        let ids = selectable.map(\.id)
        return !ids.isEmpty && ids.allSatisfy(selection.contains)
    }

    /// In Select: Cancel | "3 selected" | Select All (Deselect All once all
    /// are ticked).
    var selectHeader: some View {
        let count = identities.filter { selection.contains($0.id) }.count
        return CenteredBar {
            Button("Cancel") {
                selecting = false
                selection = []
            }
            .textAction()
            .frame(minWidth: Size.target, minHeight: Size.target, alignment: .leading)
            .accessibilityIdentifier("identities-select-cancel")
            Text(count == 0 ? String(localized: "Select names") : String(localized: "\(count) selected"))
                .font(Typography.title)
                .minimumScaleFactor(0.7)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("identities-selected-count")
            Button(allSelected ? "Deselect All" : "Select All") {
                if allSelected {
                    selection = []
                } else {
                    selection.formUnion(selectable.map(\.id))
                }
                Haptics.selection()
            }
            .textAction()
            .frame(minWidth: Size.target, minHeight: Size.target, alignment: .trailing)
            .disabled(selectable.isEmpty)
            .accessibilityIdentifier("identities-select-all")
        }
        .font(Typography.body)
        .lineLimit(1)
        .frame(minHeight: 34)
    }

    // MARK: The demo

    /// Select opened: the demo, once, with two names or more (ADR 0114).
    /// Never while the review bar shows a hint of its own.
    func startDemoIfDue() {
        guard model.offersSelectDemo, model.hint == nil, !demoShows else { return }
        demoShows = true
        Hints.shared.show(.dragMerge)
        AccessibilityNotification.Announcement(String(localized: ReviewHint.dragMerge.text)).post()
        demoTask = Task { await playDemo() }
    }

    /// The demo's beats: the rows settle into Select, the ghost lifts, moves
    /// onto the row below and settles there. About 2 s; Reduce Motion shows
    /// the ghost on the row below, still. The caption stays until a tap.
    private func playDemo() async {
        do {
            // timing: Select's ticks and handles slide in before the ghost lifts (ADR 0114)
            try await Task.sleep(for: .milliseconds(450))
        } catch { return }
        guard demoPlays else { return }
        guard let rows = demoRows() else {
            // Fewer than two rows on screen (a search, a category, Kept
            // folded): nothing to show; it waits for the next Select.
            endDemo(dismissed: false)
            return
        }
        let from = CGPoint(x: rows.first.handle.midX, y: rows.first.frame.midY)
        let onto = CGPoint(x: from.x, y: rows.second.frame.midY)
        var ghost = IdentityDragState(dragged: rows.first.id, frame: rows.first.frame, start: from, location: from)
        ghost.isDemo = true
        if reduceMotion {
            ghost.location = onto
            ghost.target = rows.second.id
            drag = ghost
            return
        }
        withAnimation(Motion.move(reduced: false)) { drag = ghost }
        do {
            // timing: the ghost hangs lifted a moment (ADR 0114)
            try await Task.sleep(for: .milliseconds(350))
        } catch { return }
        guard demoPlays else { return }
        ghost.location = onto
        withAnimation(.easeInOut(duration: 0.6)) { drag = ghost }
        do {
            // timing: the ghost's move onto the row below (ADR 0114)
            try await Task.sleep(for: .milliseconds(450))
        } catch { return }
        guard demoPlays else { return }
        ghost.target = rows.second.id
        withAnimation(Motion.move(reduced: false)) { drag = ghost }
        do {
            // timing: "Same as PERSON_1" shows on the row below a moment (ADR 0114)
            try await Task.sleep(for: .milliseconds(800))
        } catch { return }
        guard demoPlays else { return }
        withAnimation(Motion.fade) { drag = nil }
    }

    /// The demo is on and no finger's drag holds the rows: the ghost may move.
    private var demoPlays: Bool {
        demoShows && (drag == nil || drag?.isDemo == true)
    }

    /// The first two rows on screen, top to bottom, with the first's ≡
    /// handle: where the ghost lifts and where it settles.
    private func demoRows() -> (first: (id: IdentityID, frame: CGRect, handle: CGRect), second: (id: IdentityID, frame: CGRect))? {
        let visible = dragTracking.visible
        let rows = dragTracking.rows
            .filter { visible.contains(CGPoint(x: $0.value.midX, y: $0.value.midY)) }
            .sorted { $0.value.minY < $1.value.minY }
        guard rows.count >= 2, let handle = dragTracking.handles[rows[0].key] else { return nil }
        return ((rows[0].key, rows[0].value, handle), (rows[1].key, rows[1].value))
    }

    /// The demo ends: a tap anywhere dismissed it (learned for good), or
    /// Select or the panel went first (it shows again next time).
    func endDemo(dismissed: Bool) {
        guard demoShows else { return }
        demoTask?.cancel()
        demoTask = nil
        demoShows = false
        if drag?.isDemo == true {
            withAnimation(Motion.fade) { drag = nil }
        }
        if dismissed {
            Hints.shared.learn(.dragMerge, closed: true)
        } else {
            Hints.shared.show(nil)
        }
    }
}

/// The demo's caption under Select's header: a symbol and the words, which
/// VoiceOver reads. No ×: any tap ends the demo.
struct SelectDemoCaption: View {
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Space.xs) {
            Image(systemName: ReviewHint.dragMerge.symbol)
                .foregroundStyle(Palette.accent)
                .accessibilityHidden(true)
            Text(ReviewHint.dragMerge.text)
                .font(Typography.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.leading, Space.xxs)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("select-demo-caption")
    }
}

/// Three views across, as Mail's Select header: the outer two at the width
/// of the wider of them, the middle one centred in what is left, so the
/// count never runs under Cancel or Select All at any text size.
struct CenteredBar: Layout {
    var spacing: CGFloat = Space.s

    private func side(_ subviews: Subviews, width: CGFloat) -> CGFloat {
        let widest = max(subviews[0].sizeThatFits(.unspecified).width, subviews[2].sizeThatFits(.unspecified).width)
        return min(widest, max(0, (width - 2 * spacing) / 3))
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard subviews.count == 3 else { return .zero }
        let width = proposal.width ?? subviews.reduce(2 * spacing) { $0 + $1.sizeThatFits(.unspecified).width }
        let side = side(subviews, width: width)
        let middle = max(0, width - 2 * side - 2 * spacing)
        let heights = [
            subviews[0].sizeThatFits(ProposedViewSize(width: side, height: nil)).height,
            subviews[1].sizeThatFits(ProposedViewSize(width: middle, height: nil)).height,
            subviews[2].sizeThatFits(ProposedViewSize(width: side, height: nil)).height,
        ]
        return CGSize(width: width, height: heights.max() ?? 0)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard subviews.count == 3 else { return }
        let side = side(subviews, width: bounds.width)
        let middle = max(0, bounds.width - 2 * side - 2 * spacing)
        subviews[0].place(at: CGPoint(x: bounds.minX, y: bounds.midY), anchor: .leading,
                          proposal: ProposedViewSize(width: side, height: nil))
        subviews[1].place(at: CGPoint(x: bounds.midX, y: bounds.midY), anchor: .center,
                          proposal: ProposedViewSize(width: middle, height: nil))
        subviews[2].place(at: CGPoint(x: bounds.maxX, y: bounds.midY), anchor: .trailing,
                          proposal: ProposedViewSize(width: side, height: nil))
    }
}

#Preview("Select demo caption") {
    VStack(spacing: Space.l) {
        SelectDemoCaption()
        SelectDemoCaption()
            .environment(\.colorScheme, .dark)
            .background(.black)
    }
    .padding()
}
