import Foundation
import Observation
import UIKit
import UniformTypeIdentifiers

/// The sidebar's section rows: each opens a page in the main area.
enum SidebarSection: String, Hashable, CaseIterable, Identifiable {
    case cases, documents
    var id: String { rawValue }
}

/// The ways in: the camera, Photos, Files and pasted text.
enum Intake: String, Identifiable, Hashable {
    case scan, photos, files, paste
    var id: String { rawValue }
}

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
    /// The open document; nil shows Start or the sidebar section's page.
    var selection: DocumentID? {
        didSet {
            guard selection != oldValue else { return }
            showsSettings = false
            if selection != nil {
                section = nil
                showsSidebar = false
            }
        }
    }
    var showsSettings = false
    /// The sidebar, slid out over the screen on iPhone (docs/mobile-ui.md,
    /// Sidebar); a standing column on a wide iPad.
    var showsSidebar = false
    /// A sidebar section's page in the main area, in place of a document.
    var section: SidebarSection?
    /// An add flow asked for from Start or the sidebar's New menu.
    var intake: Intake?
    /// Bumped by ⌘K: the sidebar opens with its search field focused.
    var searchRequest = 0
    /// ⌘K's request until a sidebar takes it: on iPhone the drawer builds
    /// its sidebar only once it opens, after the request (#675).
    private var searchPending = false
    /// The document Rename… is asking a title for.
    var renamingDocument: DocumentID?
    /// The case Rename… is asking a name for (Cases page).
    var renamingCase: CaseID?
    /// Move to case › New case…: the document it asks a case name for.
    var movingToNewCase: DocumentID?
    /// Move to case › All cases…: the document the case list is for.
    var choosingCase: DocumentID?
    /// Share from a document's menu, over whatever shows.
    var sharing: ShareItems?
    /// "Moved to Riverside · Undo" and the like, over every screen.
    var toast: ToastMessage?
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
        home = HomeViewModel(library: core.library, findsNames: core.setup.snapshot().tier == .detector)
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
            // Appearance lives in the App Group: each UI test starts from
            // the defaults, whatever an earlier test chose.
            for key in [AppTheme.key, MarkdownDisplay.serifKey] {
                MarkdownDisplay.store.removeObject(forKey: key)
            }
        }
        do {
            let (storage, core) = try open()
            var models: any ModelDownloader = downloader ?? BackgroundModelDownloader(staging: storage.url(.downloads))
            #if DEBUG
            // UI tests download nothing unless they ask; they run Debug builds.
            if testing && !CoreFactory.realDownloadsInUITests {
                models = FakeModelDownloader(seconds: 0)
            }
            #endif
            let app = AppModel(
                core: core, storage: storage, background: BackgroundContinuations.system(),
                downloader: models, network: NetworkMonitor(), defaults: defaults)
            if testing, let seed = ProcessInfo.processInfo.environment["MDOC_SEED_FILE"] {
                app.seedForUITesting(uiTestingFixture(seed))
            }
            return app
        } catch {
            let storage = MdocStorage(root: FileManager.default.temporaryDirectory.appendingPathComponent("mdoc-unavailable"))
            return AppModel(
                core: Core(library: UnavailableLibrary(), setup: UnavailableSetup()), storage: storage,
                background: NoBackgroundContinuation(), downloader: NoModelDownloader(),
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

    /// Where `just ios-device-test` copies the UI tests' fixtures on a phone:
    /// this folder in the app's data container (the app ships none).
    static let deviceFixturesFolder = "Library/mdoc-ui-fixtures"

    /// A UI test's fixture: its path on the Mac, which the Simulator reads,
    /// or on a phone, the copy of the same name in `deviceFixturesFolder`.
    static func uiTestingFixture(_ path: String) -> URL {
        let mac = URL(fileURLWithPath: path)
        if FileManager.default.fileExists(atPath: mac.path) {
            return mac
        }
        return URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
            .appendingPathComponent(deviceFixturesFolder, isDirectory: true)
            .appendingPathComponent(mac.lastPathComponent)
    }

    /// UI tests: import a fixture file through the core and scan it with
    /// the structured rules, so the review has candidates without a model.
    /// This is the only scan below the detector floor: the app itself
    /// offers no Find names there (ADR 0088). A Markdown file comes in as
    /// pasted text, and a folder of JPEG pages as a camera scan.
    private func seedForUITesting(_ file: URL) {
        Task {
            let title = file.deletingPathExtension().lastPathComponent
            let source: ImportSource
            var isFolder: ObjCBool = false
            if FileManager.default.fileExists(atPath: file.path, isDirectory: &isFolder), isFolder.boolValue {
                let folder = storage.url(.originals).appendingPathComponent(UUID().uuidString, isDirectory: true)
                let pages = ((try? FileManager.default.contentsOfDirectory(at: file, includingPropertiesForKeys: nil)) ?? [])
                    .filter { $0.pathExtension.lowercased() == "jpg" }
                    .sorted { $0.lastPathComponent < $1.lastPathComponent }
                guard (try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)) != nil,
                      let copies = try? pages.map({ page in
                          let copy = folder.appendingPathComponent(page.lastPathComponent)
                          try FileManager.default.copyItem(at: page, to: copy)
                          return copy
                      }) else { return }
                source = .scan(copies)
            } else if file.pathExtension.lowercased() == "md" {
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
            // A scan waits here for its OCR model.
            if model.snapshot.phase == .unscanned {
                await model.findNames()
            }
            home.refresh()
        }
    }

    /// The open document shows the setup card itself (it asked for setup),
    /// so Home leaves its own out: on iPad both are on screen.
    var openDocumentShowsSetup: Bool {
        selection.flatMap { documents[$0] }?.needsSetup ?? false
    }

    /// Saves what finished while the phone was locked, and shows it saved.
    func saveDeferred() {
        do {
            let saved = try core.library.saveDeferred()
            guard !saved.isEmpty else { return }
            for id in saved {
                documents[id]?.refresh()
            }
            home.refresh()
        } catch {
            home.errorMessage = error.localizedDescription
            home.refresh()
        }
    }

    func documentModel(for id: DocumentID) -> DocumentViewModel? {
        if let model = documents[id] {
            return model
        }
        guard let session = try? core.library.session(for: id) else { return nil }
        let model = DocumentViewModel(
            session: session, jobs: jobs, storage: storage, findsNames: setup.snapshot.tier == .detector
        ) { [weak self] in
            self?.home.refresh()
        }
        documents[id] = model
        return model
    }

    // MARK: Intake

    /// Imports into the chosen case, opens the document and starts
    /// recognition, which the user asked for by adding it.
    @discardableResult
    func finishImport(_ pending: PendingImport, title: String, into caseChoice: CaseChoice) -> DocumentViewModel? {
        guard let session = home.importDocument(pending, title: title, into: caseChoice),
              let model = documentModel(for: session.id) else { return nil }
        selection = session.id
        if model.snapshot.phase == .recognizing {
            Task { await model.recognize() }
        }
        return model
    }

    /// Opens a section's page and closes the sidebar on iPhone. The
    /// Documents row lists every document, whatever case was opened
    /// before (#676).
    func open(_ section: SidebarSection) {
        if section == .documents {
            home.caseFilter = nil
        }
        selection = nil
        self.section = section
        showsSidebar = false
    }

    /// A case's row: the Documents page filtered to that case.
    func openCase(_ id: CaseID) {
        open(.documents)
        home.caseFilter = id
    }

    /// ⌘K: the sidebar's search. On iPhone the drawer opens; a split view
    /// shows its sidebar column itself, and the drawer's flag stays down so
    /// a window that turns compact opens nothing (#675).
    func requestSearch(inDrawer: Bool) {
        if inDrawer {
            showsSidebar = true
        }
        searchPending = true
        searchRequest += 1
    }

    /// Whether ⌘K asked for the search and no sidebar took it yet; taking
    /// it clears it.
    func takeSearchRequest() -> Bool {
        defer { searchPending = false }
        return searchPending
    }

    /// Start: no document and no section.
    func showStart() {
        selection = nil
        section = nil
        showsSidebar = false
    }

    /// The document most worth going back to: the newest with names left to
    /// review.
    var continueDocument: DocumentSummary? {
        home.documents.filter { $0.phase == .reviewing }.max { $0.updated < $1.updated }
    }

    /// The document a job is working on now, for the sidebar's spinner.
    var workingDocument: DocumentID? { jobs.current?.documentID }

    // MARK: Rename and Move to case (ADR 0102)

    /// The five cases most recently created, without the one the document is
    /// in: Move to case's quick list.
    func recentCases(excluding current: CaseID?) -> [CaseSummary] {
        Array(home.cases.filter { $0.id != current }.prefix(5))
    }

    func rename(_ id: DocumentID, to title: String) {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return }
        do {
            try core.library.renameDocument(id, to: title)
            documents[id]?.refresh()
            home.refresh()
            Haptics.decision()
        } catch {
            home.errorMessage = error.localizedDescription
        }
    }

    func renameCase(_ id: CaseID, to name: String) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return }
        do {
            try core.library.renameCase(id, to: name)
            home.refresh()
            documents.values.forEach { $0.refresh() }
            Haptics.decision()
        } catch {
            home.errorMessage = error.localizedDescription
        }
    }

    /// Move to case: the names take the case's aliases; the toast offers
    /// Undo, which moves the document back.
    func move(_ id: DocumentID, to target: CaseChoice) {
        var target = target
        if case .newCase(let name) = target {
            // As every new case is named (#677): trimmed, never blank, and
            // numbered when a case has the name already.
            let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !name.isEmpty else { return }
            target = .newCase(name: HomeViewModel.unusedName(name, among: home.cases.map(\.name)))
        }
        do {
            let move = try core.library.moveDocument(id, to: target)
            documents[id]?.refresh()
            home.refresh()
            Haptics.decision()
            toast = ToastMessage(
                text: String(localized: "Moved to \(move.to.name)"),
                actionTitle: move.from == nil ? nil : String(localized: "Undo"),
                action: move.from == nil ? nil : { [weak self] in self?.undoMove(move) })
        } catch {
            home.errorMessage = error.localizedDescription
        }
    }

    private func undoMove(_ move: CaseMove) {
        do {
            let back = try core.library.undoMove(move)
            documents[move.document]?.refresh()
            home.refresh()
            Haptics.warning()
            toast = ToastMessage(text: String(localized: "Moved back to \(back.to.name)"))
        } catch {
            home.errorMessage = error.localizedDescription
        }
    }

    /// Share from a menu: the reviewed text, over whatever shows.
    func share(_ id: DocumentID) {
        sharing = shareItems(for: id)
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
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        home.pendingImport = PendingImport(source: .text(text), suggestedTitle: Self.suggestedTitle(for: text))
    }

    /// Paste's one sheet: the text, its title and its case at once (ADR 0087).
    func addText(_ text: String, title: String, into caseChoice: CaseChoice) {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        finishImport(PendingImport(source: .text(text), suggestedTitle: Self.suggestedTitle(for: text)), title: title, into: caseChoice)
    }

    /// Home's Share: Copy for AI's text of a reviewed document, as the
    /// document's own Share gives it.
    func shareItems(for id: DocumentID) -> ShareItems? {
        guard let model = documentModel(for: id) else { return nil }
        model.share(asFile: false)
        defer { model.share = nil }
        if let message = model.errorMessage {
            home.errorMessage = message
            model.errorMessage = nil
        }
        return model.share
    }

    /// Try a sample (ADR 0099): the made-up agreement in the phone's
    /// language, in a case of its own, opened for review.
    func trySample() {
        let sample = SampleDocument.forLanguages()
        // Its names come in as proposals, so it reviews before the name
        // finder is downloaded; only into the sample itself, never the
        // document open before when the import fails.
        let pending = PendingImport(source: .text(sample.text), suggestedTitle: sample.title)
        finishImport(pending, title: sample.title, into: .newCase(name: sample.title))?.proposeStored(sample.names)
    }

    /// The first line, without Markdown heading marks, up to 60 characters.
    static func suggestedTitle(for text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let firstLine = trimmed.split(separator: "\n", maxSplits: 1).first.map(String.init) ?? ""
        let title = firstLine.trimmingCharacters(in: CharacterSet(charactersIn: "# ")).prefix(60)
        return title.isEmpty ? String(localized: "Pasted text") : String(title)
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
