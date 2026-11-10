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
        startupError: String? = nil,
        findsNamesWithRules: Bool = false
    ) {
        self.startupError = startupError
        self.core = core
        self.storage = storage
        self.preferences = preferences
        let jobs = LongJobRunner(background: background)
        // The UI tests' stand-in finds names with the rules (`rulesFindNames`).
        let findsNames = findsNamesWithRules || core.setup.snapshot().tier == .detector
        let library = LibraryModel(library: core.library, findsNames: findsNames)
        let setup = SetupViewModel(setup: core.setup, downloader: downloader, network: network, jobs: jobs, preferences: preferences)
        self.jobs = jobs
        self.library = library
        self.setup = setup
        importer = Importer(storage: storage)
        documents = DocumentStore { id in
            DocumentViewModel(
                session: try core.library.session(for: id), jobs: jobs, storage: storage,
                findsNames: findsNamesWithRules || setup.snapshot.tier == .detector
            ) { [weak library] in
                library?.refresh()
            }
        }
        core.library.observeSaves { [weak self] event in self?.saved(event) }
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
                downloader: models, network: NetworkMonitor(), preferences: preferences,
                findsNamesWithRules: launch.rulesFindNames)
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
        show(.document(id))
    }

    /// Opens a section's page and closes the sidebar on iPhone. The
    /// Documents row lists every document, whatever case was opened
    /// before (#676).
    func open(_ section: SidebarSection) {
        if section == .documents {
            library.caseFilter = nil
        }
        show(.section(section))
    }

    /// A case's row: the Documents page filtered to that case.
    func openCase(_ id: CaseID) {
        open(.documents)
        library.caseFilter = id
    }

    /// Start: no document and no section (⌘N).
    func showStart() {
        show(.start)
    }

    /// Every way the app moves: a document left for anything else is saved
    /// first (ADR 0104: the save follows the tap), then the models of
    /// documents no longer open go.
    private func show(_ destination: Destination) {
        if let open = navigator.openDocument, destination != .document(open) {
            // Marked (IME) text is committed first: it is text, not empty.
            documents.model(for: open)?.typing.flush?()
            if !discardIfEmpty(open) {
                followFirstLine(of: open)
                saveChanges(of: open)
            }
        }
        navigator.show(destination)
        evictClosedDocuments()
    }

    /// A new document left before its typing paused takes its first line's
    /// title now, as the pause would have (ADR 0109).
    private func followFirstLine(of id: DocumentID) {
        guard let model = documents.model(for: id), model.snapshot.origin == .new else { return }
        do {
            if try model.session.followFirstLine() != nil { library.refresh() }
        } catch {
            present(error)
        }
    }

    /// A new document left empty goes when it is left, with its case: no
    /// empty row stays in the lists (ADR 0109). True when it went.
    private func discardIfEmpty(_ id: DocumentID) -> Bool {
        guard documents.model(for: id)?.snapshot.origin == .new else { return false }
        do {
            guard try library.discardIfEmpty(id) else { return false }
        } catch {
            present(error)
            return false
        }
        documents.remove(id)
        return true
    }

    /// ⌘K: the sidebar's search.
    func requestSearch() {
        navigator.requestSearch()
    }

    /// Lets go of the models of documents not open, but those with work
    /// still to do: a job running, a job waiting for its model's download
    /// (Download & continue holds the model), or what a finished job has to
    /// say that the document has not shown yet ("3 to review", an error).
    private func evictClosedDocuments() {
        var kept = Set([navigator.openDocument, jobs.current?.documentID].compactMap { $0 })
        kept.formUnion(documents.all.filter { $0.needsSetup || $0.notice != nil || $0.errorMessage != nil }.map(\.id))
        documents.evict(keeping: kept)
    }

    /// The open document shows the setup card itself (it asked for setup),
    /// so the Documents page leaves its own out: on iPad both are on screen.
    var openDocumentShowsSetup: Bool {
        openModel?.needsSetup ?? false
    }

    /// The document a job is working on now, for the sidebar's spinner.
    var workingDocument: DocumentID? { jobs.current?.documentID }

    // MARK: Saving (ADR 0104)

    /// What the taps changed in `id`, saved now.
    private func saveChanges(of id: DocumentID) {
        do {
            try documents.model(for: id)?.session.saveChanges()
        } catch {
            present(error)
        }
    }

    /// What the taps changed in every document, saved now: mdoc leaves the
    /// foreground. It runs before the phase change returns, so iOS does not
    /// suspend mdoc in the middle of it.
    func saveChanges() {
        do {
            try core.library.saveChanges()
        } catch {
            present(error)
        }
    }

    /// A save that followed taps ended: the lists show the document as
    /// saved, and a document whose case changed elsewhere shows its review
    /// as joined again. A failure shows on the document, or over whatever
    /// shows once it is closed.
    func saved(_ event: SaveEvent) {
        switch event {
        case .saved(let id, let rejoined):
            library.refresh()
            if rejoined { documents.model(for: id)?.refresh() }
        case .failed(let id, let error):
            if let model = documents.model(for: id), navigator.openDocument == id {
                model.errorMessage = error.localizedDescription
            } else {
                present(error)
            }
        }
    }

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

    /// New document (ADR 0109): an empty document in a case of its own,
    /// open in edit mode with the keyboard up. It is named after its first
    /// line until renamed, and goes if it is left empty.
    func newDocument() {
        if case .settings = navigator.presentation { navigator.dismiss() }
        let title = String(localized: "New document")
        do {
            let session = try library.importDocument(
                PendingImport(source: .newDocument, suggestedTitle: title), title: title, into: .newCase(name: title))
            // Its empty text is read now, outside the one job slot: a job on
            // another document never holds it back (ADR 0085 Long jobs).
            _ = try session.recognize(observer: JobToken { _ in })
            open(session.id)
            documents.model(for: session.id)?.enterEditMode()
        } catch {
            present(error)
        }
    }

    /// Imports into the chosen case, opens the document and starts
    /// recognition, which the user asked for by adding it.
    @discardableResult
    func finishImport(_ pending: PendingImport, title: String, into caseChoice: CaseChoice) -> DocumentViewModel? {
        do {
            let session = try library.importDocument(pending, title: title, into: caseChoice)
            if case .importing = navigator.presentation { navigator.dismiss() }
            open(session.id)
            guard let model = documents.model(for: session.id) else { return nil }
            if model.snapshot.phase == nil {
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
    /// language, in a case of its own, opened for review. The sample is
    /// editable; trying it again opens it with its text as it came, asking
    /// first when it was edited (ADR 0109).
    func trySample() {
        if case .settings = navigator.presentation { navigator.dismiss() }
        if let sample = library.documents.first(where: { $0.origin == .sample }) {
            open(sample.id)
            guard let model = documents.model(for: sample.id), navigator.openDocument == sample.id else { return }
            if !SampleDocument.isOriginal(model.snapshot.markdown) {
                // A job on it would lose its result to the edit: it says why.
                if let reason = model.editBlockedReason {
                    model.say(reason, symbol: "hourglass")
                } else {
                    navigator.present(.restoreSample(sample.id))
                }
            }
            return
        }
        let sample = SampleDocument.forLanguages()
        // Its names come in as proposals, so it reviews before the name
        // finder is downloaded; only into the sample itself, never the
        // document open before when the import fails.
        let pending = PendingImport(source: .sample(sample.text), suggestedTitle: sample.title)
        finishImport(pending, title: sample.title, into: .newCase(name: sample.title))?.proposeStored(sample.names)
    }

    /// "Restore the sample?" answered Restore: its text as it came, with its
    /// names proposed again. One step: Undo brings the edits back.
    func restoreSample(_ id: DocumentID) {
        guard let model = documents.model(for: id) else { return }
        model.restore(SampleDocument.forLanguages())
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
        // Open again where it closed, unless something else opened since.
        if pending.wasOpen && navigator.destination == .start { open(pending.id) }
    }

    /// The Delete waiting on its toast happens now; its toast, and the
    /// Undo it offered, go with it.
    func commitPendingDelete() {
        guard let pending = pendingDelete else { return }
        pendingDelete = nil
        if toast?.id == pending.toast { toast = nil }
        documents.remove(pending.id)
        do {
            try library.delete(pending.id)
        } catch {
            present(error)
        }
    }

    /// The scene went to the background: nothing waits on a toast the user
    /// may never come back to. (Inactive is not enough: Control Center or
    /// an alert over mdoc keeps the toast and its Undo.)
    func leftForeground() {
        commitPendingDelete()
    }
}
