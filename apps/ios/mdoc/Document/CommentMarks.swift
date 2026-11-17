import Foundation
import MdocFFI
import PDFKit

/// The core's PDF reader for a rendered DOCX original: the link
/// annotations to `mdoc-comment:` that mark each comment's passage, in
/// 0..1 of the crop box from the top left, in page and annotation order.
/// Rotated pages are skipped, as on desktop.
final class CommentMarks: MdocFFI.CommentMarkReader, Sendable {
    static let scheme = "mdoc-comment:"

    func commentMarks(pdfPath: String) -> [CommentMark] {
        guard let document = PDFDocument(url: URL(fileURLWithPath: pdfPath)) else { return [] }
        var marks: [CommentMark] = []
        for index in 0..<document.pageCount {
            guard let page = document.page(at: index), page.rotation == 0 else { continue }
            let box = page.bounds(for: .cropBox)
            guard box.width > 0, box.height > 0 else { continue }
            for annotation in page.annotations {
                guard let target = Self.target(of: annotation) else { continue }
                let rect = annotation.bounds
                let clamp = { (value: CGFloat) in Float(min(max(value, 0), 1)) }
                marks.append(CommentMark(
                    page: UInt32(index),
                    rect: PageRect(
                        x: clamp((rect.minX - box.minX) / box.width),
                        y: clamp((box.maxY - rect.maxY) / box.height),
                        w: clamp(rect.width / box.width),
                        h: clamp(rect.height / box.height)),
                    target: target))
            }
        }
        return marks
    }

    /// A mark's URI after the scheme, or nil for any other annotation.
    static func target(of annotation: PDFAnnotation) -> String? {
        guard annotation.type == "Link",
            let uri = (annotation.url ?? (annotation.action as? PDFActionURL)?.url)?.absoluteString,
            uri.hasPrefix(scheme)
        else { return nil }
        return String(uri.dropFirst(scheme.count))
    }
}
