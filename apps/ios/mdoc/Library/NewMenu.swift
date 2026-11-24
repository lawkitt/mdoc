import SwiftUI
import VisionKit

/// The New menu: New document, Paste, Files, Photos, Scan. The first item
/// opens nearest the button, so the camera is last, away from it, and a
/// double tap cannot open it (ADR 0088). New document opens an empty
/// document in edit mode (ADR 0109), and Paste opens the clipboard's text
/// as a document; the others open their add flow through `Presenter`.
/// Shared by the sidebar and the Documents page.
struct NewMenuItems: View {
    let app: AppModel

    var body: some View {
        Button {
            app.newDocument()
        } label: {
            Label("New document", systemImage: "square.and.pencil")
        }
        .accessibilityHint(Text("An empty document to write in."))
        .accessibilityIdentifier("new-document")
        // The system's Paste, so one tap pastes without iOS's prompt.
        PasteButton(payloadType: String.self) { strings in
            guard let text = strings.first else { return }
            Task { @MainActor in app.paste(text) }
        }
        .accessibilityIdentifier("paste-text")
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
        PasteControl(look: .tile, types: [.plainText]) { providers in
            Task { @MainActor in
                if let text = await PastedReply.plainText(from: providers) { app.paste(text) }
            }
        }
        .frame(maxWidth: .infinity)
        .accessibilityIdentifier("empty-paste")
    }
}

/// One of the large add tiles.
struct AddTile: View {
    /// Its icon and words' least height, and its own with the card's
    /// padding, which Paste's tile matches.
    static let labelHeight: CGFloat = 96
    static let height = labelHeight + 2 * Space.s

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
            .frame(maxWidth: .infinity, minHeight: Self.labelHeight)
            .card(padding: Space.s)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(spoken))
        .accessibilityIdentifier(id)
    }
}

/// Where a screen shows the sidebar from: set where the sidebar slides
/// over (iPhone, a narrow iPad), and where a standing column is hidden; nil
/// while the column stands beside the screen.
extension EnvironmentValues {
    @Entry var showSidebar: (() -> Void)? = nil
    /// Hides a standing sidebar column (a wide iPad) while it shows: the
    /// split view's own toggle lives in the column's bar, which mdoc hides,
    /// and the Original's rail needs the room (#963). nil elsewhere.
    @Entry var hideSidebar: (() -> Void)? = nil
}

/// Hides the standing sidebar column; ☰ shows it again.
struct HideSidebarButton: ToolbarContent {
    let action: () -> Void

    var body: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Button(action: action) {
                Label("Hide sidebar", systemImage: "sidebar.left")
            }
            .accessibilityIdentifier("hide-sidebar")
        }
    }
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
