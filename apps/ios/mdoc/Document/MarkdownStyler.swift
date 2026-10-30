import UIKit

/// Builds the document's attributed Markdown source: markers dimmed as the
/// desktop editor draws them, candidates underlined in their category's
/// colour. Every range comes from the core as source bytes.
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
        let offsets = TextOffsets(snapshot.markdown)
        let body = UIFont.preferredFont(forTextStyle: .body, compatibleWith: traits)
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 2
        paragraph.paragraphSpacing = 4
        let text = NSMutableAttributedString(string: snapshot.markdown, attributes: [
            .font: body,
            .foregroundColor: UIColor.label,
            .paragraphStyle: paragraph,
        ])
        let length = text.length
        func clamp(_ range: NSRange) -> NSRange? {
            let clipped = NSIntersectionRange(range, NSRange(location: 0, length: length))
            return clipped.length > 0 ? clipped : nil
        }

        // Headings first so markers inside them still dim.
        for span in snapshot.spans {
            guard let range = clamp(offsets.nsRange(span.range)) else { continue }
            switch span.kind {
            case .heading(let level):
                text.addAttribute(.font, value: headingFont(level: level), range: range)
                text.addAttribute(.accessibilityTextHeadingLevel, value: level, range: range)
            case .strong:
                text.addAttribute(.font, value: body.withTraits(.traitBold), range: range)
            case .emphasis:
                text.addAttribute(.font, value: body.withTraits(.traitItalic), range: range)
            case .code, .codeBlock:
                text.addAttribute(.font, value: monospaced(), range: range)
            case .link:
                text.addAttribute(.foregroundColor, value: UIColor.tintColor, range: range)
            case .blockQuote:
                text.addAttribute(.foregroundColor, value: UIColor.secondaryLabel, range: range)
            case .marker:
                break
            }
        }
        for span in snapshot.spans where span.kind == .marker {
            guard let range = clamp(offsets.nsRange(span.range)) else { continue }
            text.addAttribute(.foregroundColor, value: UIColor.tertiaryLabel, range: range)
        }

        for candidate in snapshot.candidates {
            guard let range = clamp(offsets.nsRange(candidate.range)) else { continue }
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
                attributes[.font] = monospaced()
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

    private func monospaced() -> UIFont {
        let size = UIFont.preferredFont(forTextStyle: .body, compatibleWith: traits).pointSize
        return UIFont.monospacedSystemFont(ofSize: size * 0.92, weight: .regular)
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
