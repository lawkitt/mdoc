import Foundation
import Observation
import UIKit
import UniformTypeIdentifiers

/// The app's root: the core, storage, the one job runner and the screens'
/// view models.
@MainActor
@Observable
final class AppModel {
    let core: Core
    let storage: MdocStorage
    let jobs: LongJobRunner
    let home: HomeViewModel
    let setup: SetupViewModel
    var selection: DocumentID? {
        didSet { if selection != oldValue { showsSettings = false } }
    }
    var showsSettings = false
    /// Why the core could not open (the Keychain or storage failed).
    let startupError: String?
    private var documents: [DocumentID: DocumentViewModel] = [:]

    init(
        core: Core,
        storage: MdocStorage,
        background: any BackgroundContinuation,
        downloader: any ModelDownloader,
        network: any NetworkConditions,
        defaults: UserDefaults = .standard,
        startupError: String? = nil
    ) {
        self.startupError = startupError
        self.core = core
        self.storage = storage
        jobs = LongJobRunner(background: background)
        home = HomeViewModel(library: core.library)
        setup = SetupViewModel(setup: core.setup, downloader: downloader, network: network, jobs: jobs, defaults: defaults)
    }

    /// The app's model over the real core. `open` opens the storage and the
    /// core; when it fails (the Keychain or storage could not be read), the
    /// model shows why, over a core that has nothing.
    static func live(
        downloader: (any ModelDownloader)? = nil,
        open: () throws -> (MdocStorage, Core) = openCore
    ) -> AppModel {
        let testing = CoreFactory.isUITesting
        let defaults = testing ? UserDefaults(suiteName: "mdoc-ui-testing") ?? .standard : .standard
        if testing {
            defaults.removePersistentDomain(forName: "mdoc-ui-testing")
        }
        do {
            let (storage, core) = try open()
            let models = downloader ?? BackgroundModelDownloader(staging: storage.url(.downloads))
            let app = AppModel(
                core: core, storage: storage, background: BackgroundContinuations.system(),
                downloader: testing ? FakeModelDownloader(seconds: 0) : models,
                network: NetworkMonitor(), defaults: defaults)
            if testing, let seed = ProcessInfo.processInfo.environment["MDOC_SEED_FILE"] {
                app.seedForUITesting(URL(fileURLWithPath: seed))
            }
            return app
        } catch {
            let storage = MdocStorage(root: FileManager.default.temporaryDirectory.appendingPathComponent("mdoc-unavailable"))
            return AppModel(
                core: Core(library: UnavailableLibrary(), setup: UnavailableSetup()), storage: storage,
                background: NoBackgroundContinuation(), downloader: FakeModelDownloader(seconds: 0),
                network: NetworkMonitor(), defaults: defaults, startupError: error.localizedDescription)
        }
    }

    /// The App Group storage and the real core.
    nonisolated static func openCore() throws -> (MdocStorage, Core) {
        let storage = try CoreFactory.appStorage()
        try storage.prepare()
        storage.clear(.handOff)
        return (storage, try CoreFactory.appCore(storage: storage))
    }

    /// The model to show once the scene's phase changes. A launch in the
    /// background (a model download finishing) can find the device locked and
    /// the case key unreadable; once mdoc is in front, it opens again.
    func reopened(isActive: Bool, open: () -> AppModel) -> AppModel {
        isActive && startupError != nil ? open() : self
    }

    /// UI tests: import a fixture file through the core and scan it with
    /// the structured rules, as a user's import would. A Markdown file comes
    /// in as pasted text.
    private func seedForUITesting(_ file: URL) {
        Task {
            let title = file.deletingPathExtension().lastPathComponent
            let source: ImportSource
            if file.pathExtension.lowercased() == "md" {
                guard let text = try? String(contentsOf: file, encoding: .utf8) else { return }
                source = .text(text)
            } else {
                guard let copy = try? storage.importCopy(of: file, in: .originals),
                      let kind = Self.kind(of: copy) else { return }
                source = .file(copy, kind)
            }
            guard let session = home.importDocument(
                    PendingImport(source: source, suggestedTitle: title), title: title, into: .newCase(name: title)),
                  let model = documentModel(for: session.id) else { return }
            await model.recognize()
            await model.findNames()
            home.refresh()
        }
    }

    func documentModel(for id: DocumentID) -> DocumentViewModel? {
        if let model = documents[id] {
            return model
        }
        guard let session = try? core.library.session(for: id) else { return nil }
        let model = DocumentViewModel(session: session, jobs: jobs, storage: storage) { [weak self] in
            self?.home.refresh()
        }
        documents[id] = model
        return model
    }

