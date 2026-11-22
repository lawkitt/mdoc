import SwiftUI
import UIKit

/// Restore with mdoc: takes an AI reply shared as text and shows it restored
/// in place. It loads no model.
final class ShareViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        Task { @MainActor in
            let items = extensionContext?.inputItems.compactMap { $0 as? NSExtensionItem } ?? []
            let text = await PastedReply.sharedText(in: items)
            show(RestoreSheet.make(text: text, library: CoreFactory.restoreExtension))
        }
    }

    private func show(_ sheet: RestoreSheet) {
        let root = RestoreExtensionView(sheet: sheet, close: { [weak self] in self?.close() })
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
}
