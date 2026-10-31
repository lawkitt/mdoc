import LinkPresentation
import SwiftUI
import UIKit

/// The system share sheet for text or a file.
struct ActivityView: UIViewControllerRepresentable {
    let items: [Any]
    var onComplete: (Bool) -> Void = { _ in }

    func makeUIViewController(context: Context) -> UIActivityViewController {
        let controller = UIActivityViewController(activityItems: items, applicationActivities: nil)
        controller.completionWithItemsHandler = { _, completed, _, _ in
            onComplete(completed)
        }
        return controller
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

/// Text to share that the share sheet previews by a title (the document's)
/// rather than by its first words, which are Copy for AI's placeholder note.
/// The text itself is unchanged.
final class TitledText: NSObject, UIActivityItemSource {
    let text: String
    let title: String

    init(text: String, title: String) {
        self.text = text
        self.title = title
    }

    func activityViewControllerPlaceholderItem(_ controller: UIActivityViewController) -> Any { text }

    func activityViewController(_ controller: UIActivityViewController, itemForActivityType type: UIActivity.ActivityType?) -> Any? { text }

    func activityViewController(_ controller: UIActivityViewController, subjectForActivityType type: UIActivity.ActivityType?) -> String { title }

    func activityViewControllerLinkMetadata(_ controller: UIActivityViewController) -> LPLinkMetadata? {
        let metadata = LPLinkMetadata()
        metadata.title = title
        return metadata
    }
}

/// What a share sheet carries, so `.sheet(item:)` can present it.
struct ShareItems: Identifiable {
    let id = UUID()
    let items: [Any]
    /// A hand-off file to delete once the sheet closes.
    var temporaryFile: URL?
}
