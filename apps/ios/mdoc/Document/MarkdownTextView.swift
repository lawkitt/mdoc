import SwiftUI
import UIKit

/// The document's Markdown source in a read-only, selectable TextKit 2
/// `UITextView` (ADR 0085). Candidates are links: a tap opens the candidate
/// sheet, VoiceOver reaches them through the Candidates rotor, and the edit
/// menu adds Replace… for a selection.
struct MarkdownTextView: UIViewRepresentable {
    let snapshot: DocumentSnapshot
    let selected: CandidateID?
    let onCandidate: (CandidateID) -> Void
    let onReplace: (SourceRange) -> Void
    let onReviewNext: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeUIView(context: Context) -> CandidateTextView {
        let view = CandidateTextView(usingTextLayoutManager: true)
        view.isEditable = false
        view.isSelectable = true
        view.dataDetectorTypes = []
        view.textDragInteraction?.isEnabled = false
        if #available(iOS 18, *) {
            view.writingToolsBehavior = .none
        }
        view.linkTextAttributes = [:]
        view.backgroundColor = .systemBackground
        view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 32, right: 12)
        view.delegate = context.coordinator
        view.accessibilityIdentifier = "document-text"
        view.accessibilityHint = String(localized: "Underlined words are names mdoc found. Use the Candidates rotor to move between them.")
        view.installAccessibility()
        return view
    }

    func updateUIView(_ view: CandidateTextView, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self
        view.onReviewNext = onReviewNext
        let key = StyleKey(
            snapshot: snapshot, selected: selected,
            dynamicTypeSize: context.environment.dynamicTypeSize,
            colorScheme: context.environment.colorScheme)
        guard key != coordinator.styled else { return }
        let textChanged = coordinator.styled?.snapshot.markdown != snapshot.markdown
        coordinator.styled = key
        coordinator.offsets = TextOffsets(snapshot.markdown)
        let offset = view.contentOffset
        view.attributedText = MarkdownStyler(traits: view.traitCollection, selected: selected)
            .attributedText(for: snapshot)
        view.candidateRanges = snapshot.candidates.map { ($0.id, coordinator.offsets.nsRange($0.range)) }
        if !textChanged || coordinator.keepsScroll {
            view.layoutIfNeeded()
            view.setContentOffset(offset, animated: false)
        }
        coordinator.keepsScroll = true
    }

    struct StyleKey: Equatable {
        var snapshot: DocumentSnapshot
        var selected: CandidateID?
        var dynamicTypeSize: DynamicTypeSize
        var colorScheme: ColorScheme
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownTextView
        var styled: StyleKey?
        var offsets = TextOffsets("")
        /// False until the first text is shown, so a new document starts at the top.
        var keepsScroll = false

        init(parent: MarkdownTextView) {
            self.parent = parent
        }

        func textView(_ textView: UITextView, primaryActionFor textItem: UITextItem, defaultAction: UIAction) -> UIAction? {
            guard case .link(let url) = textItem.content, let id = MarkdownStyler.candidateID(from: url) else {
                return nil
            }
            return UIAction { [weak self] _ in
                self?.parent.onCandidate(id)
            }
        }

        func textView(_ textView: UITextView, menuConfigurationFor textItem: UITextItem, defaultMenu: UIMenu) -> UITextItem.MenuConfiguration? {
            // A candidate's tap is its action; no link preview menu.
            if case .link(let url) = textItem.content, MarkdownStyler.candidateID(from: url) != nil {
                return nil
            }
            return UITextItem.MenuConfiguration(menu: defaultMenu)
        }

        /// Only Replace…: text leaves mdoc through Copy for AI and Share,
        /// never a plain Copy, Share, Look Up or Translate of the selection
        /// (ADR 0085's export invariant).
        func textView(_ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]) -> UIMenu? {
            guard range.length > 0 else { return UIMenu(children: []) }
            let source = offsets.sourceRange(range)
            let replace = UIAction(title: String(localized: "Replace…"), image: UIImage(systemName: "eye.slash")) { [weak self, weak textView] _ in
                textView?.selectedRange = NSRange(location: range.location, length: 0)
                self?.parent.onReplace(source)
            }
            return UIMenu(children: [replace])
        }
    }
}

/// The text view with the Candidates rotor and a Review next action. The
/// selection exists only for Replace…: copying, sharing and dragging it out
/// are off, keyboard shortcuts included.
final class CandidateTextView: UITextView {
    var candidateRanges: [(CandidateID, NSRange)] = []
    var onReviewNext: (() -> Void)?

    /// Selecting is the only standard action left.
    static let allowedActions: Set<Selector> = [
        #selector(UIResponderStandardEditActions.select(_:)),
        #selector(UIResponderStandardEditActions.selectAll(_:)),
    ]

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        Self.allowedActions.contains(action)
    }

    override func copy(_ sender: Any?) {}

    func installAccessibility() {
        let rotor = UIAccessibilityCustomRotor(name: String(localized: "Candidates")) { [weak self] predicate in
            self?.rotorItem(after: predicate)
        }
        accessibilityCustomRotors = [rotor]
        accessibilityCustomActions = [
            UIAccessibilityCustomAction(name: String(localized: "Review next proposal")) { [weak self] _ in
                self?.onReviewNext?()
                return true
            },
        ]
    }

    private func rotorItem(after predicate: UIAccessibilityCustomRotorSearchPredicate) -> UIAccessibilityCustomRotorItemResult? {
        let current = predicate.currentItem.targetRange.map { offset(from: beginningOfDocument, to: $0.start) } ?? -1
        let ranges = candidateRanges.map(\.1).sorted { $0.location < $1.location }
        let next: NSRange? = switch predicate.searchDirection {
        case .next: ranges.first { $0.location > current }
        case .previous: ranges.last { $0.location < current || current < 0 }
        @unknown default: nil
        }
        guard let next,
              let start = position(from: beginningOfDocument, offset: next.location),
              let end = position(from: start, offset: next.length),
              let range = textRange(from: start, to: end) else { return nil }
        return UIAccessibilityCustomRotorItemResult(targetElement: self, targetRange: range)
    }
}
