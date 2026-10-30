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
    private var documents: [DocumentID: DocumentViewModel] = [:]

    init(
        core: Core,
        storage: MdocStorage,
        background: any BackgroundContinuation,
        downloader: any ModelDownloader,
        network: any NetworkConditions,
        defaults: UserDefaults = .standard
    ) {
        self.core = core
        self.storage = storage
        jobs = LongJobRunner(background: background)
        home = HomeViewModel(library: core.library)
        setup = SetupViewModel(setup: core.setup, downloader: downloader, network: network, jobs: jobs, defaults: defaults)
    }

    static func live() -> AppModel {
        let storage = (try? MdocStorage.shared()) ?? MdocStorage(root: FileManager.default.temporaryDirectory.appendingPathComponent("mdoc"))
        try? storage.prepare()
        storage.clear(.handOff)
        let arguments = ProcessInfo.processInfo.arguments
        let testing = arguments.contains(CoreFactory.uiTestingArgument)
        let defaults = testing ? UserDefaults(suiteName: "mdoc-ui-testing") ?? .standard : .standard
        if testing {
            defaults.removePersistentDomain(forName: "mdoc-ui-testing")
        }
        return AppModel(
            core: CoreFactory.app(storage: storage, arguments: arguments),
            storage: storage,
            background: BackgroundContinuations.system(),
            downloader: FakeModelDownloader(seconds: testing ? 0 : 2),
            network: NetworkMonitor(),
            defaults: defaults)
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
        do {
            let copy = try storage.importCopy(of: url, in: .documents)
            if url.path.contains("/Inbox/") {
                try? FileManager.default.removeItem(at: url)
            }
            home.pendingImport = PendingImport(
                source: kind == .image ? .photos([copy]) : .file(copy, kind),
                suggestedTitle: url.deletingPathExtension().lastPathComponent)
        } catch {
            home.errorMessage = error.localizedDescription
        }
    }

    /// Page images from the document camera or Photos.
    func receiveImages(_ images: [Data], scanned: Bool) {
        do {
            let urls = try images.enumerated().map { index, data in
                try storage.write(data, named: "\(UUID().uuidString)-\(index + 1).jpg", in: .documents)
            }
            guard !urls.isEmpty else { return }
            let title = scanned ? String(localized: "Scan") : String(localized: "Photo")
            home.pendingImport = PendingImport(
                source: scanned ? .scan(urls) : .photos(urls),
                suggestedTitle: "\(title) \(Date.now.formatted(date: .abbreviated, time: .shortened))")
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

    static let docx = UTType(importedAs: "org.openxmlformats.wordprocessingml.document")
    static let importTypes: [UTType] = [.pdf, docx, .image]

    static func kind(of url: URL) -> SourceKind? {
        guard let type = UTType(filenameExtension: url.pathExtension) else { return nil }
        if type.conforms(to: .pdf) { return .pdf }
        if type.conforms(to: docx) || url.pathExtension.lowercased() == "docx" { return .docx }
        if type.conforms(to: .image) { return .image }
        return nil
    }
}
