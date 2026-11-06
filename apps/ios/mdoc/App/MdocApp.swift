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
        ReviewTips.configure(testing: CoreFactory.isUITesting)
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

    /// Narrower than every iPad in landscape, wider than every iPad in portrait.
    static let narrowWindow: CGFloat = 1100

    /// A regular-width window too narrow for Home, the text, the original
    /// and Identities side by side; iPhone's stack never is.
    private var isNarrow: Bool { isNarrow(windowWidth) }

    private func isNarrow(_ width: CGFloat) -> Bool {
        sizeClass == .regular && width > 0 && width < Self.narrowWindow
    }

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            HomeView(app: app)
                // At accessibility sizes a wider sidebar keeps titles whole.
                .navigationSplitViewColumnWidth(
                    min: 280, ideal: dynamicTypeSize.isAccessibilitySize ? 420 : 320, max: 480)
        } detail: {
            if let error = app.startupError {
                ContentUnavailableView(
                    "mdoc could not open its documents",
                    systemImage: "exclamationmark.lock",
                    description: Text(error))
            } else if let id = app.selection, let model = app.documentModel(for: id) {
                DocumentScreen(model: model, setup: app.setup, isNarrow: isNarrow)
                    .id(id)
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
            } else {
                ContentUnavailableView(
                    "No document open",
                    systemImage: "doc.text",
                    description: Text("Choose a document, or add one with +."))
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width in
            let changes = isNarrow(width) != isNarrow
            windowWidth = width
            if changes { fitColumns(narrow: isNarrow(width)) }
        }
        .onChange(of: app.selection) { fitColumns(narrow: isNarrow) }
        .onChange(of: sizeClass) { fitColumns(narrow: isNarrow) }
        .sheet(isPresented: $app.showsSettings) {
            SettingsView(setup: app.setup) {
                app.showsSettings = false
                app.trySample()
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
