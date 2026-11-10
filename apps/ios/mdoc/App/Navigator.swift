import Foundation
import Observation

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

/// What the main area shows (ADR 0102): Start, a sidebar section's page, or
/// a document.
enum Destination: Hashable {
    case start
    case section(SidebarSection)
    case document(DocumentID)
}

/// The one thing presented over the screens, from one place in the view
/// tree (`Presenter`): a dialog, a sheet, an add flow or an error.
enum Presentation: Identifiable {
    /// Rename…: the document's new title.
    case renameDocument(DocumentID)
    /// Rename… on the Cases page: the case's new name.
    case renameCase(CaseID)
    /// Move to case › New case…: a name for the case the document moves to.
    case moveToNewCase(DocumentID)
    /// Move to case › All cases…: every other case, to move the document to.
    case chooseCase(DocumentID)
    /// Share from a document's menu: the reviewed text.
    case share(ShareItems)
    case settings
    /// An add flow from Start, the sidebar's New menu or the Documents page.
    case intake(Intake)
    /// Something brought in, waiting for its title and case.
    case importing(PendingImport)
    /// Try a sample on an edited sample: "Restore the sample?"
    case restoreSample(DocumentID)
    /// Something went wrong.
    case error(String)

    var id: String {
        switch self {
        case .renameDocument(let id): "rename-\(id.rawValue)"
        case .renameCase(let id): "rename-case-\(id.rawValue)"
        case .moveToNewCase(let id): "new-case-\(id.rawValue)"
        case .chooseCase(let id): "choose-case-\(id.rawValue)"
        case .share(let items): "share-\(items.id)"
        case .settings: "settings"
        case .intake(let intake): "intake-\(intake.rawValue)"
        case .importing(let pending): "import-\(pending.id)"
        case .restoreSample(let id): "restore-sample-\(id.rawValue)"
        case .error(let message): "error-\(message)"
        }
    }
}

/// One owner of where the app is (ADR 0104): what the main area shows,
/// whether the sidebar shows, and the one thing presented over it. Every
/// layout reads the same values; nothing else writes them.
@MainActor
@Observable
final class Navigator {
    private(set) var destination: Destination = .start
    /// The sidebar shows: the drawer is open (iPhone, a narrow iPad), or the
    /// standing column is (a wide iPad). The drawer's drag and the split
    /// view's own button set it too.
    var showsSidebar = false
    /// The sidebar is a standing column, not a drawer: the window is wide
    /// (`DocumentLayout`). The root says when that changes.
    private(set) var sidebarStands = false
    private(set) var presentation: Presentation?
    /// ⌘K's request until the sidebar takes it, focusing its search (#675).
    private(set) var searchPending = false

    /// The open document; nil shows Start or a section's page.
    var openDocument: DocumentID? {
        if case .document(let id) = destination { id } else { nil }
    }

    /// The section whose page shows, if one does.
    var section: SidebarSection? {
        if case .section(let section) = destination { section } else { nil }
    }

    /// Shows `destination` in the main area, closing the drawer and
    /// Settings over it.
    func show(_ destination: Destination) {
        self.destination = destination
        if case .settings = presentation { presentation = nil }
        if !sidebarStands { showsSidebar = false }
    }

    /// The window turned wide or narrow: a standing column shows, and a
    /// drawer starts closed, so a window that narrows opens nothing over
    /// the screen.
    func sidebarLayoutChanged(stands: Bool) {
        sidebarStands = stands
        showsSidebar = stands
    }

    /// ⌘K: the sidebar shows with its search focused.
    func requestSearch() {
        showsSidebar = true
        searchPending = true
    }

    /// Whether ⌘K asked for the search and no sidebar took it yet; taking it
    /// clears it.
    func takeSearchRequest() -> Bool {
        defer { searchPending = false }
        return searchPending
    }

    func present(_ presentation: Presentation) {
        self.presentation = presentation
    }

    /// Dismisses what is presented, if it is still `id` (a newer
    /// presentation stays).
    func dismiss(_ id: Presentation.ID? = nil) {
        guard let current = presentation, id == nil || current.id == id else { return }
        presentation = nil
    }
}
