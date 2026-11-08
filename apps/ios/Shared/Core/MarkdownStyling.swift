import Foundation
import MdocFFI

/// The Markdown spans of a text, from the core: `mdoc-markdown`, the
/// recognition the desktop editor uses, through `mdoc-ffi` (ADR 0085's Text
/// display rule). Ranges are source bytes like candidates'; each platform maps
/// the kinds to its own colours and fonts.
extension MarkdownStyling {
    static let empty = MarkdownStyling(spans: [], lines: [], listItems: [], blocks: [], tables: [], images: [])

    /// Asks the core for the spans of `markdown`.
    init(markdown: String) {
        self = MdocFFI.markdownStyling(text: markdown)
    }
}
