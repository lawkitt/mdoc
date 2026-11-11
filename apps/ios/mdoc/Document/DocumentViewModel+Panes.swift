import Foundation

/// What the panes row offers and which bar the Reply pane shows (ADR 0111).
extension DocumentViewModel {
    /// What the panes row switches between: a pane, or Identities in a
    /// narrow iPad window's right column.
    enum Shown: Hashable {
        case pane(Pane)
        case identities
    }

    /// The panes the row offers in a window of `layout`: on iPhone all of
    /// them, on iPad the right side, with Identities there in a narrow
    /// window.
    func panes(in layout: DocumentLayout) -> [Shown] {
        var panes: [Shown] = layout.isRegular ? [] : [.pane(.text)]
        if hasOriginal || layout.isRegular { panes.append(.pane(.original)) }
        // Identities are hidden while editing (ADR 0110).
        if identitiesPlacement == .column && !isEditing { panes.append(.identities) }
        if snapshot.reply != nil { panes.append(.pane(.reply)) }
        return panes
    }

    /// The row shows only with something to switch between.
    func showsPanesRow(in layout: DocumentLayout) -> Bool {
        panes(in: layout).count > 1
    }

    /// The row's selection in a window of `layout`. Beside the text (iPad)
    /// the right pane is never the text: the original stands there.
    func shown(in layout: DocumentLayout) -> Shown {
        if identitiesPlacement == .column && wantsIdentities { return .identities }
        return .pane(layout.isRegular && pane == .text ? .original : pane)
    }

    func show(_ shown: Shown) {
        switch shown {
        case .identities: showIdentities(true)
        case .pane(let pane): show(pane)
        }
    }

    /// Review is asked for: a name tapped in the text or in Identities, the
    /// Review button, a focused review, a match revealed. It acts on the
    /// text, so where one pane shows at a time the Text pane comes back;
    /// under the Reply pane the review bar would not show at all (ADR 0111).
    func showTextForReview() {
        if !layout.isRegular && pane != .text { show(.text) }
    }

    /// The reply covers the text: the Reply pane of a compact window.
    var replyCoversText: Bool {
        pane == .reply && snapshot.reply != nil && !layout.isRegular
    }

    /// The bottom bar acts on the reply, with Share, Copy and Paste: on
    /// the Reply pane once every name is decided (ADR 0087), and in place
    /// of Review and Replace N where the reply covers the text they act on.
    var barActsOnReply: Bool {
        guard pane == .reply, snapshot.reply != nil, !isEditing else { return false }
        switch primaryAction {
        case .handOff: return true
        case .replaceAll: return replyCoversText
        default: return false
        }
    }
}
