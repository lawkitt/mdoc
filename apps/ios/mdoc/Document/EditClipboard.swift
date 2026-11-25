import UIKit
import UniformTypeIdentifiers

/// Edit mode's Paste (ADR 0109): the system clipboard, as Cut and Copy
/// write it (#1209 decision 6, `SelectionCopy`). A rich copy pastes as the
/// core's Markdown of its HTML (`pasted_markdown`, ADR 0105).
@MainActor
struct EditClipboard {
    static let shared = EditClipboard()

    /// The system clipboard; tests pass their own.
    let pasteboard: UIPasteboard

    init(pasteboard: UIPasteboard = .general) {
        self.pasteboard = pasteboard
    }

    /// Whether Paste has anything, asked without reading it, so iOS shows
    /// no paste prompt.
    var hasText: Bool {
        pasteboard.hasStrings || pasteboard.contains(pasteboardTypes: [UTType.html.identifier])
    }

    /// What Paste inserts: the clipboard's text, or the Markdown of its
    /// HTML.
    func read() -> String? {
        PastedReply.text(from: pasteboard)
    }
}
