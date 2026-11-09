import MdocFFI
import UIKit

// The rendered view (ADR 0096): the Markdown source in one TextKit 2 text
// view, drawn as desktop draws it. The text never changes, so candidate
// offsets stay the core's source offsets: markers get a clear, near-zero
// font; a line that holds only markers folds to nothing; a custom layout
// fragment draws list labels, task boxes, quote bars, alert and code cards
// and rules; and each table's lines fold into one gap that a grid of cell
// text views covers.

extension NSAttributedString.Key {
    /// A source character the rendered view hides (a Markdown marker).
    static let mdocHidden = NSAttributedString.Key("io.github.lawkitt.mdoc.hidden")
    /// What a rendered line draws beside or behind its text.
    static let mdocDecoration = NSAttributedString.Key("io.github.lawkitt.mdoc.decoration")
    /// A `UIColor` to outline these characters with, a rounded box on each
    /// line they cover: the name under review (ADR 0099).
    static let mdocOutline = NSAttributedString.Key("io.github.lawkitt.mdoc.outline")
}

extension NSAttributedString {
    /// `text`, which starts at UTF-16 `start` of this string, as VoiceOver
    /// should read it: each hidden unit a word joiner (U+2060), which keeps
    /// the length and every offset, says nothing and breaks no word, so
    /// `12**34**` stays one number (ADR 0096). A hidden line break stays a
    /// line break: UIKit splits a long text view into paragraphs by the
    /// text and labels them by this, and one paragraph fewer here (a folded
    /// table's lines) shifted every later label off its range, so the
    /// candidates after it lost their link elements.
    func spoken(_ text: String, at start: Int) -> String {
        let spoken = NSMutableString(string: text)
        let range = NSIntersectionRange(NSRange(location: start, length: spoken.length), NSRange(location: 0, length: length))
        enumerateAttribute(.mdocHidden, in: range) { value, run, _ in
            guard value != nil else { return }
            var units = Array(repeating: unichar(0x2060), count: run.length)
            for index in 0..<run.length {
                let unit = spoken.character(at: run.location - start + index)
                if Self.lineBreaks.contains(unit) {
                    units[index] = unit
                }
            }
            spoken.replaceCharacters(
                in: NSRange(location: run.location - start, length: run.length),
                with: String(utf16CodeUnits: units, count: units.count))
        }
        return spoken as String
    }

    /// What ends a paragraph for `NSString`: LF, CR, NEL, LS, PS.
    private static let lineBreaks: Set<unichar> = [0x0A, 0x0D, 0x85, 0x2028, 0x2029]
}

/// The styled text and the tables laid over it.
struct RenderedDocument {
    var text: NSAttributedString
    var tables: [RenderedTable]
}

/// One table cell: its styled text and where it starts in the document.
struct RenderedCell {
    var text: NSAttributedString
    /// The cell's UTF-16 location in the document, for Replace… offsets.
    var location: Int
    /// Between the cell's pipes, in UTF-16: where edit mode's caret rests
    /// in this cell (ADR 0109). Its location is `NSNotFound` for a cell the
    /// row leaves out.
    var span = NSRange(location: NSNotFound, length: 0)
}

/// A table as its grid draws it: column widths and row heights for the text
/// column's width, each cell's text, and the line its gap sits on.
struct RenderedTable {
    /// UTF-16 location of the table's first line, which holds the gap.
    var anchor: Int
    /// Every line of the table, in UTF-16; its text is folded away.
    var range: NSRange
    var rows: [[RenderedCell]]
    var columns: [CGFloat]
    var heights: [CGFloat]
    var style: MarkdownTableStyle
    var rtl: Bool
    /// Space around a cell's text.
    var padding: UIEdgeInsets

    var width: CGFloat { columns.reduce(0, +) }
    var height: CGFloat { heights.reduce(0, +) }
}

extension RenderedTable {
    /// The table `delta` UTF-16 units further on.
    func shifted(by delta: Int) -> RenderedTable {
        var table = self
        table.anchor += delta
        table.range.location += delta
        for row in table.rows.indices {
            for column in table.rows[row].indices {
                table.rows[row][column].location += delta
                if table.rows[row][column].span.location != NSNotFound {
                    table.rows[row][column].span.location += delta
                }
            }
        }
        return table
    }
}

/// What one rendered line draws. Colours are resolved for the traits the
/// text was styled with, since the text is restyled when they change.
final class LineDecoration: NSObject {
    /// A list bullet or number, drawn where the hidden marker was.
    var label: (text: NSAttributedString, x: CGFloat)?
    /// A task box.
    var task: (checked: Bool, x: CGFloat, size: CGFloat, stroke: UIColor, fill: UIColor, tick: UIColor)?
    /// A quote's or alert's bar on the left.
    var bar: (x: CGFloat, width: CGFloat, color: UIColor)?
    /// A card behind the line, rounded at the card's first and last lines.
    var card: (x: CGFloat, fill: UIColor, top: Bool, bottom: Bool)?
    /// An alert's symbol and name, in place of its `[!NOTE]` marker.
    var title: (text: NSAttributedString, symbol: UIImage?, x: CGFloat)?
    /// A thematic break's line.
    var rule: UIColor?
    /// Image chips: each sits in the space kerned after the hidden `[` at
    /// `location` (paragraph-relative), with a symbol and, for an image with
    /// no alt text, a label.
    var chips: [(location: Int, width: CGFloat, symbol: UIImage?, label: NSAttributedString?, fill: UIColor)] = []
}

/// Lays out the rendered view's blocks over styled source text.
struct BlockLayout {
    let text: NSMutableAttributedString
    let source: NSString
    let offsets: TextOffsets
    let styling: MarkdownStyling
    let traits: UITraitCollection
    let body: UIFont
    let paragraph: NSParagraphStyle
    /// Whether each UTF-16 unit is a hidden marker.
    private var hidden: [Bool]
    /// The markers live preview's caret shows (ADR 0109), in UTF-16.
    let revealed: [NSRange]
    /// Edit mode: a line of nothing but hidden markers keeps its height, so
    /// the caret can reach it; tables and fences still fold.
    let editing: Bool
    /// Column widths to keep, by the table's UTF-16 location: while a cell
    /// is typed into, its table keeps its columns (ADR 0109).
    let pinned: [Int: [CGFloat]]

    init(
        text: NSMutableAttributedString, source: NSString, offsets: TextOffsets, styling: MarkdownStyling,
        hidden ranges: [NSRange], traits: UITraitCollection, body: UIFont, paragraph: NSParagraphStyle,
        revealed: [NSRange] = [], editing: Bool = false, pinned: [Int: [CGFloat]] = [:]
    ) {
        self.pinned = pinned
        self.text = text
        self.source = source
        self.offsets = offsets
        self.styling = styling
        self.traits = traits
        self.body = body
        self.paragraph = paragraph
        self.revealed = revealed
        self.editing = editing
        var hidden = Array(repeating: false, count: text.length)
        for range in ranges {
            for index in range.location..<min(NSMaxRange(range), hidden.count) {
                hidden[index] = true
            }
        }
        self.hidden = hidden
    }

    /// Point sizes scale with the reader's text size.
    private var scale: CGFloat { body.pointSize / 17 }

    private func resolved(_ colour: UIColor) -> UIColor {
        colour.resolvedColor(with: traits)
    }

    // MARK: Tables

