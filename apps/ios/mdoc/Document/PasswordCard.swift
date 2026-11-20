import SwiftUI

/// A locked DOCX's password, asked in place of the action bar. The
/// password lives in this field until Unlock hands it to the one import
/// that uses it; the field is emptied then, and after a wrong one.
struct PasswordCard: View {
    let prompt: DocumentViewModel.PasswordPrompt
    let unlock: @MainActor (String) async -> Void
    @State private var password = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("This document is password-protected", systemImage: "lock")
                .font(.headline)
            Text("Enter its password to import it. mdoc unlocks it in memory and never keeps the password or an unlocked copy.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
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
                    .accessibilityIdentifier("document-password")
                Button("Unlock", action: submit)
                    .primaryAction()
                    .disabled(password.isEmpty)
                    .accessibilityIdentifier("unlock-document")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onAppear { focused = true }
        // A wrong password: the empty field takes the next try.
        .onChange(of: prompt) { focused = true }
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
