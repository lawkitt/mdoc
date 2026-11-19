import PDFKit
import SwiftUI
import UIKit

/// The page the text came from: PDFKit for PDFs and rendered DOCX files, a
/// zoomable image view for scans and photos (ADR 0085).
struct OriginalView: View {
    let original: OriginalSource
    /// A long job runs on the document (`DocxOriginalView.waits`).
    var jobRuns = false
    /// Nothing sits under the pane: the page runs to the screen's bottom.
    var reachesBottom = false
    /// A rendered DOCX's comments, for the title menu.
    var onComments: (DocxComments) -> Void = { _ in }
    @State private var viewport = PageViewport()
    @State private var bottomInset: CGFloat = 0

    var body: some View {
        page
            // The page goes under the home indicator; its control stays
            // above it.
            .environment(\.pageControlInset, reachesBottom ? bottomInset : 0)
            .ignoresSafeArea(.container, edges: reachesBottom ? .bottom : [])
            // Read outside the page, where the home indicator is still unsafe.
            .onGeometryChange(for: CGFloat.self) { $0.safeAreaInsets.bottom } action: { bottomInset = $0 }
    }

    @ViewBuilder
    private var page: some View {
        switch original {
        case .none:
            ContentUnavailableView(
                "No original page",
                systemImage: "doc.plaintext",
                description: Text("Pasted text shows its text only."))
        case .pdf(let url):
            PDFKitView(url: url, viewport: viewport)
                .accessibilityLabel(Text("Original PDF"))
                .overlay(alignment: .bottomTrailing) { PageControlButton(viewport: viewport) }
        case .docx(let url):
            DocxOriginalView(source: url, waits: jobRuns, onComments: onComments)
        case .images(let urls):
            TabView {
                ForEach(Array(urls.enumerated()), id: \.offset) { index, url in
                    ZoomablePage(url: url)
                        .accessibilityLabel(Text("Original page \(index + 1) of \(urls.count)"))
                }
            }
            .tabViewStyle(.page(indexDisplayMode: urls.count > 1 ? .always : .never))
            .indexViewStyle(.page(backgroundDisplayMode: .always))
            .background(Color(uiColor: .secondarySystemBackground))
        }
    }
}

struct PDFKitView: UIViewRepresentable {
    let url: URL
    let viewport: PageViewport

    func makeUIView(context: Context) -> FittingPDFView {
        let view = FittingPDFView()
        view.autoScales = true
        view.displayMode = .singlePageContinuous
        view.backgroundColor = .secondarySystemBackground
        view.document = PDFDocument(url: url)
        view.onControl = { [weak viewport] in viewport?.report($0) }
        viewport.attach { [weak view] in view?.perform($0) }
        return view
    }

    func updateUIView(_ view: FittingPDFView, context: Context) {
        if view.document?.documentURL != url {
            view.document = PDFDocument(url: url)
        }
    }
}

/// A scan page and its ⤡ control, shown while zoomed past fit; a page
/// turned sideways has no "top" to go back to.
private struct ZoomablePage: View {
    let url: URL
    @State private var viewport = PageViewport()

    var body: some View {
        ZoomableImageView(url: url, viewport: viewport)
            .overlay(alignment: .bottomTrailing) { PageControlButton(viewport: viewport) }
    }
}

/// One page image that pinches to zoom and double-taps between fit and 2×.
struct ZoomableImageView: UIViewRepresentable {
    let url: URL
    let viewport: PageViewport

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> UIScrollView {
        let scroll = UIScrollView()
        scroll.delegate = context.coordinator
        scroll.minimumZoomScale = 1
        scroll.maximumZoomScale = 5
        scroll.showsHorizontalScrollIndicator = false
        scroll.backgroundColor = .secondarySystemBackground
        let image = UIImageView()
        image.contentMode = .scaleAspectFit
        image.translatesAutoresizingMaskIntoConstraints = false
        image.isAccessibilityElement = false
        scroll.addSubview(image)
        NSLayoutConstraint.activate([
            image.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor),
            image.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor),
            image.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor),
            image.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor),
            image.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor),
            image.heightAnchor.constraint(equalTo: scroll.frameLayoutGuide.heightAnchor),
        ])
        context.coordinator.imageView = image
        context.coordinator.viewport = viewport
        viewport.attach { [weak scroll] _ in scroll?.setZoomScale(1, animated: true) }
        let doubleTap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.toggleZoom(_:)))
        doubleTap.numberOfTapsRequired = 2
        scroll.addGestureRecognizer(doubleTap)
        return scroll
    }

    func updateUIView(_ scroll: UIScrollView, context: Context) {
        guard context.coordinator.url != url else { return }
        context.coordinator.url = url
        context.coordinator.imageView?.image = UIImage(contentsOfFile: url.path)
        scroll.zoomScale = 1
    }

    final class Coordinator: NSObject, UIScrollViewDelegate {
        var imageView: UIImageView?
        var url: URL?
        weak var viewport: PageViewport?

        func viewForZooming(in scrollView: UIScrollView) -> UIView? {
            imageView
        }

        func scrollViewDidZoom(_ scrollView: UIScrollView) {
            MainActor.assumeIsolated {
                viewport?.report(scrollView.zoomScale > 1 + FittingPDFView.fitTolerance ? .fit : nil)
            }
        }

        @objc func toggleZoom(_ gesture: UITapGestureRecognizer) {
            guard let scroll = gesture.view as? UIScrollView else { return }
            scroll.setZoomScale(scroll.zoomScale > 1 ? 1 : 2, animated: true)
        }
    }
}
