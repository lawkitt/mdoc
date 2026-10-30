import SwiftUI

@main
struct MdocApp: App {
    @State private var app = AppModel.live()

    var body: some Scene {
        WindowGroup {
            RootView(app: app)
                .onOpenURL { url in
                    // "Open in mdoc" from the share sheet's app row.
                    app.receiveFile(url)
                }
        }
    }
}

/// Home beside the open document on iPad; a stack on iPhone.
struct RootView: View {
    @Bindable var app: AppModel

    var body: some View {
        NavigationSplitView {
            HomeView(app: app)
        } detail: {
            if let id = app.selection, let model = app.documentModel(for: id) {
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
