import Foundation
import Observation
import UIKit

/// The app's root (ADR 0104): the core and its storage, the one job runner,
/// and the owners the screens read: the navigator (where the app is), the
/// library, setup, the open documents' models and the preferences. Its
/// methods are the app's commands, shared by every menu and screen; a
/// command that fails presents why.
@MainActor
@Observable
final class AppModel {
    let core: Core
    let storage: MdocStorage
    let jobs: LongJobRunner
    let navigator = Navigator()
    let library: LibraryModel
    let setup: SetupViewModel
    let importer: Importer
    let preferences: Preferences
    /// Outside observation: the navigator says what is open.
    @ObservationIgnored let documents: DocumentStore
    /// "Moved to Riverside · Undo" and the like, over every screen.
    private(set) var toast: ToastMessage?
    /// A Delete whose toast still offers Undo (Q17).
    private var pendingDelete: PendingDelete?
    /// Why the core could not open (the Keychain or storage failed).
    let startupError: String?

    private struct PendingDelete {
        let id: DocumentID
        let toast: ToastMessage.ID
        let wasOpen: Bool
    }

    init(
        core: Core,
        storage: MdocStorage,
        background: any BackgroundContinuation,
        downloader: any ModelDownloader,
        network: any NetworkConditions,
        preferences: Preferences = .shared,
        startupError: String? = nil
    ) {
        self.startupError = startupError
        self.core = core
        self.storage = storage
        self.preferences = preferences
        let jobs = LongJobRunner(background: background)
        let library = LibraryModel(library: core.library, findsNames: core.setup.snapshot().tier == .detector)
        let setup = SetupViewModel(setup: core.setup, downloader: downloader, network: network, jobs: jobs, preferences: preferences)
        self.jobs = jobs
        self.library = library
        self.setup = setup
        importer = Importer(storage: storage)
        documents = DocumentStore { id in
            DocumentViewModel(
                session: try core.library.session(for: id), jobs: jobs, storage: storage,
                findsNames: setup.snapshot.tier == .detector
            ) { [weak library] in
                library?.refresh()
            }
        }
    }

    /// The app's model over the real core. `open` opens the storage and the
    /// core; when it fails (the Keychain or storage could not be read), the
    /// model shows why, over a core that has nothing.
    static func live(
        downloader: (any ModelDownloader)? = nil,
        launch: LaunchConfiguration = .current,
        open: () throws -> (MdocStorage, Core) = { try openCore(.current) }
    ) -> AppModel {
        let preferences = Preferences.shared
        if launch.isUITesting {
            // Each UI test starts from the defaults, whatever an earlier
            // test chose; the user's own choices are kept apart.
            preferences.reset()
        }
        do {
            let (storage, core) = try open()
            var models: any ModelDownloader = downloader ?? BackgroundModelDownloader(staging: storage.url(.downloads))
            #if DEBUG
            // UI tests download nothing unless they ask; they run Debug builds.
            if launch.isUITesting && !launch.realDownloads {
                models = FakeModelDownloader(seconds: 0)
            }
            #endif
            let app = AppModel(
                core: core, storage: storage, background: BackgroundContinuations.system(),
                downloader: models, network: NetworkMonitor(), preferences: preferences)
            if let seed = launch.seedFile {
                UITestSeed.run(UITestSeed.fixture(seed), in: app)
            }
            return app
        } catch {
            let storage = MdocStorage(root: FileManager.default.temporaryDirectory.appendingPathComponent("mdoc-unavailable"))
            return AppModel(
                core: Core(library: UnavailableLibrary(), setup: UnavailableSetup()), storage: storage,
                background: NoBackgroundContinuation(), downloader: NoModelDownloader(),
                network: NetworkMonitor(), preferences: preferences, startupError: error.localizedDescription)
        }
    }

    /// The App Group storage and the real core.
    nonisolated static func openCore(_ launch: LaunchConfiguration) throws -> (MdocStorage, Core) {
        let storage = try CoreFactory.appStorage(launch)
        try storage.prepare()
        storage.clear(.handOff)
        return (storage, try CoreFactory.appCore(storage: storage, launch))
    }

