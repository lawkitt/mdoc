import UIKit

/// Builds the document's attributed Markdown source from the core's spans,
/// candidates underlined in their category's colour. Every range comes from
/// the core as source bytes, and the text is always the source.
///
/// Rendered (ADR 0096, the default) it reads as desktop draws it: markers
/// hidden, lines that only hold markers folded away, list labels, quote bars,
/// alert and code cards and rules drawn by `MarkdownLayoutFragment`, tables
/// as grids `RenderedTextView` lays over the gap their lines leave. With Show
/// Markdown it is the source with dimmed markers (ADR 0085).
struct MarkdownStyler {
    static let candidateScheme = "mdoc-candidate"

    var traits: UITraitCollection
    var selected: CandidateID?
    /// The other mentions the review bar's scope reaches: outlined, so All
    /// N shows what it will change (ADR 0098).
    var scoped: Set<CandidateID> = []
    /// Markers hidden and blocks drawn; false shows the source, markers dimmed.
    var rendered = true
    /// The text column's width, which tables are laid out for; nil leaves
    /// tables as source lines.
    var width: CGFloat?

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
        document(markdown: markdown, styling: styling, candidates: candidates).text
    }

    func document(for snapshot: DocumentSnapshot) -> RenderedDocument {
        document(markdown: snapshot.markdown, styling: snapshot.styling, candidates: snapshot.candidates)
    }

    /// The styled text and, rendered with a width, its tables. `extra` adds
    /// attributes before the tables take their cells, so table cells show
    /// them too.
    func document(
        markdown: String, styling: MarkdownStyling, candidates: [Candidate],
        extra: ((NSMutableAttributedString) -> Void)? = nil
    ) -> RenderedDocument {
        let offsets = TextOffsets(markdown)
        let body = Self.font(.body, traits: traits)
        let paragraph = Self.paragraph()
        let text = NSMutableAttributedString(string: markdown, attributes: [
            .font: body,
            .foregroundColor: UIColor.label,
            .paragraphStyle: paragraph,
        ])
        let length = text.length
        // A blank source line draws about a third of a line high: the text,
        // and so every offset, stays the same (ADR 0087).
        let blank = NSMutableParagraphStyle()
        blank.setParagraphStyle(paragraph)
        blank.lineSpacing = 0
        blank.paragraphSpacing = 0
        let blankFont = UIFont.systemFont(ofSize: max(body.pointSize / 3, 4))
        let source = markdown as NSString
        var index = 0
        while index < source.length {
            let line = source.lineRange(for: NSRange(location: index, length: 0))
            if source.substring(with: line).trimmingCharacters(in: .newlines).isEmpty {
                text.addAttributes([.font: blankFont, .paragraphStyle: blank], range: line)
            }
            index = NSMaxRange(line)
        }
        func clamp(_ range: SourceRange) -> NSRange? {
            let clipped = NSIntersectionRange(offsets.nsRange(range), NSRange(location: 0, length: length))
            return clipped.length > 0 ? clipped : nil
        }

        let tableFont = UIFont.monospacedSystemFont(
            ofSize: Self.font(.footnote, traits: traits).pointSize, weight: .regular)
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
                // As source: table rows in a smaller monospaced font, so
                // columns line up and wrap less. Rendered: the grid's cells
                // take the body font and bold the header.
                text.addAttribute(.font, value: rendered ? body.withTraits(.traitBold) : tableFont.withTraits(.traitBold), range: range)
            case .tableRow:
                if !rendered {
                    text.addAttribute(.font, value: tableFont, range: range)
                }
            case .rule:
                break
            }
        }
        if rendered {
            for block in styling.blocks where block.kind == .muted {
                guard let range = clamp(block.range) else { continue }
                text.addAttribute(.foregroundColor, value: UIColor.secondaryLabel, range: range)
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
        // Markers last, so they stay dimmed, or hidden, whatever they sit in.
        var hidden: [NSRange] = []
        for span in styling.spans where span.kinds.contains(.marker) {
            guard let range = clamp(span.range) else { continue }
            if rendered {
                text.addAttributes(Self.hiddenAttributes, range: range)
                text.removeAttribute(.backgroundColor, range: range)
                hidden.append(range)
            } else {
                text.addAttribute(.foregroundColor, value: Self.marker, range: range)
            }
        }

        for candidate in candidates {
            guard let range = clamp(candidate.range) else { continue }
            let colour = candidate.category.uiColor
            var attributes: [NSAttributedString.Key: Any] = [
                .link: Self.link(for: candidate.id),
                .underlineColor: colour,
                .accessibilityTextCustom: [Self.spokenState(of: candidate)],
            ]
            let fill = Self.fill(dark: traits.userInterfaceStyle == .dark)
            switch candidate.state {
            case .proposed:
                // A light tint and a thick underline while proposed.
                attributes[.underlineStyle] = NSUnderlineStyle.thick.rawValue
                attributes[.backgroundColor] = colour.withAlphaComponent(fill.proposed)
            case .replaced:
                // An alias chip once replaced.
                attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue
                attributes[.backgroundColor] = colour.withAlphaComponent(fill.replaced)
                // Monospaced at the size and weight of what it sits in: a
                // table cell's smaller font, a heading's bold.
                text.enumerateAttribute(.font, in: range) { value, part, _ in
                    let font = value as? UIFont ?? body
                    let traits = font.fontDescriptor.symbolicTraits
                    var alias = traits.contains(.traitMonoSpace) ? font : monospaced(size: font.pointSize)
                    if traits.contains(.traitBold) {
                        alias = alias.withTraits(.traitBold)
                    }
                    text.addAttribute(.font, value: alias, range: part)
                }
            case .kept:
                // A muted dotted underline for a kept original.
                attributes[.underlineStyle] = NSUnderlineStyle.single.union(.patternDot).rawValue
                attributes[.underlineColor] = UIColor.secondaryLabel
            }
            if candidate.id == selected {
                attributes[.backgroundColor] = colour.withAlphaComponent(fill.selected)
            } else if scoped.contains(candidate.id) {
                attributes[.backgroundColor] = colour.withAlphaComponent(fill.selected * 0.45)
                attributes[.underlineStyle] = NSUnderlineStyle.thick.rawValue
                attributes[.underlineColor] = colour
            }
            text.addAttributes(attributes, range: range)
        }
        extra?(text)
        guard rendered else { return RenderedDocument(text: text, tables: []) }
        var layout = BlockLayout(
            text: text, source: source, offsets: offsets, styling: styling, hidden: hidden,
            traits: traits, body: body, paragraph: paragraph)
        let tables = width.map { layout.tables(width: $0) } ?? []
        layout.apply(tables: tables)
        return RenderedDocument(text: text, tables: tables)
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
        return Self.font(style, traits: traits, weight: .bold)
    }

    /// The text style's size for the reader's text size, as a plain system
    /// font: a text-style font makes iOS 26 hyphenate a word too long for
    /// its line, such as an email at large sizes (ADR 0087).
    /// With Document font › Serif it is New York at the same size (ADR 0099).
    static func font(_ style: UIFont.TextStyle, traits: UITraitCollection, weight: UIFont.Weight = .regular) -> UIFont {
        let font = UIFont.systemFont(ofSize: UIFont.preferredFont(forTextStyle: style, compatibleWith: traits).pointSize, weight: weight)
        guard MarkdownDisplay.serif, let serif = font.fontDescriptor.withDesign(.serif) else { return font }
        return UIFont(descriptor: serif, size: 0)
    }

    /// The body paragraph: no hyphenation, since a hyphen inside a name or an
    /// email reads as part of it (ADR 0087).
    static func paragraph() -> NSMutableParagraphStyle {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 2
        paragraph.paragraphSpacing = 4
        paragraph.usesDefaultHyphenation = false
        paragraph.hyphenationFactor = 0
        paragraph.lineBreakStrategy = []
        return paragraph
    }

    /// Monospaced, a little smaller than the text it sits in.
    private func monospaced(size: CGFloat) -> UIFont {
        UIFont.monospacedSystemFont(ofSize: size * 0.92, weight: .regular)
    }

    /// Candidate fills: stronger on a dark background, where a light tint
    /// of a category colour all but disappears (ADR 0087).
    static func fill(dark: Bool) -> (proposed: CGFloat, replaced: CGFloat, selected: CGFloat) {
        dark ? (0.26, 0.42, 0.6) : (0.12, 0.24, 0.4)
    }

    /// Dimmed syntax.
    static let marker = UIColor.tertiaryLabel
    /// `==highlight==`, light and dark.
    static let highlight = UIColor { traits in
        UIColor.systemYellow.withAlphaComponent(traits.userInterfaceStyle == .dark ? 0.32 : 0.4)
    }

    /// A hidden marker: no ink and next to no width. The characters stay, so
    /// every offset does too.
    static let hiddenFont = UIFont.systemFont(ofSize: 0.01)
    static var hiddenAttributes: [NSAttributedString.Key: Any] {
        [
            .font: hiddenFont,
            .foregroundColor: UIColor.clear,
            .mdocHidden: true,
        ]
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

extension UIFont {
    func withTraits(_ traits: UIFontDescriptor.SymbolicTraits) -> UIFont {
        guard let descriptor = fontDescriptor.withSymbolicTraits(fontDescriptor.symbolicTraits.union(traits)) else {
            return self
        }
        return UIFont(descriptor: descriptor, size: pointSize)
    }
}
