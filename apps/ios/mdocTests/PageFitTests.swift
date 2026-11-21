import PDFKit
import UIKit
import XCTest
@testable import mdoc

/// The Original's page view keeps fit to the width as its floor (#1036):
/// fit is taken again for a new width and for a document set after the
/// first layout, zoom never goes under it, and a page found under it
/// offers ⤢ back to fit.
@MainActor
final class PageFitTests: XCTestCase {
    /// A few A4 pages, drawn here.
    private static func document(pages: Int = 3) -> PDFDocument {
        let data = UIGraphicsPDFRenderer(bounds: CGRect(x: 0, y: 0, width: 595, height: 842)).pdfData { context in
            for number in 1...pages {
                context.beginPage()
                ("Page \(number)" as NSString).draw(at: CGPoint(x: 72, y: 72), withAttributes: [.font: UIFont.systemFont(ofSize: 24)])
            }
        }
        return PDFDocument(data: data)!
    }

    private func makeView(width: CGFloat, document: PDFDocument?) -> (UIWindow, FittingPDFView, () -> PageControl?) {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 440, height: 956))
        let view = FittingPDFView(frame: CGRect(x: 0, y: 0, width: width, height: 700))
        view.autoScales = true
        view.displayMode = .singlePageContinuous
        var control: PageControl?
        view.onControl = { control = $0 }
        view.document = document
        window.addSubview(view)
        window.isHidden = false
        settle(view)
        return (window, view, { control })
    }

    /// Lays the view out and lets its asynchronous fit run.
    private func settle(_ view: UIView) {
        for _ in 0..<5 {
            view.setNeedsLayout()
            view.layoutIfNeeded()
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
    }

    private func assertAtFit(_ view: FittingPDFView, _ message: String, file: StaticString = #filePath, line: UInt = #line) {
        let fit = view.scaleFactorForSizeToFit
        XCTAssertGreaterThan(fit, 0, message, file: file, line: line)
        XCTAssertEqual(view.minScaleFactor, fit, accuracy: fit * 0.01, "\(message): fit is the floor", file: file, line: line)
        XCTAssertEqual(view.scaleFactor, fit, accuracy: fit * 0.02, "\(message): at fit", file: file, line: line)
        XCTAssertFalse(view.isZoomed, message, file: file, line: line)
    }

    func testFitFollowsTheWidth() {
        let (window, view, _) = makeView(width: 300, document: Self.document())
        assertAtFit(view, "first layout")
        let narrow = view.scaleFactor
        view.frame.size.width = 440
        settle(view)
        assertAtFit(view, "wider")
        XCTAssertGreaterThan(view.scaleFactor, narrow * 1.2, "a wider view fits a larger page")
        _ = window
    }

    func testADocumentSetAfterTheFirstLayoutFits() {
        let (window, view, _) = makeView(width: 402, document: nil)
        view.document = Self.document()
        settle(view)
        assertAtFit(view, "late document")
        _ = window
    }

    func testAHeightChangeKeepsAReadersZoom() {
        let (window, view, control) = makeView(width: 402, document: Self.document())
        let fit = view.scaleFactor
        view.scaleFactor = fit * 2
        settle(view)
        XCTAssertEqual(control(), .fit, "zoomed in: ⤡")
        XCTAssertEqual(PageControl.fit.symbol, "arrow.down.right.and.arrow.up.left")
        view.frame.size.height = 600
        settle(view)
        XCTAssertEqual(view.scaleFactor, fit * 2, accuracy: fit * 0.02, "the zoom stays")
        _ = window
    }

    func testAPageUnderFitOffersToGrowBack() {
        let (window, view, control) = makeView(width: 402, document: Self.document())
        let fit = view.scaleFactor
        // What a stale fit left: the floor and the page well under fit.
        view.minScaleFactor = fit * 0.5
        view.scaleFactor = fit * 0.57
        XCTAssertTrue(view.isBelowFit)
        XCTAssertEqual(control(), .grow, "under fit: ⤢")
        XCTAssertEqual(PageControl.grow.symbol, "arrow.up.left.and.arrow.down.right")
        // The next layout takes fit again, so the page is never left there.
        view.frame.size.height = 650
        settle(view)
        assertAtFit(view, "after the next layout")
        XCTAssertNotEqual(control(), .grow)
        _ = window
    }
}
