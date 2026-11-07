import SwiftUI

/// Data for `#Preview`s (ADR 0099): the sample document, and the real core
/// over a temporary library holding it, its names proposed as Try a sample
/// proposes them. Previews show every component and screen state without
/// walking the journey in the Simulator. The sample is not behind
/// `#if DEBUG`, since previews of components compile in Release too; the
/// core and app model are, as only `#if DEBUG` previews use them and the
/// fixed case key they open the library with ships in no Release build.
@MainActor
enum PreviewFixtures {
    static let sample = SampleDocument.english
    static let russianSample = SampleDocument.russian

    #if DEBUG
    /// A fresh library on the real core, with the sample to review. A
    /// device below the detector floor, so no model is needed.
    static func core() -> (Core, MdocStorage) {
        let storage = MdocStorage(
            root: FileManager.default.temporaryDirectory.appendingPathComponent("mdoc-preview-\(UUID().uuidString)"))
        do {
            try storage.prepare()
            let core = try RealCore.make(storage: storage, keys: FixedCaseKeyStore(), physicalMemory: CoreFactory.belowFloorMemory)
            let session = try core.library.importDocument(.text(sample.text), title: sample.title, into: .newCase(name: sample.title))
            _ = try session.recognize(observer: PreviewObserver())
            _ = try session.propose(sample.names)
            return (core, storage)
        } catch {
            return (Core(library: UnavailableLibrary(), setup: UnavailableSetup()), storage)
        }
    }

    /// The app model over the real core, for screen previews.
    static func app() -> AppModel {
        let (core, storage) = core()
        return AppModel(
            core: core, storage: storage,
            background: NoBackgroundContinuation(), downloader: FakeModelDownloader(seconds: 0),
            network: NetworkMonitor(), defaults: UserDefaults(suiteName: "mdoc-preview") ?? .standard)
    }
    #endif
}

#if DEBUG
/// A preview's import: nothing to report, never cancelled.
private final class PreviewObserver: JobObserver, @unchecked Sendable {
    func progress(_ update: JobUpdate) {}
    func isCancelled() -> Bool { false }
    func onCancel(_ handler: @escaping @Sendable () -> Void) {}
}
#endif

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
