import SwiftUI
import UIKit

@main
struct MdocApp: App {
    @UIApplicationDelegateAdaptor private var delegate: AppDelegate
    @State private var app = AppModel.live(downloader: AppDelegate.downloader)
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage(AppTheme.key, store: MarkdownDisplay.store) private var theme = AppTheme.automatic.rawValue

    private var appTheme: AppTheme { AppTheme(rawValue: theme) ?? .automatic }

    init() {
        ReviewHint.configure(testing: CoreFactory.isUITesting)
    }

    var body: some Scene {
        WindowGroup {
            RootView(app: app)
                .onOpenURL { url in
                    // "Open in mdoc" from the share sheet's app row.
                    app.receiveFile(url)
                }
                .onReceive(NotificationCenter.default.publisher(for: UIApplication.protectedDataDidBecomeAvailableNotification)) { _ in
                    app.saveDeferred()
                }
                // Settings › Appearance › Theme (ADR 0099), for every window.
                .preferredColorScheme(appTheme.colorScheme)
                .onChange(of: theme, initial: true) { appTheme.apply() }
        }
        .onChange(of: scenePhase) { _, phase in
            app = app.reopened(isActive: phase == .active) {
                AppModel.live(downloader: AppDelegate.downloader)
            }
            // The unlock's notification can come while mdoc is suspended.
            if phase == .active {
                app.saveDeferred()
            }
        }
    }
}

/// Hands background model-download events to the downloader.
final class AppDelegate: NSObject, UIApplicationDelegate {
    /// One downloader for the app's life, so a session the system wakes mdoc
    /// for reconnects to the same delegate.
    static let downloader = BackgroundModelDownloader(
        staging: ((try? MdocStorage.shared()) ?? MdocStorage(root: FileManager.default.temporaryDirectory)).url(.downloads))

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        // Downloads reconnect whether or not the core can open yet.
        Self.downloader.resumeSessions()
        ProtectedData.isAvailable = application.isProtectedDataAvailable
        return true
    }

    func applicationProtectedDataWillBecomeUnavailable(_ application: UIApplication) {
        ProtectedData.isAvailable = false
    }

    func applicationProtectedDataDidBecomeAvailable(_ application: UIApplication) {
        ProtectedData.isAvailable = true
    }

    func application(
        _ application: UIApplication,
        handleEventsForBackgroundURLSession identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        Self.downloader.handleEvents(for: identifier, completion: completionHandler)
    }
}

/// Home beside the open document on iPad; a stack on iPhone. In a narrow
/// iPad window (portrait) the sidebar hides once a document opens, so the
/// text gets the room (ADR 0087).
struct RootView: View {
    @Bindable var app: AppModel
    @State private var columns: NavigationSplitViewVisibility = .automatic
    @State private var windowWidth: CGFloat = 0
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Narrower than every iPad in landscape, wider than every iPad in portrait.
    static let narrowWindow: CGFloat = 1100

    /// A regular-width window too narrow for Home, the text, the original
    /// and Identities side by side; iPhone's stack never is.
    private var isNarrow: Bool { isNarrow(windowWidth) }

    private func isNarrow(_ width: CGFloat) -> Bool {
        sizeClass == .regular && width > 0 && width < Self.narrowWindow
    }

    var body: some View {
        Group {
            if sizeClass == .compact {
                compact
            } else {
                regular
            }
        }
        .modifier(IntakeFlows(app: app))
        .modifier(DocumentCommandsPresenter(app: app))
        .environment(\.appModel, app)
        .background { keyboardShortcuts }
        .sheet(isPresented: $app.showsSettings) {
            SettingsView(setup: app.setup) {
                app.showsSettings = false
                app.trySample()
            }
        }
    }

    /// A hardware keyboard: ⌘K searches documents and cases, ⌘N starts
    /// something new, as in the Claude app.
    private var keyboardShortcuts: some View {
        Group {
            Button("Search documents and cases") { app.requestSearch(inDrawer: sizeClass == .compact) }
                .keyboardShortcut("k", modifiers: .command)
            Button("New document") { app.showStart() }
                .keyboardShortcut("n", modifiers: .command)
        }
        .opacity(0)
        .frame(width: 0, height: 0)
        .accessibilityHidden(true)
    }

    /// iPhone (and a narrow iPad window): the sidebar lies under the screen,
    /// which slides aside to show it (docs/mobile-ui.md, Sidebar).
    private var compact: some View {
        SideDrawer(isOpen: $app.showsSidebar) {
            SidebarView(app: app)
        } content: {
            IdentitiesDrawer(app: app) {
                NavigationStack {
                    main
                }
            }
            .environment(\.showSidebar) {
                withAnimation(Motion.move(reduced: reduceMotion)) { app.showsSidebar = true }
            }
        }
    }

    /// A wide iPad: the same sidebar as the standing first column.
    private var regular: some View {
        NavigationSplitView(columnVisibility: $columns) {
            SidebarView(app: app)
                // At accessibility sizes a wider sidebar keeps titles whole.
                .navigationSplitViewColumnWidth(
                    min: 280, ideal: dynamicTypeSize.isAccessibilitySize ? 420 : 320, max: 480)
                .toolbar(.hidden, for: .navigationBar)
        } detail: {
            main
                .toolbar {
                    // The system's sidebar button does not show beside
                    // the document's inspector, so it gets its own.
                    if isNarrow && columns == .detailOnly {
                        ToolbarItem(placement: .topBarLeading) {
                            Button {
                                columns = .all
                            } label: {
                                Label("Show Sidebar", systemImage: "sidebar.leading")
                            }
                            .accessibilityIdentifier("show-sidebar")
                        }
                    }
                }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width in
            let changes = isNarrow(width) != isNarrow
            windowWidth = width
            if changes { fitColumns(narrow: isNarrow(width)) }
        }
        .onChange(of: app.selection) { fitColumns(narrow: isNarrow) }
        // ⌘K shows the sidebar column where it is hidden (#675).
        .onChange(of: app.searchRequest) {
            if columns == .detailOnly { columns = .all }
        }
        .onChange(of: sizeClass) { fitColumns(narrow: isNarrow) }
    }

    /// The main area: a document, a section's page, or Start.
    @ViewBuilder
    private var main: some View {
        if let error = app.startupError {
            ContentUnavailableView(
                "mdoc could not open its documents",
                systemImage: "exclamationmark.lock",
                description: Text(error))
        } else if let id = app.selection, let model = app.documentModel(for: id) {
            DocumentScreen(model: model, setup: app.setup, isNarrow: isNarrow)
                .id(id)
        } else {
            switch app.section {
            case .cases: CasesView(app: app)
            case .documents: HomeView(app: app)
            case nil: StartView(app: app)
            }
        }
    }

    /// The document alone in a narrow window, Home until one opens (a bound
    /// `.automatic` would hide Home there too); the system's choice when wide.
    /// The sidebar button still brings Home back.
    private func fitColumns(narrow: Bool) {
        guard narrow else {
            columns = .automatic
            return
        }
        columns = app.selection == nil ? .all : .detailOnly
    }
}
