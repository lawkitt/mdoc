import SwiftUI

/// "Paste reply?" over the bar, back from the AI app Ask AI shared to
/// (#1164 decision 11): one tap on the system's Paste restores the answer,
/// as the bar's Paste does; Type a reply stays for one that is not on the
/// clipboard. × puts it away; the bar's Paste and Type a reply stay.
struct ReplyBanner: View {
    let onPaste: (String) -> Void
    let onType: () -> Void
    let onClose: () -> Void

    var body: some View {
        NoticeCard(
            message: Text("**Paste reply?** Copy the AI app's answer, then paste it here to read it with the real names."),
            symbol: "text.bubble", onClose: onClose
        ) {
            ButtonRow {
                // HTML first, so a table copied from the ChatGPT app stays one.
                PasteControl(look: .bar, types: [.html, .plainText]) { providers in
                    Task { @MainActor in
                        if let text = await PastedReply.text(from: providers) { onPaste(text) }
                    }
                }
                .accessibilityLabel(Text("Paste reply"))
                .accessibilityIdentifier("reply-banner-paste")
                Button(action: onType) {
                    Text("Type a reply")
                        .frame(maxWidth: .infinity)
                }
                .secondaryAction()
                .accessibilityIdentifier("reply-banner-type")
            }
            .lineLimit(1)
        }
        .accessibilityIdentifier("reply-banner")
    }
}

#Preview("Paste reply?") {
    PreviewMatrix {
        ReplyBanner(onPaste: { _ in }, onType: {}, onClose: {})
    }
}
