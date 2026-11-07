import SwiftUI
import UIKit

@main
struct MdocApp: App {
    @UIApplicationDelegateAdaptor private var delegate: AppDelegate
    @State private var app = AppModel.live(downloader: AppDelegate.downloader)
    @Environment(\.scenePhase) private var scenePhase

    init() {
        ReviewHint.configure(testing: LaunchConfiguration.current.isUITesting)
    }

    private var theme: AppTheme { app.preferences.theme }

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
                .preferredColorScheme(theme.colorScheme)
                .onChange(of: theme, initial: true) { theme.apply() }
        }
        .onChange(of: scenePhase) { _, phase in
            // A Delete waiting on its toast happens before mdoc goes (Q17).
            if phase == .background {
                app.leftForeground()
            }
            app = app.reopened(isActive: phase == .active) {
                AppModel.live(downloader: AppDelegate.downloader)
            }
            // The unlock's notification can come while mdoc is suspended.
            if phase == .active {
                app.saveDeferred()
            }
        }
        .commands {
            // A hardware keyboard, as in the Claude app; iPad lists them
            // when ⌘ is held.
            CommandGroup(replacing: .newItem) {
                Button("New document") { app.showStart() }
                    .keyboardShortcut("n", modifiers: .command)
                Button("Search documents and cases") { app.requestSearch() }
                    .keyboardShortcut("k", modifiers: .command)
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
