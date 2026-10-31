import SwiftUI
import UIKit

/// A restored reply, styled as the review screen styles Markdown (the core's
/// spans), with the values put back marked and alias-shaped tokens no
/// identity owns highlighted as unrecognised (ADR 0087). Read-only and
/// selectable.
struct RestoredTextView: UIViewRepresentable {
    let result: RestoreResult

    func makeUIView(context: Context) -> UITextView {
        let view = UITextView(usingTextLayoutManager: true)
        view.isEditable = false
        view.isSelectable = true
        view.dataDetectorTypes = []
        // Writing Tools would act on the originals (#321).
        view.writingToolsBehavior = .none
        view.backgroundColor = .systemBackground
        view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 32, right: 12)
        view.textLayoutManager?.usesHyphenation = false
        view.accessibilityIdentifier = "restored-text"
        return view
    }

    func updateUIView(_ view: UITextView, context: Context) {
        let traits = UITraitCollection(traitsFrom: [
            UITraitCollection(userInterfaceStyle: context.environment.colorScheme == .dark ? .dark : .light),
            UITraitCollection(preferredContentSizeCategory: UIContentSizeCategory(context.environment.dynamicTypeSize)),
        ])
        view.attributedText = Self.attributed(result, traits: traits)
    }

    static func attributed(_ result: RestoreResult, traits: UITraitCollection) -> NSAttributedString {
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
        let styled = NSMutableAttributedString(attributedString: MarkdownStyler(traits: traits, selected: nil)
            .attributedText(markdown: text, styling: MarkdownStyling(markdown: text), candidates: []))
        let dark = traits.userInterfaceStyle == .dark
        for range in restored {
            styled.addAttributes([
                .backgroundColor: UIColor.tintColor.withAlphaComponent(dark ? 0.3 : 0.15),
                .underlineStyle: NSUnderlineStyle.single.rawValue,
            ], range: range)
        }
        let body = MarkdownStyler.font(.body, traits: traits)
        for range in unresolved {
            styled.addAttributes([
                .backgroundColor: UIColor.systemOrange.withAlphaComponent(dark ? 0.4 : 0.3),
                .font: UIFont.monospacedSystemFont(ofSize: body.pointSize * 0.92, weight: .regular),
            ], range: range)
        }
        return styled
    }
}

/// "7 values restored · 1 unrecognised".
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
        let unresolved = String(localized: "\(result.unresolvedCount) unrecognised")
        return "\(restored) · \(unresolved)"
    }
}
