import Foundation
import Observation
import UIKit

/// What the Restore with mdoc sheet shows.
enum RestoreSheet {
    case ready(RestoreViewModel)
    /// Nothing was shared as text.
    case noText
    /// mdoc's Vault or its key could not be opened: the error's own words.
    case failed(String)

    @MainActor
    static func make(
        text: String?, store: () throws -> (store: any ReplyStore, sharedLast: RestorePlace?)
    ) -> RestoreSheet {
        guard let text else { return .noText }
        do {
            let (store, sharedLast) = try store()
            return .ready(RestoreViewModel(store: store, reply: text, sharedLast: sharedLast))
        } catch {
            return .failed(error.localizedDescription)
        }
    }
}

/// Restores a shared AI reply in place for the Restore with mdoc extension,
/// and saves it in the Vault for the app to list. It uses one place at a
/// time, a Project or an Unfiled document, so aliases from different
/// places never mix (ADR 0084's rule): the place of the last Copy for AI or
/// Share unless the user picks another (#1091, the maintainer's (a)). It
/// needs no model.
@MainActor
@Observable
final class RestoreViewModel {
    let reply: String
    private let store: any ReplyStore
    private(set) var places: RestorePlaces
    private(set) var selected: RestorePlace?
    private(set) var result: RestoreResult?
    private(set) var errorMessage: String?
    /// The reply as saved with the selected place.
    private(set) var saved: SavedReply?
    /// The restored text shown was copied.
    private(set) var copied = false

    init(store: any ReplyStore, reply: String, sharedLast: RestorePlace?) {
        self.store = store
        self.reply = reply
        places = store.places()
        selected = Self.defaultPlace(sharedLast: sharedLast, in: places)
        restore()
    }

    /// The place of the last Copy for AI or Share while it is there, else
    /// the first Project (pinned, then most recently updated), else the
    /// first Unfiled document. A document shared from Unfiled that has
    /// moved into a Project since is no longer a place of its own.
    static func defaultPlace(sharedLast: RestorePlace?, in places: RestorePlaces) -> RestorePlace? {
        if let sharedLast, places.contains(sharedLast) { return sharedLast }
        return places.all.first
    }

    var selectedName: String? {
        selected.flatMap(places.name(of:))
    }

    func select(_ place: RestorePlace) {
        guard place != selected else { return }
        selected = place
        saved = nil
        copied = false
        restore()
    }

    private func restore() {
        guard let selected else {
            result = nil
            errorMessage = CoreError.noProject.errorDescription
            return
        }
        do {
            result = try store.restore(reply, with: selected)
            errorMessage = nil
        } catch {
            result = nil
            errorMessage = error.localizedDescription
        }
    }

    /// Saves the reply, as received, with the selected place: the app lists
    /// it there and shows it restored. Once per place.
    func save() {
        guard let selected, saved == nil else { return }
        do {
            let title = String(localized: "Reply to \(selectedName ?? "")")
            saved = try store.save(reply, with: selected, title: title)
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func copy(to pasteboard: UIPasteboard = .general) {
        guard let result else { return }
        RestoredPasteboard.copy(result.text, to: pasteboard)
        copied = true
    }
}
