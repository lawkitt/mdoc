import SwiftUI
import VisionKit

/// The New menu: Paste, Files, Photos, Scan. The first item opens nearest
/// the button, so the camera is last, away from it, and a double tap cannot
/// open it (ADR 0088). Each opens its add flow through `Presenter`, shared
/// by Start, the sidebar and the Documents page.
struct NewMenuItems: View {
    let app: AppModel

    var body: some View {
        Button {
            app.navigator.present(.intake(.paste))
        } label: {
            Label("Paste text", systemImage: "doc.on.clipboard")
        }
        Button {
            app.navigator.present(.intake(.files))
        } label: {
            Label("Files", systemImage: "folder")
        }
        Button {
            app.navigator.present(.intake(.photos))
        } label: {
            Label("Photos", systemImage: "photo.on.rectangle")
        }
        if VNDocumentCameraViewController.isSupported {
            Button {
                app.navigator.present(.intake(.scan))
            } label: {
                Label("Scan", systemImage: "doc.viewfinder")
            }
        }
    }
}

/// Start's and the empty Documents page's large tiles: Scan, Files and
/// Paste, in a row when they fit.
struct AddTiles: View {
    let app: AppModel

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: Space.s) { tiles }
            VStack(spacing: Space.s) { tiles }
        }
    }

    @ViewBuilder
    private var tiles: some View {
        if VNDocumentCameraViewController.isSupported {
            AddTile(title: "Scan", spoken: "Scan a document", id: "empty-scan", systemImage: "doc.viewfinder", color: .teal) {
                app.navigator.present(.intake(.scan))
            }
        }
        AddTile(title: "Files", spoken: "Add from Files", id: "empty-files", systemImage: "folder", color: .blue) {
            app.navigator.present(.intake(.files))
        }
        AddTile(title: "Paste", spoken: "Paste text", id: "empty-paste", systemImage: "doc.on.clipboard", color: .orange) {
            app.navigator.present(.intake(.paste))
        }
    }
}

/// One of the large add tiles.
struct AddTile: View {
    let title: LocalizedStringKey
    /// Apart from the + menu's item of the same name.
    let spoken: LocalizedStringKey
    let id: String
    let systemImage: String
    let color: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: Space.xs) {
                IconTile(systemImage: systemImage, color: color, size: 44)
                Text(title)
                    .font(Typography.chip)
                    .foregroundStyle(.primary)
            }
            .frame(maxWidth: .infinity, minHeight: 96)
            .card(padding: Space.s)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(spoken))
        .accessibilityIdentifier(id)
    }
}

/// Where a screen shows the sidebar from: set where the sidebar slides
/// over (iPhone, a narrow iPad); nil where it stands beside the screen.
extension EnvironmentValues {
    @Entry var showSidebar: (() -> Void)? = nil
}

/// ☰ at the top left of a page or document, where the sidebar slides over.
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
