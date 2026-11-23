import Foundation
import Observation
import UIKit

/// What the Restore with mdoc sheet shows.
enum RestoreSheet {
    case ready(RestoreViewModel)
    /// Nothing was shared as text.
    case noText
    /// mdoc's storage or its case key could not be opened: the error's own
    /// words.
    case failed(String)

    @MainActor
    static func make(text: String?, library: () throws -> any Library) -> RestoreSheet {
        guard let text else { return .noText }
        do {
            return .ready(RestoreViewModel(library: try library(), reply: text))
        } catch {
            return .failed(error.localizedDescription)
        }
    }
}

/// Restores a shared AI reply in place for the Restore with mdoc extension.
/// It uses one case at a time, the case of the document shared last unless
/// the user picks another, so aliases from different projects never mix
/// (ADR 0084's rule). It needs no model.
@MainActor
@Observable
final class RestoreViewModel {
    let reply: String
    private let library: any Library
    private(set) var snapshot: LibrarySnapshot
    private(set) var selectedProject: ProjectID?
    private(set) var result: RestoreResult?
    private(set) var errorMessage: String?
    private(set) var savedTo: DocumentID?
    /// The restored text shown was copied.
    private(set) var copied = false

    init(library: any Library, reply: String) {
        self.library = library
        self.reply = reply
        snapshot = library.snapshot()
        selectedProject = snapshot.currentProject ?? snapshot.projects.first?.id
        restore()
    }

    var projects: [ProjectSummary] { snapshot.projects }

    var selectedProjectName: String? {
        projects.first { $0.id == selectedProject }?.name
    }

    var documentsInProject: [LibraryDocument] {
        guard let selectedProject else { return [] }
        return snapshot.documents(in: selectedProject)
    }

    /// Save to document's default: the case's most recently changed document.
    var defaultSaveTarget: LibraryDocument? {
        documentsInProject.first
    }

    func selectProject(_ id: ProjectID) {
        guard id != selectedProject else { return }
        selectedProject = id
        savedTo = nil
        copied = false
        restore()
    }

    private func restore() {
        guard let selectedProject else {
            result = nil
            errorMessage = CoreError.noProject.errorDescription
            return
        }
        do {
            result = try library.restore(reply: reply, in: selectedProject)
            errorMessage = nil
        } catch {
            result = nil
            errorMessage = error.localizedDescription
        }
    }

    /// Keeps the reply, in alias form, as the document's reply (ADR 0078).
    func save(to document: DocumentID) {
        do {
            try library.saveReply(reply, to: document)
            savedTo = document
            errorMessage = nil
            snapshot = library.snapshot()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func copy(to pasteboard: UIPasteboard = .general) {
        guard let result else { return }
        RestoredPasteboard.copy(result.text, to: pasteboard)
        copied = true
    }

    func title(of document: DocumentID) -> String? {
        snapshot.documents.first { $0.id == document }?.title
    }
}
