import MdocFFI
import SwiftUI

/// Fix the words' bar (ADR 0114), in the review bar's place while a name's
/// handles are in the text: what to do, Cancel, and Hide «…» for the words
/// the handles hold, following them as they move. The one Hide «…»: no
/// menu shows over the selection (ADR 0115).
struct FixWordsBar: View {
    @Bindable var model: DocumentViewModel

    var body: some View {
        if let fix = model.wordsFix {
            VStack(alignment: .leading, spacing: Space.xs) {
                Text(problem(of: fix) ?? String(localized: "Drag the handles onto the name's words."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("fix-words-hint")
                HStack(spacing: Space.s) {
                    Button(String(localized: "Cancel")) {
                        model.cancelFixWords()
                    }
                    .secondaryGlass()
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("fix-words-cancel")
                    Spacer(minLength: 0)
                    Button {
                        model.confirmFixWords()
                    } label: {
                        Text(Self.hideTitle(for: fix))
                            .lineLimit(1)
                            .foregroundStyle(Color.onAccent)
                    }
                    .primaryGlass()
                    .keyboardShortcut(.defaultAction)
                    .disabled(problem(of: fix) != nil)
                    .accessibilityIdentifier("fix-words-hide")
                }
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
            .background(.bar)
        }
    }

    private func problem(of fix: WordsFix) -> String? {
        if case .failure(let error) = fix.target { return error.localizedDescription }
        return nil
    }

    /// Hide «Тагиев Тебриз…»: the words the handles hold, cut at about 24
    /// characters; Hide alone while they can't be hidden.
    static func hideTitle(for fix: WordsFix) -> String {
        guard case .success(let target) = fix.target else { return String(localized: "Hide") }
        return String(localized: "Hide «\(target.wording.truncatedForMenu)»")
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
