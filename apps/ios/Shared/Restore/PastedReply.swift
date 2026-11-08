import MdocFFI
import UIKit
import UniformTypeIdentifiers

/// What an AI reply on the clipboard, or shared, reads as. The ChatGPT app
/// copies no Markdown, only HTML and a flattened plain text: the core makes
/// the Markdown from the HTML and decides which of the two to use (ADR 0105).
@MainActor
enum PastedReply {
    /// The text for a copy holding `html` and `plain`, either of them missing.
    static func text(html: String?, plain: String?) -> String? {
        guard let html else { return plain }
        return MdocFFI.pastedMarkdown(html: html, plain: plain) ?? plain
    }

    /// The HTML a pasteboard or a share holds, as bytes in UTF-8 or Latin 1.
    nonisolated static func decode(html data: Data?) -> String? {
        guard let data else { return nil }
        return String(data: data, encoding: .utf8) ?? String(data: data, encoding: .isoLatin1)
    }

    static func text(from pasteboard: UIPasteboard) -> String? {
        text(html: decode(html: pasteboard.data(forPasteboardType: UTType.html.identifier)), plain: pasteboard.string)
    }

    /// The first provider's text that has any.
    static func text(from providers: [NSItemProvider]) async -> String? {
        for provider in providers {
            let html = await html(from: provider)
            let plain = await plain(from: provider)
            if let text = text(html: html, plain: plain), !text.isEmpty {
                return text
            }
        }
        return nil
    }

    private static func html(from provider: NSItemProvider) async -> String? {
        guard provider.hasItemConformingToTypeIdentifier(UTType.html.identifier) else { return nil }
        return await withCheckedContinuation { continuation in
            _ = provider.loadDataRepresentation(forTypeIdentifier: UTType.html.identifier) { data, _ in
                continuation.resume(returning: decode(html: data))
            }
        }
    }

    private static func plain(from provider: NSItemProvider) async -> String? {
        guard provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier) else { return nil }
        return await withCheckedContinuation { continuation in
            _ = provider.loadObject(ofClass: String.self) { text, _ in
                continuation.resume(returning: text)
            }
        }
    }
}
