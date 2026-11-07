import PhotosUI
import SwiftUI
import VisionKit

/// The ways in, on the root so Start, the sidebar's New menu and the
/// Documents page share them: the document camera, Photos, Files and the
/// paste sheet set `AppModel.intake`; what comes in goes through the import
/// sheet (title and case), and errors show here.
struct IntakeFlows: ViewModifier {
    @Bindable var app: AppModel
    @State private var photoItems: [PhotosPickerItem] = []

    private var home: HomeViewModel { app.home }

    private func showing(_ intake: Intake) -> Binding<Bool> {
        Binding(get: { app.intake == intake }, set: { if !$0, app.intake == intake { app.intake = nil } })
    }

    func body(content: Content) -> some View {
        content
            .fullScreenCover(isPresented: showing(.scan)) {
                DocumentScanner { images in
                    app.intake = nil
                    app.receiveImages(images, scanned: true)
                } onCancel: {
                    app.intake = nil
                }
                .ignoresSafeArea()
            }
            .photosPicker(isPresented: showing(.photos), selection: $photoItems, maxSelectionCount: 20, matching: .images)
            .onChange(of: photoItems) { _, items in
                guard !items.isEmpty else { return }
                photoItems = []
                Task {
                    var images: [Data] = []
                    for item in items {
                        if let data = try? await item.loadTransferable(type: Data.self) {
                            images.append(data)
                        }
                    }
                    app.receiveImages(images, scanned: false)
                }
            }
            .fileImporter(isPresented: showing(.files), allowedContentTypes: AppModel.importTypes) { result in
                switch result {
                case .success(let url): app.receiveFile(url)
                case .failure(let error): home.errorMessage = error.localizedDescription
                }
            }
            .sheet(isPresented: showing(.paste)) {
                PasteTextSheet(cases: home.cases) { text, title, caseChoice in
                    app.intake = nil
                    app.addText(text, title: title, into: caseChoice)
                }
            }
            .sheet(item: Bindable(home).pendingImport) { pending in
                ImportSheet(pending: pending, cases: home.cases) { title, caseChoice in
                    app.finishImport(pending, title: title, into: caseChoice)
                }
            }
            .alert(
                "Something went wrong",
                isPresented: Binding(get: { home.errorMessage != nil }, set: { if !$0 { home.errorMessage = nil } }),
                presenting: home.errorMessage
            ) { _ in
                Button("OK") { home.errorMessage = nil }
            } message: { message in
                Text(message)
            }
    }
}

/// The New menu: Paste, Files, Photos, Scan. The first item opens nearest
/// the button, so the camera is last, away from it, and a double tap cannot
/// open it (ADR 0088).
struct NewMenuItems: View {
    let app: AppModel

    var body: some View {
        Button {
            app.intake = .paste
        } label: {
            Label("Paste text", systemImage: "doc.on.clipboard")
        }
        Button {
            app.intake = .files
        } label: {
            Label("Files", systemImage: "folder")
        }
        Button {
            app.intake = .photos
        } label: {
            Label("Photos", systemImage: "photo.on.rectangle")
        }
        if VNDocumentCameraViewController.isSupported {
            Button {
                app.intake = .scan
            } label: {
                Label("Scan", systemImage: "doc.viewfinder")
            }
        }
    }
}

/// Where a screen shows the sidebar from: set on iPhone, where the sidebar
/// slides over; nil where it stands beside the screen.
extension EnvironmentValues {
    @Entry var showSidebar: (() -> Void)? = nil
}

/// ☰ at the top left of a page or document, on iPhone.
struct SidebarButton: ToolbarContent {
    let action: () -> Void

    var body: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Button(action: action) {
                Label("Show sidebar", systemImage: "line.3.horizontal")
            }
            .accessibilityIdentifier("show-sidebar")
        }
    }
}
