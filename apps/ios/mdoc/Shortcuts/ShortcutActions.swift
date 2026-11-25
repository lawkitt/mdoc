import Foundation

/// Pseudonymize text and Restore text as Shortcuts actions (#1164 decision
/// 11, Milestone 2 item 19, the maintainer's answers of 2026-11-25). Only
/// behind Settings › AI connector (preview): while it is off each action
/// refuses, and no App Shortcut offers them.
///
/// - Pseudonymize text saves the text as a new document in a Project, or
///   in Unfiled, and opens it in mdoc for review. It returns no text: the
///   document it made is not reviewed yet, and no action hides names
///   without review.
/// - Restore text restores an AI reply with a Project's names, or with the
///   place of the last Copy for AI or Share, as Restore with mdoc does. It
///   saves nothing; the restored text goes wherever the shortcut sends it.
enum ShortcutActions {
    /// Refused while Settings › AI connector (preview) is off.
    static func checkPreview(_ preferences: Preferences) throws {
        guard preferences.aiConnectorPreview else { throw ShortcutFailure.previewOff }
    }

    /// Restore text: `reply` restored with `project`'s names, or, with no
    /// Project, with the place of the last Copy for AI or Share. Unlike the
    /// Restore sheet, which shows the place it picked, nothing falls back
    /// to another Project: two Projects can give one alias to different
    /// people, and nobody sees which restored. Nothing is saved.
    @MainActor
    static func restore(
        _ reply: String, project: ProjectID?, store: any ReplyStore, sharedLast: RestorePlace?,
        preferences: Preferences
    ) throws -> String {
        try checkPreview(preferences)
        guard !reply.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw ShortcutFailure.noReply }
        let places = store.places()
        let place: RestorePlace
        if let project {
            place = .project(project)
            guard places.contains(place) else { throw ShortcutFailure.projectGone }
        } else {
            guard let sharedLast, places.contains(sharedLast) else { throw ShortcutFailure.chooseProject }
            place = sharedLast
        }
        return try store.restore(reply, with: place).text
    }

    /// The Projects the Project parameter offers, pinned first, then most
    /// recently updated, from a read-only handle on the Vault; none while
    /// the preview is off, or mdoc has no Vault yet or it can't be opened.
    static func suggestedProjects(
        preferences: Preferences, store: () throws -> any ReplyStore
    ) -> [ProjectEntity] {
        guard preferences.aiConnectorPreview else { return [] }
        return ((try? store().places().projects) ?? []).map(ProjectEntity.init)
    }

    /// The Projects a saved shortcut names, one for each id: as the Vault
    /// has it, or kept by its id under a placeholder name when it is no
    /// longer there, or while the preview is off (the Vault is not read
    /// then). The action then runs and says why it refuses, rather than
    /// running with no Project: Unfiled, or the place shared last.
    static func savedProjects(
        _ ids: [String], preferences: Preferences, store: () throws -> any ReplyStore
    ) -> [ProjectEntity] {
        guard preferences.aiConnectorPreview else {
            return ids.map { ProjectEntity(id: $0, name: String(localized: "Project")) }
        }
        let projects = (try? store().places().projects) ?? []
        return ids.map { id in
            projects.first { $0.id.rawValue == id }.map(ProjectEntity.init)
                ?? ProjectEntity(id: id, name: String(localized: "Project no longer in mdoc"))
        }
    }
}

/// Why a Shortcuts action returned nothing, in words the Shortcuts app
/// shows. None names a document: titles hold names as written.
enum ShortcutFailure: Error, Equatable, LocalizedError {
    case previewOff
    case noText
    case noReply
    case projectGone
    /// Restore text with no Project, and no place shared last in mdoc.
    case chooseProject
    /// The new document is open in mdoc, not yet Ready.
    case needsReview
    /// The new document is in mdoc, not read yet: another job held the
    /// one job slot.
    case notReadYet

    var errorDescription: String? {
        switch self {
        case .previewOff:
            String(localized: "Turn on AI connector (preview) in Settings")
        case .noText:
            String(localized: "There is no text to pseudonymize.")
        case .noReply:
            String(localized: "There is no reply to restore.")
        case .projectGone:
            String(localized: "That Project is no longer in mdoc.")
        case .chooseProject:
            String(localized: "Choose a Project: mdoc has no document or Project you shared last.")
        case .notReadYet:
            String(localized: "The text is in mdoc as a new document, not read yet while mdoc finishes another job. Open it in mdoc to read and review it.")
        case .needsReview:
            String(localized: "The text is in mdoc for review, open as a new document. Once it is reviewed, Copy for AI gives its text with names replaced.")
        }
    }
}

extension AppModel {
    /// Pseudonymize text from Shortcuts: `text` becomes a new document in
    /// `project`, or Unfiled for nil, opens and is read as a paste is, and
    /// waits there for review. Once it is open this throws `needsReview`,
    /// which stops the shortcut: nothing comes back on the run that made
    /// it, even when the Vault would count it Ready (below the name
    /// finder's floor, a document with no names marked by hand is), since
    /// nobody has read it yet and no action hides names without review.
    /// Once reviewed, Copy for AI and Ask AI give its text.
    func addForReview(fromShortcut text: String, into project: ProjectID?) async throws {
        try ShortcutActions.checkPreview(preferences)
        guard let pending = Importer.text(text) else { throw ShortcutFailure.noText }
        if let project, library.project(project) == nil { throw ShortcutFailure.projectGone }
        let session = try library.importDocument(pending, title: pending.suggestedTitle, into: project)
        navigator.dismiss()
        open(session.id)
        guard let model = documents.model(for: session.id) else { throw CoreError.notFound }
        if model.snapshot.phase == nil {
            await model.recognize()
        }
        // Another document's job held the slot: it is in mdoc, unread, and
        // a second run would add it again.
        guard model.snapshot.phase != nil else { throw ShortcutFailure.notReadYet }
        throw ShortcutFailure.needsReview
    }
}
