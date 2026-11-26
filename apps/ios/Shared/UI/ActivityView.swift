import LinkPresentation
import SwiftUI
import UIKit

/// The system share sheet for text or a file.
struct ActivityView: UIViewControllerRepresentable {
    let items: [Any]
    /// The system's entries left out (Ask AI's, which are no AI app).
    var excluded: [UIActivity.ActivityType] = []
    var onComplete: (Bool) -> Void = { _ in }
    /// The entry that took the items; nil when the sheet was cancelled.
    var onActivity: (UIActivity.ActivityType?) -> Void = { _ in }

    func makeUIViewController(context: Context) -> UIActivityViewController {
        let controller = UIActivityViewController(activityItems: items, applicationActivities: nil)
        controller.excludedActivityTypes = excluded
        controller.completionWithItemsHandler = { activity, completed, _, _ in
            onComplete(completed)
            onActivity(completed ? activity : nil)
        }
        return controller
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

/// Text to share that the share sheet previews by a title rather than by its
/// first words, which are Copy for AI's alias note. The title leaves
/// with the text, as Mail's subject and the preview's title, so it must
/// hold no more than the text does: for Copy for AI's text, the file's
/// neutral name (#1188, #1205); for a restored reply, which holds the real
/// names by design, its document's title or its own.
/// The text itself is unchanged. Save to Files gets `file`, the same text as
/// a `.md` file, rather than a `.txt` the system would make of it.
final class TitledText: NSObject, UIActivityItemSource {
    let text: String
    let title: String
    let file: URL?

    /// The Files app's Save to Files.
    static let saveToFiles = UIActivity.ActivityType("com.apple.DocumentManagerUICore.SaveToFiles")

    init(text: String, title: String, file: URL? = nil) {
        self.text = text
        self.title = title
        self.file = file
    }

    func activityViewControllerPlaceholderItem(_ controller: UIActivityViewController) -> Any { text }

    func activityViewController(_ controller: UIActivityViewController, itemForActivityType type: UIActivity.ActivityType?) -> Any? {
        if type == Self.saveToFiles, let file {
            return file
        }
        return text
    }

    func activityViewController(_ controller: UIActivityViewController, subjectForActivityType type: UIActivity.ActivityType?) -> String { title }

    func activityViewControllerLinkMetadata(_ controller: UIActivityViewController) -> LPLinkMetadata? {
        let metadata = LPLinkMetadata()
        metadata.title = title
        // A document symbol in place of the sheet's blank placeholder. A
        // template symbol draws as nothing there, so it is drawn into an
        // image of its own, in a grey that reads on light and dark sheets.
        metadata.iconProvider = NSItemProvider(object: Self.documentIcon)
        return metadata
    }

    private static let documentIcon: UIImage = {
        let size = CGSize(width: 120, height: 120)
        let symbol = UIImage(
            systemName: "doc.text", withConfiguration: UIImage.SymbolConfiguration(pointSize: 64, weight: .regular)
        )?.withTintColor(.systemGray, renderingMode: .alwaysOriginal)
        return UIGraphicsImageRenderer(size: size).image { _ in
            guard let symbol else { return }
            symbol.draw(at: CGPoint(x: (size.width - symbol.size.width) / 2, y: (size.height - symbol.size.height) / 2))
        }
    }()
}

/// What a share sheet carries, so `.sheet(item:)` can present it.
struct ShareItems: Identifiable {
    let id = UUID()
    let items: [Any]
}