    /// Each table's grid for a text column `width` points wide: cells wrap
    /// inside columns at least about 140 pt wide (scaled with the text), a
    /// table scrolls sideways when they do not fit, and stored column
    /// widths keep their proportions (ADR 0096).
    func tables(width: CGFloat) -> [RenderedTable] {
        let cellSize = MarkdownStyler.font(.subheadline, traits: traits).pointSize
        let ratio = cellSize / body.pointSize
        let padding = UIEdgeInsets(top: 6 * scale, left: 8 * scale, bottom: 6 * scale, right: 8 * scale)
        let minimum = 140 * scale
        let maximum = max(minimum, width * 0.8)
        return styling.tables.compactMap { table -> RenderedTable? in
            let tableRange = offsets.nsRange(table.range)
            guard tableRange.length > 0, !table.rows.isEmpty else { return nil }
            let count = max(table.aligns.count, table.rows.map(\.cells.count).max() ?? 0)
            guard count > 0 else { return nil }
            var rows: [[RenderedCell]] = []
            let spans = table.rows.map(\.spans)
            for (rowIndex, row) in table.rows.map(\.cells).enumerated() {
                var cells: [RenderedCell] = []
                for column in 0..<count {
                    let range = column < row.count ? offsets.nsRange(row[column]) : nil
                    let content = range.map { NSIntersectionRange($0, NSRange(location: 0, length: text.length)) }
                    let piece = NSMutableAttributedString(
                        attributedString: content.map { text.attributedSubstring(from: $0) } ?? NSAttributedString())
                    let align = column < table.aligns.count ? table.aligns[column] : .left
                    style(cell: piece, ratio: ratio, header: rowIndex == 0, align: align, rtl: table.rtl)
                    if let content {
                        // The cell's images, as chips drawn by the cell's own fragment.
                        let decoration = LineDecoration()
                        for image in styling.images {
                            let range = offsets.nsRange(image.range)
                            guard NSLocationInRange(range.location, content), range.length > 2 else { continue }
                            let alt = offsets.nsRange(image.alt)
                            decoration.chips.append(chip(
                                in: piece, image: NSRange(location: range.location - content.location, length: range.length),
                                alt: alt, font: body.withSize(body.pointSize * ratio), paragraph: 0))
                        }
                        if !decoration.chips.isEmpty {
                            let whole = NSRange(location: 0, length: piece.length)
                            piece.addAttribute(.mdocDecoration, value: decoration, range: whole)
                            // A cell of nothing but hidden image syntax keeps a line's height.
                            if let paragraph = (piece.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?
                                .mutableCopy() as? NSMutableParagraphStyle {
                                paragraph.minimumLineHeight = body.withSize(body.pointSize * ratio).lineHeight
                                piece.addAttribute(.paragraphStyle, value: paragraph, range: whole)
                            }
                        }
                    }
                    var cell = RenderedCell(text: piece, location: content?.location ?? tableRange.location)
                    if column < row.count, column < spans[rowIndex].count {
                        cell.span = offsets.nsRange(spans[rowIndex][column])
                    }
                    cells.append(cell)
                }
                rows.append(cells)
            }
            let heights = { (columns: [CGFloat]) -> [CGFloat] in
                rows.map { row -> CGFloat in
                    row.enumerated().map { column, cell -> CGFloat in
                        let inner = columns[column] - padding.left - padding.right
                        let size = cell.text.boundingRect(
                            with: CGSize(width: inner, height: .greatestFiniteMagnitude),
                            options: [.usesLineFragmentOrigin, .usesFontLeading], context: nil)
                        return ceil(size.height) + padding.top + padding.bottom + 2
                    }.max() ?? 0
                }
            }
            if let kept = pinned[tableRange.location], kept.count == count {
                return RenderedTable(
                    anchor: tableRange.location, range: tableRange, rows: rows, columns: kept, heights: heights(kept),
                    style: table.style, rtl: table.rtl, padding: padding)
            }
            // Natural widths: each column's longest unwrapped cell; floors:
            // its longest word, so a word never breaks inside.
            var natural = Array(repeating: CGFloat(0), count: count)
            var words = Array(repeating: CGFloat(0), count: count)
            for row in rows {
                for (column, cell) in row.enumerated() {
                    let size = cell.text.boundingRect(
                        with: CGSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude),
                        options: [.usesLineFragmentOrigin, .usesFontLeading], context: nil)
                    natural[column] = max(natural[column], ceil(size.width) + padding.left + padding.right)
                    words[column] = max(words[column], Self.longestWord(in: cell.text) + padding.left + padding.right)
                }
            }
            natural = natural.map { min(max($0, 44 * scale), maximum) }
            // The word floor may pass the cap: a long identifier widens its
            // column and the table scrolls, rather than breaking it.
            let floors = zip(natural, words).map { max(min($0, minimum), $1) }
            natural = zip(natural, floors).map { max($0, $1) }
            var columns: [CGFloat]
            if let stored = table.widths?.map(Double.init), stored.count == count, stored.reduce(0, +) > 0 {
                let total = stored.reduce(0, +)
                let span = max(width, CGFloat(count) * minimum * 0.75)
                columns = zip(stored, floors).map { max(CGFloat($0 / total) * span, $1) }
            } else if natural.reduce(0, +) <= width {
                // Everything fits on one line: stretch to the column.
                let total = natural.reduce(0, +)
                columns = natural.map { $0 / total * width }
            } else {
                // Shrink to the column, wrapping, but no narrower than the
                // minimum (or the cell's own width when that is less).
                let total = natural.reduce(0, +)
                columns = zip(natural, floors).map { max($0 / total * width, $1) }
            }
            columns = columns.map { $0.rounded(.up) }
            return RenderedTable(
                anchor: tableRange.location, range: tableRange, rows: rows, columns: columns, heights: heights(columns),
                style: table.style, rtl: table.rtl, padding: padding)
        }
    }

    /// The width of a cell's widest word, as drawn.
    private static func longestWord(in text: NSAttributedString) -> CGFloat {
        let string = text.string as NSString
        var widest: CGFloat = 0
        string.enumerateSubstrings(in: NSRange(location: 0, length: string.length), options: .byWords) { _, range, _, _ in
            widest = max(widest, ceil(text.attributedSubstring(from: range).size().width))
        }
        return widest
    }

    /// A cell's text: the document's styling at the cell's size, the header
    /// bold, aligned as its column.
    private func style(cell: NSMutableAttributedString, ratio: CGFloat, header: Bool, align: MarkdownAlign, rtl: Bool) {
        let whole = NSRange(location: 0, length: cell.length)
        cell.enumerateAttribute(.font, in: whole) { value, part, _ in
            guard let font = value as? UIFont, cell.attribute(.mdocHidden, at: part.location, effectiveRange: nil) == nil else { return }
            var sized = UIFont(descriptor: font.fontDescriptor, size: font.pointSize * ratio)
            if header {
                sized = sized.withTraits(.traitBold)
            }
            cell.addAttribute(.font, value: sized, range: part)
        }
        let paragraph = NSMutableParagraphStyle()
        paragraph.usesDefaultHyphenation = false
        paragraph.hyphenationFactor = 0
        paragraph.lineBreakStrategy = []
        paragraph.lineSpacing = 1
        paragraph.alignment = switch align {
        case .left: rtl ? .right : .left
        case .center: .center
        case .right: rtl ? .left : .right
        }
        cell.addAttribute(.paragraphStyle, value: paragraph, range: whole)
    }

    // MARK: Lines

    private enum Role {
        case plain
        case blank
        case heading(Int)
        case quote
        case alert(MarkdownAlertKind, title: Bool)
        case fence(open: Bool)
        case code
        case rule
        case list(MarkdownListItem)
        case table(Int, anchor: Bool)
        case muted
    }

