import SwiftUI
import UIKit

@main
struct MdocApp: App {
    @UIApplicationDelegateAdaptor private var delegate: AppDelegate
    @State private var app = AppModel.live(downloader: AppDelegate.downloader)
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView(app: app)
                .onOpenURL { url in
                    // "Open in mdoc" from the share sheet's app row.
                    app.receiveFile(url)
                }
        }
        .onChange(of: scenePhase) { _, phase in
            // A launch in the background (a model download finishing) can
            // find the device locked and the case key unreadable; try again
            // once mdoc is in front.
            if phase == .active, app.startupError != nil {
                app = AppModel.live(downloader: AppDelegate.downloader)
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
        return true
    }

    func application(
        _ application: UIApplication,
        handleEventsForBackgroundURLSession identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        Self.downloader.handleEvents(for: identifier, completion: completionHandler)
    }
}

/// Home beside the open document on iPad; a stack on iPhone.
struct RootView: View {
    @Bindable var app: AppModel

    var body: some View {
        NavigationSplitView {
            HomeView(app: app)
        } detail: {
            if let error = app.startupError {
                ContentUnavailableView(
                    "mdoc could not open its documents",
                    systemImage: "exclamationmark.lock",
                    description: Text(error))
            } else if let id = app.selection, let model = app.documentModel(for: id) {
                DocumentScreen(model: model, setup: app.setup)
                    .id(id)
            } else {
                ContentUnavailableView(
                    "No document open",
                    systemImage: "doc.text",
                    description: Text("Choose a document, or add one with +."))
            }
        }
        .sheet(isPresented: $app.showsSettings) {
            SettingsView(setup: app.setup)
        }
    }
}
