import AppIntents
import Foundation

// The Shortcuts actions (`ShortcutActions`). Neither is in an App
// Shortcuts provider, so Siri and Spotlight never suggest them; the
// Shortcuts app lists them, and while Settings › AI connector (preview) is
// off each refuses.

/// Pseudonymize text: the text becomes a new document in mdoc, opened for
/// review; its text for AI is Copy for AI's once it is reviewed.
struct PseudonymizeTextIntent: AppIntent {
    static let title: LocalizedStringResource = "Pseudonymize text"
    static let description = IntentDescription(
        "Adds the text to mdoc as a new document in a Project, or in Unfiled when no Project is chosen, and opens it for review. Once it is reviewed, Copy for AI in mdoc gives its text with names replaced.")
    /// It opens the document in mdoc.
    static let supportedModes: IntentModes = .foreground

    @Parameter(title: "Text", inputOptions: String.IntentInputOptions(multiline: true))
    var text: String

    @Parameter(title: "Project", description: "Leave empty for a new Unfiled document.")
    var project: ProjectEntity?

    static var parameterSummary: some ParameterSummary {
        Summary("Pseudonymize \(\.$text) in \(\.$project)")
    }

    /// Adds the text for review and stops the shortcut, saying so
    /// (`AppModel.addForReview` throws once the document is open): what
    /// follows gets nothing to send.
    @MainActor
    func perform() async throws -> some IntentResult {
        try ShortcutActions.checkPreview(.shared)
        let app = await ShortcutsHost.shared.app()
        try await app.addForReview(fromShortcut: text, into: project.map { ProjectID(rawValue: $0.id) })
        return .result()
    }
}

/// Restore text: an AI reply with the real names back, saved nowhere.
struct RestoreTextIntent: AppIntent {
    static let title: LocalizedStringResource = "Restore text"
    static let description = IntentDescription(
        "Restores an AI reply with the real names, using a Project's names, or those of the last document or Project you shared. Nothing is saved; the restored text goes wherever the shortcut sends it next.")
    static let supportedModes: IntentModes = .background

    @Parameter(title: "AI reply", inputOptions: String.IntentInputOptions(multiline: true))
    var reply: String

    @Parameter(title: "Project", description: "Leave empty for the last document or Project you shared.")
    var project: ProjectEntity?

    static var parameterSummary: some ParameterSummary {
        Summary("Restore \(\.$reply) with \(\.$project)")
    }

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        try ShortcutActions.checkPreview(.shared)
        let (store, sharedLast) = try CoreFactory.restoreExtension()
        let restored = try ShortcutActions.restore(
            reply, project: project.map { ProjectID(rawValue: $0.id) }, store: store, sharedLast: sharedLast,
            preferences: .shared)
        return .result(value: restored)
    }
}

/// A Project, as the actions' Project parameter lists it: by its name.
struct ProjectEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Project"
    static let defaultQuery = ProjectQuery()

    let id: String
    let name: String

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)")
    }

    init(_ project: ProjectSummary) {
        id = project.id.rawValue
        name = project.name
    }
}

struct ProjectQuery: EntityQuery {
    func entities(for identifiers: [String]) async throws -> [ProjectEntity] {
        let wanted = Set(identifiers)
        return Self.projects().filter { wanted.contains($0.id) }
    }

    func suggestedEntities() async throws -> [ProjectEntity] {
        Self.projects()
    }

    /// Every Project, read through the Restore extension's read-only
    /// handle; none while the preview is off.
    private static func projects() -> [ProjectEntity] {
        guard Preferences.shared.aiConnectorPreview else { return [] }
        return ShortcutActions.projects { try CoreFactory.restoreExtension().store }.map(ProjectEntity.init)
    }
}

/// The app's model for Pseudonymize text, which runs in mdoc's process
/// once mdoc is in front: the window's model, attached as it opens.
@MainActor
final class ShortcutsHost {
    static let shared = ShortcutsHost()

    private weak var current: AppModel?
    private var waiting: [CheckedContinuation<AppModel, Never>] = []

    /// The window shows `app`: an action waiting for it runs now.
    func attach(_ app: AppModel) {
        current = app
        let waiting = self.waiting
        self.waiting = []
        for continuation in waiting {
            continuation.resume(returning: app)
        }
    }

    /// The window's model, once mdoc's window has opened.
    func app() async -> AppModel {
        if let current { return current }
        return await withCheckedContinuation { waiting.append($0) }
    }
}
