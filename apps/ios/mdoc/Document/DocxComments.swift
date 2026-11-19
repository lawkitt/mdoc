import Foundation
import MdocFFI

/// A rendered DOCX original's comments, placed: the core's comments and
/// anchors grouped into threads (a comment and its replies) and the page's
/// markers, one per line, in page order. Comments are the document's own
/// words, never scanned (they never leave the device).
struct DocxComments: Sendable {
    /// A comment and the replies to it, in order.
    struct Thread: Identifiable, Hashable, Sendable {
        let comment: OriginalComment
        let replies: [OriginalComment]
        var id: String { comment.id }
    }

    /// One marker: the threads whose passages start on one line.
    struct Marker: Identifiable, Hashable, Sendable {
        /// The first thread's id.
        let id: String
        let page: Int
        /// The line's first rectangle, 0..1 of the page from the top left.
        let line: PageRect
        let threads: [Thread]

        /// Every thread on the line was marked done in Word.
        var resolved: Bool { threads.allSatisfy(\.comment.resolved) }
    }

    /// Every comment, replies included.
    let all: [OriginalComment]
    let markers: [Marker]
    /// Threads with no place on the page: in a header, a footer or a range
    /// the renderer did not mark.
    let unplaced: [Thread]
    /// Each open thread's passage, a rectangle per line and page, for the
    /// tint; a resolved thread has none.
    let passages: [String: [(page: Int, rect: PageRect)]]

    init(comments: [OriginalComment], anchors: [CommentAnchor]) {
        all = comments
        let ids = Set(comments.map(\.id))
        // A reply to a comment that is not there reads as a thread of its own.
        let replies = Dictionary(grouping: comments.filter { $0.parent.map(ids.contains) == true }, by: { $0.parent ?? "" })
        let threads = comments.filter { $0.parent.map(ids.contains) != true }
            .map { Thread(comment: $0, replies: replies[$0.id] ?? []) }
        let byID = Dictionary(threads.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        var passages: [String: [(page: Int, rect: PageRect)]] = [:]
        var placed: Set<String> = []
        var starts: [(page: Int, rect: PageRect, thread: Thread)] = []
        for anchor in anchors {
            guard let thread = byID[anchor.id], let first = anchor.rects.first else { continue }
            if placed.insert(anchor.id).inserted {
                starts.append((Int(anchor.page), first, thread))
            }
            if !thread.comment.resolved {
                passages[anchor.id, default: []] += anchor.rects.map { (Int(anchor.page), $0) }
            }
        }
        self.passages = passages
        unplaced = threads.filter { !placed.contains($0.id) }
        // Page order, then top to bottom; one marker per line.
        starts.sort { ($0.page, $0.rect.y, $0.rect.x) < ($1.page, $1.rect.y, $1.rect.x) }
        var markers: [Marker] = []
        for start in starts {
            if let last = markers.last, last.page == start.page,
                abs((last.line.y + last.line.h / 2) - (start.rect.y + start.rect.h / 2))
                    < max(last.line.h, start.rect.h) / 2
            {
                markers[markers.count - 1] = Marker(
                    id: last.id, page: last.page, line: last.line, threads: last.threads + [start.thread])
            } else {
                markers.append(Marker(id: start.thread.id, page: start.page, line: start.rect, threads: [start.thread]))
            }
        }
        self.markers = markers
    }

    /// The marker a thread sits at, if it has a place.
    func marker(of threadID: String) -> Marker? {
        markers.first { $0.threads.contains { $0.id == threadID } }
    }
}

extension OriginalComment {
    /// Word's ISO date as a short date, or as written when it isn't one.
    var shownDate: String {
        guard let date = ISO8601DateFormatter().date(from: date) else { return date }
        return date.formatted(date: .abbreviated, time: .omitted)
    }

    var shownAuthor: String {
        author.isEmpty ? String(localized: "Unknown author") : author
    }

    /// Up to two initials of the author's name.
    var initials: String {
        String(shownAuthor.split(separator: " ").prefix(2).compactMap(\.first)).uppercased()
    }

    /// What VoiceOver reads for the comment.
    var spokenLabel: String {
        var label = String(localized: "Comment by \(shownAuthor), \(shownDate)")
        if !quote.isEmpty {
            label += String(localized: ", on ‘\(quote)’")
        }
        return label + ": " + text
    }
}

extension DocxComments.Thread {
    /// The card's label, in sentences: the comment, then how many replies
    /// and whether it is resolved.
    var spokenLabel: String {
        var label = comment.spokenLabel
        if !label.hasSuffix(".") { label += "." }
        if !replies.isEmpty {
            label += " " + (replies.count == 1 ? String(localized: "1 reply.") : String(localized: "\(replies.count) replies."))
        }
        if comment.resolved {
            label += " " + String(localized: "Resolved.")
        }
        return label
    }
}

extension DocxComments.Marker {
    /// A marker's label: its one thread's, or the count and each thread's.
    var spokenLabel: String {
        guard threads.count > 1 else { return threads.first?.spokenLabel ?? "" }
        return ([String(localized: "\(threads.count) comments.")] + threads.map(\.spokenLabel)).joined(separator: " ")
    }
}
