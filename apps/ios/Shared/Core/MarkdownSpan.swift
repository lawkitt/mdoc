import Foundation

/// A piece of Markdown syntax over source bytes. The kinds will be filled from
/// `mdoc-markdown`, the GUI-free span recognition desktop and mobile share
/// (ADR 0085); each platform maps them to its own colours and fonts.
struct MarkdownSpan: Hashable, Sendable {
    var kind: MarkdownSpanKind
    var range: SourceRange
}

enum MarkdownSpanKind: Hashable, Sendable {
    /// Syntax characters (`#`, `**`, `|`, list bullets): drawn dimmed.
    case marker
    case heading(level: Int)
    case strong
    case emphasis
    case code
    case codeBlock
    case link
    case blockQuote
}
