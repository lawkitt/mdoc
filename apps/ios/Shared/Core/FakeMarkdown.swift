import Foundation

/// The fake core's Markdown spans: just enough of headings, emphasis, lists
/// and tables for the fixtures. The real spans come from `mdoc-markdown`.
enum FakeMarkdown {
    private static let rules: [(NSRegularExpression, [Int: MarkdownSpanKind], MarkdownSpanKind?)] = [
        // (pattern, kinds of capture groups, kind of the whole match)
        (regex(#"^(#{1,6} )(.*)$"#), [1: .marker], nil),
        (regex(#"(\*\*)([^*\n]+)(\*\*)"#), [1: .marker, 2: .strong, 3: .marker], nil),
        (regex(#"(?<![*\w])(\*)([^*\n]+)(\*)(?![*\w])"#), [1: .marker, 2: .emphasis, 3: .marker], nil),
        (regex(#"(`)([^`\n]+)(`)"#), [1: .marker, 2: .code, 3: .marker], nil),
        (regex(#"^(\s*(?:\d+\.|[-*+]) )"#), [1: .marker], nil),
        (regex(#"^(> ?)(.*)$"#), [1: .marker, 2: .blockQuote], nil),
        (regex(#"^\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?$"#), [:], .marker),
        (regex(#"\|"#), [:], .marker),
    ]

    private static func regex(_ pattern: String) -> NSRegularExpression {
        try! NSRegularExpression(pattern: pattern, options: [.anchorsMatchLines])
    }

    static func spans(in markdown: String) -> [MarkdownSpan] {
        let offsets = TextOffsets(markdown)
        let whole = NSRange(location: 0, length: (markdown as NSString).length)
        var spans: [MarkdownSpan] = []
        for (expression, groups, matchKind) in rules {
            for match in expression.matches(in: markdown, range: whole) {
                if let matchKind {
                    spans.append(MarkdownSpan(kind: matchKind, range: offsets.sourceRange(match.range)))
                }
                for (group, kind) in groups where match.range(at: group).location != NSNotFound {
                    spans.append(MarkdownSpan(kind: kind, range: offsets.sourceRange(match.range(at: group))))
                }
            }
        }
        // Headings style their whole line.
        for match in regex(#"^(#{1,6}) .*$"#).matches(in: markdown, range: whole) {
            let level = match.range(at: 1).length
            spans.append(MarkdownSpan(kind: .heading(level: level), range: offsets.sourceRange(match.range)))
        }
        return spans.sorted { $0.range.start < $1.range.start }
    }
}
