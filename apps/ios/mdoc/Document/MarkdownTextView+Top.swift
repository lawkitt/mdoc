import UIKit

/// Back to the top: the ↑ control after one screen of text, and the status
/// bar's tap while the text shows (originals.md, Original page controls).
extension MarkdownTextView.Coordinator {
    /// Wires the control to this view, and gives the status bar's tap to
    /// the text only while it shows: UIKit scrolls nothing when two views
    /// claim it.
    func connectTop(_ view: CandidateTextView) {
        view.scrollsToTop = parent.shown
        parent.viewport?.attach { [weak view] _ in
            guard let view else { return }
            view.setContentOffset(CGPoint(x: view.contentOffset.x, y: -view.adjustedContentInset.top), animated: true)
        }
        // Not while SwiftUI updates the view.
        DispatchQueue.main.async { [weak self, weak view] in
            if let self, let view { self.reportTop(view) }
        }
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard let view = scrollView as? CandidateTextView, view === shownView else { return }
        reportTop(view)
    }

    private func reportTop(_ view: CandidateTextView) {
        let scrolled = view.contentOffset.y + view.adjustedContentInset.top
        // Not while editing or reviewing: the keys and the review card
        // are the bottom's then, and the name under review is near it.
        let free = !parent.editing && parent.selected == nil && parent.shown
        // One screen of what shows between the bars.
        let screen = view.bounds.height - view.adjustedContentInset.top - view.adjustedContentInset.bottom
        let past = free && screen > 0 && scrolled > screen
        parent.viewport?.report(past ? .top : nil)
    }
}
