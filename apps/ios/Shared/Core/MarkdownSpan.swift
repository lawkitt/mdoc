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
    /// List items, in source order (ADR 0096).
    var listItems: [MarkdownListItem]
    /// Code fences, alerts and greyed lines, in source order (ADR 0096).
    var blocks: [MarkdownBlock]
    /// Tables, in source order (ADR 0096).
    var tables: [MarkdownTable]
    /// Inline images outside code and tables, in source order (ADR 0096).
    var images: [MarkdownImage]

    static let empty = MarkdownStyling(spans: [], lines: [])

    init(
        spans: [MarkdownSpan], lines: [MarkdownLine], listItems: [MarkdownListItem] = [],
        blocks: [MarkdownBlock] = [], tables: [MarkdownTable] = [], images: [MarkdownImage] = []
    ) {
        self.spans = spans
        self.lines = lines
        self.listItems = listItems
        self.blocks = blocks
        self.tables = tables
        self.images = images
    }

    /// Asks the core for the spans of `markdown`.
    init(markdown: String) {
        let styling = MdocFFI.markdownStyling(text: markdown)
        spans = styling.spans.map(MarkdownSpan.init)
        lines = styling.lines.map(MarkdownLine.init)
        listItems = styling.listItems.map(MarkdownListItem.init)
        blocks = styling.blocks.map(MarkdownBlock.init)
        tables = styling.tables.map(MarkdownTable.init)
        images = styling.images.map { MarkdownImage(range: SourceRange($0.range), alt: SourceRange($0.alt)) }
    }
}

private extension SourceRange {
    init(_ range: MdocFFI.TextRange) {
        self.init(start: Int(range.start), end: Int(range.end))
    }
}

/// An inline `![alt](src)`, drawn as an Image chip.
struct MarkdownImage: Hashable, Sendable {
    var range: SourceRange
    /// The alt text; empty for `![](src)`.
    var alt: SourceRange
}

/// A list item's line: the bullet, number or box desktop paints where its
/// hidden marker was, and how deep it sits.
struct MarkdownListItem: Hashable, Sendable {
    var range: SourceRange
    /// Lists open above the item; 0 at the top level.
    var depth: Int
    /// `•`, or the renumbered `1.`, `a.` or `i.`; empty for a task.
    var label: String
    /// A task item's box, checked or not; nil for a plain item.
    var task: Bool?
    /// Shared by the items of one ordered list, so they share a marker width.
    var list: Int?

    init(range: SourceRange, depth: Int, label: String, task: Bool? = nil, list: Int? = nil) {
        self.range = range
        self.depth = depth
        self.label = label
        self.task = task
        self.list = list
    }

    fileprivate init(_ item: MdocFFI.MarkdownListItem) {
        self.init(
            range: SourceRange(item.range), depth: Int(item.depth), label: item.label,
            task: item.task, list: item.list.map(Int.init))
    }
}

/// A line's rendered-view role beyond its `MarkdownLine` kind.
struct MarkdownBlock: Hashable, Sendable {
    enum Alert: Hashable, Sendable {
        case note, tip, important, warning, caution
    }

    enum Kind: Hashable, Sendable {
        /// A code block's opening or closing fence.
        case codeFence
        /// A line of an alert; the title line holds its hidden marker.
        case alert(Alert, title: Bool)
        /// A footnote definition or raw HTML line, drawn greyed.
        case muted
    }

    var range: SourceRange
    var kind: Kind

    fileprivate init(_ block: MdocFFI.MarkdownBlock) {
        range = SourceRange(block.range)
        kind = switch block.kind {
        case .codeFence: .codeFence
        case .muted: .muted
        case .alert(let alert, let title):
            .alert(Self.alert(alert), title: title)
        }
    }

    private static func alert(_ alert: MdocFFI.MarkdownAlertKind) -> Alert {
        switch alert {
        case .note: .note
        case .tip: .tip
        case .important: .important
        case .warning: .warning
        case .caution: .caution
        }
    }
}

/// A GFM table the rendered view draws as a grid.
struct MarkdownTable: Hashable, Sendable {
    enum Align: Hashable, Sendable {
        case left, center, right
    }

    enum Style: Hashable, Sendable {
        case grid, striped, header, minimal
    }

    /// Every line of the table, its style marker included.
    var range: SourceRange
    /// The header row's cells, then each body row's; trimmed content ranges.
    var rows: [[SourceRange]]
    var aligns: [Align]
    var style: Style
    /// Desktop column widths whose proportions the grid keeps.
    var widths: [Double]?
    var rtl: Bool

    fileprivate init(_ table: MdocFFI.MarkdownTable) {
        range = SourceRange(table.range)
        rows = table.rows.map { $0.cells.map(SourceRange.init) }
        aligns = table.aligns.map {
            switch $0 {
            case .left: .left
            case .center: .center
            case .right: .right
            }
        }
        style = switch table.style {
        case .grid: .grid
        case .striped: .striped
        case .header: .header
        case .minimal: .minimal
        }
        widths = table.widths?.map(Double.init)
        rtl = table.rtl
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