    /// Folds lines, sets indents and spacing, and attaches each line's
    /// decoration; `tables` hold the grids whose gaps the table lines leave.
    mutating func apply(tables: [RenderedTable]) {
        let length = text.length
        var lineKinds: [Int: MarkdownLineKind] = [:]
        for line in styling.lines {
            lineKinds[offsets.nsRange(line.range).location] = line.kind
        }
        var items: [Int: MarkdownListItem] = [:]
        for item in styling.listItems {
            items[offsets.nsRange(item.range).location] = item
        }
        var blocks: [Int: MarkdownBlockKind] = [:]
        for block in styling.blocks {
            blocks[offsets.nsRange(block.range).location] = block.kind
        }
        let tableRanges = styling.tables.map { offsets.nsRange($0.range) }
        let images = styling.images.map { (offsets.nsRange($0.range), offsets.nsRange($0.alt)) }
        let laidOut = Dictionary(tables.map { ($0.anchor, $0) }, uniquingKeysWith: { first, _ in first })

        // Each line and its role, in order.
        var lines: [(range: NSRange, role: Role)] = []
        var index = 0
        var fenceOpen = false
        while index < length {
            let range = source.lineRange(for: NSRange(location: index, length: 0))
            index = NSMaxRange(range)
            let start = range.location
            let content = source.substring(with: range).trimmingCharacters(in: .newlines)
            let role: Role
            if let table = tableRanges.firstIndex(where: { NSLocationInRange(start, $0) }) {
                role = .table(table, anchor: start == tableRanges[table].location)
            } else if blocks[start] == .codeFence {
                role = .fence(open: !fenceOpen)
                fenceOpen.toggle()
            } else if lineKinds[start] == .codeBlock {
                role = .code
            } else if content.trimmingCharacters(in: .whitespaces).isEmpty {
                role = .blank
            } else if case .alert(let kind, let title) = blocks[start] {
                role = .alert(kind, title: title)
            } else if lineKinds[start] == .rule {
                role = .rule
            } else if let item = items[start] {
                role = .list(item)
            } else if case .heading(let level) = lineKinds[start] {
                role = .heading(Int(level))
            } else if lineKinds[start] == .quote {
                role = .quote
            } else if blocks[start] == .muted {
                role = .muted
            } else {
                role = .plain
            }
            lines.append((range, role))
        }

        // Ordered lists share one label width per list.
        var labelWidths: [UInt32: CGFloat] = [:]
        for item in styling.listItems {
            guard let list = item.list else { continue }
            labelWidths[list] = max(labelWidths[list] ?? 0, labelWidth(item.label))
        }

        let lineHeight = body.lineHeight
        for (position, line) in lines.enumerated() {
            let range = line.range
            let style = (text.attribute(.paragraphStyle, at: range.location, effectiveRange: nil) as? NSParagraphStyle ?? paragraph)
                .mutableCopy() as! NSMutableParagraphStyle
            let decoration = LineDecoration()
            var decorated = false
            var fold = false
            func neighbour(_ offset: Int) -> Role? {
                let other = position + offset
                return lines.indices.contains(other) ? lines[other].role : nil
            }
            switch line.role {
            case .plain, .muted:
                fold = !editing && isHidden(range)
            case .blank:
                break
            case .heading(let level):
                style.paragraphSpacingBefore = [14, 10, 6][min(level, 3) - 1] * scale
                style.paragraphSpacing = 4 * scale
                fold = !editing && isHidden(range)
            case .quote:
                card(style)
                style.firstLineHeadIndent = 16 * scale
                style.headIndent = 16 * scale
                style.minimumLineHeight = lineHeight * 1.15
                decoration.bar = (2 * scale, 3 * scale, resolved(.tertiaryLabel))
                decorated = true
            case .alert(let kind, let title):
                card(style)
                let colour = resolved(Self.colour(of: kind))
                let indent = 18 * scale
                let last: Bool = if case .alert(_, title: false)? = neighbour(1) { false } else { true }
                decoration.bar = (0, 3 * scale, colour)
                decoration.card = (0, colour.withAlphaComponent(traits.userInterfaceStyle == .dark ? 0.18 : 0.08), title, last)
                style.minimumLineHeight = lineHeight * (title ? 1.5 : 1.1)
                style.tailIndent = -8 * scale
                if title && shows(range) {
                    // The caret's line shows the `[!NOTE]` marker, not the title.
                    style.firstLineHeadIndent = indent
                    style.headIndent = indent
                } else if title {
                    let font = body.withTraits(.traitBold)
                    let name = NSAttributedString(string: Self.name(of: kind), attributes: [.font: font, .foregroundColor: colour])
                    let symbol = UIImage(systemName: Self.symbol(of: kind), withConfiguration: UIImage.SymbolConfiguration(font: body))?
                        .withTintColor(colour, renderingMode: .alwaysOriginal)
                    let symbolWidth = (symbol?.size.width ?? 0) + 6 * scale
                    decoration.title = (name, symbol, indent)
                    // Any words after the marker follow the drawn title.
                    style.firstLineHeadIndent = indent + symbolWidth + ceil(name.size().width) + 6 * scale
                    style.headIndent = indent
                } else {
                    style.firstLineHeadIndent = indent
                    style.headIndent = indent
                }
                decorated = true
            case .fence(let open):
                card(style)
                if shows(range) {
                    // The caret's fence shows its source on the card's edge.
                    style.firstLineHeadIndent = 12 * scale
                    style.headIndent = 12 * scale
                } else {
                    // The fence folds to the card's top or bottom padding.
                    style.minimumLineHeight = 8 * scale
                    style.maximumLineHeight = 8 * scale
                    text.addAttributes(MarkdownStyler.hiddenAttributes, range: range)
                }
                decoration.card = (0, resolved(.secondarySystemBackground), open, !open)
                decorated = true
            case .code:
                card(style)
                style.firstLineHeadIndent = 12 * scale
                style.headIndent = 12 * scale
                style.tailIndent = -12 * scale
                decoration.card = (0, resolved(.secondarySystemBackground), false, false)
                decorated = true
            case .rule:
                style.minimumLineHeight = lineHeight
                // The caret's rule shows its source instead.
                if !shows(range) {
                    decoration.rule = resolved(.separator)
                    decorated = true
                }
            case .list(let item):
                let x = CGFloat(item.depth) * 22 * scale
                let column: CGFloat
                let marker = resolved(.secondaryLabel)
                let source = shows(range)
                if source {
                    // The caret's line shows its marker where the label was;
                    // wrapped lines keep the body's indent.
                    let width = item.list.flatMap { labelWidths[$0] } ?? labelWidth(item.label)
                    column = item.task == nil ? max(width + 6 * scale, 18 * scale) : ceil(body.capHeight * 1.25) + 8 * scale
                } else if let checked = item.task {
                    let size = ceil(body.capHeight * 1.25)
                    decoration.task = (checked, x, size, marker, resolved(.tintColor), resolved(.onAccent))
                    column = size + 8 * scale
                } else {
                    let label = NSAttributedString(string: item.label, attributes: [.font: body, .foregroundColor: marker])
                    let width = item.list.flatMap { labelWidths[$0] } ?? labelWidth(item.label)
                    decoration.label = (label, x)
                    column = max(width + 6 * scale, 18 * scale)
                }
                style.firstLineHeadIndent = source ? x : x + column
                style.headIndent = x + column
                style.minimumLineHeight = lineHeight
                decorated = !source
            case .table(let table, let anchor):
                if anchor, let grid = laidOut[tableRanges[table].location] {
                    text.addAttributes(MarkdownStyler.hiddenAttributes, range: range)
                    strip(range)
                    style.minimumLineHeight = grid.height
                    style.maximumLineHeight = grid.height
                    style.lineSpacing = 0
                    style.paragraphSpacingBefore = 6 * scale
                    style.paragraphSpacing = 6 * scale
                } else if laidOut[tableRanges[table].location] != nil {
                    fold = true
                } else {
                    // No width yet: the source rows, as Show Markdown draws them.
                    break
                }
            }
            // Image chips, kerned into the space after each hidden `[`.
            for (image, alt) in images where NSLocationInRange(image.location, range) && image.length > 2 {
                if case .table = line.role { continue }
                if case .code = line.role { continue }
                // The caret's image shows its syntax, not a chip.
                if revealed.contains(where: { NSIntersectionRange($0, image).length > 0 }) { continue }
                decoration.chips.append(chip(in: text, image: image, alt: alt, font: body, paragraph: range.location))
                decorated = true
                fold = false
                // A line of nothing but hidden image syntax keeps a line's height.
                style.minimumLineHeight = max(style.minimumLineHeight, lineHeight)
            }
            if fold {
                text.addAttributes(MarkdownStyler.hiddenAttributes, range: range)
                strip(range)
                style.minimumLineHeight = Self.folded
                style.maximumLineHeight = Self.folded
                style.lineSpacing = 0
                style.paragraphSpacing = 0
                style.paragraphSpacingBefore = 0
            }
            text.addAttribute(.paragraphStyle, value: style, range: range)
            if decorated {
                text.addAttribute(.mdocDecoration, value: decoration, range: range)
            }
        }
    }

