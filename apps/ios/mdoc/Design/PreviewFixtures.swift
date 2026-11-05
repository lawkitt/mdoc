#if DEBUG
import SwiftUI

/// Data for `#Preview`s (ADR 0099): the fake core with models installed and
/// no delays, and the sample document. Previews show every component and
/// screen state without walking the journey in the Simulator.
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

/// The traits each screen preview is shown in.
enum PreviewTraits {
    static let sizes: [DynamicTypeSize] = [.large, .accessibility5]
}
#endif
