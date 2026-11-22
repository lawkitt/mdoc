import Foundation
import ImageIO
import MdocFFI
import UIKit
import UniformTypeIdentifiers

/// File intake (ADR 0104): what comes in from Files, "Open in mdoc", the
/// camera, Photos or the clipboard becomes a `PendingImport` over a copy in
/// mdoc's storage. PDF, DOCX and text go to the core as they are; images
/// become pages the core reads with OCR, kept without their metadata.
struct Importer {
    let storage: MdocStorage

    enum Failure: LocalizedError {
        /// Not a PDF, DOCX or image, or an image that cannot be read.
        case unsupported
        /// The core refused the image, in its words (too large to keep).
        case refused(String)
        /// A page the camera or Photos couldn't hand over: the batch is
        /// refused rather than imported without it.
        case unreadablePage

        var errorDescription: String? {
            switch self {
            case .unsupported: String(localized: "mdoc opens PDF, DOCX and image files.")
            case .refused(let message): message
            case .unreadablePage: String(localized: "A page couldn't be read, so nothing was imported. Try again.")
            }
        }
    }

    /// A file from Files or "Open in mdoc". It is copied into mdoc's storage
    /// and the original left alone (an Inbox copy is removed). An image
    /// becomes a page like the camera's, one document of its own. What it
    /// is comes from its first bytes, which must agree with its name: a
    /// photo named `.pdf`, or anything named `.jpg` that isn't an image, is
    /// refused before a copy is kept.
    func file(_ url: URL) throws -> PendingImport? {
        let scoped = url.startAccessingSecurityScopedResource()
        defer {
            if scoped { url.stopAccessingSecurityScopedResource() }
        }
        let header = (try? FileHandle(forReadingFrom: url))
            .flatMap { handle in defer { try? handle.close() }; return try? handle.read(upToCount: 1024) } ?? Data()
        guard let named = Self.kind(of: url), let kind = Self.kind(named, header: header) else {
            throw Failure.unsupported
        }
        if kind == .image {
            guard let data = try? Data(contentsOf: url),
                  let pending = try images([data], scanned: false, title: url.deletingPathExtension().lastPathComponent)
            else { throw Failure.unsupported }
            removeInboxCopy(url)
            return pending
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

    /// Page images from the document camera or Photos, picked together as
    /// one document, each kept as `page` keeps it. A page that can't be
    /// kept refuses the whole batch (`Failure.unsupported`): no page is
    /// left out without a word.
    /// A page that arrives as nil (the camera couldn't encode it, Photos
    /// couldn't load it) refuses the batch too (`Failure.unreadablePage`).
    func images(_ pages: [Data?], scanned: Bool, title: String? = nil) throws -> PendingImport? {
        guard !pages.isEmpty else { return nil }
        let images = try pages.map { page -> Data in
            guard let page else { throw Failure.unreadablePage }
            return page
        }
        let folder = storage.url(.originals).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let urls: [URL]
        do {
            urls = try images.enumerated().map { index, data -> URL in
                guard let (page, format) = try Self.page(data) else { throw Failure.unsupported }
                let url = folder.appendingPathComponent("page-\(index + 1).\(format)")
                try page.write(to: url, options: [.atomic, .completeFileProtection])
                return url
            }
        } catch {
            try? FileManager.default.removeItem(at: folder)
            throw error
        }
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

    /// An image as mdoc keeps it, which the core reads and the Original
    /// shows (#1021), with its file extension: the core rewrites JPEG, PNG,
    /// WebP, GIF and TIFF (every page) without their metadata (GPS
    /// position, camera, dates), the pixels and orientation as they were; HEIC and anything else iOS
    /// reads is drawn upright as a JPEG without metadata. Nil when neither
    /// can read it.
    static func page(_ data: Data) throws -> (Data, String)? {
        let kept: Data?
        do {
            kept = try imageWithoutMetadata(image: data)
        } catch {
            if case .importFailed(let message) = CoreFailure(error) { throw Failure.refused(message) }
            throw error
        }
        if let kept, let format = coreFormat(kept) {
            return (kept, format)
        }
        // A TIFF the core can't keep without its metadata (BigTIFF) isn't
        // taken: iOS would draw only its first page.
        if isTIFF(data) { return nil }
        return uprightJPEG(data).map { ($0, "jpg") }
    }

    /// The extension of an image the core reads, by its signature.
    static func coreFormat(_ data: Data) -> String? {
        let bytes = [UInt8](data.prefix(12))
        if bytes.starts(with: [0xff, 0xd8, 0xff]) { return "jpg" }
        if bytes.starts(with: [0x89, 0x50, 0x4e, 0x47]) { return "png" }
        if bytes.starts(with: Array("GIF8".utf8)) { return "gif" }
        if bytes.starts(with: [0x49, 0x49, 0x2a, 0]) || bytes.starts(with: [0x4d, 0x4d, 0, 0x2a]) { return "tiff" }
        if bytes.count == 12, bytes.starts(with: Array("RIFF".utf8)), Array(bytes[8...]) == Array("WEBP".utf8) {
            return "webp"
        }
        return nil
    }

    /// A classic TIFF or BigTIFF, in either byte order.
    static func isTIFF(_ data: Data) -> Bool {
        let bytes = [UInt8](data.prefix(4))
        return [[0x49, 0x49, 0x2a, 0], [0x4d, 0x4d, 0, 0x2a], [0x49, 0x49, 0x2b, 0], [0x4d, 0x4d, 0, 0x2b]]
            .contains(bytes)
    }

    /// What a file named as `named` is by its first bytes, `header`, or nil
    /// when they disagree: a PDF has its header, a DOCX is a ZIP package
    /// (or, locked, an OLE compound file), and an image starts with a
    /// signature mdoc or iOS reads.
    static func kind(_ named: SourceKind, header: Data) -> SourceKind? {
        let bytes = [UInt8](header)
        let pdf = header.range(of: Data("%PDF-".utf8)) != nil
        let zip = bytes.starts(with: [0x50, 0x4b, 0x03, 0x04])
        // A password-protected Office file is an OLE compound file.
        let compound = bytes.starts(with: [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1])
        let image = coreFormat(header) != nil || isTIFF(header)
            || (bytes.count >= 12 && Array(bytes[4..<8]) == Array("ftyp".utf8))
        // An image's signature wins: a photo with `%PDF-` in an early
        // comment is a photo, as the core reads it.
        if image { return named == .image ? .image : nil }
        switch named {
        case .pdf: return pdf ? .pdf : nil
        case .docx: return zip || compound ? .docx : nil
        case .image:
            // Other formats iOS reads (BMP, ICO, …), if their bytes say so.
            guard !pdf, !zip, let source = CGImageSourceCreateWithData(header as CFData, nil),
                  CGImageSourceGetType(source) != nil else { return nil }
            return .image
        default: return nil
        }
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