    /// An Image chip for `image` in `text`: a gap kerned after the hidden
    /// `[`, which the layout fragment fills with a photo symbol and, with no
    /// alt text, "Image"; VoiceOver hears "Image" after the alt text.
    /// `paragraph` is where the chip's paragraph starts in `text`.
    private func chip(
        in text: NSMutableAttributedString, image: NSRange, alt: NSRange, font: UIFont, paragraph: Int
    ) -> (location: Int, width: CGFloat, symbol: UIImage?, label: NSAttributedString?, fill: UIColor) {
        let symbol = UIImage(systemName: "photo", withConfiguration: UIImage.SymbolConfiguration(font: font))?
            .withTintColor(resolved(.secondaryLabel), renderingMode: .alwaysOriginal)
        let label: NSAttributedString? = alt.length == 0
            ? NSAttributedString(string: Self.imageLabel, attributes: [.font: font, .foregroundColor: resolved(.secondaryLabel)])
            : nil
        let pad = 4 * scale
        let width = pad + (symbol?.size.width ?? 0) + (label.map { 4 * scale + ceil($0.size().width) } ?? 0) + pad + 2 * scale
        text.addAttribute(.kern, value: width, range: NSRange(location: image.location + 1, length: 1))
        text.addAttribute(.accessibilityTextCustom, value: [Self.imageLabel], range: image)
        return (image.location + 1 - paragraph, width, symbol, label, resolved(.tertiarySystemFill))
    }

    /// A folded line's height: as near nothing as a paragraph style keeps
    /// (it reads 0.01 back as 0, which means no limit).
    static let folded: CGFloat = 0.1

    /// A card's or bar's lines touch: no paragraph spacing between them.
    private func card(_ style: NSMutableParagraphStyle) {
        style.paragraphSpacing = 0
        style.paragraphSpacingBefore = 0
    }

    /// Whether the caret shows this line's first marker (ADR 0109): its line
    /// prefix, or the whole fence or rule.
    private func shows(_ line: NSRange) -> Bool {
        guard !revealed.isEmpty else { return false }
        var first = line.location
        while first < NSMaxRange(line), [0x20, 0x09].contains(source.character(at: first)) { first += 1 }
        return revealed.contains { NSLocationInRange(first, $0) }
    }

    /// Whether every visible character of a line is a hidden marker.
    private func isHidden(_ range: NSRange) -> Bool {
        let string = source.substring(with: range) as NSString
        var any = false
        for offset in 0..<string.length {
            let unit = string.character(at: offset)
            if unit == 0x0A || unit == 0x0D || unit == 0x20 || unit == 0x09 { continue }
            guard hidden.indices.contains(range.location + offset), hidden[range.location + offset] else { return false }
            any = true
        }
        return any
    }

    /// No underline, fill or link left on folded text.
    private func strip(_ range: NSRange) {
        for key: NSAttributedString.Key in [.backgroundColor, .underlineStyle, .strikethroughStyle, .link] {
            text.removeAttribute(key, range: range)
        }
    }

    private func labelWidth(_ label: String) -> CGFloat {
        ceil((label as NSString).size(withAttributes: [.font: body]).width)
    }

    /// What an image with no alt text says in its chip.
    static var imageLabel: String {
        String(localized: "Image", comment: "Chip in place of a Markdown image with no description")
    }

    static func colour(of alert: MarkdownAlertKind) -> UIColor {
        switch alert {
        case .note: .systemBlue
        case .tip: .systemGreen
        case .important: .systemPurple
        case .warning: .systemOrange
        case .caution: .systemRed
        }
    }

    static func name(of alert: MarkdownAlertKind) -> String {
        switch alert {
        case .note: String(localized: "Note", comment: "A Markdown alert's title")
        case .tip: String(localized: "Tip", comment: "A Markdown alert's title")
        case .important: String(localized: "Important", comment: "A Markdown alert's title")
        case .warning: String(localized: "Warning", comment: "A Markdown alert's title")
        case .caution: String(localized: "Caution", comment: "A Markdown alert's title")
        }
    }

    static func symbol(of alert: MarkdownAlertKind) -> String {
        switch alert {
        case .note: "info.circle"
        case .tip: "lightbulb"
        case .important: "exclamationmark.bubble"
        case .warning: "exclamationmark.triangle"
        case .caution: "exclamationmark.octagon"
        }
    }
}

// MARK: Drawing

/// Draws a rendered line's decoration around the text TextKit lays out.
final class MarkdownLayoutFragment: NSTextLayoutFragment {
    private var decoration: LineDecoration? {
        guard let paragraph = textElement as? NSTextParagraph, paragraph.attributedString.length > 0 else { return nil }
        return paragraph.attributedString.attribute(.mdocDecoration, at: 0, effectiveRange: nil) as? LineDecoration
    }

    private var containerWidth: CGFloat {
        textLayoutManager?.textContainer?.size.width ?? layoutFragmentFrame.width
    }

    /// The paragraph's outlined stretches (`mdocOutline`), paragraph-relative.
    private var outlines: [(range: NSRange, colour: UIColor)] {
        guard let paragraph = textElement as? NSTextParagraph else { return [] }
        let string = paragraph.attributedString
        var found: [(range: NSRange, colour: UIColor)] = []
        string.enumerateAttribute(.mdocOutline, in: NSRange(location: 0, length: string.length)) { value, range, _ in
            if let colour = value as? UIColor { found.append((range, colour)) }
        }
        return found
    }

    /// How far an outline stands off its characters, and its line.
    private static let outlineOutset = CGSize(width: 2, height: 1)
    private static let outlineWidth: CGFloat = 1.5

    override var renderingSurfaceBounds: CGRect {
        var bounds = super.renderingSurfaceBounds
        if decoration != nil {
            bounds = bounds.union(CGRect(x: -layoutFragmentFrame.minX, y: 0, width: containerWidth, height: layoutFragmentFrame.height))
        }
        if !outlines.isEmpty {
            let margin = Self.outlineOutset.width + Self.outlineWidth
            bounds = bounds.insetBy(dx: -margin, dy: -margin)
        }
        return bounds
    }

