import MdocFFI
import SwiftUI

/// The threads at one marker, or those with no place on the page, read in
/// order: each comment, then its replies. Read-only, because mdoc never
/// writes the source file. A comment's text is selectable, so it copies like
/// the page's text.
struct CommentThreadSheet: View {
    let title: String
    let threads: [DocxComments.Thread]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Space.l) {
                    ForEach(threads) { thread in
                        CommentThreadView(thread: thread)
                    }
                }
                .padding()
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .reviewSheetDetents()
        .accessibilityIdentifier("comment-sheet")
    }
}

/// A comment and its replies, nested under it; a resolved thread dimmed,
/// with "Resolved" beside it in words as well.
struct CommentThreadView: View {
    let thread: DocxComments.Thread

    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            if thread.comment.resolved {
                Label("Resolved", systemImage: "checkmark")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            CommentCard(comment: thread.comment)
            ForEach(thread.replies, id: \.id) { reply in
                CommentCard(comment: reply)
                    .padding(.leading, Space.l)
            }
        }
        .opacity(thread.comment.resolved ? 0.7 : 1)
    }
}

/// One comment as written: its author, date, passage and text.
struct CommentCard: View {
    let comment: OriginalComment

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            HStack(alignment: .firstTextBaseline, spacing: Space.xs) {
                Text(comment.initials)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Color.onAccent)
                    .padding(6)
                    .background(Circle().fill(Color.ink))
                    .accessibilityHidden(true)
                Text(comment.shownAuthor)
                    .font(.headline)
                Text(comment.shownDate)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            if !comment.quote.isEmpty, comment.parent == nil {
                Text("On ‘\(comment.quote)’")
                    .font(.subheadline)
                    .italic()
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
            Text(comment.text)
                .font(.body)
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(comment.parent == nil ? comment.spokenLabel : "Reply. \(comment.spokenLabel)"))
    }
}

#Preview("Comment threads") {
    PreviewMatrix {
        ForEach(PreviewFixtures.commentThreads) { CommentThreadView(thread: $0) }
    }
}

#Preview("Comment sheet") {
    CommentThreadSheet(title: "2 comments", threads: PreviewFixtures.commentThreads)
}

extension PreviewFixtures {
    static let commentThreads: [DocxComments.Thread] = DocxComments(
        comments: [
            OriginalComment(
                id: "0", author: "Reviewer A", date: "2026-11-18T10:00:00Z",
                text: "Check the notice period against the master agreement.", quote: "within fourteen days",
                parent: nil, resolved: false),
            OriginalComment(
                id: "1", author: "Рецензент Б", date: "", text: "Согласовано.", quote: "", parent: "0",
                resolved: false),
            OriginalComment(
                id: "2", author: "Reviewer A", date: "2026-11-17T09:00:00Z", text: "Fixed the defined term.",
                quote: "the Goods", parent: nil, resolved: true),
        ],
        anchors: []
    ).unplaced
}
