import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// Restore with mdoc: takes an AI reply shared as text and shows it restored
/// in place. It loads no model.
final class ShareViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        Task { @MainActor in
            let text = await Self.sharedText(from: extensionContext)
            show(text: text)
        }
    }

    private func show(text: String?) {
        let library = try? CoreFactory.restoreExtension()
        let root = RestoreExtensionView(
            model: library.flatMap { library in
                text.map { RestoreViewModel(library: library, reply: $0) }
            },
            close: { [weak self] in self?.close() })
        let host = UIHostingController(rootView: root)
        addChild(host)
        host.view.frame = view.bounds
        host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(host.view)
        host.didMove(toParent: self)
    }

    private func close() {
        extensionContext?.completeRequest(returningItems: nil)
    }

    /// The first plain-text attachment, or the item's own text.
    private static func sharedText(from context: NSExtensionContext?) async -> String? {
        let items = context?.inputItems.compactMap { $0 as? NSExtensionItem } ?? []
        for item in items {
            for provider in item.attachments ?? [] where provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier) {
                if let text = await loadText(provider), !text.isEmpty {
                    return text
                }
            }
            if let text = item.attributedContentText?.string, !text.isEmpty {
                return text
            }
        }
        return nil
    }

    private static func loadText(_ provider: NSItemProvider) async -> String? {
        await withCheckedContinuation { continuation in
            _ = provider.loadObject(ofClass: String.self) { text, _ in
                continuation.resume(returning: text)
            }
        }
    }
}
