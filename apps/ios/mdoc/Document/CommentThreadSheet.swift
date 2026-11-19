import MdocFFI
import SwiftUI

/// The comment sheet (originals.md, Phones): one thread at a time, "2 of
/// 5" with ‹ › and a sideways swipe stepping through every thread in page
/// order, then those not on the page. It keeps one height while open, so
/// stepping never resizes it, and the page behind follows it. Read-only,
/// because mdoc never writes the source file; a comment's text is
/// selectable, so it copies like the page's text.
struct CommentSheet: View {
    @Binding var state: CommentSheetState
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Which way the last step went, for the card's slide.
    @State private var forward = true

    var body: some View {
        NavigationStack {
            ScrollView {
                if let thread = state.thread {
                    VStack(alignment: .leading, spacing: Space.s) {
                        if state.unplaced.contains(thread.id) {
                            Label("Not on the page", systemImage: "doc.text")
                                .font(.footnote.weight(.medium))
                                .foregroundStyle(.secondary)
                        }
                        CommentThreadView(thread: thread)
                    }
                    .padding()
                    .id(thread.id)
                    .transition(slide)
                }
            }
            .scrollBounceBehavior(.basedOnSize)
            .contentShape(Rectangle())
            .simultaneousGesture(swipe)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(role: .close) { dismiss() }
                        .accessibilityIdentifier("comment-sheet-close")
                }
                ToolbarItem(placement: .principal) {
                    stepper
                }
            }
        }
        .reviewSheetDetents()
        .presentationBackgroundInteraction(.enabled(upThrough: .medium))
        .accessibilityIdentifier("comment-sheet")
    }

    /// ‹ 2 of 5 ›; one thread shows no arrows.
    private var stepper: some View {
        HStack(spacing: Space.xxs) {
            if state.threads.count > 1 {
                Button { step(-1) } label: {
                    Image(systemName: "chevron.left")
                        .frame(width: Size.target, height: Size.target)
                }
                .disabled(!state.canGoBack)
                .accessibilityLabel(Text("Previous comment"))
                .accessibilityIdentifier("comment-previous")
            }
            Text("\(state.index + 1) of \(state.threads.count)")
                .font(.headline.monospacedDigit())
                .lineLimit(1)
                .accessibilityIdentifier("comment-position")
            if state.threads.count > 1 {
                Button { step(1) } label: {
                    Image(systemName: "chevron.right")
                        .frame(width: Size.target, height: Size.target)
                }
                .disabled(!state.canGoOn)
                .accessibilityLabel(Text("Next comment"))
                .accessibilityIdentifier("comment-next")
            }
        }
        .buttonStyle(.plain)
        // Grows with the text no further than a bar does.
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
    }

    /// A mostly sideways swipe steps: left to the next thread.
    private var swipe: some Gesture {
        DragGesture(minimumDistance: 24)
            .onEnded { drag in
                let size = drag.translation
                guard abs(size.width) > 60, abs(size.width) > abs(size.height) * 1.5 else { return }
                step(size.width < 0 ? 1 : -1)
            }
    }

    private var slide: AnyTransition {
        guard !reduceMotion else { return .opacity }
        return .asymmetric(
            insertion: .move(edge: forward ? .trailing : .leading).combined(with: .opacity),
            removal: .move(edge: forward ? .leading : .trailing).combined(with: .opacity))
    }

    private func step(_ by: Int) {
        guard by > 0 ? state.canGoOn : state.canGoBack else { return }
        forward = by > 0
        withAnimation(Motion.move(reduced: reduceMotion)) { state.step(by) }
        Haptics.selection()
    }
}

/// A comment and its replies joined by a thin line; a resolved thread
/// dimmed, with "Resolved" in words as well.
struct CommentThreadView: View {
    let thread: DocxComments.Thread

    var body: some View {
        VStack(alignment: .leading, spacing: Space.m) {
            if thread.comment.resolved {
                Label("Resolved", systemImage: "checkmark")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, Space.xs)
                    .padding(.vertical, Space.xxs)
                    .background(Palette.chipFill, in: Capsule())
            }
            CommentCard(comment: thread.comment)
            if !thread.replies.isEmpty {
                VStack(alignment: .leading, spacing: Space.m) {
                    ForEach(thread.replies, id: \.id) { reply in
                        CommentCard(comment: reply, small: true)
                    }
                }
                .padding(.leading, Space.m)
                .overlay(alignment: .leading) {
                    Rectangle()
                        .fill(Palette.separator)
                        .frame(width: 1)
                        .accessibilityHidden(true)
                }
                .padding(.leading, CommentCard.avatar / 2)
            }
        }
        .opacity(thread.comment.resolved ? 0.7 : 1)
    }
}

/// One comment as written: its author with a colour of their own, the
/// date, the passage as a quote with a yellow rule, and the text.
struct CommentCard: View {
    let comment: OriginalComment
    /// A reply: a smaller avatar and no passage.
    var small = false

    static let avatar: CGFloat = 32

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            HStack(spacing: Space.xs) {
                Text(comment.initials)
                    .font((small ? Font.caption2 : .caption).weight(.semibold))
                    .foregroundStyle(.primary)
                    .frame(width: small ? 26 : Self.avatar, height: small ? 26 : Self.avatar)
                    .background(Circle().fill(AuthorColor.of(comment.shownAuthor).opacity(0.28)))
                    .accessibilityHidden(true)
                Text(comment.shownAuthor)
                    .font(small ? .subheadline.weight(.semibold) : .headline)
                    .lineLimit(1)
                Text(comment.shownDate)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if !comment.quote.isEmpty, comment.parent == nil {
                Text(comment.quote)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
                    .padding(.leading, Space.xs)
                    .overlay(alignment: .leading) {
                        Rectangle()
                            .fill(Color(uiColor: CommentedPDFContainer.tint))
                            .frame(width: 3)
                    }
                    .padding(.top, Space.xxs)
            }
            Text(comment.text)
                .font(small ? .callout : .body)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(comment.parent == nil ? comment.spokenLabel : "Reply. \(comment.spokenLabel)"))
    }
}

/// A colour of each author's own, the same every time they show: never a
/// state, and only behind initials, never as text.
enum AuthorColor {
    private static let palette: [Color] = [.purple, .teal, .orange, .pink, .blue, .green, .indigo, .mint]

    static func of(_ author: String) -> Color {
        // Not `hashValue`, which changes between launches.
        let sum = author.unicodeScalars.reduce(0) { ($0 &* 31 &+ Int($1.value)) & 0xFFFF }
        return palette[sum % palette.count]
    }
}

#Preview("Comment threads") {
    PreviewMatrix {
        ForEach(PreviewFixtures.commentThreads) { CommentThreadView(thread: $0) }
    }
}

#Preview("Comment sheet") {
    @Previewable @State var state = CommentSheetState(
        DocxComments(comments: PreviewFixtures.commentComments, anchors: []), at: nil)
    Color.clear.sheet(isPresented: .constant(true)) { CommentSheet(state: $state) }
}

extension PreviewFixtures {
    static let commentComments: [OriginalComment] = [
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
    ]

    static let commentThreads: [DocxComments.Thread] =
        DocxComments(comments: commentComments, anchors: []).unplaced
}
