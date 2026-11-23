import MdocFFI
import PDFKit
import SwiftUI
import UIKit

/// The page the text came from: PDFKit for PDFs and rendered DOCX files, a
/// zoomable image view for scans and photos (ADR 0085).
struct OriginalView: View {
    let original: OriginalSource
    @Binding var unlockedDocx: UnlockedDocx?
    var importPending = false
    /// A long job runs on the document (`DocxOriginalView.waits`).
    var jobRuns = false
    /// Nothing sits under the pane: the page runs to the screen's bottom.
    var reachesBottom = false
    /// A rendered DOCX's comments, for the title menu.
    var onComments: (DocxComments) -> Void = { _ in }
    /// The comment sheet's thread, and opening it (`DocxOriginalView`).
    var activeComment: String?
    var onOpenComment: (String) -> Void = { _ in }
    /// A locked PDF, unlocked in memory (`DocumentViewModel.unlockedOriginal`).
    var unlockedPDF: PDFDocument?
    /// The import's password prompt takes the place of the page.
    var importPrompt: DocumentViewModel.PasswordPrompt?
    /// The pane hosts the card when it covers Text and import isn't running.
    var importCardInPane = false
    var onImportUnlock: @MainActor (String) async -> Void = { _ in }
    /// The pane's own card unlocks a locked PDF; false for a wrong password.
    var onUnlockPDF: @MainActor (String) -> Bool = { _ in false }
    /// The Dark page hint shows, for the moon's one bounce.
    var onDarkPageHint: () -> Void = {}
    @State private var viewport = PageViewport()
    @State private var bottomInset: CGFloat = 0
    /// A page is on screen (`OriginalPageShowsKey`).
    @State private var pageShows = false
    @Environment(\.preferences) private var preferences
    @Environment(\.colorScheme) private var colorScheme

    private var dark: DarkPageState {
        DarkPageState(preference: preferences.darkPage, colorScheme: colorScheme)
    }

    /// The Dark page hint (#1070): a page on screen shown dark, as it is by
    /// default in a dark app, and the hints' one-at-a-time rule letting it
    /// show. It says where the moon is that switches it.
    private var hintShows: Bool {
        dark.isOn && pageShows && importPrompt == nil
            && Hints.shared.teachable.contains(.darkPage)
    }

    var body: some View {
        Group {
            if let importPrompt {
                if importCardInPane {
                    OriginalPasswordCard(prompt: importPrompt, purpose: .importing, unlock: onImportUnlock)
                } else {
                    ContentUnavailableView("This document is password-protected", systemImage: "lock")
                }
            } else {
                page
            }
        }
            .overlay(alignment: .bottom) {
                ZStack {
                    if hintShows {
                        DarkPageHintCapsule { Hints.shared.learn(.darkPage, closed: true) }
                        .padding(Space.m)
                        .padding(.bottom, reachesBottom ? bottomInset : 0)
                    }
                }
                .animation(Motion.snappy, value: hintShows)
            }
            // The page goes under the home indicator; its control stays
            // above it.
            .environment(\.pageControlInset, reachesBottom ? bottomInset : 0)
            .ignoresSafeArea(.container, edges: reachesBottom ? .bottom : [])
            // Read outside the page, where the home indicator is still unsafe.
            .onGeometryChange(for: CGFloat.self) { $0.safeAreaInsets.bottom } action: { bottomInset = $0 }
            .onPreferenceChange(OriginalPageShowsKey.self) { pageShows = $0 }
            .onChange(of: hintShows, initial: true) { _, shows in
                if shows { onDarkPageHint() }
            }
    }

    @ViewBuilder
    private var page: some View {
        switch original {
        case .none:
            ContentUnavailableView(
                "No original page",
                systemImage: "doc.plaintext",
                description: Text("Pasted text shows its text only."))
        case .pdf(let bytes):
            PDFOriginal(bytes: bytes, unlocked: unlockedPDF, unlock: onUnlockPDF, viewport: viewport, darkPage: dark.isOn)
        case .docx(let bytes):
            DocxOriginalView(
                source: bytes, unlocked: $unlockedDocx, importPending: importPending, waits: jobRuns,
                darkPage: dark.isOn, onComments: onComments, activeComment: activeComment,
                onOpenComment: onOpenComment)
        case .images(let pages):
            TabView {
                ForEach(Array(pages.enumerated()), id: \.offset) { index, bytes in
                    ZoomablePage(bytes: bytes, darkPage: dark.isOn)
                        .accessibilityLabel(Text("Original page \(index + 1) of \(pages.count)"))
                }
            }
            .tabViewStyle(.page(indexDisplayMode: pages.count > 1 ? .always : .never))
            .indexViewStyle(.page(backgroundDisplayMode: .always))
            .background(Color(uiColor: .secondarySystemBackground))
        }
    }
}

/// A PDF's pages. A locked PDF asks for its password in the pane, and
/// PDFKit unlocks it in memory: no unlocked copy is written. A PDF locked
/// only against editing opens as it is.
private struct PDFOriginal: View {
    let bytes: OriginalBytes
    let unlocked: PDFDocument?
    let unlock: @MainActor (String) -> Bool
    let viewport: PageViewport
    let darkPage: Bool
    /// The file as read, once per original.
    @State private var read: (bytes: OriginalBytes, document: PDFDocument?)?
    @State private var prompt = DocumentViewModel.PasswordPrompt.needed