    /// The model to show once the scene's phase changes. A launch in the
    /// background (a model download finishing) can find the device locked and
    /// the case key unreadable; once mdoc is in front, it opens again.
    func reopened(isActive: Bool, open: () -> AppModel) -> AppModel {
        isActive && startupError != nil ? open() : self
    }

    // MARK: Navigation

    /// The open document's model.
    var openModel: DocumentViewModel? { documents.model(for: navigator.openDocument) }

    /// Opens a document: its model is made now, before any view draws it,
    /// and the models of documents no longer open go, but one a job still
    /// works on.
    func open(_ id: DocumentID) {
        do {
            _ = try documents.open(id)
        } catch {
            present(error)
            return
        }
        navigator.show(.document(id))
        evictClosedDocuments()
    }

    /// Opens a section's page and closes the sidebar on iPhone. The
    /// Documents row lists every document, whatever case was opened
    /// before (#676).
    func open(_ section: SidebarSection) {
        if section == .documents {
            library.caseFilter = nil
        }
        navigator.show(.section(section))
        evictClosedDocuments()
    }

    /// A case's row: the Documents page filtered to that case.
    func openCase(_ id: CaseID) {
        open(.documents)
        library.caseFilter = id
    }

    /// Start: no document and no section (⌘N).
    func showStart() {
        navigator.show(.start)
        evictClosedDocuments()
    }

    /// ⌘K: the sidebar's search.
    func requestSearch() {
        navigator.requestSearch()
    }

    private func evictClosedDocuments() {
        documents.evict(keeping: Set([navigator.openDocument, jobs.current?.documentID].compactMap { $0 }))
    }

    /// The open document shows the setup card itself (it asked for setup),
    /// so the Documents page leaves its own out: on iPad both are on screen.
    var openDocumentShowsSetup: Bool {
        openModel?.needsSetup ?? false
    }

    /// The document a job is working on now, for the sidebar's spinner.
    var workingDocument: DocumentID? { jobs.current?.documentID }

    /// Saves what finished while the phone was locked, and shows it saved.
    func saveDeferred() {
        do {
            let saved = try library.saveDeferred()
            for model in documents.all where saved.contains(model.id) {
                model.refresh()
            }
        } catch {
            present(error)
        }
    }

    // MARK: Presenting

    func present(_ error: Error) {
        navigator.present(.error(error.localizedDescription))
    }

    /// Shows `toast` over every screen. A Delete's toast that it replaces
    /// has ended: that document is deleted now.
    func show(_ toast: ToastMessage) {
        if let pending = pendingDelete, pending.toast != toast.id {
            commitPendingDelete()
        }
        self.toast = toast
    }

    /// The toast went: its time ran out, or it was put away.
    func toastEnded(_ id: ToastMessage.ID) {
        if toast?.id == id { toast = nil }
        if pendingDelete?.toast == id { commitPendingDelete() }
    }

    // MARK: Intake

    /// Imports into the chosen case, opens the document and starts
    /// recognition, which the user asked for by adding it.
    @discardableResult
    func finishImport(_ pending: PendingImport, title: String, into caseChoice: CaseChoice) -> DocumentViewModel? {
        do {
            let session = try library.importDocument(pending, title: title, into: caseChoice)
            if case .importing = navigator.presentation { navigator.dismiss() }
            open(session.id)
            guard let model = documents.model(for: session.id) else { return nil }
            if model.snapshot.phase == .recognizing {
                Task { await model.recognize() }
            }
            return model
        } catch {
            present(error)
            return nil
        }
    }

    /// A file from Files or "Open in mdoc", waiting for its title and case.
    func receiveFile(_ url: URL) {
        do {
            if let pending = try importer.file(url) {
                navigator.present(.importing(pending))
            }
        } catch {
            present(error)
        }
    }

    /// Pages from the document camera or Photos.
    func receiveImages(_ images: [Data], scanned: Bool) {
        do {
            if let pending = try importer.images(images, scanned: scanned) {
                navigator.present(.importing(pending))
            }
        } catch {
            present(error)
        }
    }

    /// Paste's one sheet: the text, its title and its case at once (ADR 0087).
    func addText(_ text: String, title: String, into caseChoice: CaseChoice) {
        guard let pending = Importer.text(text) else { return }
        finishImport(pending, title: title, into: caseChoice)
    }

