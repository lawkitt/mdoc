import Foundation
import MdocFFI
import PDFKit
import Synchronization

/// The core's PDF reader for a rendered DOCX original: the link
/// annotations to `mdoc-comment:` that mark each comment's passage, in
/// 0..1 of the crop box from the top left, in page and annotation order.
/// Rotated pages are skipped, as on desktop.
final class CommentMarks: MdocFFI.CommentMarkReader, Sendable {
    static let scheme = "mdoc-comment:"

    private let receivedPDF = Mutex<Data?>(nil)

    /// Reuse the bytes delivered for comment placement when opening the page.
    var renderedPDF: Data? { receivedPDF.withLock { $0 } }

    func commentMarks(pdf: Data) -> [CommentMark] {
        receivedPDF.withLock { $0 = pdf }
        return autoreleasepool { Self.readMarks(pdf) }
    }

    private static func readMarks(_ pdf: Data) -> [CommentMark] {
        guard let document = PDFDocument(data: pdf) else { return [] }
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
