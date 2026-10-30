import Foundation
import MdocFFI

/// The Markdown spans of a text, from the core: `mdoc-markdown`, the
/// recognition the desktop editor uses, through `mdoc-ffi` (ADR 0085's Text
/// display rule). Ranges are source bytes like candidates'; each platform maps
/// the kinds to its own colours and fonts.
struct MarkdownStyling: Equatable, Sendable {
    /// In source order; they never overlap.
    var spans: [MarkdownSpan]
    /// Lines with a block role, in source order.
    var lines: [MarkdownLine]

    static let empty = MarkdownStyling(spans: [], lines: [])

    init(spans: [MarkdownSpan], lines: [MarkdownLine]) {
        self.spans = spans
        self.lines = lines
    }

    /// Asks the core for the spans of `markdown`.
    init(markdown: String) {
        let styling = MdocFFI.markdownStyling(text: markdown)
        spans = styling.spans.map(MarkdownSpan.init)
        lines = styling.lines.map(MarkdownLine.init)
    }
}

/// An inline piece of Markdown.
struct MarkdownSpan: Hashable, Sendable {
    /// What the span is; a bold link inside a highlight is several at once.
    struct Kinds: OptionSet, Hashable, Sendable {
        let rawValue: Int
        /// Syntax (`**`, `#`, `[`, `](…)`, bullets, pipes, fences): drawn dimmed.
        static let marker = Kinds(rawValue: 1 << 0)
        static let strong = Kinds(rawValue: 1 << 1)
        static let emphasis = Kinds(rawValue: 1 << 2)
        static let strikethrough = Kinds(rawValue: 1 << 3)
        static let underline = Kinds(rawValue: 1 << 4)
        static let code = Kinds(rawValue: 1 << 5)
        static let link = Kinds(rawValue: 1 << 6)
        static let highlight = Kinds(rawValue: 1 << 7)
    }

    var range: SourceRange
    var kinds: Kinds
    /// A text colour the document names, as `0xRRGGBBAA`.
    var color: UInt32?
    /// A background the document names, as `0xRRGGBBAA`.
    var background: UInt32?

    init(range: SourceRange, kinds: Kinds, color: UInt32? = nil, background: UInt32? = nil) {
        self.range = range
        self.kinds = kinds
        self.color = color
        self.background = background
    }

    fileprivate init(_ span: MdocFFI.MarkdownSpan) {
        var kinds: Kinds = []
        for (on, kind) in [
            (span.marker, Kinds.marker), (span.strong, .strong), (span.emphasis, .emphasis),
            (span.strikethrough, .strikethrough), (span.underline, .underline), (span.code, .code),
            (span.link, .link), (span.highlight, .highlight),
        ] where on {
            kinds.insert(kind)
        }
        self.init(
            range: SourceRange(start: Int(span.range.start), end: Int(span.range.end)),
            kinds: kinds, color: span.color, background: span.background)
    }
}

/// A line's block role.
struct MarkdownLine: Hashable, Sendable {
    enum Kind: Hashable, Sendable {
        case heading(level: Int)
        case quote
        /// A fenced code block's line, the fences included.
        case codeBlock
        case tableHeader
        /// A table's body row or its delimiter row.
        case tableRow
        /// A thematic break (`---`).
        case rule
    }

    /// The line without its line break.
    var range: SourceRange
    var kind: Kind

    fileprivate init(_ line: MdocFFI.MarkdownLine) {
        range = SourceRange(start: Int(line.range.start), end: Int(line.range.end))
        kind = switch line.kind {
        case .heading(let level): .heading(level: Int(level))
        case .quote: .quote
        case .codeBlock: .codeBlock
        case .tableHeader: .tableHeader
        case .tableRow: .tableRow
        case .rule: .rule
        }
    }
}
