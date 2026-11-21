import SwiftUI

/// A locked DOCX's or PDF's password, asked in place of the action bar,
/// or in the Original pane for a locked PDF opened again. The password
/// lives in this field until Unlock hands it to the one import or unlock
/// that uses it; the field is emptied then, and after a wrong one.
struct PasswordCard: View {
    let prompt: DocumentViewModel.PasswordPrompt
    var purpose = Purpose.importing
    let unlock: @MainActor (String) async -> Void

    enum Purpose {
        /// The import asks, in place of the action bar.
        case importing
        /// A locked PDF's Original asks, in its pane.
        case original
    }
    @State private var password = ""
    @FocusState private var focused: Bool
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    /// Four lines of the card's words at its largest text size
    /// (accessibility2): at accessibility sizes they scroll in this much
    /// room, so the whole card fits between the panes row and the keyboard.
    static let wordsHeight = UIFont.preferredFont(
        forTextStyle: .subheadline, compatibleWith: UITraitCollection(preferredContentSizeCategory: .accessibilityLarge)
    ).lineHeight * 4

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Whole at every size: never in the scrolling area.
            Label("This document is password-protected", systemImage: "lock")
                .font(.headline)
                .fixedSize(horizontal: false, vertical: true)
            if dynamicTypeSize.isAccessibilitySize {
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        // First, so it shows without scrolling.
                        wrongLine
                        promise
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(height: Self.wordsHeight)
                .scrollIndicators(.visible)
                .scrollIndicatorsFlash(onAppear: true)
                .scrollIndicatorsFlash(trigger: prompt)
                .accessibilityIdentifier("password-words")
            } else {
                promise
                wrongLine
            }
            HStack(spacing: 12) {
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
                    .accessibilityIdentifier(purpose == .importing ? "document-password" : "original-password")
                Button("Unlock", action: submit)
                    .primaryAction()
                    .fixedSize()
                    .disabled(password.isEmpty)
                    .accessibilityIdentifier(purpose == .importing ? "unlock-document" : "unlock-original")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // Grows with the text no further than the bar's other cards.
        .dynamicTypeSize(...DynamicTypeSize.accessibility2)
        .onAppear { focused = true }
        // A wrong password: the empty field takes the next try.
        .onChange(of: prompt) { focused = true }
    }

    /// The privacy promise: always on the card, scrolled to at the largest
    /// sizes.
    private var promise: some View {
        Group {
            switch purpose {
            case .importing:
                Text("Enter its password to import it. mdoc unlocks it in memory while this document is open and never saves the password or an unlocked copy.")
            case .original:
                Text("Enter its password to show the Original. mdoc unlocks it in memory while this document is open and never saves the password or an unlocked copy.")
            }
        }
        .font(.subheadline)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
    }

    @ViewBuilder
    private var wrongLine: some View {
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
        PasswordCard(prompt: .needed, purpose: .original) { _ in }
    }
}
