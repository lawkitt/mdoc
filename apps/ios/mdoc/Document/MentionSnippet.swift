import Foundation

/// One mention in the review bar's full view: a few words either side of
/// the name, the name itself in its original wording, and the heading it
/// sits under ("Parties"). Markdown markers and line breaks are dropped and
/// the ends are cut at word boundaries, so the name is never cut off.
struct MentionSnippet: Hashable, Sendable {
    var heading: String?
    var before: String
    var name: String
    var after: String

    /// Characters kept on each side of the name.
    static let radius = 44

    /// `range` is in UTF-8 bytes of `markdown`, as the core's offsets are.
    init(markdown: String, range: SourceRange, name: String, radius: Int = MentionSnippet.radius) {
        let utf8 = markdown.utf8
        let lower = utf8.index(utf8.startIndex, offsetBy: min(max(range.start, 0), utf8.count))
        let upper = utf8.index(utf8.startIndex, offsetBy: min(max(range.end, range.start), utf8.count))
        // Only what a row can use: a window around the name, and the heading
        // found by searching back, so a long document is never split whole.
        // Windows line ends become plain ones (Swift reads "\r\n" as one
        // character).
        let window = radius * 6
        let head = Substring(markdown[..<lower].suffix(window).replacingOccurrences(of: "\r\n", with: "\n"))
        let tail = Substring(markdown[upper...].prefix(window).replacingOccurrences(of: "\r\n", with: "\n"))
        // The words come from the name's own paragraph: never across a
        // blank line or a heading.
        let ownHead = Self.paragraph(endingWith: head)
        let ownTail = Self.paragraph(startingWith: tail)
        self.name = name
        heading = Self.heading(before: markdown[..<lower])
        // A generous slice first, cleaned, then trimmed to the radius: the
        // markers it drops would otherwise eat the budget.
        let rawBefore = Self.plain(String(ownHead.suffix(radius * 3)), currentLine: true)
        let rawAfter = Self.plain(String(ownTail.prefix(radius * 3)), currentLine: true)
        before = Self.trimFront(rawBefore, to: radius, cut: ownHead.count > radius * 3 || rawBefore.count > radius)
        after = Self.trimBack(rawAfter, to: radius, cut: ownTail.count > radius * 3 || rawAfter.count > radius)
    }

    init(heading: String?, before: String, name: String, after: String) {
        self.heading = heading
        self.before = before
        self.name = name
        self.after = after
    }

    /// The lines before the name back to a blank line or a heading.
    static func paragraph(endingWith head: Substring) -> Substring {
        let lines = head.split(separator: "\n", omittingEmptySubsequences: false)
        var kept = 1
        for line in lines.dropLast().reversed() {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty || trimmed.hasPrefix("#") { break }
            kept += 1
        }
        return Substring(lines.suffix(kept).joined(separator: "\n"))
    }

    /// The lines after the name up to a blank line or a heading.
    static func paragraph(startingWith tail: Substring) -> Substring {
        guard let end = tail.firstRange(of: /\n[ \t]*(\n|#)/)?.lowerBound else { return tail }
        return tail[..<end]
    }

    /// The nearest heading above, without its `#` markers; nil when there
    /// is none or it is the line the name is on.
    static func heading(before head: Substring) -> String? {
        // The name's own line is not a heading above it.
        guard let lineStart = head.lastIndex(where: { $0 == "\n" || $0 == "\r\n" }) else { return nil }
        var above = head[..<lineStart]
        // Back line by line until one starts with "#".
        while !above.isEmpty {
            let start = above.lastIndex(where: { $0 == "\n" || $0 == "\r\n" }).map { above.index(after: $0) } ?? above.startIndex
            let trimmed = above[start...].trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("#") {
                let text = plain(String(trimmed.drop { $0 == "#" }), currentLine: false)
                guard !text.isEmpty else { return nil }
                return text.count > 32 ? String(text.prefix(31)).trimmingCharacters(in: .whitespaces) + "…" : text
            }
            guard start > above.startIndex else { break }
            above = above[..<above.index(before: start)]
        }
        return nil
    }

    /// The text without Markdown's markers, on one line.
    static func plain(_ text: String, currentLine: Bool) -> String {
        var out = text
        // Links and images keep their text, also when the window cut one
        // in half.
        out = out.replacing(/!?\[([^\]]*)\]\([^)]*\)/) { String($0.output.1) }
        out = out.replacing(/^([^\[\]]*?)\]\([^)]*\)/) { match in String(match.output.1) }
        out = out.replacing(/!?\[([^\]]*)(\]\([^)]*)?$/) { String($0.output.1) }
        // Emphasis, code and strike markers; a lone `_` stays (PERSON_1).
        out = out.replacing(/\*{1,3}|_{2,3}|~~|`+/, with: "")
        // Line-start markers: headings, quotes and bullets. List numbers stay
        // ("5. Return of property").
        out = out.replacing(/^[ \t]*(#{1,6}[ \t]+|>[ \t]?|[-*+][ \t]+)/.anchorsMatchLineEndings(), with: "")
        // Table pipes and rules, escapes, then whitespace.
        out = out.replacing(/\|[-: |]*\|?/, with: " ")
        out = out.replacing(/\\(.)/) { String($0.output.1) }
        out = out.replacing(/\s+/, with: " ")
        return currentLine ? out : out.trimmingCharacters(in: .whitespaces)
    }

    /// The last `limit` characters, starting at a word, with "…" in front
    /// when something was cut.
    static func trimFront(_ text: String, to limit: Int, cut: Bool) -> String {
        guard text.count > limit else { return (cut ? "…" : "") + text.drop { $0 == " " } }
        var kept = String(text.suffix(limit))
        if let space = kept.firstIndex(of: " "), kept.distance(from: kept.startIndex, to: space) < limit / 2 {
            kept = String(kept[kept.index(after: space)...])
        }
        return "…" + kept
    }

    /// The first `limit` characters, ending at a word, with "…" after them
    /// when something was cut.
    static func trimBack(_ text: String, to limit: Int, cut: Bool) -> String {
        guard text.count > limit else { return cut ? text.trimmingCharacters(in: .whitespaces) + "…" : text }
        var kept = String(text.prefix(limit))
        if let space = kept.lastIndex(of: " "), kept.distance(from: space, to: kept.endIndex) < limit / 2 {
            kept = String(kept[..<space])
        }
        return kept + "…"
    }
}
