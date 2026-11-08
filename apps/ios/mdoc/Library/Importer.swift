import Foundation
import UIKit
import UniformTypeIdentifiers

/// File intake (ADR 0104): what comes in from Files, "Open in mdoc", the
/// camera, Photos or the clipboard becomes a `PendingImport` over a copy in
/// mdoc's storage. PDF, DOCX and text go to the core as they are; images
/// become upright JPEG pages, which the core reads with OCR.
struct Importer {
    let storage: MdocStorage

    enum Failure: LocalizedError {
        /// Not a PDF, DOCX or image, or an image that cannot be read.
        case unsupported

        var errorDescription: String? {
            String(localized: "mdoc opens PDF, DOCX and image files.")
        }
    }

    /// A file from Files or "Open in mdoc". It is copied into mdoc's storage
    /// and the original left alone (an Inbox copy is removed). Images become
    /// JPEG pages like the camera's.
    func file(_ url: URL) throws -> PendingImport? {
        let scoped = url.startAccessingSecurityScopedResource()
        defer {
            if scoped { url.stopAccessingSecurityScopedResource() }
        }
        guard let kind = Self.kind(of: url) else { throw Failure.unsupported }
        if kind == .image {
            guard let data = try? Data(contentsOf: url) else { throw Failure.unsupported }
            removeInboxCopy(url)
            return try images([data], scanned: false, title: url.deletingPathExtension().lastPathComponent)
        }
        let copy = try storage.importCopy(of: url, in: .originals)
        removeInboxCopy(url)
        return PendingImport(source: .file(copy, kind), suggestedTitle: url.deletingPathExtension().lastPathComponent)
    }

    private func removeInboxCopy(_ url: URL) {
        if url.path.contains("/Inbox/") {
            try? FileManager.default.removeItem(at: url)
        }
    }

    /// Page images from the document camera or Photos, as upright JPEGs:
    /// the core reads JPEG only (PNG and HEIC are converted here). Nil when
    /// no page could be read.
    func images(_ images: [Data], scanned: Bool, title: String? = nil) throws -> PendingImport? {
        let folder = storage.url(.originals).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let urls = try images.enumerated().compactMap { index, data -> URL? in
            guard let jpeg = Self.uprightJPEG(data) else { return nil }
            let url = folder.appendingPathComponent("page-\(index + 1).jpg")
            try jpeg.write(to: url, options: [.atomic, .completeFileProtection])
            return url
        }
        guard !urls.isEmpty else { return nil }
        let kind = scanned ? String(localized: "Scan") : String(localized: "Photo")
        return PendingImport(
            source: scanned ? .scan(urls) : .photos(urls),
            suggestedTitle: title ?? "\(kind) \(Date.now.formatted(date: .abbreviated, time: .shortened))")
    }

    /// Pasted text; nil when there is none.
    static func text(_ text: String) -> PendingImport? {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        return PendingImport(source: .text(text), suggestedTitle: suggestedTitle(for: text))
    }

    /// The first line, without Markdown heading marks, up to 60 characters.
    static func suggestedTitle(for text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let firstLine = trimmed.split(separator: "\n", maxSplits: 1).first.map(String.init) ?? ""
        let title = firstLine.trimmingCharacters(in: CharacterSet(charactersIn: "# ")).prefix(60)
        return title.isEmpty ? String(localized: "Pasted text") : String(title)
    }

    /// Redraws an image with its orientation applied, as JPEG.
    static func uprightJPEG(_ data: Data) -> Data? {
        guard let image = UIImage(data: data) else { return nil }
        guard image.imageOrientation != .up else { return image.jpegData(compressionQuality: 0.9) }
        let format = UIGraphicsImageRendererFormat()
        format.scale = image.scale
        let upright = UIGraphicsImageRenderer(size: image.size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: image.size))
        }
        return upright.jpegData(compressionQuality: 0.9)
    }

    /// The system declares DOCX; the extension lookup never needs an import.
    static let docx = UTType("org.openxmlformats.wordprocessingml.document") ?? UTType(filenameExtension: "docx") ?? .data
    static let importTypes: [UTType] = [.pdf, docx, .image]

    static func kind(of url: URL) -> SourceKind? {
        guard let type = UTType(filenameExtension: url.pathExtension) else { return nil }
        if type.conforms(to: .pdf) { return .pdf }
        if type.conforms(to: docx) || url.pathExtension.lowercased() == "docx" { return .docx }
        if type.conforms(to: .image) { return .image }
        return nil
    }
}
