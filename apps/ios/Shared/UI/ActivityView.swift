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

/// What a share sheet carries, so `.sheet(item:)` can present it.
struct ShareItems: Identifiable {
    let id = UUID()
    let items: [Any]
    /// A hand-off file to delete once the sheet closes.
    var temporaryFile: URL?
}
