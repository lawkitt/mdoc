import SwiftUI

/// Data for `#Preview`s (ADR 0099): the fake core with models installed and
/// no delays, and the sample document. Previews show every component and
/// screen state without walking the journey in the Simulator. Not behind
/// `#if DEBUG`: previews compile in Release too, so an archive needs these.
@MainActor
enum PreviewFixtures {
    static let sample = SampleDocument.english
    static let russianSample = SampleDocument.russian

    /// A fresh fake core: the seeded cases, the letter to review, models
    /// installed.
    static func core() -> Core {
        FakeCore.make(FakeCore.Options(timing: .instant, modelsInstalled: true))
    }

    /// The app model over the fake core, for screen previews.
    static func app() -> AppModel {
        AppModel(
            core: core(), storage: MdocStorage(root: FileManager.default.temporaryDirectory.appendingPathComponent("mdoc-preview")),
            background: NoBackgroundContinuation(), downloader: FakeModelDownloader(seconds: 0),
            network: NetworkMonitor(), defaults: UserDefaults(suiteName: "mdoc-preview") ?? .standard)
    }
}

/// Shows `content` in the preview matrix ADR 0099 asks for: light, dark,
/// AX5, and an iPad-wide frame.
struct PreviewMatrix<Content: View>: View {
    @ViewBuilder var content: () -> Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Space.l) {
                variant("Light") { content() }
                variant("Dark") { content().environment(\.colorScheme, .dark) }
                variant("AX5") { content().environment(\.dynamicTypeSize, .accessibility5) }
                variant("iPad width") { content().frame(width: 700) }
            }
            .padding()
        }
    }

    private func variant(_ title: String, @ViewBuilder _ view: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            Text(verbatim: title).font(Typography.caption).foregroundStyle(.secondary)
            view()
                .padding(Space.s)
                .background(Color(uiColor: .systemBackground))
        }
    }
}
