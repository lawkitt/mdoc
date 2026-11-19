import SwiftUI

/// "Recognition stopped when mdoc left the screen.", over the text: a
/// notice card with no ×, because its button is the way on.
struct InterruptionCard: View {
    let kind: JobKind
    /// The stopped job read a file that needs no recognition.
    let isImport: Bool
    let resume: () -> Void

    var body: some View {
        // OCR keeps its finished pages and resumes; the scan reruns.
        NoticeCard(message: Text(message), symbol: "pause.circle") {
            Button(button, action: resume)
                .secondaryAction()
                .accessibilityIdentifier("interruption-resume")
        }
        .accessibilityIdentifier("interruption")
    }

    private var message: LocalizedStringKey {
        switch kind {
        case .recognize where isImport: "The import stopped when mdoc left the screen."
        case .recognize: "Recognition stopped when mdoc left the screen. Finished pages are kept."
        case .scan, .install: "Finding names stopped when mdoc left the screen."
        }
    }

    private var button: LocalizedStringKey {
        switch kind {
        case .recognize where isImport: "Finish import"
        case .recognize: "Resume"
        case .scan, .install: "Start again"
        }
    }
}

#Preview("Interruption") {
    PreviewMatrix {
        InterruptionCard(kind: .recognize, isImport: false) {}
        InterruptionCard(kind: .scan, isImport: false) {}
    }
}
