import UIKit

/// Builds the document's attributed Markdown source from the core's spans:
/// markers dimmed as the desktop editor draws them, candidates underlined in
/// their category's colour. Every range comes from the core as source bytes.
struct MarkdownStyler {
    static let candidateScheme = "mdoc-candidate"

    var traits: UITraitCollection
    var selected: CandidateID?

    static func link(for id: CandidateID) -> URL {
        var components = URLComponents()
        components.scheme = candidateScheme
        components.host = "candidate"
        components.queryItems = [URLQueryItem(name: "id", value: id.rawValue)]
        return components.url!
    }

    static func candidateID(from url: URL) -> CandidateID? {
        guard url.scheme == candidateScheme,
              let value = URLComponents(url: url, resolvingAgainstBaseURL: false)?
                .queryItems?.first(where: { $0.name == "id" })?.value else { return nil }
        return CandidateID(rawValue: value)
    }

    func attributedText(for snapshot: DocumentSnapshot) -> NSAttributedString {
        attributedText(markdown: snapshot.markdown, styling: snapshot.styling, candidates: snapshot.candidates)
    }

    func attributedText(markdown: String, styling: MarkdownStyling, candidates: [Candidate]) -> NSAttributedString {
        let offsets = TextOffsets(markdown)
        let body = UIFont.preferredFont(forTextStyle: .body, compatibleWith: traits)
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 2
        paragraph.paragraphSpacing = 4
        let text = NSMutableAttributedString(string: markdown, attributes: [
            .font: body,
            .foregroundColor: UIColor.label,
            .paragraphStyle: paragraph,
        ])
        let length = text.length
        func clamp(_ range: SourceRange) -> NSRange? {
            let clipped = NSIntersectionRange(offsets.nsRange(range), NSRange(location: 0, length: length))
            return clipped.length > 0 ? clipped : nil
        }

        // A line's block role sets its font and colour.
        for line in styling.lines {
            guard let range = clamp(line.range) else { continue }
            switch line.kind {
            case .heading(let level):
                text.addAttribute(.font, value: headingFont(level: level), range: range)
                text.addAttribute(.accessibilityTextHeadingLevel, value: level, range: range)
            case .quote:
                text.addAttribute(.foregroundColor, value: UIColor.secondaryLabel, range: range)
            case .codeBlock:
                text.addAttribute(.font, value: monospaced(size: body.pointSize), range: range)
            case .tableHeader:
                text.addAttribute(.font, value: body.withTraits(.traitBold), range: range)
            case .tableRow, .rule:
                break
            }
        }

        // Inline spans add to the line's font: bold in a heading stays the
        // heading's size.
        for span in styling.spans where !span.kinds.contains(.marker) {
            guard let range = clamp(span.range) else { continue }
            var added: UIFontDescriptor.SymbolicTraits = []
            if span.kinds.contains(.strong) { added.insert(.traitBold) }
            if span.kinds.contains(.emphasis) { added.insert(.traitItalic) }
            let code = span.kinds.contains(.code)
            if code || !added.isEmpty {
                text.enumerateAttribute(.font, in: range) { value, part, _ in
                    var font = value as? UIFont ?? body
                    if code {
                        font = monospaced(size: font.pointSize)
                    }
                    text.addAttribute(.font, value: font.withTraits(added), range: part)
                }
            }
            if code {
                text.addAttribute(.backgroundColor, value: UIColor.tertiarySystemFill, range: range)
            }
            if span.kinds.contains(.strikethrough) {
                text.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
            }
            if span.kinds.contains(.underline) {
                text.addAttribute(.underlineStyle, value: NSUnderlineStyle.single.rawValue, range: range)
            }
            if span.kinds.contains(.link) {
                text.addAttribute(.foregroundColor, value: UIColor.tintColor, range: range)
            }
            if span.kinds.contains(.highlight) {
                text.addAttribute(.backgroundColor, value: Self.highlight, range: range)
            }
            // The document's own colours, as the desktop draws them.
            if let rgba = span.color {
                text.addAttribute(.foregroundColor, value: UIColor(rgba: rgba), range: range)
            }
            if let rgba = span.background {
                text.addAttribute(.backgroundColor, value: UIColor(rgba: rgba), range: range)
            }
        }
        // Markers last, so they stay dimmed whatever they sit in.
        for span in styling.spans where span.kinds.contains(.marker) {
            guard let range = clamp(span.range) else { continue }
            text.addAttribute(.foregroundColor, value: Self.marker, range: range)
        }

        for candidate in candidates {
            guard let range = clamp(candidate.range) else { continue }
            let colour = candidate.category.uiColor
            var attributes: [NSAttributedString.Key: Any] = [
                .link: Self.link(for: candidate.id),
                .underlineColor: colour,
                .accessibilityTextCustom: [Self.spokenState(of: candidate)],
            ]
            switch candidate.state {
            case .proposed:
                // A light tint and a thick underline while proposed.
                attributes[.underlineStyle] = NSUnderlineStyle.thick.rawValue
                attributes[.backgroundColor] = colour.withAlphaComponent(0.12)
            case .replaced:
                // An alias chip once replaced.
                attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue
                attributes[.backgroundColor] = colour.withAlphaComponent(0.24)
                attributes[.font] = monospaced(size: body.pointSize)
            case .kept:
                // A muted dotted underline for a kept original.
                attributes[.underlineStyle] = NSUnderlineStyle.single.union(.patternDot).rawValue
                attributes[.underlineColor] = UIColor.secondaryLabel
            }
            if candidate.id == selected {
                attributes[.backgroundColor] = colour.withAlphaComponent(0.4)
            }
            text.addAttributes(attributes, range: range)
        }
        return text
    }

    /// What VoiceOver adds after a candidate's words.
    static func spokenState(of candidate: Candidate) -> String {
        switch candidate.state {
        case .proposed:
            String(localized: "\(candidate.category.label), proposed as \(candidate.alias)")
        case .replaced:
            String(localized: "\(candidate.category.label), replaced; was \(candidate.original)")
        case .kept:
            String(localized: "\(candidate.category.label), kept")
        }
    }

    private func headingFont(level: Int) -> UIFont {
        let style: UIFont.TextStyle = switch level {
        case 1: .title2
        case 2: .title3
        default: .headline
        }
        return UIFont.preferredFont(forTextStyle: style, compatibleWith: traits).withTraits(.traitBold)
    }

    /// Monospaced, a little smaller than the text it sits in.
    private func monospaced(size: CGFloat) -> UIFont {
        UIFont.monospacedSystemFont(ofSize: size * 0.92, weight: .regular)
    }

    /// Dimmed syntax.
    static let marker = UIColor.tertiaryLabel
    /// `==highlight==`, light and dark.
    static let highlight = UIColor { traits in
        UIColor.systemYellow.withAlphaComponent(traits.userInterfaceStyle == .dark ? 0.32 : 0.4)
    }
}

extension UIColor {
    /// A colour the document names, `0xRRGGBBAA`.
    convenience init(rgba: UInt32) {
        self.init(
            red: CGFloat((rgba >> 24) & 0xFF) / 255, green: CGFloat((rgba >> 16) & 0xFF) / 255,
            blue: CGFloat((rgba >> 8) & 0xFF) / 255, alpha: CGFloat(rgba & 0xFF) / 255)
    }
}

private extension UIFont {
    func withTraits(_ traits: UIFontDescriptor.SymbolicTraits) -> UIFont {
        guard let descriptor = fontDescriptor.withSymbolicTraits(fontDescriptor.symbolicTraits.union(traits)) else {
            return self
        }
        return UIFont(descriptor: descriptor, size: pointSize)
    }
}
