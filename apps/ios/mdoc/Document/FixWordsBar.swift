import MdocFFI
import SwiftUI

/// Adjust Words' bar (ADR 0114, 0115; "Fix the words" in code), in the
/// review bar's place while a name's handles are in the text: one row of
/// Liquid Glass, ✕ (Cancel), the title over the words the handles hold in
/// the category's tint with the alias they will keep, and ✓ Done. The words
/// follow the handles; while they can't be taken, the core's reason shows
/// in their place and Done waits. No menu shows over the selection: this
/// bar is the one place to confirm. The first time, a caption over it goes
/// with the demo in the text (`ReviewHint.adjustWords`).
struct FixWordsBar: View {
    @Bindable var model: DocumentViewModel

    var body: some View {
        if let fix = model.wordsFix {
            VStack(spacing: Space.xs) {
                if model.adjustWordsDemo {
                    AdjustWordsCaption { model.endAdjustWordsDemo(learned: true, closed: true) }
                        .transition(.opacity.combined(with: .move(edge: .bottom)))
                }
                GlassEffectContainer(spacing: Space.xs) {
                    HStack(spacing: Space.xs) {
                        Button {
                            model.cancelFixWords()
                        } label: {
                            Image(systemName: "xmark")
                                .fontWeight(.semibold)
                        }
                        .secondaryAction(circle: true)
                        .keyboardShortcut(.cancelAction)
                        .accessibilityLabel(Text("Cancel"))
                        .accessibilityHint(Text("Leaves the name's words as they were."))
                        .accessibilityIdentifier("fix-words-cancel")
                        summary(fix)
                        Button {
                            model.confirmFixWords()
                        } label: {
                            Label("Done", systemImage: "checkmark")
                                .lineLimit(1)
                        }
                        .primaryAction()
                        .keyboardShortcut(.defaultAction)
                        .disabled(Self.words(for: fix) == nil)
                        .accessibilityHint(Text("The name takes these words everywhere they appear."))
                        .accessibilityIdentifier("fix-words-done")
                    }
                }
            }
            .padding(.horizontal, Space.s)
            .padding(.bottom, Space.xs)
            .animation(Motion.fade, value: model.adjustWordsDemo)
        }
    }

    /// The middle of the row: "Adjust Words" over the words and their
    /// alias, or why they can't be taken.
    private func summary(_ fix: WordsFix) -> some View {
        let candidate = fix.draft.snapshot.candidate(fix.candidate)
        let tint = candidate?.category.color ?? Palette.accent
        return VStack(spacing: 2) {
            Text("Adjust Words")
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
            if let words = Self.words(for: fix) {
                HStack(spacing: Space.xxs) {
                    Text(verbatim: words)
                        .font(Typography.chip)
                        .foregroundStyle(tint)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .padding(.horizontal, Space.xs)
                        .padding(.vertical, 2)
                        .background(tint.opacity(0.15), in: Capsule())
                        .layoutPriority(1)
                    if let alias = candidate?.alias {
                        Image(systemName: "arrow.right")
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(.tertiary)
                            .accessibilityHidden(true)
                        Text(verbatim: alias)
                            .font(Typography.alias)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            } else {
                Text(Self.problem(of: fix) ?? String(localized: "Select the name's words."))
                    .font(Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity, minHeight: BigButton.height)
        .padding(.horizontal, Space.s)
        .padding(.vertical, Space.xxs)
        .glassEffect(.regular, in: .capsule)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("fix-words-words")
    }

    /// The words the handles hold, as the core would take them; nil while
    /// they can't be taken.
    static func words(for fix: WordsFix) -> String? {
        guard case .success(let target) = fix.target else { return nil }
        return target.wording
    }

    private static func problem(of fix: WordsFix) -> String? {
        if case .failure(let error) = fix.target { return error.localizedDescription }
        return nil
    }
}

/// The demo's caption over Adjust Words' bar: a symbol and the words, which
/// VoiceOver announces. A tap puts it away for good.
struct AdjustWordsCaption: View {
    let onDismiss: () -> Void

    var body: some View {
        Button(action: onDismiss) {
            HStack(spacing: Space.xs) {
                Image(systemName: ReviewHint.adjustWords.symbol)
                    .foregroundStyle(Palette.accent)
                    .accessibilityHidden(true)
                Text(ReviewHint.adjustWords.text)
                    .font(Typography.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.horizontal, Space.s)
            .padding(.vertical, Space.xs)
            .glassEffect(.regular, in: .capsule)
        }
        .buttonStyle(.plain)
        .accessibilityHint(Text("Hides this tip."))
        .accessibilityIdentifier("adjust-words-caption")
        .onAppear {
            AccessibilityNotification.Announcement(String(localized: ReviewHint.adjustWords.text)).post()
        }
    }
}

extension String {
    /// A name as a menu title quotes it: whole up to 24 characters, else
    /// cut at a word's end with "…".
    var truncatedForMenu: String {
        let limit = 24
        guard count > limit else { return self }
        let cut = prefix(limit)
        let atWord = cut.lastIndex(of: " ").map { cut[..<$0] } ?? cut
        return atWord.trimmingCharacters(in: .whitespaces.union(.punctuationCharacters)) + "…"
    }
}