    override func draw(at point: CGPoint, in context: CGContext) {
        guard let decoration else {
            super.draw(at: point, in: context)
            drawOutlines(at: point, in: context)
            return
        }
        defer { drawOutlines(at: point, in: context) }
        UIGraphicsPushContext(context)
        defer { UIGraphicsPopContext() }
        let left = point.x - layoutFragmentFrame.minX
        let height = layoutFragmentFrame.height
        if let card = decoration.card {
            let rect = CGRect(x: left + card.x, y: point.y, width: containerWidth - card.x, height: height)
            var corners: UIRectCorner = []
            if card.top { corners.formUnion([.topLeft, .topRight]) }
            if card.bottom { corners.formUnion([.bottomLeft, .bottomRight]) }
            card.fill.setFill()
            UIBezierPath(roundedRect: rect, byRoundingCorners: corners, cornerRadii: CGSize(width: 6, height: 6)).fill()
        }
        if let bar = decoration.bar {
            bar.color.setFill()
            UIBezierPath(rect: CGRect(x: left + bar.x, y: point.y, width: bar.width, height: height)).fill()
        }
        if let rule = decoration.rule {
            rule.setFill()
            let scale = max(UITraitCollection.current.displayScale, 1)
            UIBezierPath(rect: CGRect(x: left, y: point.y + height / 2, width: containerWidth, height: 1 / scale)).fill()
        }
        super.draw(at: point, in: context)
        // Labels sit on the first line's baseline.
        let line = textLineFragments.first
        let baseline = point.y + (line.map { $0.typographicBounds.minY + $0.glyphOrigin.y } ?? height * 0.8)
        if let label = decoration.label {
            let font = label.text.attribute(.font, at: 0, effectiveRange: nil) as? UIFont
            label.text.draw(at: CGPoint(x: left + label.x, y: baseline - (font?.ascender ?? 0)))
        }
        if let task = decoration.task {
            let box = CGRect(x: left + task.x, y: baseline - task.size, width: task.size, height: task.size)
                .insetBy(dx: 0.75, dy: 0.75)
            let path = UIBezierPath(roundedRect: box, cornerRadius: 3)
            path.lineWidth = 1.5
            if task.checked {
                task.fill.setFill()
                path.fill()
                let tick = UIBezierPath()
                tick.move(to: CGPoint(x: box.minX + box.width * 0.24, y: box.minY + box.height * 0.52))
                tick.addLine(to: CGPoint(x: box.minX + box.width * 0.42, y: box.minY + box.height * 0.70))
                tick.addLine(to: CGPoint(x: box.minX + box.width * 0.76, y: box.minY + box.height * 0.28))
                tick.lineWidth = 1.6
                task.tick.setStroke()
                tick.stroke()
            } else {
                task.stroke.setStroke()
                path.stroke()
            }
        }
        for chip in decoration.chips {
            // The kerned space starts at the hidden `[`, which has next to no
            // width of its own.
            guard let line = textLineFragments.first(where: { NSLocationInRange(chip.location, $0.characterRange) }) else { continue }
            let start = line.locationForCharacter(at: chip.location)
            let top = point.y + line.typographicBounds.minY
            // Centred in its line: a line of nothing but hidden image syntax
            // has its baseline at the bottom.
            let font = chip.label?.attribute(.font, at: 0, effectiveRange: nil) as? UIFont
            let lineHeight = line.typographicBounds.height
            let height = min(font?.lineHeight ?? lineHeight * 0.8, lineHeight) + 2
            let box = CGRect(
                x: point.x + line.typographicBounds.minX + start.x + 1,
                y: top + (lineHeight - height) / 2, width: chip.width - 5, height: height)
            chip.fill.setFill()
            UIBezierPath(roundedRect: box, cornerRadius: 5).fill()
            var x = box.minX + 4
            if let symbol = chip.symbol {
                symbol.draw(at: CGPoint(x: x, y: box.midY - symbol.size.height / 2))
                x += symbol.size.width + 4
            }
            if let label = chip.label {
                label.draw(at: CGPoint(x: x, y: box.midY - (font?.lineHeight ?? label.size().height) / 2))
            }
        }
        if let title = decoration.title {
            // The title's line holds only the hidden marker, so the title
            // centres in the line rather than sitting on its baseline.
            let font = title.text.attribute(.font, at: 0, effectiveRange: nil) as? UIFont
            let lineTop = point.y + (line?.typographicBounds.minY ?? 0)
            let lineHeight = line?.typographicBounds.height ?? height
            let middle = lineTop + lineHeight / 2
            var x = left + title.x
            if let symbol = title.symbol {
                symbol.draw(at: CGPoint(x: x, y: middle - symbol.size.height / 2))
                x += symbol.size.width + 6
            }
            title.text.draw(at: CGPoint(x: x, y: middle - (font?.lineHeight ?? lineHeight) / 2))
        }
    }

    private func drawOutlines(at point: CGPoint, in context: CGContext) {
        let boxes = outlineBoxes(at: point)
        guard !boxes.isEmpty else { return }
        UIGraphicsPushContext(context)
        defer { UIGraphicsPopContext() }
        for (box, colour) in boxes {
            let path = UIBezierPath(roundedRect: box, cornerRadius: 4)
            path.lineWidth = Self.outlineWidth
            colour.setStroke()
            path.stroke()
        }
    }

    /// A rounded box around each outlined stretch on each line it covers,
    /// for a fragment drawn at `point`. Its sides span the stretch's text
    /// segments on that line, which stay true where the stretch and its
    /// line run in different directions (a Hebrew name in English text);
    /// its height is the font's there, from the line's baseline, so a
    /// heading's name gets a taller box.
    func outlineBoxes(at point: CGPoint) -> [(box: CGRect, colour: UIColor)] {
        let outlines = outlines
        guard !outlines.isEmpty, let layout = textLayoutManager, let content = layout.textContentManager,
              let start = textElement?.elementRange?.location else { return [] }
        let origin = CGPoint(x: point.x - layoutFragmentFrame.minX, y: point.y - layoutFragmentFrame.minY)
        var boxes: [(box: CGRect, colour: UIColor)] = []
        for outline in outlines {
            for line in textLineFragments {
                let part = NSIntersectionRange(outline.range, line.characterRange)
                guard part.length > 0,
                      let from = content.location(start, offsetBy: part.location),
                      let to = content.location(from, offsetBy: part.length),
                      let range = NSTextRange(location: from, end: to) else { continue }
                var left = CGFloat.infinity, right = -CGFloat.infinity
                layout.enumerateTextSegments(in: range, type: .standard, options: [.rangeNotRequired]) { _, frame, _, _ in
                    left = min(left, frame.minX)
                    right = max(right, frame.maxX)
                    return true
                }
                guard left < right else { continue }
                let font = line.attributedString.attribute(.font, at: part.location, effectiveRange: nil) as? UIFont
                let baseline = point.y + line.typographicBounds.minY + line.glyphOrigin.y
                let ascender = font?.ascender ?? line.typographicBounds.height * 0.8
                let descender = font?.descender ?? 0
                let box = CGRect(x: origin.x + left, y: baseline - ascender, width: right - left, height: ascender - descender)
                    .insetBy(dx: -Self.outlineOutset.width, dy: -Self.outlineOutset.height)
                boxes.append((box, outline.colour))
            }
        }
        return boxes
    }
}

// MARK: Tables

