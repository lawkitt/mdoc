import SwiftUI

/// "Recognition stopped when mdoc left the screen."
struct InterruptionBanner: View {
    let kind: JobKind
    /// The stopped job read a file that needs no recognition.
    let isImport: Bool
    let resume: () -> Void

    var body: some View {
        HStack {
            // OCR keeps its finished pages and resumes; the scan reruns.
            Text(message)
                .font(.footnote)
            Spacer()
            Button(button, action: resume)
                .secondaryAction()
                .accessibilityIdentifier("interruption-resume")
        }
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
