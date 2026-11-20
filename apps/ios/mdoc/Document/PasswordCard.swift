import SwiftUI

/// A locked DOCX's password, asked in place of the action bar. The
/// password lives in this field until Unlock hands it to the one import
/// that uses it; the field is emptied then, and after a wrong one.
struct PasswordCard: View {
    let prompt: DocumentViewModel.PasswordPrompt
    /// The tallest the card's words grow at accessibility sizes; past it
    /// they scroll, so the field and Unlock stay on screen above the
    /// keyboard and the card stays under the toolbar.
    var maxWordsHeight: CGFloat = 200
    let unlock: @MainActor (String) async -> Void
    @State private var password = ""
    @FocusState private var focused: Bool
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if dynamicTypeSize.isAccessibilitySize {
                ScrollView {
                    words
                }
                // The error line over the field takes some of the words' room:
                // they were read on the first try.
                .frame(height: prompt == .wrong ? maxWordsHeight * 0.6 : maxWordsHeight)
                .scrollIndicators(.visible)
                .scrollIndicatorsFlash(onAppear: true)
                .accessibilityIdentifier("password-words")
            } else {
                words
            }
            // Over the field, never scrolled away with the words.
            if prompt == .wrong {
                Label {
                    Text("The password is incorrect. Check it and try again.")
                } icon: {
                    Image(systemName: "exclamationmark.circle.fill")
                        .foregroundStyle(Palette.warning)
                }
                .font(.subheadline)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("password-wrong")
            }
            // Side by side, the field leaves the button no room at
            // accessibility sizes.
            let layout = dynamicTypeSize.isAccessibilitySize
                ? AnyLayout(VStackLayout(alignment: .leading, spacing: 12))
                : AnyLayout(HStackLayout(spacing: 12))
            layout {
                SecureField("Password", text: $password)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .privacySensitive()
                    .submitLabel(.go)
                    .onSubmit(submit)
                    .focused($focused)
                    .padding(.horizontal, 12)
                    .frame(minHeight: Size.target)
                    .background(Palette.chipFill, in: RoundedRectangle(cornerRadius: Radius.control, style: .continuous))
                    .accessibilityIdentifier("document-password")
                Button("Unlock", action: submit)
                    .primaryAction()
                    .disabled(password.isEmpty)
                    .accessibilityIdentifier("unlock-document")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // Grows with the text no further than the bar's other cards.
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .onAppear { focused = true }
        // A wrong password: the empty field takes the next try.
        .onChange(of: prompt) { focused = true }
    }

    private var words: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("This document is password-protected", systemImage: "lock")
                .font(.headline)
                .fixedSize(horizontal: false, vertical: true)
            Text("Enter its password to import it. mdoc unlocks it in memory and never keeps the password or an unlocked copy.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func submit() {
        guard !password.isEmpty else { return }
        let given = password
        password = ""
        Task { await unlock(given) }
    }
}

#Preview("Password") {
    PreviewMatrix {
        PasswordCard(prompt: .needed) { _ in }
        PasswordCard(prompt: .wrong) { _ in }
    }
}