/// A table cell's selectable text. Its location maps a selection back to
/// document offsets; outside the Reply view it allows no copy, as the main
/// text does (ADR 0085's export invariant).
final class CellTextView: UITextView, UIAccessibilityContainerDataTableCell {
    var location = 0
    var row = 0
    var column = 0
    var allowsCopy = false

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if allowsCopy { return super.canPerformAction(action, withSender: sender) }
        return [
            #selector(UIResponderStandardEditActions.select(_:)),
            #selector(UIResponderStandardEditActions.selectAll(_:)),
        ].contains(action)
    }

    override func copy(_ sender: Any?) {
        if allowsCopy { super.copy(sender) }
    }

    /// In the review, where nothing copies, VoiceOver hears the cell as
    /// drawn: hidden markers as word joiners (see `spoken`).
    override func text(in range: UITextRange) -> String? {
        guard let text = super.text(in: range) else { return nil }
        return allowsCopy ? text : attributedText.spoken(text, at: offset(from: beginningOfDocument, to: range.start))
    }

    override var accessibilityValue: String? {
        get { allowsCopy ? super.accessibilityValue : attributedText.spoken(attributedText.string, at: 0) }
        set { super.accessibilityValue = newValue }
    }

    func accessibilityRowRange() -> NSRange { NSRange(location: row, length: 1) }
    func accessibilityColumnRange() -> NSRange { NSRange(location: column, length: 1) }

    /// Edit mode (ADR 0109): VoiceOver's double-tap puts the caret in this
    /// cell, where the keyboard types into it. Nil in review.
    var onActivate: (() -> Bool)? {
        didSet {
            accessibilityHint = onActivate == nil
                ? nil
                : String(localized: "Double-tap to edit this cell.", comment: "VoiceOver hint on a table cell in edit mode")
        }
    }

    override func accessibilityActivate() -> Bool {
        onActivate?() ?? super.accessibilityActivate()
    }
}

/// One table's grid: cell text views over drawn lines and fills, scrolling
/// sideways when wider than the text column, with a fade at the edge that
/// has more. VoiceOver reads it as a data table.
///
/// In edit mode (ADR 0109) the cells take no touches: the text view behind
/// keeps the caret, in the cell's source, and draws it in the cell. A tap
/// on the grid says where (`onTap`), and the grid still scrolls sideways.
final class MarkdownTableView: UIScrollView, UIScrollViewDelegate, UIAccessibilityContainerDataTable {
    /// The table drawn; edit mode moves its place as text before it changes.
    var table: RenderedTable
    private(set) var cells: [[CellTextView]] = []
    private let grid: GridView
    private let fade = CAGradientLayer()
    private let configure: (CellTextView) -> Void