    private var document: PDFDocument? {
        unlocked ?? read.flatMap { $0.bytes == bytes ? $0.document : nil }
    }

    var body: some View {
        Group {
            if document?.isLocked == true {
                OriginalPasswordCard(prompt: prompt) { password in
                    if !unlock(password) { prompt = .wrong }
                }
            } else {
                PDFKitView(document: document, viewport: viewport)
                    .darkPage(darkPage)
                    .originalPage()
                    .accessibilityLabel(Text("Original PDF"))
                    .overlay(alignment: .bottomTrailing) { PageControlButton(viewport: viewport) }
            }
        }
        .task(id: bytes) {
            if read?.bytes != bytes {
                // A stored original decrypts off the main thread.
                let resolved = try? await Task.detached(priority: .userInitiated) { [bytes] in try bytes.resolved() }.value
                let document: PDFDocument? = switch resolved {
                case .file(let url): PDFDocument(url: url)
                case .memory(let data): PDFDocument(data: data.bytes)
                case .stored, nil: nil
                }
                read = (bytes, document)
            }
        }
    }
}

struct PDFKitView: UIViewRepresentable {
    let document: PDFDocument?
    let viewport: PageViewport

    func makeUIView(context: Context) -> FittingPDFView {
        let view = FittingPDFView()
        view.autoScales = true
        view.displayMode = .singlePageContinuous
        view.backgroundColor = .secondarySystemBackground
        view.document = document
        view.onControl = { [weak viewport] in viewport?.report($0) }
        viewport.attach { [weak view] in view?.perform($0) }
        return view
    }

    func updateUIView(_ view: FittingPDFView, context: Context) {
        if view.document !== document {
            view.document = document
        }
    }
}

/// A scan page and its ⤡ control, shown while zoomed past fit; a page
/// turned sideways has no "top" to go back to.
private struct ZoomablePage: View {
    let bytes: OriginalBytes
    let darkPage: Bool
    @State private var viewport = PageViewport()

    var body: some View {
        ZoomableImageView(bytes: bytes, viewport: viewport)
            .darkPage(darkPage)
            .originalPage()
            .overlay(alignment: .bottomTrailing) { PageControlButton(viewport: viewport) }
    }
}

/// One page image that pinches to zoom and double-taps between fit and 2×.
struct ZoomableImageView: UIViewRepresentable {
    let bytes: OriginalBytes
    let viewport: PageViewport

    init(bytes: OriginalBytes, viewport: PageViewport) {
        self.bytes = bytes
        self.viewport = viewport
    }

    init(url: URL, viewport: PageViewport) {
        self.init(bytes: .file(url), viewport: viewport)
    }

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
            // At least the view's height, so a wide page sits in its middle;
            // a tall one grows past it (`fitWidth`) and scrolls.
            image.heightAnchor.constraint(greaterThanOrEqualTo: scroll.frameLayoutGuide.heightAnchor),
        ])
        // The picture's own size never wins over the width rule: tied with
        // it, a re-layout could take the picture's pixel height and leave
        // the page in the middle of a screen of blank (#1070).
        image.setContentCompressionResistancePriority(.defaultLow, for: .vertical)
        image.setContentHuggingPriority(.defaultLow, for: .vertical)
        context.coordinator.imageView = image
        context.coordinator.viewport = viewport
        viewport.attach { [weak scroll] _ in scroll?.setZoomScale(1, animated: true) }
        let doubleTap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.toggleZoom(_:)))
        doubleTap.numberOfTapsRequired = 2
        scroll.addGestureRecognizer(doubleTap)
        return scroll
    }

    func updateUIView(_ scroll: UIScrollView, context: Context) {
        guard context.coordinator.bytes != bytes else { return }
        context.coordinator.bytes = bytes
        let image = switch try? bytes.resolved() {
        case .file(let url): UIImage(contentsOfFile: url.path)
        case .memory(let data): UIImage(data: data.bytes)
        case .stored, nil: nil as UIImage?
        }
        context.coordinator.imageView?.image = image
        context.coordinator.fitWidth(image)
        scroll.zoomScale = 1
    }

    final class Coordinator: NSObject, UIScrollViewDelegate {
        var imageView: UIImageView?
        var bytes: OriginalBytes?
        weak var viewport: PageViewport?
        private var ratio: NSLayoutConstraint?

        /// Zoom 1 is the page at the view's width, as a PDF's fit is: a
        /// tall scan or receipt is never smaller than the width (#1036).
        func fitWidth(_ image: UIImage?) {
            ratio?.isActive = false
            ratio = nil
            guard let imageView, let image, image.size.width > 0 else { return }
            let constraint = imageView.heightAnchor.constraint(
                equalTo: imageView.widthAnchor, multiplier: image.size.height / image.size.width)
            // Under the floor (at least the view's height), over the
            // picture's own size.
            constraint.priority = .required - 1
            constraint.isActive = true
            ratio = constraint
        }

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
