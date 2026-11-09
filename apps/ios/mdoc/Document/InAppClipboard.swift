import UIKit
import UniformTypeIdentifiers

/// Edit mode's clipboard (ADR 0109): Cut and Copy write a named pasteboard
/// only mdoc reads, so document text never reaches other apps through a
/// plain copy (it leaves through Copy for AI and Share). Paste reads it
/// first, unless something was copied in another app since, and then the
/// system clipboard; a rich copy pastes as the core's Markdown of its HTML
/// (`pasted_markdown`, ADR 0105).
@MainActor
struct InAppClipboard {
    static let shared = InAppClipboard()

    /// The app-private pasteboard; tests pass their own.
    let pasteboard: UIPasteboard
    /// The system clipboard, read only on Paste.
    let system: UIPasteboard

    init(
        pasteboard: UIPasteboard = UIPasteboard(name: UIPasteboard.Name("io.github.lawkitt.mdoc.in-app"), create: true)
            ?? .withUniqueName(),
        system: UIPasteboard = .general
    ) {
        self.pasteboard = pasteboard
        self.system = system
    }

    /// The system clipboard's change count when mdoc last copied: a later
    /// copy in another app is newer than mdoc's, and Paste takes it.
    private static var systemCountAtCopy: [String: Int] = [:]

    func write(_ text: String) {
        pasteboard.string = text
        Self.systemCountAtCopy[pasteboard.name.rawValue] = system.changeCount
    }

    /// Whether Paste has anything: mdoc's copy, or the system clipboard's
    /// text (asked without reading it, so iOS shows no paste prompt).
    var hasText: Bool {
        pasteboard.hasStrings || system.hasStrings || system.contains(pasteboardTypes: [UTType.html.identifier])
    }

    /// What Paste inserts: mdoc's copy unless the system clipboard changed
    /// since, then the system clipboard's text or the Markdown of its HTML.
    func read() -> String? {
        let copiedElsewhere = Self.systemCountAtCopy[pasteboard.name.rawValue] != system.changeCount
        if let own = pasteboard.string, !copiedElsewhere {
            return own
        }
        return PastedReply.text(from: system) ?? pasteboard.string
    }
}
