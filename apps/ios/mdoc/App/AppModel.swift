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
        let ids: [DocumentID]
        let toast: ToastMessage.ID
        /// The one of them that was open.
        let wasOpen: DocumentID?
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
        let setup = SetupViewModel(setup: core.setup, downloader: downloader, network: network, jobs: jobs, preferences: preferences)
        let library = LibraryModel(library: core.library) { [weak setup] in setup?.snapshot.tier == .detector }
        self.jobs = jobs
        self.library = library
        self.setup = setup
        importer = Importer(storage: storage)
        documents = DocumentStore { id in
            DocumentViewModel(
                session: try core.library.session(for: id), jobs: jobs, storage: storage,
                canFindNames: { [weak setup] in setup?.snapshot.tier == .detector }
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
                downloader: models, network: NetworkMonitor(), preferences: preferences)
            #if DEBUG
            if let seed = launch.seedFile {
                UITestSeed.run(UITestSeed.fixture(seed), in: app)
            }
            if launch.seedsRestorePlaces {
                UITestSeed.seedRestorePlaces(in: app)
            }
            #endif
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
        // Remove PDFs left by versions that rendered DOCX Originals to disk.
        try? FileManager.default.removeItem(
            at: FileManager.default.temporaryDirectory.appendingPathComponent("originals", isDirectory: true))
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

    /// Opens a section's page and closes the sidebar on iPhone.
    func open(_ section: SidebarSection) {
        show(.section(section))
    }

    /// A Project's row: its page (#1091 Decision 34).
    func openProject(_ id: ProjectID) {
        guard library.project(id) != nil else { return }
        show(.project(id))
    }

    /// Home: no document and no section.
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
            documents.model(for: open)?.closeOriginal()
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
            // Its reply restores from the names as saved now.
            if let model = documents.model(for: id), rejoined || model.snapshot.reply != nil { model.refresh() }
        case .failed(let id, let error):
            if let model = documents.model(for: id), navigator.openDocument == id {
                model.errorMessage = error.localizedDescription
            } else {
                present(error)
            }
        }
    }

    /// Shows what another handle saved in the Vault meanwhile: a reply
    /// Restore with mdoc kept (#1091 Decision 20). mdoc comes to the
    /// foreground, a list is pulled, or a share sheet closes.
    func takeInChanges() {
        guard core.library.takeInChanges() else { return }
        library.refresh()
        refreshDocuments()
    }

    /// Every open document shows the Vault as it is now.
    private func refreshDocuments() {
        for model in documents.all {
            model.refresh()
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

    /// The Project an intake is started from: the Project whose page shows,
    /// else nil, Unfiled (#1091 Decision 35).
    var intakeProject: ProjectID? {
        navigator.project
    }

    /// Something brought in waits for its title and Project, unless its
    /// file is already in mdoc: then it asks first (#1091 Decision 7).
    func offer(_ pending: PendingImport) {
        if let repeated = library.repeatOf(pending) {
            navigator.present(.repeatedImport(repeated))
        } else {
            navigator.present(.importing(pending))
        }
    }

    /// "Already in mdoc" answered Open Existing: the document imported
    /// before opens, and what was brought in goes.
    func openExisting(_ repeated: RepeatedImport) {
        navigator.dismiss(Presentation.repeatedImport(repeated).id)
        Importer.discard(repeated.pending)
        open(repeated.existing)
    }

    /// Answered Import Again: it imports as a new document, as any other.
    func importAgain(_ repeated: RepeatedImport) {
        navigator.present(.importing(repeated.pending))
    }

    /// Answered Cancel: nothing is imported, and what was brought in goes.
    func cancelImport(_ repeated: RepeatedImport) {
        navigator.dismiss(Presentation.repeatedImport(repeated).id)
        Importer.discard(repeated.pending)
    }

    /// New document (ADR 0109; also ⌘N): an empty document in Unfiled or the
    /// Project it was started from, open in edit mode with the keyboard up. It is named after its first
    /// line until renamed, and goes if it is left empty.
    func newDocument() {
        if case .settings = navigator.presentation { navigator.dismiss() }
        let title = String(localized: "New document")
        do {
            let session = try library.importDocument(
                PendingImport(source: .newDocument, suggestedTitle: title), title: title, into: intakeProject)
            // Its empty text is read now, outside the one job slot: a job on
            // another document never holds it back (ADR 0085 Long jobs).
            _ = try session.recognize(observer: JobToken { _ in })
            open(session.id)
            documents.model(for: session.id)?.enterEditMode()
        } catch {
            present(error)
        }
    }

    /// Imports into Unfiled or the chosen Project, opens the document and
    /// starts recognition, which the user asked for by adding it.
    @discardableResult
    func finishImport(_ pending: PendingImport, title: String, into project: ProjectID?) -> DocumentViewModel? {
        do {
            let session = try library.importDocument(pending, title: title, into: project)
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

    /// A file from Files, waiting for its title and Project: picked from
    /// the + menu, it starts in the Project it was started from.
    @discardableResult
    func receiveFile(_ url: URL) -> Task<Void, Never> {
        receiveFile(url, from: intakeProject)
    }

    /// "Open in mdoc" from another app: started from no Project, it starts
    /// in Unfiled whatever page shows (#1091 Decision 35).
    @discardableResult
    func openInMdoc(_ url: URL) -> Task<Void, Never> {
        receiveFile(url, from: nil)
    }

    /// A file waiting for its title and Project. It is copied (an image
    /// read, stripped and encoded) off the main actor; the task ends once
    /// it waits.
    private func receiveFile(_ url: URL, from project: ProjectID?) -> Task<Void, Never> {
        let importer = importer
        return Task {
            do {
                if var pending = try await Task.detached(priority: .userInitiated, operation: { try importer.file(url) }).value {
                    pending.project = project
                    offer(pending)
                }
            } catch {
                present(error)
            }
        }
    }

    /// Pages from the document camera or Photos, each read, stripped and
    /// encoded off the main actor; the task ends once they wait, in the
    /// Project they were started from.
    @discardableResult
    func receiveImages(_ images: [Data?], scanned: Bool) -> Task<Void, Never> {
        let importer = importer
        let project = intakeProject
        return Task {
            do {
                let pending = try await Task.detached(priority: .userInitiated) {
                    try importer.images(images, scanned: scanned)
                }.value
                if var pending {
                    pending.project = project
                    offer(pending)
                    // Never silent: the pages left out are counted.
                    if pending.unreadPages > 0 {
                        show(ToastMessage(text: Importer.unreadNote(pending.unreadPages)))
                    }
                }
            } catch {
                present(error)
            }
        }
    }

    /// Paste's one sheet: the text, its title and its Project at once
    /// (ADR 0087).
    func addText(_ text: String, title: String, into project: ProjectID?) {
        guard let pending = Importer.text(text) else { return }
        finishImport(pending, title: title, into: project)
    }

    /// Try a sample (ADR 0099): the made-up agreement in the phone's
    /// language, in Unfiled, opened for review. The sample is
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
        finishImport(pending, title: sample.title, into: nil)?.proposeStored(sample.names)
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

    func renameProject(_ id: ProjectID, to name: String) {
        guard !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        do {
            try library.renameProject(id, to: name)
            documents.all.forEach { $0.refresh() }
            Haptics.decision()
        } catch {
            present(error)
        }
    }

    /// Move to case: the names take the case's aliases; the toast offers
    /// Undo, which moves the document back.
    func move(_ id: DocumentID, to target: ProjectChoice) {
        do {
            guard let move = try library.move(id, to: target) else { return }
            // Every open document: a Project's replies follow its documents.
            refreshDocuments()
            Haptics.decision()
            show(ToastMessage(
                text: String(localized: "Moved to \(move.placeName)"),
                actionTitle: String(localized: "Undo"),
                action: { [weak self] in self?.undoMove(move) }))
        } catch {
            present(error)
        }
    }

    private func undoMove(_ move: ProjectMove) {
        do {
            let back = try library.undoMove(move)
            refreshDocuments()
            Haptics.warning()
            show(ToastMessage(text: String(localized: "Moved back to \(back.placeName)")))
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
        delete([id])
    }

    /// Delete of documents selected together (Unfiled's Select): one toast
    /// offers Undo for all of them, as Delete does for one.
    func delete(_ ids: [DocumentID]) {
        commitPendingDelete()
        let listed = ids.compactMap { library.document($0) }
        guard !listed.isEmpty else { return }
        let ids = listed.map(\.id)
        let open = navigator.openDocument.flatMap { ids.contains($0) ? $0 : nil }
        if open != nil { showStart() }
        library.hide(ids)
        let text = listed.count == 1
            ? String(localized: "Deleted \(listed[0].title)")
            : String(localized: "Deleted \(listed.count) documents")
        let toast = ToastMessage(
            text: text, actionTitle: String(localized: "Undo"),
            action: { [weak self] in self?.undoDelete() })
        pendingDelete = PendingDelete(ids: ids, toast: toast.id, wasOpen: open)
        show(toast)
    }

    /// Undo of Delete: the documents are back where they were, the open one
    /// open again.
    func undoDelete() {
        guard let pending = pendingDelete else { return }
        pendingDelete = nil
        library.unhide(pending.ids)
        Haptics.warning()
        // Open again where it closed, unless something else opened since.
        if let open = pending.wasOpen, navigator.destination == .start { self.open(open) }
    }

    /// The Delete waiting on its toast happens now; its toast, and the
    /// Undo it offered, go with it.
    func commitPendingDelete() {
        guard let pending = pendingDelete else { return }
        pendingDelete = nil
        if toast?.id == pending.toast { toast = nil }
        // Its job stops: a scan or import that finished later would save
        // the document again.
        if let working = jobs.current?.documentID, pending.ids.contains(working) { jobs.cancel() }
        pending.ids.forEach(documents.remove)
        do {
            try library.delete(pending.ids)
        } catch {
            present(error)
        }
        // Their replies went with them, from other documents' Reply panes
        // too; after a failure, the ones deleted before it.
        refreshDocuments()
    }

    // MARK: Projects (#1091 Decisions 9, 33, 34)

    /// New Project: an empty Project, named as every new one is, whose page
    /// opens.
    func newProject(named name: String) {
        do {
            guard let project = try library.createProject(named: name) else { return }
            Haptics.decision()
            openProject(project.id)
        } catch {
            present(error)
        }
    }

    /// Delete Project…: with documents it asks what becomes of them; empty,
    /// it asks to delete (`Presenter`).
    func askToDeleteProject(_ id: ProjectID) {
        guard library.project(id) != nil else { return }
        navigator.present(.deleteProject(id))
    }

    /// Delete Project, answered: the Project and its own AI replies go, and
    /// its documents go to Unfiled or with it. No Undo: the question was
    /// the confirmation. Its page, if it shows, gives way to All Projects.
    func deleteProject(_ id: ProjectID, documents choice: ProjectDeletion) {
        // The question goes even though its alert stops matching first.
        navigator.dismiss(Presentation.deleteProject(id).id)
        commitPendingDelete()
        guard let project = library.project(id) else { return }
        let members = library.documents(in: id).map(\.id)
        if choice == .deleteDocuments {
            if let open = navigator.openDocument, members.contains(open) { showStart() }
            if let working = jobs.current?.documentID, members.contains(working) { jobs.cancel() }
        }
        do {
            try library.deleteProject(id, documents: choice)
        } catch {
            present(error)
        }
        if choice == .deleteDocuments {
            members.filter { library.document($0) == nil }.forEach(documents.remove)
        }
        // Moved ones show Unfiled, and their replies; the others lose the
        // Project's own.
        refreshDocuments()
        guard library.project(id) == nil else { return }
        if navigator.project == id { open(.projects) }
        Haptics.warning()
        show(ToastMessage(text: String(localized: "Deleted \(project.name)")))
    }

    /// A swipe on a Project page's Always or Never row.
    func removeListed(_ wording: String, always: Bool, from project: ProjectID) {
        do {
            try library.removeListed(wording, always: always, from: project)
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
