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
            if sizeClass != .compact && windowWidth == 0 {
                // A regular window's width decides between the two; until
                // it is known (the first layout), neither is made, so a
                // portrait iPad never builds the split view to drop it.
                Palette.background.ignoresSafeArea()
            } else if layout.sidebarStands {
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
    /// slides aside to show it (docs/mobile-ui.md, Sidebar), and an open
    /// document's Identities under its other side.
    private var drawer: some View {
        SideDrawer(
            leading: DrawerPanel(
                isOpen: Bindable(app.navigator).showsSidebar,
                closeLabel: "Close sidebar",
                // Editing, or fixing a name's words, only the edge opens it:
                // a sideways drag in the text is the text's (ADR 0114, 0115).
                opens: app.openModel.map { $0.isEditing || $0.wordsFix != nil } == true ? .edge : .anywhere
            ) {
                SidebarView(app: app)
            },
            trailing: IdentitiesPanel.drawer(model: app.openModel, layout: layout)
        ) {
            NavigationStack {
                main
            }
            .environment(\.showSidebar) {
                withAnimation(Motion.move(reduced: reduceMotion)) { app.navigator.showsSidebar = true }
            }
        }
        // A narrow iPad's Identities inspector stands beside the drawer, in
        // the window's own host: inside the drawer's hosting controller an
        // inspector reset its child's traits without end (ADR 0114).
        .modifier(IdentitiesInspector(model: layout.isRegular ? app.openModel : nil))
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

/// A document's Identities as the inspector beside the screen, while the
/// window wants them there (`IdentitiesPlacement.inspector`).
private struct IdentitiesInspector: ViewModifier {
    let model: DocumentViewModel?

    func body(content: Content) -> some View {
        if let model {
            content.inspector(isPresented: Binding(
                get: { model.identitiesPlacement == .inspector && model.wantsIdentities },
                set: { model.showIdentities($0) })
            ) {
                IdentitiesPanel(model: model)
                    .inspectorColumnWidth(min: 260, ideal: 320, max: 420)
                    .reviewSheetDetents()
            }
        } else {
            content
        }
    }
}
