import MdocFFI
import SwiftUI
import UIKit

/// A restored reply, styled as the review screen styles Markdown (the core's
/// spans, rendered unless Show Markdown is on, ADR 0096), with the values put
/// back marked and alias-shaped tokens no identity owns highlighted as
/// unrecognized (ADR 0087). Read-only and selectable.
struct RestoredTextView: UIViewRepresentable {
    let result: RestoreResult
    @Environment(\.preferences) private var preferences
    private var showsSource: Bool { preferences.showsSource }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> RenderedTextView {
        let view = RestoredReplyTextView()
        view.isEditable = false
        view.isSelectable = true
        view.dataDetectorTypes = []
        // Writing Tools would act on the originals (#321).
        view.writingToolsBehavior = .none
        view.textDragInteraction?.isEnabled = false
        view.backgroundColor = .systemBackground
        view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 32, right: 12)
        view.textLayoutManager?.usesHyphenation = false
        view.accessibilityIdentifier = "restored-text"
        let coordinator = context.coordinator
        view.delegate = coordinator
        // A reply is the user's to copy, marked, in a table cell too.
        view.configureCell = { [weak coordinator] cell in
            cell.allowsCopy = true
            cell.delegate = coordinator
        }
        view.onWidth = { [weak coordinator, weak view] width in
            guard let coordinator, let view else { return }
            coordinator.render(view, width: width)
        }
        return view
    }

    func updateUIView(_ view: RenderedTextView, context: Context) {
        let traits = UITraitCollection(traitsFrom: [
            UITraitCollection(userInterfaceStyle: context.environment.colorScheme == .dark ? .dark : .light),
            UITraitCollection(preferredContentSizeCategory: UIContentSizeCategory(context.environment.dynamicTypeSize)),
        ])
        let coordinator = context.coordinator
        coordinator.result = result
        coordinator.traits = traits
        coordinator.rendered = !showsSource
        coordinator.render(view, width: view.textWidth)
    }

    /// Restyles for a new text column width, and keeps a selection's
    /// menu to Copy and Select All (#1228).
    @MainActor
    final class Coordinator: NSObject, UITextViewDelegate {
        var result: RestoreResult?
        var traits = UITraitCollection.current
        var rendered = true

        func textView(_ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]) -> UIMenu? {
            RestoredReplyTextView.menu(from: suggestedActions)
        }

        func render(_ view: RenderedTextView, width: CGFloat) {
            guard let result else { return }
            view.show(
                RestoredTextView.document(result, traits: traits, rendered: rendered, width: width > 0 ? width : nil),
                width: width)
        }
    }

    static func attributed(_ result: RestoreResult, traits: UITraitCollection) -> NSAttributedString {
        document(result, traits: traits, rendered: false, width: nil).text
    }

    static func document(_ result: RestoreResult, traits: UITraitCollection, rendered: Bool, width: CGFloat?) -> RenderedDocument {
        var text = ""
        var restored: [NSRange] = []
        var unresolved: [NSRange] = []
        for segment in result.segments {
            let start = text.utf16.count
            switch segment {
            case .text(let plain):
                text += plain
            case .restored(let value):
                text += value
                restored.append(NSRange(location: start, length: value.utf16.count))
            case .unresolved(let token):
                text += token
                unresolved.append(NSRange(location: start, length: token.utf16.count))
            }
        }
        let dark = traits.userInterfaceStyle == .dark
        let body = MarkdownStyler.font(.body, traits: traits)
        let styler = MarkdownStyler(traits: traits, selected: nil, rendered: rendered, width: width)
        return styler.document(markdown: text, styling: MarkdownStyling(markdown: text), candidates: []) { styled in
            for range in restored {
                styled.addAttributes([
                    .backgroundColor: UIColor.tintColor.withAlphaComponent(dark ? 0.3 : 0.15),
                    .underlineStyle: NSUnderlineStyle.single.rawValue,
                ], range: range)
            }
            for range in unresolved {
                styled.addAttributes([
                    .backgroundColor: UIColor.systemOrange.withAlphaComponent(dark ? 0.4 : 0.3),
                    .font: UIFont.monospacedSystemFont(ofSize: body.pointSize * 0.92, weight: .regular),
                ], range: range)
            }
        }
    }
}

/// A restored reply's text (#1228): it holds the user's real names, so a
/// selection's Copy, ⌘C and VoiceOver's Copy are marked as Copy reply's
/// copy is (`RestoredPasteboard`), and nothing sends a selection on: no
/// Share, Look Up, Translate, Search Web, Writing Tools or drag. Its table
/// cells follow the same rule (`CellTextView.allowsCopy`).
final class RestoredReplyTextView: RenderedTextView {
    /// The standard actions a selection keeps.
    static let allowedActions: Set<Selector> = [
        #selector(UIResponderStandardEditActions.copy(_:)),
        #selector(UIResponderStandardEditActions.select(_:)),
        #selector(UIResponderStandardEditActions.selectAll(_:)),
    ]

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        switch action {
        case #selector(UIResponderStandardEditActions.copy(_:)):
            return selectedRange.length > 0
        case #selector(UIResponderStandardEditActions.selectAll(_:)):
            // UIKit refuses Select All in a text view that is not editable
            // (#1257), as the review text's would.
            return textStorage.length > 0
        case #selector(UIResponderStandardEditActions.select(_:)):
            return super.canPerformAction(action, withSender: sender)
        default:
            return false
        }
    }

    override func copy(_ sender: Any?) {
        let range = selectedRange
        guard range.length > 0, NSMaxRange(range) <= textStorage.length else { return }
        RestoredPasteboard.copy((textStorage.string as NSString).substring(with: range))
    }

    /// The edit menu UIKit suggests, kept to Copy and Select All, in its
    /// words and order.
    static func menu(from suggested: [UIMenuElement]) -> UIMenu {
        func kept(_ element: UIMenuElement) -> UIMenuElement? {
            if let command = element as? UICommand {
                return allowedActions.contains(command.action) ? command : nil
            }
            if let menu = element as? UIMenu {
                let children = menu.children.compactMap(kept)
                return children.isEmpty ? nil : menu.replacingChildren(children)
            }
            return nil
        }
        return UIMenu(children: suggested.compactMap(kept))
    }
}

/// "7 values restored · 1 unrecognized".
struct RestoreSummary: View {
    let result: RestoreResult

    var body: some View {
        Text(Self.text(result))
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("restore-summary")
    }

    static func text(_ result: RestoreResult) -> String {
        let restored = String(localized: "\(result.restoredCount) values restored")
        guard result.unresolvedCount > 0 else { return restored }
        let unresolved = String(localized: "\(result.unresolvedCount) unrecognized")
        return "\(restored) · \(unresolved)"
    }
}