    /// Try a sample (ADR 0099): the made-up agreement in the phone's
    /// language, in a case of its own, opened for review.
    func trySample() {
        if case .settings = navigator.presentation { navigator.dismiss() }
        let sample = SampleDocument.forLanguages()
        // Its names come in as proposals, so it reviews before the name
        // finder is downloaded; only into the sample itself, never the
        // document open before when the import fails.
        let pending = PendingImport(source: .text(sample.text), suggestedTitle: sample.title)
        finishImport(pending, title: sample.title, into: .newCase(name: sample.title))?.proposeStored(sample.names)
    }

    // MARK: Rename and Move to case (ADR 0102)

    func rename(_ id: DocumentID, to title: String) {
        guard !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        do {
            try library.rename(id, to: title)
            documents.model(for: id)?.refresh()
            Haptics.decision()
        } catch {
            present(error)
        }
    }

    func renameCase(_ id: CaseID, to name: String) {
        guard !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        do {
            try library.renameCase(id, to: name)
            documents.all.forEach { $0.refresh() }
            Haptics.decision()
        } catch {
            present(error)
        }
    }

    /// Move to case: the names take the case's aliases; the toast offers
    /// Undo, which moves the document back.
    func move(_ id: DocumentID, to target: CaseChoice) {
        do {
            guard let move = try library.move(id, to: target) else { return }
            documents.model(for: id)?.refresh()
            Haptics.decision()
            show(ToastMessage(
                text: String(localized: "Moved to \(move.to.name)"),
                actionTitle: move.from == nil ? nil : String(localized: "Undo"),
                action: move.from == nil ? nil : { [weak self] in self?.undoMove(move) }))
        } catch {
            present(error)
        }
    }

    private func undoMove(_ move: CaseMove) {
        do {
            let back = try library.undoMove(move)
            documents.model(for: move.document)?.refresh()
            Haptics.warning()
            show(ToastMessage(text: String(localized: "Moved back to \(back.to.name)")))
        } catch {
            present(error)
        }
    }

    // MARK: Share

    /// Share from a menu: the reviewed text, over whatever shows.
    func share(_ id: DocumentID) {
        if let items = shareItems(for: id) {
            navigator.present(.share(items))
        }
    }

    /// Copy for AI's text of a reviewed document, as the document's own
    /// Share gives it. A document not open is shared through a model made
    /// for it, which goes again.
    func shareItems(for id: DocumentID) -> ShareItems? {
        let wasHeld = documents.ids.contains(id)
        defer { if !wasHeld { evictClosedDocuments() } }
        let model: DocumentViewModel
        do {
            model = try documents.open(id)
        } catch {
            present(error)
            return nil
        }
        model.share(asFile: false)
        defer { model.share = nil }
        if let message = model.errorMessage {
            navigator.present(.error(message))
            model.errorMessage = nil
        }
        return model.share
    }

    // MARK: Delete with Undo (Q17)

    /// Delete: the document leaves the lists and a toast offers Undo; it is
    /// deleted when the toast ends, or when mdoc leaves the foreground.
    func delete(_ id: DocumentID) {
        commitPendingDelete()
        guard let document = library.document(id) else { return }
        let wasOpen = navigator.openDocument == id
        if wasOpen { showStart() }
        library.hide(id)
        let toast = ToastMessage(
            text: String(localized: "Deleted \(document.title)"),
            actionTitle: String(localized: "Undo"),
            action: { [weak self] in self?.undoDelete() })
        pendingDelete = PendingDelete(id: id, toast: toast.id, wasOpen: wasOpen)
        show(toast)
    }

    /// Undo of Delete: the document is back where it was, open again if it
    /// was open.
    func undoDelete() {
        guard let pending = pendingDelete else { return }
        pendingDelete = nil
        library.unhide(pending.id)
        Haptics.warning()
        if pending.wasOpen { open(pending.id) }
    }

    /// The Delete waiting on its toast happens now.
    func commitPendingDelete() {
        guard let pending = pendingDelete else { return }
        pendingDelete = nil
        documents.remove(pending.id)
        do {
            try library.delete(pending.id)
        } catch {
            present(error)
        }
    }

    /// The scene left the foreground: nothing waits on a toast the user may
    /// never come back to.
    func leftForeground() {
        commitPendingDelete()
    }
}