    init(table: RenderedTable, configure: @escaping (CellTextView) -> Void) {
        self.table = table
        self.configure = configure
        grid = GridView(table: table)
        super.init(frame: .zero)
        showsHorizontalScrollIndicator = false
        showsVerticalScrollIndicator = false
        alwaysBounceVertical = false
        isDirectionalLockEnabled = true
        delegate = self
        addSubview(grid)
        for (rowIndex, row) in table.rows.enumerated() {
            var views: [CellTextView] = []
            for (column, cell) in row.enumerated() {
                let view = makeCell(cell, row: rowIndex, column: column)
                addSubview(view)
                views.append(view)
            }
            cells.append(views)
        }
        layoutCells()
        fade.colors = [UIColor.black.cgColor, UIColor.black.cgColor, UIColor.clear.cgColor]
        fade.startPoint = CGPoint(x: 0, y: 0.5)
        fade.endPoint = CGPoint(x: 1, y: 0.5)
        accessibilityContainerType = .dataTable
        accessibilityLabel = String(localized: "Table", comment: "VoiceOver name of a Markdown table")
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is unused") }

    private func makeCell(_ cell: RenderedCell, row: Int, column: Int) -> CellTextView {
        // TextKit 2, so the cell's layout fragments draw Image chips;
        // `configure` sets their delegate before the text arrives.
        let view = CellTextView(usingTextLayoutManager: true)
        view.isEditable = false
        view.isSelectable = true
        view.isScrollEnabled = false
        view.backgroundColor = .clear
        view.dataDetectorTypes = []
        view.writingToolsBehavior = .none
        view.linkTextAttributes = [:]
        view.textDragInteraction?.isEnabled = false
        view.textContainerInset = table.padding
        view.textContainer.lineFragmentPadding = 0
        view.textLayoutManager?.usesHyphenation = false
        configure(view)
        view.attributedText = cell.text
        view.location = cell.location
        view.row = row
        view.column = column
        if row == 0 {
            view.accessibilityTraits.insert(.header)
        }
        view.isUserInteractionEnabled = onTap == nil
        view.onActivate = onTap == nil ? nil : { [weak self, weak view] in
            guard let self, let view else { return false }
            self.onTap?(view.center)
            return true
        }
        return view
    }

    /// Frames for every cell and the grid at the table's sizes.
    private func layoutCells() {
        var y: CGFloat = 0
        for (rowIndex, views) in cells.enumerated() {
            var x: CGFloat = 0
            for (column, view) in views.enumerated() {
                let frame = CGRect(x: x, y: y, width: table.columns[column], height: table.heights[rowIndex])
                if view.frame != frame { view.frame = frame }
                x += table.columns[column]
            }
            y += table.heights[rowIndex]
        }
        grid.frame = CGRect(x: 0, y: 0, width: table.width, height: table.height)
        contentSize = grid.frame.size
    }

    /// Shows `new` in place when it has the same rows and columns: only the
    /// cells whose text changed take it, and frames follow the new sizes. A
    /// cell typed into relayouts its row and leaves the other cells alone
    /// (ADR 0109). False, changing nothing, when the shape differs.
    func update(to new: RenderedTable) -> Bool {
        guard new.rows.count == table.rows.count,
              zip(new.rows, table.rows).allSatisfy({ $0.count == $1.count }),
              new.columns.count == table.columns.count, new.padding == table.padding, new.rtl == table.rtl else { return false }
        for (row, views) in cells.enumerated() {
            for (column, view) in views.enumerated() {
                let cell = new.rows[row][column]
                if !view.attributedText.isEqual(to: cell.text) {
                    view.attributedText = cell.text
                }
                view.location = cell.location
            }
        }
        let resized = new.columns != table.columns || new.heights != table.heights || new.style != table.style
        table = new
        if resized {
            grid.table = new
            grid.setNeedsDisplay()
            layoutCells()
        }
        return true
    }

    /// A horizontal offset to return to once the grid has its frame.
    var restoredOffset: CGFloat?

    /// Edit mode (ADR 0109): a tap on the grid runs this with where it
    /// landed, in this view's coordinates, and the cells take no touches.
    /// Nil outside edit mode, where cells are selectable.
    var onTap: ((CGPoint) -> Void)? {
        didSet {
            let editing = onTap != nil
            for view in cells.joined() {
                view.isUserInteractionEnabled = !editing
                view.onActivate = editing ? { [weak self, weak view] in
                    guard let self, let view else { return false }
                    self.onTap?(view.center)
                    return true
                } : nil
            }
            if editing, tap == nil {
                let tap = UITapGestureRecognizer(target: self, action: #selector(tapped(_:)))
                addGestureRecognizer(tap)
                self.tap = tap
            }
            tap?.isEnabled = editing
        }
    }

    /// The grid scrolled sideways: edit mode draws its caret again.
    var onScroll: (() -> Void)?

    private var tap: UITapGestureRecognizer?

    @objc private func tapped(_ recognizer: UITapGestureRecognizer) {
        onTap?(recognizer.location(in: self))
    }

    /// The cell at `point` (this view's coordinates), or the nearest one.
    func cellView(at point: CGPoint) -> CellTextView? {
        cells.joined().min { a, b in
            Self.distance(from: point, to: a.frame) < Self.distance(from: point, to: b.frame)
        }
    }

    private static func distance(from point: CGPoint, to rect: CGRect) -> CGFloat {
        let dx = max(rect.minX - point.x, 0, point.x - rect.maxX)
        let dy = max(rect.minY - point.y, 0, point.y - rect.maxY)
        return dx * dx + dy * dy
    }

    /// The document's UTF-16 offset a tap at `point` puts the caret at: the
    /// nearest cell's character there.
    func offset(at point: CGPoint) -> Int? {
        guard let view = cellView(at: point) else { return nil }
        let inside = convert(point, to: view)
        let position = view.closestPosition(to: inside) ?? view.endOfDocument
        return view.location + view.offset(from: view.beginningOfDocument, to: position)
    }

    /// The cell whose span holds the document's UTF-16 `offset`, and where
    /// in its text that is: a caret in the cell's spaces sits at its text's
    /// edge.
    func cell(at offset: Int) -> (view: CellTextView, offset: Int)? {
        for (row, cellsInRow) in table.rows.enumerated() {
            for (column, cell) in cellsInRow.enumerated() where cell.span.location != NSNotFound {
                guard cell.span.location <= offset, offset <= NSMaxRange(cell.span),
                      cells.indices.contains(row), cells[row].indices.contains(column) else { continue }
                let view = cells[row][column]
                let length = view.textStorage.length
                return (view, min(max(offset - view.location, 0), length))
            }
        }
        return nil
    }

    /// The span between `view`'s pipes, in the document's UTF-16 offsets.
    func span(of view: CellTextView) -> NSRange? {
        guard table.rows.indices.contains(view.row), table.rows[view.row].indices.contains(view.column) else { return nil }
        let span = table.rows[view.row][view.column].span
        return span.location == NSNotFound ? nil : span
    }

    /// Scrolls sideways so `rect` (this view's coordinates) shows, with a
    /// cell's padding beside it.
    func reveal(_ rect: CGRect) {
        guard contentSize.width > bounds.width, bounds.width > 0 else { return }
        let wanted = rect.insetBy(dx: -table.padding.left - 2, dy: 0)
        var x = contentOffset.x
        if wanted.minX < x {
            x = wanted.minX
        } else if wanted.maxX > x + bounds.width {
            x = wanted.maxX - bounds.width
        }
        x = min(max(x, 0), contentSize.width - bounds.width)
        if abs(x - contentOffset.x) > 0.5 {
            setContentOffset(CGPoint(x: x, y: 0), animated: false)
        }
    }

    /// Moves the table's place by `delta` UTF-16 units: text before it changed.
    func shift(by delta: Int) {
        table = table.shifted(by: delta)
        for views in cells {
            for view in views {
                view.location += delta
            }
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if let x = restoredOffset, bounds.width > 0 {
            restoredOffset = nil
            contentOffset.x = min(x, max(contentSize.width - bounds.width, 0))
        }
        updateFade()
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        updateFade()
        onScroll?()
    }

    /// Fades the trailing edge while more of the table is off screen.
    private func updateFade() {
        let more = contentSize.width - contentOffset.x - bounds.width > 1
        guard more, bounds.width > 0 else {
            layer.mask = nil
            return
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        fade.frame = bounds
        let edge = min(28 / bounds.width, 0.3)
        fade.locations = [0, NSNumber(value: Double(1 - edge)), 1]
        layer.mask = fade
        CATransaction.commit()
    }

    func accessibilityRowCount() -> Int { cells.count }
    func accessibilityColumnCount() -> Int { cells.first?.count ?? 0 }

    func accessibilityDataTableCellElement(forRow row: Int, column: Int) -> (any UIAccessibilityContainerDataTableCell)? {
        guard cells.indices.contains(row), cells[row].indices.contains(column) else { return nil }
        return cells[row][column]
    }

    func accessibilityHeaderElements(forColumn column: Int) -> [any UIAccessibilityContainerDataTableCell]? {
        guard let header = cells.first, header.indices.contains(column) else { return nil }
        return [header[column]]
    }

    override var accessibilityElements: [Any]? {
        get { cells.flatMap { $0 } }
        set {}
    }
}

/// A table's lines and fills, in its style.
private final class GridView: UIView {
    var table: RenderedTable

    init(table: RenderedTable) {
        self.table = table
        super.init(frame: .zero)
        backgroundColor = .clear
        isUserInteractionEnabled = false
        isAccessibilityElement = false
        // Lines and fills are dynamic colours: redraw in the new appearance.
        registerForTraitChanges([UITraitUserInterfaceStyle.self, UITraitAccessibilityContrast.self]) { (view: GridView, _) in
            view.setNeedsDisplay()
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is unused") }

    override func draw(_ rect: CGRect) {
        let hairline = 1 / max(traitCollection.displayScale, 1)
        let header = table.heights.first ?? 0
        let bounds = self.bounds
        // Fills: the header (not Minimal), striped body rows.
        if table.style != .minimal {
            UIColor.secondarySystemFill.setFill()
            UIBezierPath(rect: CGRect(x: 0, y: 0, width: bounds.width, height: header)).fill()
        }
        if table.style == .striped {
            UIColor.quaternarySystemFill.setFill()
            var y = header
            for (index, height) in table.heights.dropFirst().enumerated() {
                if index % 2 == 1 {
                    UIBezierPath(rect: CGRect(x: 0, y: y, width: bounds.width, height: height)).fill()
                }
                y += height
            }
        }
        UIColor.separator.setFill()
        func line(_ rect: CGRect) { UIBezierPath(rect: rect).fill() }
        switch table.style {
        case .grid:
            var y: CGFloat = 0
            for height in table.heights.dropLast() {
                y += height
                line(CGRect(x: 0, y: y - hairline / 2, width: bounds.width, height: hairline))
            }
            var x: CGFloat = 0
            for width in table.columns.dropLast() {
                x += width
                line(CGRect(x: x - hairline / 2, y: 0, width: hairline, height: bounds.height))
            }
            let box = UIBezierPath(rect: bounds.insetBy(dx: hairline / 2, dy: hairline / 2))
            box.lineWidth = hairline
            UIColor.separator.setStroke()
            box.stroke()
        case .striped, .minimal:
            line(CGRect(x: 0, y: header - hairline / 2, width: bounds.width, height: hairline))
        case .header:
            break
        }
    }

}

// MARK: The text view

/// A read-only TextKit 2 text view that shows a `RenderedDocument`: it
/// draws line decorations through `MarkdownLayoutFragment` and lays each
/// table's grid over its gap. When its text column changes width it asks
/// for the document again (`onWidth`), since tables are laid out for one.
class RenderedTextView: UITextView, NSTextLayoutManagerDelegate {
    /// Asks the owner to render for a new text column width.
    var onWidth: ((CGFloat) -> Void)?
    /// Sets up each table cell (delegate, copy rule) as it is made.
    var configureCell: ((CellTextView) -> Void)?
    private(set) var tableViews: [MarkdownTableView] = []
    private var shownWidth: CGFloat = 0

    /// A TextKit 2 text view that draws decorations. Built here rather than
    /// through `UITextView(usingTextLayoutManager:)`, which skips a Swift
    /// subclass's property defaults.
    init() {
        let container = NSTextContainer(size: CGSize(width: 0, height: CGFloat.greatestFiniteMagnitude))
        container.widthTracksTextView = true
        let layout = NSTextLayoutManager()
        layout.textContainer = container
        let content = NSTextContentStorage()
        content.addTextLayoutManager(layout)
        content.primaryTextLayoutManager = layout
        super.init(frame: .zero, textContainer: container)
        layout.delegate = self
        // A short text takes a drag too, so it never feels frozen.
        alwaysBounceVertical = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is unused") }

    /// The width text lays out in: the view less its insets and padding.
    var textWidth: CGFloat {
        bounds.width - textContainerInset.left - textContainerInset.right - 2 * textContainer.lineFragmentPadding
    }

    /// Shows `document`, laid out for `width`.
    func show(_ document: RenderedDocument, width: CGFloat) {
        attributedText = document.text
        shownWidth = width
        setTables(document.tables)
    }

    /// Edit mode's tap on a table, with where it landed in the table's
    /// coordinates (ADR 0109): nil outside edit mode, where cells are
    /// selectable.
    var onTableTap: ((MarkdownTableView, CGPoint) -> Void)? {
        didSet {
            for view in tableViews { connectTap(view) }
        }
    }

    /// A table scrolled sideways (edit mode draws its caret again).
    var onTableScroll: ((MarkdownTableView) -> Void)? {
        didSet {
            for view in tableViews { connectTap(view) }
        }
    }

    private func connectTap(_ view: MarkdownTableView) {
        view.onTap = onTableTap.map { tap in { [weak view] point in if let view { tap(view, point) } } }
        view.onScroll = onTableScroll.map { scrolled in { [weak view] in if let view { scrolled(view) } } }
    }

    /// Lays `tables` over the text, in place of the grids shown. Restyling
    /// (a candidate opened or decided) keeps each table where the reader
    /// scrolled it.
    func setTables(_ tables: [RenderedTable]) {
        replaceTables(in: nil, with: tables)
    }

    /// Replaces the grids of the tables inside `range` (UTF-16; every grid
    /// when nil) with `tables`: edit mode restyles one block at a time.
    func replaceTables(in range: NSRange?, with tables: [RenderedTable]) {
        var scrolled: [Int: CGFloat] = [:]
        var kept: [MarkdownTableView] = []
        var replaced: [Int: MarkdownTableView] = [:]
        for view in tableViews {
            if let range, NSIntersectionRange(range, view.table.range).length == 0 && !NSLocationInRange(view.table.anchor, range) {
                kept.append(view)
                continue
            }
            replaced[view.table.anchor] = view
        }
        // A table of the same shape is shown again in place (a cell typed
        // into), keeping its cells and where it is scrolled.
        var fresh: [RenderedTable] = []
        for table in tables {
            if let view = replaced[table.anchor], view.update(to: table) {
                replaced[table.anchor] = nil
                kept.append(view)
            } else {
                fresh.append(table)
            }
        }
        for view in replaced.values {
            scrolled[view.table.anchor] = view.contentOffset.x
            view.removeFromSuperview()
        }
        let added = fresh.map { table in
            let view = MarkdownTableView(table: table) { [weak self] cell in
                cell.textLayoutManager?.delegate = self
                self?.configureCell?(cell)
            }
            addSubview(view)
            connectTap(view)
            if let x = scrolled[table.anchor] {
                view.restoredOffset = x
            }
            return view
        }
        tableViews = (kept + added).sorted { $0.table.anchor < $1.table.anchor }
        setNeedsLayout()
    }

    /// Called at the end of a layout pass whose text and tables are placed:
    /// where a scroll to a range can trust the text's rects. A pass that
    /// rendered again for a new width skips it; the next pass calls it.
    var onLayout: (() -> Void)?

    override func layoutSubviews() {
        super.layoutSubviews()
        let width = textWidth
        var renderedAgain = false
        if width > 0, abs(width - shownWidth) > 0.5 {
            shownWidth = width
            onWidth?(width)
            renderedAgain = true
        }
        if !tableViews.isEmpty {
            placeTables()
        }
        if renderedAgain {
            setNeedsLayout()
        } else {
            onLayout?()
        }
    }

    /// Puts each grid on the gap its table's first line leaves, on every
    /// layout pass (scrolling is one). TextKit 2 estimates the lines it has
    /// not laid out and moves the text as it lays them out for the
    /// viewport, so a grid placed once drifted off its gap: after a restyle
    /// while scrolled, screens away. Here, after the viewport's layout, the
    /// fragment's frame is the one the text is drawn with.
    func placeTables() {
        guard let layout = textLayoutManager, let content = layout.textContentManager else { return }
        let start = content.documentRange.location
        let x = textContainerInset.left + textContainer.lineFragmentPadding
        let column = textWidth
        for view in tableViews {
            guard let location = content.location(start, offsetBy: view.table.anchor),
                  let fragment = layout.textLayoutFragment(for: location) else { continue }
            if fragment.state != .layoutAvailable {
                // Off screen and not laid out yet: an estimate, corrected
                // once it scrolls into the viewport.
                layout.ensureLayout(for: fragment.rangeInElement)
            }
            // The line the table starts on: its paragraph may begin earlier.
            let paragraphStart = fragment.textElement?.elementRange?.location ?? fragment.rangeInElement.location
            let into = content.offset(from: paragraphStart, to: location)
            let line = fragment.textLineFragments.first { NSLocationInRange(into, $0.characterRange) }
                ?? fragment.textLineFragments.first
            let y = fragment.layoutFragmentFrame.minY + (line?.typographicBounds.minY ?? 0) + textContainerInset.top
            let width = min(column, view.table.width)
            let offset = view.table.rtl ? column - width : 0
            let frame = CGRect(x: x + offset, y: y, width: width, height: view.table.height)
            if view.frame != frame {
                view.frame = frame
            }
        }
    }

    /// Where the first line of `range` is drawn, in this view's
    /// coordinates, and the grids put on their gaps again. TextKit 2 lays
    /// out lazily: outside the viewport a fragment's place is an estimate,
    /// which `firstRect(for:)` answers with, and an edit (recolouring the
    /// name under review) makes every place after it an estimate again.
    /// Laying out only up to the range, fragment by fragment or with
    /// `ensureLayout(for:)`, kept the estimate; only the whole text gives
    /// the place it is drawn at once scrolled there. So a range outside the
    /// viewport lays out the whole text first; one inside it is measured
    /// where it is drawn.
    func laidOutRect(of range: NSRange) -> CGRect? {
        guard let layout = textLayoutManager, let content = layout.textContentManager else { return nil }
        let start = content.documentRange.location
        // The storage's length: `attributedText` copies the whole text.
        guard NSMaxRange(range) <= textStorage.length,
              let from = content.location(start, offsetBy: range.location) else { return nil }
        let viewport = layout.textViewportLayoutController.viewportRange
        let shown = viewport.map { $0.contains(from) && layout.textLayoutFragment(for: from)?.state == .layoutAvailable } ?? false
        if !shown {
            layout.ensureLayout(for: content.documentRange)
            if !tableViews.isEmpty { placeTables() }
        }
        guard let fragment = layout.textLayoutFragment(for: from) else { return nil }
        // Lines count from their paragraph's start, as in `placeTables`.
        let paragraphStart = fragment.textElement?.elementRange?.location ?? fragment.rangeInElement.location
        let into = content.offset(from: paragraphStart, to: from)
        guard let line = fragment.textLineFragments.first(where: { NSLocationInRange(into, $0.characterRange) })
            ?? fragment.textLineFragments.last else { return nil }
        let frame = fragment.layoutFragmentFrame
        let bounds = line.typographicBounds
        let x = line.locationForCharacter(at: into).x
        let end = line.locationForCharacter(at: min(into + range.length, NSMaxRange(line.characterRange))).x
        return CGRect(
            x: frame.minX + bounds.minX + min(x, end) + textContainerInset.left,
            y: frame.minY + bounds.minY + textContainerInset.top,
            width: abs(end - x), height: bounds.height)
    }

    /// The text's height as laid out so far: the content size lags behind
    /// a layout `laidOutRect(of:)` asked for until the next pass.
    var laidOutContentHeight: CGFloat {
        let used = textLayoutManager?.usageBoundsForTextContainer.maxY ?? 0
        return max(contentSize.height, used + textContainerInset.top + textContainerInset.bottom)
    }

    nonisolated func textLayoutManager(
        _ textLayoutManager: NSTextLayoutManager, textLayoutFragmentFor location: any NSTextLocation,
        in textElement: NSTextElement
    ) -> NSTextLayoutFragment {
        MarkdownLayoutFragment(textElement: textElement, range: textElement.elementRange)
    }
}
