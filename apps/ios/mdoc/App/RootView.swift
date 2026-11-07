import SwiftUI

/// The window: on a wide iPad the sidebar stands beside the main area; on
/// iPhone and a narrow iPad it lies under the screen, which slides aside to
/// show it (ADR 0102). The layout comes from the window's size
/// (`DocumentLayout`); what is open and presented, from the navigator.
struct RootView: View {
    @Bindable var app: AppModel
    @State private var windowWidth: CGFloat = 0
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var layout: DocumentLayout { DocumentLayout(sizeClass: sizeClass, width: windowWidth) }

    var body: some View {
        Group {
            if layout.sidebarStands {
                standing
            } else {
                drawer
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { windowWidth = $0 }
        .onChange(of: layout.sidebarStands, initial: true) { _, stands in
            app.navigator.sidebarLayoutChanged(stands: stands)
        }
        .environment(\.documentLayout, layout)
        // The one place anything is presented over the screens (ADR 0104).
        .modifier(Presenter(app: app))
        .environment(\.appModel, app)
        .environment(\.preferences, app.preferences)
    }

    /// iPhone and a narrow iPad: the sidebar lies under the screen, which
    /// slides aside to show it (docs/mobile-ui.md, Sidebar).
    private var drawer: some View {
        SideDrawer(isOpen: Bindable(app.navigator).showsSidebar) {
            SidebarView(app: app)
        } content: {
            IdentitiesDrawer(model: app.openModel, layout: layout) {
                NavigationStack {
                    main
                }
            }
            .environment(\.showSidebar) {
                withAnimation(Motion.move(reduced: reduceMotion)) { app.navigator.showsSidebar = true }
            }
        }
    }

    /// A wide iPad: the same sidebar as the standing first column.
    private var standing: some View {
        NavigationSplitView(columnVisibility: columns) {
            SidebarView(app: app)
                // At accessibility sizes a wider sidebar keeps titles whole.
                .navigationSplitViewColumnWidth(
                    min: 280, ideal: dynamicTypeSize.isAccessibilitySize ? 420 : 320, max: 480)
                .toolbar(.hidden, for: .navigationBar)
        } detail: {
            main
        }
    }

    /// The split view's columns follow whether the sidebar shows; its own
    /// button hides and shows it.
    private var columns: Binding<NavigationSplitViewVisibility> {
        Binding(
            get: { app.navigator.showsSidebar ? .automatic : .detailOnly },
            set: { app.navigator.showsSidebar = $0 != .detailOnly })
    }

    /// The main area: a document, a section's page, or Start.
    @ViewBuilder
    private var main: some View {
        if let error = app.startupError {
            ContentUnavailableView(
                "mdoc could not open its documents",
                systemImage: "exclamationmark.lock",
                description: Text(error))
        } else {
            switch app.navigator.destination {
            case .document(let id):
                if let model = app.documents.model(for: id) {
                    DocumentScreen(model: model, setup: app.setup)
                        .id(id)
                } else {
                    StartView(app: app)
                }
            case .section(.cases): CasesView(app: app)
            case .section(.documents): DocumentsPage(app: app)
            case .start: StartView(app: app)
            }
        }
    }
}