    // MARK: Intake

    /// Imports into the chosen case, opens the document and starts
    /// recognition, which the user asked for by adding it.
    func finishImport(_ pending: PendingImport, title: String, into caseChoice: CaseChoice) {
        guard let session = home.importDocument(pending, title: title, into: caseChoice),
              let model = documentModel(for: session.id) else { return }
        selection = session.id
        if model.snapshot.phase == .recognizing {
            Task { await model.recognize() }
        }
    }

    func deleteDocument(_ id: DocumentID) {
        if selection == id {
            selection = nil
        }
        documents[id] = nil
        home.delete(id)
    }

    /// A file from Files or "Open in mdoc". It is copied into mdoc's storage
    /// and the original left alone (an Inbox copy is removed).
    func receiveFile(_ url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer {
            if scoped { url.stopAccessingSecurityScopedResource() }
        }
        guard let kind = Self.kind(of: url) else {
            home.errorMessage = String(localized: "mdoc opens PDF, DOCX and image files.")
            return
        }
        if kind == .image {
            // Photos from Files become JPEG pages like the camera's.
            guard let data = try? Data(contentsOf: url) else {
                home.errorMessage = String(localized: "mdoc opens PDF, DOCX and image files.")
                return
            }
            if url.path.contains("/Inbox/") {
                try? FileManager.default.removeItem(at: url)
            }
            receiveImages([data], scanned: false, title: url.deletingPathExtension().lastPathComponent)
            return
        }
        do {
            let copy = try storage.importCopy(of: url, in: .originals)
            if url.path.contains("/Inbox/") {
                try? FileManager.default.removeItem(at: url)
            }
            home.pendingImport = PendingImport(
                source: .file(copy, kind), suggestedTitle: url.deletingPathExtension().lastPathComponent)
        } catch {
            home.errorMessage = error.localizedDescription
        }
    }

    /// Page images from the document camera or Photos, as upright JPEGs:
    /// the core reads JPEG only (PNG and HEIC are converted here).
    func receiveImages(_ images: [Data], scanned: Bool, title: String? = nil) {
        do {
            let folder = storage.url(.originals).appendingPathComponent(UUID().uuidString, isDirectory: true)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let urls = try images.enumerated().compactMap { index, data -> URL? in
                guard let jpeg = Self.uprightJPEG(data) else { return nil }
                let url = folder.appendingPathComponent("page-\(index + 1).jpg")
                try jpeg.write(to: url, options: [.atomic, .completeFileProtection])
                return url
            }
            guard !urls.isEmpty else { return }
            let kind = scanned ? String(localized: "Scan") : String(localized: "Photo")
            home.pendingImport = PendingImport(
                source: scanned ? .scan(urls) : .photos(urls),
                suggestedTitle: title ?? "\(kind) \(Date.now.formatted(date: .abbreviated, time: .shortened))")
        } catch {
            home.errorMessage = error.localizedDescription
        }
    }

    func receiveText(_ text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        let firstLine = trimmed.split(separator: "\n", maxSplits: 1).first.map(String.init) ?? ""
        let title = firstLine.trimmingCharacters(in: CharacterSet(charactersIn: "# ")).prefix(60)
        home.pendingImport = PendingImport(
            source: .text(text),
            suggestedTitle: title.isEmpty ? String(localized: "Pasted text") : String(title))
    }

    /// The system declares DOCX; the extension lookup never needs an import.
    /// Redraws an image with its orientation applied, as JPEG.
    static func uprightJPEG(_ data: Data) -> Data? {
        guard let image = UIImage(data: data) else { return nil }
        guard image.imageOrientation != .up else { return image.jpegData(compressionQuality: 0.9) }
        let format = UIGraphicsImageRendererFormat()
        format.scale = image.scale
        let upright = UIGraphicsImageRenderer(size: image.size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: image.size))
        }
        return upright.jpegData(compressionQuality: 0.9)
    }

    static let docx = UTType("org.openxmlformats.wordprocessingml.document") ?? UTType(filenameExtension: "docx") ?? .data
    static let importTypes: [UTType] = [.pdf, docx, .image]

    static func kind(of url: URL) -> SourceKind? {
        guard let type = UTType(filenameExtension: url.pathExtension) else { return nil }
        if type.conforms(to: .pdf) { return .pdf }
        if type.conforms(to: docx) || url.pathExtension.lowercased() == "docx" { return .docx }
        if type.conforms(to: .image) { return .image }
        return nil
    }
}
