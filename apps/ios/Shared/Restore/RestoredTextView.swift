import SwiftUI
import UIKit

/// A restored reply, styled as the review screen styles Markdown (the core's
/// spans, rendered unless Show Markdown is on, ADR 0096), with the values put
/// back marked and alias-shaped tokens no identity owns highlighted as
/// unrecognized (ADR 0087). Read-only and selectable.
struct RestoredTextView: UIViewRepresentable {
    let result: RestoreResult
    @AppStorage(MarkdownDisplay.showsSourceKey, store: MarkdownDisplay.store) private var showsSource = false

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> RenderedTextView {
        let view = RenderedTextView()
        view.isEditable = false
        view.isSelectable = true
        view.dataDetectorTypes = []
        // Writing Tools would act on the originals (#321).
        view.writingToolsBehavior = .none
        view.backgroundColor = .systemBackground
        view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 32, right: 12)
        view.textLayoutManager?.usesHyphenation = false
        view.accessibilityIdentifier = "restored-text"
        // A reply is the user's to copy, in a table cell too.
        view.configureCell = { $0.allowsCopy = true }
        let coordinator = context.coordinator
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

    /// Restyles for a new text column width.
    @MainActor
    final class Coordinator {
        var result: RestoreResult?
        var traits = UITraitCollection.current
        var rendered = true

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
            case .restored(let value, _):
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
