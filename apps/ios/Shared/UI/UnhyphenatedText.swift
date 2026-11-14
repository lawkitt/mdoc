import SwiftUI
import UIKit

/// Text that wraps without hyphens: a name, an email or an alias shown with a
/// hyphen inside reads as part of the value (ADR 0087). On iOS 26 a
/// text-style font (SwiftUI's `Text`, `preferredFont`) hyphenates a word too
/// long for its line, so this draws a label in a plain system font of the
/// reader's text size, and lets an email or a path break after `@`, `.`,
/// `_` and `/` rather than mid-word. `lines` caps the lines, the last
/// ending in "…" (VoiceOver still hears the whole text); 0, as many as it
/// takes.
struct UnhyphenatedText: UIViewRepresentable {
    let text: String
    var style: UIFont.TextStyle = .body
    var weight: UIFont.Weight = .regular
    var monospaced = false
    var color: UIColor = .label
    var lines = 0

    init(
        _ text: String, style: UIFont.TextStyle = .body, weight: UIFont.Weight = .regular, monospaced: Bool = false,
        color: UIColor = .label, lines: Int = 0
    ) {
        self.text = text
        self.style = style
        self.weight = weight
        self.monospaced = monospaced
        self.color = color
        self.lines = lines
    }

    func makeUIView(context: Context) -> UILabel {
        let label = UILabel()
        label.numberOfLines = 0
        // The standard UI strategy hyphenates on iOS 26; plain word wrap.
        label.lineBreakStrategy = []
        label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        label.setContentHuggingPriority(.required, for: .vertical)
        return label
    }

    func updateUIView(_ label: UILabel, context: Context) {
        // Reading the size here re-runs this when the reader's text size changes.
        let traits = UITraitCollection(preferredContentSizeCategory: UIContentSizeCategory(context.environment.dynamicTypeSize))
        label.attributedText = Self.attributed(text, font: font(for: traits), color: color)
        label.numberOfLines = lines
        if lines > 0 {
            // Set after the text: the label still wraps at words, as the
            // paragraph says, and cuts only its last line.
            label.lineBreakMode = .byTruncatingTail
        }
        label.accessibilityLabel = text
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView label: UILabel, context: Context) -> CGSize? {
        let width = proposal.width ?? .greatestFiniteMagnitude
        let size = label.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
        return CGSize(width: min(ceil(size.width), width), height: ceil(size.height))
    }

    private func font(for traits: UITraitCollection) -> UIFont {
        let size = UIFont.preferredFont(forTextStyle: style, compatibleWith: traits).pointSize
        return monospaced ? .monospacedSystemFont(ofSize: size, weight: weight) : .systemFont(ofSize: size, weight: weight)
    }

    nonisolated static func attributed(_ text: String, font: UIFont, color: UIColor) -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.usesDefaultHyphenation = false
        paragraph.hyphenationFactor = 0
        paragraph.lineBreakMode = .byWordWrapping
        paragraph.lineBreakStrategy = []
        // Zero-width spaces after the separators: break points for display
        // only; VoiceOver reads the label's own text.
        var display = ""
        for character in text {
            display.append(character)
            if "@._/".contains(character) { display.append("\u{200B}") }
        }
        return NSAttributedString(string: display, attributes: [
            .font: font, .foregroundColor: color, .paragraphStyle: paragraph,
        ])
    }
}
